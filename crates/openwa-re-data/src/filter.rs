//! Filtering rules applied when ingesting a Ghidra XML dump.
//!
//! The XML is the union of (user-added metadata + Ghidra auto-analysis +
//! Function ID library matches + PE headers). We keep only the first bucket.
//! Rules are derived empirically from the 43.9 MB bootstrap dump at
//! `c:/tmp/wa_export.xml`; revisit if the dump shape changes upstream.

/// True if a Ghidra DTM namespace string identifies a built-in or auto type
/// archive that we should drop. User types live under `/`.
pub fn is_builtin_dtm_namespace(ns: &str) -> bool {
    // Empty / root → user type. Keep.
    if ns.is_empty() || ns == "/" {
        return false;
    }
    // Anonymous switch-statement enums Ghidra generates for jump tables.
    if ns.starts_with("switchD_") {
        return true;
    }
    // Holdover from an earlier version that wrote stub typedefs here. Filter
    // defensively in case any persist.
    if ns == "/openwa-re" || ns.starts_with("/openwa-re/") {
        return true;
    }
    // PE-loader / debugger / demangler categories.
    if ns == "/PE"
        || ns == "/DOS"
        || ns == "/PDB"
        || ns == "/Demangler"
        || ns.starts_with("/Demangler/")
        || ns.starts_with("/MSDataTypes")
    {
        return true;
    }
    // Header-style namespaces: any segment ends in `.h`. Catches:
    //   `/winnt.h`, `/winnt.h/functions`, `/sys/stat.h`, `/sys/timeb.h`, …
    // User namespaces (`/OpenWA`, `/auto_structs`) have no `.h` segment.
    for seg in ns.split('/') {
        if seg.ends_with(".h") {
            return true;
        }
    }
    false
}

/// True if a function name is one Ghidra auto-generated (a `FUN_xxxxxxxx`
/// or `thunk_FUN_xxxxxxxx` placeholder) and contains nothing worth
/// round-tripping. Marking either as `SOURCE_TYPE="USER_DEFINED"` on import
/// makes Ghidra throw `IllegalArgumentException: Can't change between DEFAULT
/// and non-default symbol`.
pub fn is_auto_function_name(name: &str) -> bool {
    for prefix in ["FUN_", "thunk_FUN_"] {
        if let Some(suffix) = name.strip_prefix(prefix)
            && suffix.len() == 8
            && suffix.chars().all(|c| c.is_ascii_hexdigit())
        {
            return true;
        }
    }
    false
}

/// True if a function name carries a `::`-qualified namespace prefix that
/// indicates a Ghidra auto-import: PE library imports (`WSOCK32.DLL::*`,
/// `WINSPOOL.DRV::*`, `KERNEL32.DLL::*`) and MFC/CRT class methods
/// recognised by the Function ID Analyzer + Demangler combo (`CWnd::*`,
/// `CWinApp::*`, `CFixedAllocNoSync::*`). Round-tripping these via
/// `<SYMBOL SOURCE_TYPE="USER_DEFINED">` fails in two distinct ways:
///   - PE IAT addresses: `getPrimarySymbol(addr)` returns null →
///     SymbolUtilities.create… returns null → NPE at
///     `SymbolTableXmlMgr.java:270` (`s.setPrimary()`).
///   - MFC class thunks: existing DEFAULT symbol can't be promoted to
///     USER_DEFINED → `IllegalArgumentException` at
///     `SymbolTableXmlMgr.java:272` (`s.setSource(...)`).
///
/// In both cases Ghidra recreates these during analysis anyway, so
/// dropping them on parse is lossless. The user's intentional class-scoped
/// renames live in the Rust crates without a `::` separator — we use the
/// `Foo__bar` flavour throughout the codebase (CLAUDE.md / naming memory).
pub fn is_qualified_import_name(name: &str) -> bool {
    name.contains("::")
}

/// True if a symbol name is a Ghidra-default `LAB_xxxxxxxx` / `DAT_xxxxxxxx` /
/// `SUB_xxxxxxxx` placeholder.
pub fn is_auto_symbol_name(name: &str) -> bool {
    for prefix in ["LAB_", "DAT_", "SUB_", "UNK_", "PTR_"] {
        if let Some(suffix) = name.strip_prefix(prefix)
            && suffix.len() >= 8
            && suffix[..8].chars().all(|c| c.is_ascii_hexdigit())
        {
            return true;
        }
    }
    false
}

/// True if a type name is one of Ghidra's RTTI / EH auto-analyser
/// internal struct names. These all live under `/` so the namespace
/// filter alone doesn't catch them; their field types resolve to types
/// that look identical in Ghidra's DTM but match neither the namespaced
/// nor the root-namespaced flavour exactly, so re-importing produces
/// `.conflict` copies on every run. Ghidra always recreates these during
/// auto-analysis, so dropping them is lossless.
pub fn is_rtti_or_eh_internal_type(name: &str) -> bool {
    // EH: _s_FuncInfo, _s_HandlerType, _s_TryBlockMapEntry, _s_UnwindMapEntry.
    // RTTI: _s__RTTIBaseClassDescriptor, _s__RTTIClassHierarchyDescriptor,
    // _s__RTTICompleteObjectLocator, _s__RTTIClassArray, etc.
    name.starts_with("_s_")
}

/// True if a data type name is a Ghidra-fabricated anonymous structure /
/// union / enum (`_struct_19`, `_union_2685`, `enum_3272`, etc.).
pub fn is_anonymous_type_name(name: &str) -> bool {
    let trimmed = name.trim_start_matches('_');
    for prefix in ["struct_", "union_", "enum_", "func_"] {
        if let Some(suffix) = trimmed.strip_prefix(prefix)
            && !suffix.is_empty()
            && suffix.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// True if a DTM type name is a Ghidra-builtin primitive (and so should never
/// appear in user TOML — we reference it by string in `type` fields directly).
pub fn is_primitive_type_name(name: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtm_namespace_filtering() {
        // Root and user namespaces are kept.
        assert!(!is_builtin_dtm_namespace(""));
        assert!(!is_builtin_dtm_namespace("/"));
        assert!(!is_builtin_dtm_namespace("/OpenWA"));
        assert!(!is_builtin_dtm_namespace("/auto_structs"));
        // Header-style namespaces are dropped.
        assert!(is_builtin_dtm_namespace("/winnt.h"));
        assert!(is_builtin_dtm_namespace("/winbase.h/functions"));
        assert!(is_builtin_dtm_namespace("/vadefs.h"));
        assert!(is_builtin_dtm_namespace("/mbstring.h"));
        assert!(is_builtin_dtm_namespace("/sys/stat.h"));
        // Well-known system categories.
        assert!(is_builtin_dtm_namespace("/Demangler"));
        assert!(is_builtin_dtm_namespace("/Demangler/std"));
        assert!(is_builtin_dtm_namespace("/PE"));
        // Anonymous switch enums.
        assert!(is_builtin_dtm_namespace("switchD_005faf84::"));
    }

    #[test]
    fn auto_names() {
        assert!(is_auto_function_name("FUN_00401000"));
        assert!(is_auto_function_name("thunk_FUN_005bacf0"));
        assert!(!is_auto_function_name("FUN_0040"));
        assert!(!is_auto_function_name("FUN_xxxxxxxx"));
        assert!(!is_auto_function_name("thunk_user_named_fn"));
        assert!(!is_auto_function_name("WormEntity__OnContact"));

        assert!(is_auto_symbol_name("LAB_00501234"));
        assert!(is_auto_symbol_name("DAT_00501234"));
        assert!(!is_auto_symbol_name("LAB_short"));
        assert!(!is_auto_symbol_name("g_my_global"));
    }

    #[test]
    fn anonymous_types() {
        assert!(is_anonymous_type_name("_struct_19"));
        assert!(is_anonymous_type_name("_union_2685"));
        assert!(is_anonymous_type_name("enum_3272"));
        assert!(!is_anonymous_type_name("BaseEntity"));
        assert!(!is_anonymous_type_name("_AFX_BASE_MODULE_STATE"));
    }
}
