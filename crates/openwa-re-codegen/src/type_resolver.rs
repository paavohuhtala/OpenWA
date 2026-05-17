//! Parser + resolver for TOML `TypeRef` strings (`"BaseEntity *"`,
//! `"char *[7]"`, `"const int *"`, …) → Rust type rendering.
//!
//! Strategy:
//!   1. Tokenise the TypeRef from the right: strip trailing `[N]` arrays,
//!      then trailing `*` (and the C grammar's `* const` modifier).
//!   2. When stripping a `*`, inspect the *pointee* text for `const` to
//!      decide `*const T` vs `*mut T`.
//!   3. Resolve the base name via:
//!        - built-in primitive map (`int → i32`, etc.)
//!        - `Struct/Union/Enum/Typedef.rust_path` from the [`Catalog`]
//!        - typedef chasing (recurse into `target`)
//!        - `FunctionDef` → `ResolvedTy::Fn` (for function-pointer fields)
//!   4. Any unresolved base bubbles up as [`ResolvedTy::Unresolved`].
//!
//! User types resolve only when their TOML entry carries an explicit
//! `rust_path` (e.g. `rust_path = "openwa_game::engine::game_info::GameInfo"`).
//! Otherwise the name surfaces as [`ResolvedTy::Unresolved`] and the
//! consumer (e.g. `emit_wa_calls`) silently skips the wrapper — the
//! documented best-effort behaviour.

use anyhow::{Result, anyhow};
use openwa_re_data::toml_io::Catalog;

/// A fully (or partially) resolved Rust type, suitable for emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTy {
    /// Absolute Rust path, e.g. `"i32"`, `"core::ffi::c_void"`,
    /// `"openwa_game::game::GameRuntime"`.
    Path(String),
    Ptr {
        mutable: bool,
        inner: Box<ResolvedTy>,
    },
    Array {
        len: u32,
        inner: Box<ResolvedTy>,
    },
    /// A base name we couldn't resolve. Carries the original token so the
    /// audit log can group skipped wrappers by unresolved type.
    Unresolved(String),
}

impl ResolvedTy {
    /// Render to Rust source text (e.g. `*const openwa_game::game::Foo`).
    /// Returns `Err` if any sub-component is `Unresolved` — callers use this
    /// to decide whether to emit a wrapper or skip it.
    pub fn render(&self) -> Result<String> {
        let mut out = String::new();
        write(self, &mut out)?;
        Ok(out)
    }

    /// First unresolved base type encountered when walking the tree, if any.
    pub fn first_unresolved(&self) -> Option<&str> {
        match self {
            ResolvedTy::Unresolved(s) => Some(s.as_str()),
            ResolvedTy::Path(_) => None,
            ResolvedTy::Ptr { inner, .. } | ResolvedTy::Array { inner, .. } => {
                inner.first_unresolved()
            }
        }
    }
}

fn write(ty: &ResolvedTy, out: &mut String) -> Result<()> {
    use std::fmt::Write as _;
    match ty {
        ResolvedTy::Path(p) => out.push_str(p),
        ResolvedTy::Ptr {
            mutable: true,
            inner,
        } => {
            out.push_str("*mut ");
            write(inner, out)?;
        }
        ResolvedTy::Ptr {
            mutable: false,
            inner,
        } => {
            out.push_str("*const ");
            write(inner, out)?;
        }
        ResolvedTy::Array { len, inner } => {
            out.push('[');
            write(inner, out)?;
            write!(out, "; {len}").unwrap();
            out.push(']');
        }
        ResolvedTy::Unresolved(n) => {
            return Err(anyhow!("unresolved type `{n}`"));
        }
    }
    Ok(())
}

/// Maps a primitive C-ish type name (as written in `re/*.toml`) to the
/// canonical Rust spelling. Returns `None` for non-primitive names.
pub fn primitive_rust(name: &str) -> Option<&'static str> {
    Some(match name {
        "void" => "core::ffi::c_void",
        "char" => "core::ffi::c_char",
        "uchar" | "byte" | "u8" => "u8",
        "schar" | "sbyte" | "i8" => "i8",
        "short" | "i16" => "i16",
        "ushort" | "u16" | "word" => "u16",
        "int" | "long" | "i32" | "LONG" | "BOOL" => "i32",
        "uint" | "ulong" | "u32" | "dword" | "DWORD" | "UINT" => "u32",
        "longlong" | "i64" | "__int64" => "i64",
        "ulonglong" | "qword" | "u64" => "u64",
        "float" => "f32",
        "double" => "f64",
        "bool" => "bool",
        "wchar_t" | "wchar16" | "WCHAR" => "u16",
        // Ghidra placeholders for un-analysed widths.
        "undefined" | "undefined1" => "u8",
        "undefined2" => "u16",
        "undefined4" => "u32",
        "undefined8" => "u64",
        _ => return None,
    })
}

/// Entry point: parse a raw TOML TypeRef and resolve it against `cat`.
pub fn parse_type_ref(raw: &str, cat: &Catalog) -> ResolvedTy {
    parse_inner(raw, cat, 0)
}

fn parse_inner(raw: &str, cat: &Catalog, depth: u32) -> ResolvedTy {
    // Cheap guard against accidental cycles from malformed typedef chains.
    if depth > 32 {
        return ResolvedTy::Unresolved(raw.trim().to_string());
    }
    let s = raw.trim();

    if let Some((base, len)) = strip_trailing_array(s) {
        return ResolvedTy::Array {
            len,
            inner: Box::new(parse_inner(base, cat, depth + 1)),
        };
    }

    // C grammar's `T * const` (constant pointer) — strip the trailing `const`
    // marker; it has no Rust representation at the pointer-type level.
    let s = strip_trailing_const(s);

    if let Some(inner_text) = s.strip_suffix('*') {
        let (mutable, pointee_text) = strip_pointee_const(inner_text);
        return ResolvedTy::Ptr {
            mutable,
            inner: Box::new(parse_inner(pointee_text, cat, depth + 1)),
        };
    }

    // Base name. Drop any leading `const ` / `volatile ` on a non-pointer type
    // — has no effect on the Rust rendering.
    let name = strip_leading_cv(s).trim();
    resolve_base(name, cat, depth)
}

fn resolve_base(name: &str, cat: &Catalog, depth: u32) -> ResolvedTy {
    if let Some(prim) = primitive_rust(name) {
        return ResolvedTy::Path(prim.to_string());
    }

    // Explicit `rust_path` wins over typedef chasing — a typedef tagged with
    // a Rust path is treating the Rust newtype as the canonical target,
    // even if its TOML `target` would resolve to a primitive.
    if let Some(td) = cat.typedefs.get(name) {
        if let Some(rp) = td.value.rust_path.as_deref() {
            return ResolvedTy::Path(rp.to_string());
        }
        return parse_inner(&td.value.target, cat, depth + 1);
    }

    if let Some(rp) = cat
        .structs
        .get(name)
        .and_then(|e| e.value.rust_path.as_deref())
        .or_else(|| {
            cat.unions
                .get(name)
                .and_then(|e| e.value.rust_path.as_deref())
        })
        .or_else(|| {
            cat.enums
                .get(name)
                .and_then(|e| e.value.rust_path.as_deref())
        })
    {
        return ResolvedTy::Path(rp.to_string());
    }

    ResolvedTy::Unresolved(name.to_string())
}

// ── tokenisation helpers ────────────────────────────────────────────────────

/// If `s` ends in `[N]`, returns `(s_before_bracket, N)`. Only matches the
/// outermost array suffix.
fn strip_trailing_array(s: &str) -> Option<(&str, u32)> {
    let s = s.trim_end();
    let stripped = s.strip_suffix(']')?;
    // Find the matching '[' at depth 0.
    let bytes = stripped.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().rev() {
        match b {
            b']' => depth += 1,
            b'[' if depth == 0 => {
                let (base, len_text) = stripped.split_at(i);
                let len_text = &len_text[1..]; // skip '['
                let len = parse_int(len_text.trim())?;
                return Some((base.trim_end(), len));
            }
            b'[' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Strip a trailing `const` keyword (preceded by whitespace or a `*`). This
/// is the C grammar's `T * const` constant-pointer modifier, which Rust
/// doesn't represent at the type level — we discard it.
fn strip_trailing_const(s: &str) -> &str {
    let trimmed = s.trim_end();
    if let Some(prefix) = trimmed.strip_suffix("const") {
        // Must be a standalone keyword (preceded by whitespace or '*').
        if prefix.ends_with(|c: char| c.is_whitespace() || c == '*') {
            return prefix.trim_end();
        }
    }
    s
}

/// Given the text immediately *before* a trailing `*`, decide whether the
/// pointer should be `*mut` (default) or `*const` (pointee carries `const`),
/// and return the pointee text with the `const` token stripped.
///
/// Handles both `const T` and `T const`. Only consumes the `const` token
/// when no further `*` remains in the text — in `const char **` the
/// leftmost `const` modifies `char` at the deepest pointee level, not the
/// outer pointer we just stripped. The recursion picks it up on its own.
fn strip_pointee_const(inner_text: &str) -> (bool, &str) {
    let t = inner_text.trim();
    if t.contains('*') {
        return (true, t);
    }
    // `const T`
    if let Some(rest) = t.strip_prefix("const")
        && rest.starts_with(|c: char| c.is_whitespace())
    {
        return (false, rest.trim_start());
    }
    // `T const`
    if let Some(rest) = t.strip_suffix("const")
        && rest.ends_with(|c: char| c.is_whitespace())
    {
        return (false, rest.trim_end());
    }
    (true, t)
}

/// Strip a leading `const ` / `volatile ` from a base-type name.
fn strip_leading_cv(s: &str) -> &str {
    let s = s.trim_start();
    for kw in ["const ", "volatile "] {
        if let Some(rest) = s.strip_prefix(kw) {
            return strip_leading_cv(rest);
        }
    }
    s
}

fn parse_int(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwa_re_data::model::{Enum, Struct, Typedef};
    use openwa_re_data::toml_io::OwnedEntry;
    use std::path::PathBuf;

    fn empty_cat() -> Catalog {
        Catalog::default()
    }

    fn with_typedef(name: &str, target: &str) -> Catalog {
        let mut cat = Catalog::default();
        cat.typedefs.insert(
            name.to_string(),
            OwnedEntry {
                value: Typedef {
                    name: name.to_string(),
                    namespace: None,
                    rust_path: None,
                    target: target.to_string(),
                },
                source: PathBuf::from("<test>"),
            },
        );
        cat
    }

    fn with_struct_rust_path(name: &str, rust_path: &str) -> Catalog {
        let mut cat = Catalog::default();
        cat.structs.insert(
            name.to_string(),
            OwnedEntry {
                value: Struct {
                    name: name.to_string(),
                    namespace: None,
                    size: 0,
                    plate_comment: None,
                    rust_path: Some(rust_path.to_string()),
                    field: Vec::new(),
                },
                source: PathBuf::from("<test>"),
            },
        );
        cat
    }

    fn with_typedef_rust_path(name: &str, target: &str, rust_path: &str) -> Catalog {
        let mut cat = Catalog::default();
        cat.typedefs.insert(
            name.to_string(),
            OwnedEntry {
                value: Typedef {
                    name: name.to_string(),
                    namespace: None,
                    rust_path: Some(rust_path.to_string()),
                    target: target.to_string(),
                },
                source: PathBuf::from("<test>"),
            },
        );
        cat
    }

    fn with_enum_rust_path(name: &str, rust_path: &str) -> Catalog {
        let mut cat = Catalog::default();
        cat.enums.insert(
            name.to_string(),
            OwnedEntry {
                value: Enum {
                    name: name.to_string(),
                    namespace: None,
                    size: 4,
                    rust_path: Some(rust_path.to_string()),
                    variant: Default::default(),
                },
                source: PathBuf::from("<test>"),
            },
        );
        cat
    }

    fn render(raw: &str, cat: &Catalog) -> String {
        parse_type_ref(raw, cat).render().unwrap()
    }

    #[test]
    fn primitives() {
        let c = empty_cat();
        assert_eq!(render("int", &c), "i32");
        assert_eq!(render("uint", &c), "u32");
        assert_eq!(render("void", &c), "core::ffi::c_void");
        assert_eq!(render("char", &c), "core::ffi::c_char");
        assert_eq!(render("undefined4", &c), "u32");
        assert_eq!(render("byte", &c), "u8");
    }

    #[test]
    fn pointer_to_primitive() {
        let c = empty_cat();
        assert_eq!(render("int *", &c), "*mut i32");
        assert_eq!(render("void *", &c), "*mut core::ffi::c_void");
        assert_eq!(render("char *", &c), "*mut core::ffi::c_char");
    }

    #[test]
    fn const_pointee() {
        let c = empty_cat();
        assert_eq!(render("const int *", &c), "*const i32");
        assert_eq!(render("const char *", &c), "*const core::ffi::c_char");
        assert_eq!(render("int const *", &c), "*const i32");
    }

    #[test]
    fn const_pointer_itself_drops_to_mut() {
        // `T * const` is a const pointer in C — Rust has no equivalent at
        // the pointer-type level, so we render `*mut T`.
        let c = empty_cat();
        assert_eq!(render("int * const", &c), "*mut i32");
        assert_eq!(render("int *const", &c), "*mut i32");
    }

    #[test]
    fn array_of_primitive() {
        let c = empty_cat();
        assert_eq!(render("byte[160]", &c), "[u8; 160]");
        assert_eq!(render("int[4]", &c), "[i32; 4]");
    }

    #[test]
    fn array_of_pointer() {
        let c = empty_cat();
        assert_eq!(render("char *[7]", &c), "[*mut core::ffi::c_char; 7]");
        assert_eq!(
            render("const char *[3]", &c),
            "[*const core::ffi::c_char; 3]"
        );
    }

    #[test]
    fn pointer_to_pointer() {
        let c = empty_cat();
        assert_eq!(render("void **", &c), "*mut *mut core::ffi::c_void");
        assert_eq!(render("const char **", &c), "*mut *const core::ffi::c_char");
    }

    #[test]
    fn typedef_chase_to_primitive() {
        let c = with_typedef("WormIdx", "byte");
        assert_eq!(render("WormIdx", &c), "u8");
        assert_eq!(render("WormIdx *", &c), "*mut u8");
        assert_eq!(render("WormIdx[16]", &c), "[u8; 16]");
    }

    #[test]
    fn unresolved_struct_bubbles_up() {
        let c = empty_cat();
        let r = parse_type_ref("BaseEntity *", &c);
        // The pointer resolves; the pointee doesn't.
        assert_eq!(r.first_unresolved(), Some("BaseEntity"));
        assert!(r.render().is_err());
    }

    #[test]
    fn unresolved_inside_array() {
        let c = empty_cat();
        let r = parse_type_ref("CustomType[5]", &c);
        assert_eq!(r.first_unresolved(), Some("CustomType"));
    }

    #[test]
    fn hex_array_length() {
        let c = empty_cat();
        assert_eq!(render("byte[0x10]", &c), "[u8; 16]");
    }

    #[test]
    fn typedef_cycle_terminates() {
        // Self-referential typedef shouldn't infinite-loop; depth guard kicks in.
        let c = with_typedef("Loop", "Loop");
        let r = parse_type_ref("Loop", &c);
        // Either Unresolved (depth limit) or Unresolved at the leaf. Test only
        // that we don't panic / overflow.
        assert!(r.first_unresolved().is_some() || r.render().is_ok());
    }

    #[test]
    fn primitive_array_hex_and_decimal() {
        let c = empty_cat();
        assert_eq!(render("int[4]", &c), "[i32; 4]");
        assert_eq!(render("int[0x4]", &c), "[i32; 4]");
    }

    #[test]
    fn struct_with_rust_path_resolves() {
        let c = with_struct_rust_path("BaseEntity", "openwa_game::entity::BaseEntity");
        assert_eq!(render("BaseEntity", &c), "openwa_game::entity::BaseEntity");
        assert_eq!(
            render("BaseEntity *", &c),
            "*mut openwa_game::entity::BaseEntity"
        );
        assert_eq!(
            render("const BaseEntity *", &c),
            "*const openwa_game::entity::BaseEntity"
        );
        assert_eq!(
            render("BaseEntity *[4]", &c),
            "[*mut openwa_game::entity::BaseEntity; 4]"
        );
    }

    #[test]
    fn enum_with_rust_path_resolves() {
        let c = with_enum_rust_path("Weapon", "openwa_core::weapon::Weapon");
        assert_eq!(render("Weapon", &c), "openwa_core::weapon::Weapon");
    }

    #[test]
    fn typedef_rust_path_overrides_target_chase() {
        // `Fixed` is a `Typedef` with target = "int" but a Rust newtype as
        // its true representation — rust_path must win over chasing into i32.
        let c = with_typedef_rust_path("Fixed", "int", "openwa_core::fixed::Fixed");
        assert_eq!(render("Fixed", &c), "openwa_core::fixed::Fixed");
        assert_eq!(render("Fixed *", &c), "*mut openwa_core::fixed::Fixed");
    }
}
