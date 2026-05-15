//! On-disk schema for `re/**/*.toml`.
//!
//! Each TOML file in `re/` deserializes into [`ReFile`]; the import path merges
//! every file into a flat in-memory model keyed by VA (functions, globals,
//! labels, defined data) or name (types). File placement is purely organizational.
//!
//! Conventions:
//!   - All addresses are u32 (i686 target). TOML accepts them as integers (hex `0x...` or decimal).
//!   - All `va = ...` fields are absolute Ghidra VAs.
//!   - Type identity is `(name, namespace)`; for user types we default `namespace = "/"`
//!     and omit it in TOML. Built-in / system types are referenced by name string and
//!     never round-trip through `re/`.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Top-level structure of a single `re/*.toml` file.
///
/// Every section is optional; a file can declare any mix. Vec ordering is
/// preserved at parse time and re-emitted in normalised (VA-sorted / name-sorted)
/// order on round-trip.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ReFile {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function: Vec<Function>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub global: Vec<Global>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub label: Vec<Label>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub r#struct: Vec<Struct>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub union: Vec<Union>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub r#enum: Vec<Enum>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub typedef: Vec<Typedef>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_def: Vec<FunctionDef>,

    /// Type names defined in Ghidra's built-in archives (Win32 / MFC / CRT
    /// headers, the PE/DOS loader, etc.) that user TOML may legitimately
    /// reference. Treated as known type names by [`crate::validate`] but
    /// never round-tripped through the import path — Ghidra re-supplies them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_types: Vec<String>,
}

// ─── Functions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Function {
    pub va: Va,
    pub name: String,

    /// `__stdcall` | `__cdecl` | `__thiscall` | `__fastcall` | `__usercall`
    ///
    /// Tracked here because Ghidra's XML DTD has no calling-convention attribute
    /// — the importer infers from storage. We emit it via the extras sidecar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calling_convention: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_comment: Option<String>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub no_return: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param: Vec<Param>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local: Vec<Local>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comment: Vec<InlineComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Signature {
    pub returns: TypeRef,
    /// Override storage for the return value. Omit unless non-default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_storage: Option<Storage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    /// Storage spec. Required iff [`Function::calling_convention`] is `__usercall`.
    /// For default conventions, omit on every param (Ghidra computes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<Storage>,
}

/// Listing-style local (stack variable). Mostly noise; we keep an opt-in slot
/// because the Ghidra XML exporter writes one row per stack slot and round-trip
/// needs them. Filtered to user-named locals on export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Local {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    pub stack_offset: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct InlineComment {
    pub va: Va,
    pub kind: CommentKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentKind {
    /// Block comment shown above the listing line. Same as Ghidra `plate`.
    Plate,
    /// `end-of-line` in Ghidra; appears trailing the instruction.
    Eol,
    Pre,
    Post,
    Repeatable,
    /// Decompiler-only comment (vs. listing-only).
    Decompiler,
}

// ─── Globals / Labels ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Global {
    pub va: Va,
    pub name: String,
    /// Applied data type. Optional — Ghidra leaves many user-named globals
    /// untyped (a name alone is still load-bearing metadata for xrefs).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Label {
    pub va: Va,
    pub name: String,
}

// ─── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Struct {
    pub name: String,
    /// Ghidra DTM namespace; default `/` when omitted.
    #[serde(default, skip_serializing_if = "is_root_namespace")]
    pub namespace: Option<String>,
    pub size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_comment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Union {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_root_namespace")]
    pub namespace: Option<String>,
    pub size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_comment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Field {
    pub offset: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Enum {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_root_namespace")]
    pub namespace: Option<String>,
    /// Underlying width in bytes (1/2/4/8).
    pub size: u32,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub variant: IndexMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Typedef {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_root_namespace")]
    pub namespace: Option<String>,
    pub target: TypeRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct FunctionDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "is_root_namespace")]
    pub namespace: Option<String>,
    pub returns: TypeRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param: Vec<FunctionDefParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct FunctionDefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: TypeRef,
}

// ─── Primitives ──────────────────────────────────────────────────────────────

/// Absolute virtual address. Encoded as `0x00500000` integer in TOML.
pub type Va = u32;

/// A reference to a type. We hold it as the raw Ghidra string verbatim
/// (`int`, `BaseEntity *`, `char *[7]`, `_struct_19`) — pointers and arrays
/// are textual derivations of a base, not first-class elements. Validation
/// is performed by walking the catalog of known names.
pub type TypeRef = String;

/// Storage specification for a parameter, return value, or local.
///
/// Syntax we accept:
///   - `"ECX"`, `"EAX"` — single register
///   - `"EDX:EAX"` — multi-register split (low:high) for 64-bit values
///   - `"stack:0x4"` — single stack slot (size derived from type)
///   - `"stack:0x8:4"` — stack slot with explicit byte size
///
/// These mirror Ghidra's `<REGISTER_VAR REGISTER="…">` and
/// `<STACK_VAR STACK_PTR_OFFSET="…">` element attributes.
pub type Storage = String;

fn is_false(b: &bool) -> bool {
    !*b
}

/// Serde skip-helper: omit `namespace` from TOML when absent or `/`.
fn is_root_namespace(ns: &Option<String>) -> bool {
    match ns {
        None => true,
        Some(s) => s.is_empty() || s == "/",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_minimal_function() {
        let src = r#"
[[function]]
va = 0x004FE070
name = "WorldEntity__TryMovePosition"
calling_convention = "__thiscall"

  [[function.param]]
  name = "this"
  type = "WorldEntity *"
  storage = "ECX"

  [[function.param]]
  name = "dx"
  type = "Fixed"
  storage = "stack:0x4"
"#;
        let parsed: ReFile = toml::from_str(src).unwrap();
        assert_eq!(parsed.function.len(), 1);
        assert_eq!(parsed.function[0].param.len(), 2);
        assert_eq!(parsed.function[0].param[0].storage.as_deref(), Some("ECX"));

        let reserialised = toml::to_string(&parsed).unwrap();
        assert!(reserialised.contains("va = "));
        assert!(reserialised.contains("WorldEntity__TryMovePosition"));
    }

    #[test]
    fn deny_unknown_field() {
        let src = r#"
[[function]]
va = 0x500
name = "x"
made_up_key = 42
"#;
        let r: Result<ReFile, _> = toml::from_str(src);
        assert!(
            r.is_err(),
            "deny_unknown_fields should reject `made_up_key`"
        );
    }
}
