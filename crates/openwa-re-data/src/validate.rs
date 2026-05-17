//! Pure-parse schema checks. No Ghidra, no XML, no network.

use crate::model::*;
use crate::toml_io::Catalog;
use anyhow::{Result, bail};
use std::collections::HashSet;

/// Result of a validation run.
///
/// Errors are structural problems that block round-trip (bad storage syntax,
/// duplicate field offsets, field offset past struct end, unknown calling
/// convention, etc.). Warnings flag suspect data that may still round-trip
/// fine — currently just type references that aren't defined in `re/`,
/// which is expected for Win32/MFC system types Ghidra resolves from its
/// built-in archives.
#[derive(Debug, Default)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate(cat: &Catalog) -> Result<ValidationReport> {
    let mut report = ValidationReport::default();
    let known_types = build_known_type_set(cat);

    for entry in cat.functions.values() {
        validate_function(&entry.value, &known_types, &mut report);
    }
    for entry in cat.globals.values() {
        if let Some(ty) = &entry.value.ty {
            validate_type_ref(ty, &known_types, &mut report, || {
                format!("global at 0x{:08X}", entry.value.va)
            });
        }
    }
    for entry in cat.structs.values() {
        validate_struct(&entry.value, &known_types, &mut report);
    }
    for entry in cat.unions.values() {
        validate_union(&entry.value, &known_types, &mut report);
    }
    for entry in cat.typedefs.values() {
        validate_type_ref(&entry.value.target, &known_types, &mut report, || {
            format!("typedef `{}`", entry.value.name)
        });
    }
    for entry in cat.function_defs.values() {
        validate_function_def(&entry.value, &known_types, &mut report);
    }

    Ok(report)
}

fn validate_function(f: &Function, known: &HashSet<String>, report: &mut ValidationReport) {
    let label = || format!("function `{}` at 0x{:08X}", f.name, f.va);

    // Calling convention check.
    if let Some(cc) = f.calling_convention.as_deref()
        && !is_known_convention(cc)
    {
        report
            .errors
            .push(format!("{}: unknown calling_convention `{cc}`", label()));
    }

    // Any function with explicit params MUST declare calling_convention.
    // Without it `OpenWAImport.java` applies the program default (`__cdecl`
    // on this program), which silently scrambles storage for everything
    // that isn't actually cdecl — the failure surfaces as a wrong-looking
    // diff after a round trip, not at validate time.
    if f.calling_convention.is_none() && !f.param.is_empty() {
        report.errors.push(format!(
            "{}: has {} param(s) but no calling_convention. \
             Set one explicitly (`__stdcall` / `__cdecl` / `__thiscall` / \
             `__fastcall` / `__usercall`) — without it, OpenWAImport.java \
             falls back to the program default and scrambles storage.",
            label(),
            f.param.len(),
        ));
    }

    // Custom-storage discipline: __usercall must declare every param's
    // storage. For other conventions, partial storage is allowed (e.g.
    // a __thiscall constructor declares `this=ECX` and leaves the rest
    // to default ABI rules).
    let total = f.param.len();
    let with_storage = f.param.iter().filter(|p| p.storage.is_some()).count();
    let is_custom = f.calling_convention.as_deref() == Some("__usercall");
    if is_custom && total > 0 && with_storage != total {
        report.errors.push(format!(
            "{}: __usercall requires explicit storage on every param ({}/{} declared)",
            label(),
            with_storage,
            total
        ));
    }

    // Storage syntax check.
    for p in &f.param {
        if let Some(s) = &p.storage
            && !is_valid_storage(s)
        {
            report.errors.push(format!(
                "{}: param `{}` has invalid storage `{s}`",
                label(),
                p.name
            ));
        }
        validate_type_ref(&p.ty, known, report, || {
            format!("{}: param `{}`", label(), p.name)
        });
    }

    if let Some(sig) = &f.signature {
        validate_type_ref(&sig.returns, known, report, || {
            format!("{}: return type", label())
        });
        if let Some(s) = &sig.return_storage
            && !is_valid_storage(s)
        {
            report
                .errors
                .push(format!("{}: invalid return_storage `{s}`", label()));
        }
    }

    for c in &f.comment {
        if c.text.is_empty() {
            report.errors.push(format!(
                "{}: empty comment at 0x{:08X} ({:?})",
                label(),
                c.va,
                c.kind
            ));
        }
    }
}

fn validate_struct(s: &Struct, known: &HashSet<String>, report: &mut ValidationReport) {
    let mut seen_offsets = HashSet::new();
    for fld in &s.field {
        if fld.offset > s.size {
            // A field starting past `size` is broken; a field starting AT
            // `size` is a flex-array tail (RTTI `TypeDescriptor.name`, etc.)
            // and tolerated.
            report.errors.push(format!(
                "struct `{}`: field `{}` offset 0x{:X} > size 0x{:X}",
                s.name,
                fld.name.as_deref().unwrap_or("<unnamed>"),
                fld.offset,
                s.size
            ));
        }
        if !seen_offsets.insert(fld.offset) {
            report.errors.push(format!(
                "struct `{}`: duplicate field offset 0x{:X}",
                s.name, fld.offset
            ));
        }
        validate_type_ref(&fld.ty, known, report, || {
            format!(
                "struct `{}` field `{}`",
                s.name,
                fld.name.as_deref().unwrap_or("<unnamed>")
            )
        });
    }
}

fn validate_union(u: &Union, known: &HashSet<String>, report: &mut ValidationReport) {
    for fld in &u.field {
        if fld.offset != 0 {
            report.errors.push(format!(
                "union `{}`: field `{}` offset must be 0 (got 0x{:X})",
                u.name,
                fld.name.as_deref().unwrap_or("<unnamed>"),
                fld.offset
            ));
        }
        validate_type_ref(&fld.ty, known, report, || {
            format!(
                "union `{}` field `{}`",
                u.name,
                fld.name.as_deref().unwrap_or("<unnamed>")
            )
        });
    }
}

fn validate_function_def(fd: &FunctionDef, known: &HashSet<String>, report: &mut ValidationReport) {
    validate_type_ref(&fd.returns, known, report, || {
        format!("function_def `{}`: return type", fd.name)
    });
    for p in &fd.param {
        validate_type_ref(&p.ty, known, report, || {
            format!("function_def `{}`: param `{}`", fd.name, p.name)
        });
    }
}

fn validate_type_ref(
    tref: &TypeRef,
    known: &HashSet<String>,
    report: &mut ValidationReport,
    ctx: impl FnOnce() -> String,
) {
    if tref.trim().is_empty() {
        report
            .errors
            .push(format!("{}: empty type reference", ctx()));
        return;
    }
    // Strip pointer/array sugar to get the base name.
    let base = base_type_name(tref);
    if base.is_empty() {
        report
            .errors
            .push(format!("{}: cannot extract base name from `{tref}`", ctx()));
        return;
    }
    if !is_builtin_type(&base) && !known.contains(&base) {
        report
            .warnings
            .push(format!("{}: unknown type `{tref}` (base `{base}`)", ctx()));
    }
}

fn build_known_type_set(cat: &Catalog) -> HashSet<String> {
    let mut s = HashSet::new();
    for k in cat.structs.keys() {
        s.insert(k.clone());
    }
    for k in cat.unions.keys() {
        s.insert(k.clone());
    }
    for k in cat.enums.keys() {
        s.insert(k.clone());
    }
    for k in cat.typedefs.keys() {
        s.insert(k.clone());
    }
    for k in cat.function_defs.keys() {
        s.insert(k.clone());
    }
    s.extend(cat.external_types.iter().cloned());
    s
}

/// Strip trailing pointer/array/bit-field suffixes to expose the base type
/// name. `"char *[7]"` → `"char"`, `"BaseEntity *"` → `"BaseEntity"`,
/// `"dword:31"` → `"dword"` (Ghidra bit-field syntax).
fn base_type_name(tref: &str) -> String {
    // Drop everything from the first `*`, `[`, or `:` onwards; trim.
    let cutoff = tref.find(['*', '[', ':']).unwrap_or(tref.len());
    tref[..cutoff].trim().to_string()
}

fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "void"
            | "bool"
            | "char"
            | "uchar"
            | "schar"
            | "wchar_t"
            | "wchar16"
            | "wchar32"
            | "byte"
            | "sbyte"
            | "word"
            | "sword"
            | "dword"
            | "sdword"
            | "qword"
            | "sqword"
            | "short"
            | "ushort"
            | "int"
            | "uint"
            | "long"
            | "ulong"
            | "longlong"
            | "ulonglong"
            | "float"
            | "double"
            | "longdouble"
            | "size_t"
            | "ssize_t"
            | "int8"
            | "uint8"
            | "int16"
            | "uint16"
            | "int32"
            | "uint32"
            | "int64"
            | "uint64"
            | "pointer"
            | "pointer8"
            | "pointer16"
            | "pointer32"
            | "pointer64"
            | "string"
            | "unicode"
            | "Alignment"
            | "TerminatedCString"
            | "TerminatedUnicode"
            | "undefined"
            | "undefined1"
            | "undefined2"
            | "undefined4"
            | "undefined8"
    )
}

fn is_known_convention(cc: &str) -> bool {
    matches!(
        cc,
        "__stdcall" | "__cdecl" | "__thiscall" | "__fastcall" | "__usercall"
    )
}

/// Storage grammar:
///   register     := identifier matching `[A-Z][A-Z0-9]*`
///   split        := register ":" register
///   stack_simple := "stack:0x" hex
///   stack_sized  := "stack:0x" hex ":" decimal
fn is_valid_storage(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix("stack:") {
        let mut parts = rest.split(':');
        let off = parts.next().unwrap_or("");
        let size = parts.next();
        if parts.next().is_some() {
            return false;
        }
        if !parse_hex_or_dec(off) {
            return false;
        }
        if let Some(sz) = size {
            return sz.chars().all(|c| c.is_ascii_digit());
        }
        return true;
    }
    let mut regs = s.split(':');
    let first = regs.next().unwrap_or("");
    if !is_register_name(first) {
        return false;
    }
    if let Some(second) = regs.next()
        && !is_register_name(second)
    {
        return false;
    }
    regs.next().is_none()
}

fn is_register_name(r: &str) -> bool {
    !r.is_empty()
        && r.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn parse_hex_or_dec(s: &str) -> bool {
    if let Some(rest) = s.strip_prefix("0x") {
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    }
}

/// Convenience: run validation, bail on errors with a multi-line message.
pub fn validate_or_bail(cat: &Catalog) -> Result<()> {
    let report = validate(cat)?;
    if !report.ok() {
        bail!(
            "{} validation error(s):\n  - {}",
            report.errors.len(),
            report.errors.join("\n  - ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_grammar() {
        assert!(is_valid_storage("ECX"));
        assert!(is_valid_storage("EDX:EAX"));
        assert!(is_valid_storage("stack:0x4"));
        assert!(is_valid_storage("stack:0x10:4"));
        assert!(!is_valid_storage(""));
        assert!(!is_valid_storage("eax"));
        assert!(!is_valid_storage("stack:abc"));
        assert!(!is_valid_storage("stack:0x4:0x4"));
    }

    use crate::toml_io::{Catalog, OwnedEntry};
    use std::path::PathBuf;

    fn make_fn(va: Va, name: &str) -> Function {
        Function {
            va,
            name: name.into(),
            calling_convention: None,
            plate_comment: None,
            no_return: false,
            custom_storage: false,
            signature: None,
            param: vec![],
            local: vec![],
            comment: vec![],
        }
    }

    fn cat_with(f: Function) -> Catalog {
        let mut c = Catalog::default();
        c.functions.insert(
            f.va,
            OwnedEntry {
                value: f,
                source: PathBuf::from("re/test.toml"),
            },
        );
        c
    }

    #[test]
    fn missing_calling_convention_with_params_is_error() {
        let mut f = make_fn(0x500, "f");
        f.param.push(Param {
            name: "x".into(),
            ty: "int".into(),
            storage: None,
        });
        let report = validate(&cat_with(f)).unwrap();
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("no calling_convention"));
    }

    #[test]
    fn missing_calling_convention_without_params_is_fine() {
        let f = make_fn(0x500, "f");
        let report = validate(&cat_with(f)).unwrap();
        assert!(report.ok());
    }

    #[test]
    fn explicit_calling_convention_with_params_validates() {
        let mut f = make_fn(0x500, "f");
        f.calling_convention = Some("__stdcall".into());
        f.param.push(Param {
            name: "x".into(),
            ty: "int".into(),
            storage: None,
        });
        let report = validate(&cat_with(f)).unwrap();
        assert!(report.ok(), "errors: {:?}", report.errors);
    }

    #[test]
    fn base_type_extraction() {
        assert_eq!(base_type_name("int"), "int");
        assert_eq!(base_type_name("char *"), "char");
        assert_eq!(base_type_name("char *[7]"), "char");
        assert_eq!(base_type_name("BaseEntity *"), "BaseEntity");
        assert_eq!(base_type_name("BaseEntity * *"), "BaseEntity");
        assert_eq!(base_type_name("ULONG_PTR[15]"), "ULONG_PTR");
    }
}
