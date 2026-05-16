//! Hand-rolled TOML writer for [`ReFile`] / [`XmlProgram`].
//!
//! We can't use `toml::to_string` directly because the default integer
//! serializer emits decimal for every `va` / `offset` / `size` field, which
//! makes the files unreadable. The schema is small, so emitting TOML
//! ourselves is faster than wrestling serde for hex output.
//!
//! Format conventions (matching the human-eye reading of Ghidra output):
//!   - `va`, `offset`, `size` (struct/union/enum), `value` (enum)            → hex (`0x004FE070`)
//!   - `stack_offset` (signed)                                               → decimal
//!   - Everything else                                                       → default TOML quoting

use crate::model::*;
use crate::xml_in::XmlProgram;
use anyhow::Result;
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

/// Render a single [`ReFile`] to a TOML string.
pub fn write_re_file(file: &ReFile) -> String {
    let mut out = String::new();
    let mut first_section = true;
    macro_rules! section_gap {
        () => {
            if !first_section {
                out.push('\n');
            }
            first_section = false;
        };
    }

    // Top-level keys (anything not in a `[[...]]` table) MUST come before any
    // array-of-tables in TOML — emit `external_types` first.
    if !file.external_types.is_empty() {
        section_gap!();
        write_external_types(&mut out, &file.external_types);
    }

    for f in &file.function {
        section_gap!();
        write_function(&mut out, f);
    }
    for g in &file.global {
        section_gap!();
        write_global(&mut out, g);
    }
    for l in &file.label {
        section_gap!();
        write_label(&mut out, l);
    }
    for s in &file.r#struct {
        section_gap!();
        write_struct(&mut out, s);
    }
    for u in &file.union {
        section_gap!();
        write_union(&mut out, u);
    }
    for e in &file.r#enum {
        section_gap!();
        write_enum(&mut out, e);
    }
    for t in &file.typedef {
        section_gap!();
        write_typedef(&mut out, t);
    }
    for fd in &file.function_def {
        section_gap!();
        write_function_def(&mut out, fd);
    }

    out
}

/// Render the bulk `external_types = [...]` array. One name per line for
/// diff hygiene; alphabetical order is set by the caller.
fn write_external_types(out: &mut String, names: &[String]) {
    out.push_str("# Types defined in Ghidra's built-in archives (Win32, MFC, CRT,\n");
    out.push_str("# PE/DOS loader, etc.). Listed so the validator recognises them\n");
    out.push_str("# as legitimate type names; their full definitions live in Ghidra,\n");
    out.push_str("# not in `re/`, and they are not round-tripped on import.\n");
    out.push_str("external_types = [\n");
    for n in names {
        writeln!(out, "  {},", toml_quote(n)).unwrap();
    }
    out.push_str("]\n");
}

// ─── Bootstrap export (XmlProgram → sharded re/) ─────────────────────────────

/// One TOML file to be written. The actual write is a separate step so a
/// dry-run mode can introspect the layout without touching disk.
pub struct PendingFile {
    pub path: PathBuf,
    pub contents: String,
    pub kind: &'static str,
    pub entries: usize,
}

/// Shard an [`XmlProgram`] across multiple TOML files for a fresh bootstrap dump.
///
/// Routing rules:
///   - Items whose name follows the `Foo__bar` convention (one or more `__`
///     separators) are grouped by class: into `re/Foo.toml`. The class file
///     gathers the class's functions + globals + labels in one place.
///   - Items without `__` fall back to a VA shard (`_bootstrap_<va>.toml`)
///     for functions, or `globals.toml` / `labels.toml`.
///   - Types (struct/union/enum/typedef/fn-def) always land in `types.toml`.
///
/// `Foo` must be a sane file-name (ASCII alnum + `_`); anything weirder
/// falls back to the unclassified buckets.
pub fn bootstrap_files(prog: &XmlProgram, re_dir: &Path) -> Vec<PendingFile> {
    // Per-class buckets: class name → accumulated entries.
    let mut by_class: std::collections::BTreeMap<String, ReFile> =
        std::collections::BTreeMap::new();

    // Unclassified functions get sharded by VA.
    let mut by_shard: std::collections::BTreeMap<u32, Vec<Function>> =
        std::collections::BTreeMap::new();
    let mut unclassified_globals: Vec<Global> = Vec::new();
    let mut unclassified_labels: Vec<Label> = Vec::new();

    for f in &prog.functions {
        match class_from_name(&f.name) {
            Some(c) => by_class.entry(c).or_default().function.push(f.clone()),
            None => {
                by_shard
                    .entry(f.va >> SHARD_BITS)
                    .or_default()
                    .push(f.clone());
            }
        }
    }
    for g in &prog.globals {
        match class_from_name(&g.name) {
            Some(c) => by_class.entry(c).or_default().global.push(g.clone()),
            None => unclassified_globals.push(g.clone()),
        }
    }
    for l in &prog.labels {
        match class_from_name(&l.name) {
            Some(c) => by_class.entry(c).or_default().label.push(l.clone()),
            None => unclassified_labels.push(l.clone()),
        }
    }

    let mut out = Vec::new();

    // Per-class files first (sorted alphabetically by BTreeMap iteration).
    for (class, rf) in by_class {
        let entries = rf.function.len() + rf.global.len() + rf.label.len();
        out.push(PendingFile {
            path: re_dir.join(format!("{class}.toml")),
            contents: write_re_file(&rf),
            kind: "class",
            entries,
        });
    }

    // VA-sharded functions (unclassified).
    for (shard_key, funcs) in by_shard {
        let shard_va = shard_key << SHARD_BITS;
        let path = re_dir.join(format!("_bootstrap_{shard_va:08x}.toml"));
        let entries = funcs.len();
        let rf = ReFile {
            function: funcs,
            ..Default::default()
        };
        out.push(PendingFile {
            path,
            contents: write_re_file(&rf),
            kind: "functions",
            entries,
        });
    }

    // Normalise external types: dedupe + sort. Drop names that we kept as
    // user types in this dump (no point listing them twice).
    let user_names: std::collections::HashSet<&str> = prog
        .structs
        .iter()
        .map(|s| s.name.as_str())
        .chain(prog.unions.iter().map(|u| u.name.as_str()))
        .chain(prog.enums.iter().map(|e| e.name.as_str()))
        .chain(prog.typedefs.iter().map(|t| t.name.as_str()))
        .chain(prog.function_defs.iter().map(|fd| fd.name.as_str()))
        .collect();
    let mut external_types: Vec<String> = prog
        .external_types
        .iter()
        .filter(|n| !user_names.contains(n.as_str()))
        .cloned()
        .collect();
    external_types.sort();
    external_types.dedup();

    let has_user_types = !prog.structs.is_empty()
        || !prog.unions.is_empty()
        || !prog.enums.is_empty()
        || !prog.typedefs.is_empty()
        || !prog.function_defs.is_empty();
    if has_user_types || !external_types.is_empty() {
        let entries = prog.structs.len()
            + prog.unions.len()
            + prog.enums.len()
            + prog.typedefs.len()
            + prog.function_defs.len()
            + external_types.len();
        let rf = ReFile {
            r#struct: prog.structs.clone(),
            union: prog.unions.clone(),
            r#enum: prog.enums.clone(),
            typedef: prog.typedefs.clone(),
            function_def: prog.function_defs.clone(),
            external_types,
            ..Default::default()
        };
        out.push(PendingFile {
            path: re_dir.join("types.toml"),
            contents: write_re_file(&rf),
            kind: "types",
            entries,
        });
    }

    if !unclassified_globals.is_empty() {
        let entries = unclassified_globals.len();
        let rf = ReFile {
            global: unclassified_globals,
            ..Default::default()
        };
        out.push(PendingFile {
            path: re_dir.join("globals.toml"),
            contents: write_re_file(&rf),
            kind: "globals",
            entries,
        });
    }

    if !unclassified_labels.is_empty() {
        let entries = unclassified_labels.len();
        let rf = ReFile {
            label: unclassified_labels,
            ..Default::default()
        };
        out.push(PendingFile {
            path: re_dir.join("labels.toml"),
            contents: write_re_file(&rf),
            kind: "labels",
            entries,
        });
    }

    out
}

/// 1 << 16 = 64 KB. With WA's .text being ~2 MB, this gives ~32 shards.
const SHARD_BITS: u32 = 16;

/// Pull the leading `Class` prefix out of `Class__rest` if `Class` is a
/// safe file-name stem. Returns `None` for names without `__` or with a
/// non-alphanumeric prefix.
pub(crate) fn class_from_name(name: &str) -> Option<String> {
    let idx = name.find("__")?;
    if idx == 0 {
        return None; // leading `__` → not a class prefix
    }
    let prefix = &name[..idx];
    if prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(prefix.to_string())
    } else {
        None
    }
}

// ─── Per-section writers ─────────────────────────────────────────────────────

fn write_function(out: &mut String, f: &Function) {
    writeln!(out, "[[function]]").unwrap();
    writeln!(out, "va = 0x{:08X}", f.va).unwrap();
    write_string_kv(out, "name", &f.name);
    if let Some(cc) = &f.calling_convention {
        write_string_kv(out, "calling_convention", cc);
    }
    if f.no_return {
        writeln!(out, "no_return = true").unwrap();
    }
    if let Some(plate) = &f.plate_comment {
        write_string_kv(out, "plate_comment", plate);
    }

    if let Some(sig) = &f.signature {
        writeln!(out, "\n  [function.signature]").unwrap();
        write_string_kv_indented(out, "  ", "returns", &sig.returns);
        if let Some(rs) = &sig.return_storage {
            write_string_kv_indented(out, "  ", "return_storage", rs);
        }
    }

    for p in &f.param {
        writeln!(out, "\n  [[function.param]]").unwrap();
        write_string_kv_indented(out, "  ", "name", &p.name);
        write_string_kv_indented(out, "  ", "type", &p.ty);
        if let Some(s) = &p.storage {
            write_string_kv_indented(out, "  ", "storage", s);
        }
    }

    for l in &f.local {
        writeln!(out, "\n  [[function.local]]").unwrap();
        write_string_kv_indented(out, "  ", "name", &l.name);
        write_string_kv_indented(out, "  ", "type", &l.ty);
        writeln!(out, "  stack_offset = {}", l.stack_offset).unwrap();
    }

    for c in &f.comment {
        writeln!(out, "\n  [[function.comment]]").unwrap();
        writeln!(out, "  va = 0x{:08X}", c.va).unwrap();
        write_string_kv_indented(out, "  ", "kind", comment_kind_str(c.kind));
        write_string_kv_indented(out, "  ", "text", &c.text);
    }
}

fn write_global(out: &mut String, g: &Global) {
    writeln!(out, "[[global]]").unwrap();
    writeln!(out, "va = 0x{:08X}", g.va).unwrap();
    write_string_kv(out, "name", &g.name);
    if let Some(ty) = &g.ty {
        write_string_kv(out, "type", ty);
    }
    if let Some(c) = &g.comment {
        write_string_kv(out, "comment", c);
    }
}

fn write_label(out: &mut String, l: &Label) {
    writeln!(out, "[[label]]").unwrap();
    writeln!(out, "va = 0x{:08X}", l.va).unwrap();
    write_string_kv(out, "name", &l.name);
}

fn write_struct(out: &mut String, s: &Struct) {
    writeln!(out, "[[struct]]").unwrap();
    write_string_kv(out, "name", &s.name);
    write_namespace_kv(out, &s.namespace);
    writeln!(out, "size = 0x{:X}", s.size).unwrap();
    if let Some(p) = &s.plate_comment {
        write_string_kv(out, "plate_comment", p);
    }
    for fld in &s.field {
        write_struct_field(out, fld);
    }
}

fn write_union(out: &mut String, u: &Union) {
    writeln!(out, "[[union]]").unwrap();
    write_string_kv(out, "name", &u.name);
    write_namespace_kv(out, &u.namespace);
    writeln!(out, "size = 0x{:X}", u.size).unwrap();
    if let Some(p) = &u.plate_comment {
        write_string_kv(out, "plate_comment", p);
    }
    for fld in &u.field {
        write_union_field(out, fld);
    }
}

fn write_struct_field(out: &mut String, fld: &Field) {
    write_field(out, fld, "struct");
}

fn write_union_field(out: &mut String, fld: &Field) {
    write_field(out, fld, "union");
}

/// Emit a single `[[<parent>.field]]` block. `type_namespace` and `size`
/// are load-bearing: Ghidra's `DataTypesXmlMgr` keys struct equality on
/// `(NAME, NAMESPACE, SIZE) + (per-member NAME, OFFSET, DATATYPE,
/// DATATYPE_NAMESPACE, SIZE)`. Dropping member SIZE makes Ghidra create a
/// `.conflict` copy on every import even when the type is byte-identical.
fn write_field(out: &mut String, fld: &Field, parent: &str) {
    writeln!(out, "\n  [[{parent}.field]]").unwrap();
    writeln!(out, "  offset = 0x{:X}", fld.offset).unwrap();
    write_string_kv_indented(out, "  ", "name", &fld.name);
    write_string_kv_indented(out, "  ", "type", &fld.ty);
    if let Some(ns) = fld
        .type_namespace
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "/")
    {
        write_string_kv_indented(out, "  ", "type_namespace", ns);
    }
    if let Some(sz) = fld.size {
        writeln!(out, "  size = 0x{:X}", sz).unwrap();
    }
    if let Some(c) = &fld.comment {
        write_string_kv_indented(out, "  ", "comment", c);
    }
}

fn write_enum(out: &mut String, e: &Enum) {
    writeln!(out, "[[enum]]").unwrap();
    write_string_kv(out, "name", &e.name);
    write_namespace_kv(out, &e.namespace);
    writeln!(out, "size = {}", e.size).unwrap();
    if !e.variant.is_empty() {
        writeln!(out, "\n  [enum.variant]").unwrap();
        for (name, value) in &e.variant {
            writeln!(out, "  {} = 0x{:X}", toml_bare_or_quoted_key(name), value).unwrap();
        }
    }
}

fn write_typedef(out: &mut String, t: &Typedef) {
    writeln!(out, "[[typedef]]").unwrap();
    write_string_kv(out, "name", &t.name);
    write_namespace_kv(out, &t.namespace);
    write_string_kv(out, "target", &t.target);
}

fn write_function_def(out: &mut String, fd: &FunctionDef) {
    writeln!(out, "[[function_def]]").unwrap();
    write_string_kv(out, "name", &fd.name);
    write_namespace_kv(out, &fd.namespace);
    write_string_kv(out, "returns", &fd.returns);
    for p in &fd.param {
        writeln!(out, "\n  [[function_def.param]]").unwrap();
        write_string_kv_indented(out, "  ", "name", &p.name);
        write_string_kv_indented(out, "  ", "type", &p.ty);
    }
}

fn write_namespace_kv(out: &mut String, ns: &Option<String>) {
    if let Some(s) = ns.as_deref().filter(|s| !s.is_empty() && *s != "/") {
        write_string_kv(out, "namespace", s);
    }
}

// ─── Primitive writers ───────────────────────────────────────────────────────

fn write_string_kv(out: &mut String, key: &str, value: &str) {
    if value.contains('\n') || value.contains('\r') {
        write_multiline_kv(out, key, value);
    } else {
        writeln!(out, "{key} = {}", toml_quote(value)).unwrap();
    }
}

fn write_string_kv_indented(out: &mut String, indent: &str, key: &str, value: &str) {
    if value.contains('\n') || value.contains('\r') {
        write_multiline_kv_indented(out, indent, key, value);
    } else {
        writeln!(out, "{indent}{key} = {}", toml_quote(value)).unwrap();
    }
}

fn write_multiline_kv(out: &mut String, key: &str, value: &str) {
    // Use TOML's basic multi-line string `"""..."""`, escaping triple-quotes
    // and backslashes. Trailing whitespace inside the value is preserved.
    let escaped = toml_basic_multiline_escape(value);
    writeln!(out, "{key} = \"\"\"\n{escaped}\"\"\"").unwrap();
}

fn write_multiline_kv_indented(out: &mut String, indent: &str, key: &str, value: &str) {
    let escaped = toml_basic_multiline_escape(value);
    writeln!(out, "{indent}{key} = \"\"\"\n{escaped}\"\"\"").unwrap();
}

/// TOML basic single-line string quoting. Handles the typical escapes; the
/// model never produces NUL bytes or DEL so we don't need the full table.
fn toml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => write!(out, "\\u{:04X}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Basic multi-line strings only need `"""` and `\` escapes; bare newlines are
/// fine. We also escape control chars to keep diffs sane.
fn toml_basic_multiline_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => {
                // A triple `"""` would terminate the string. Escape a `"` that
                // is the start of a `"""` run; standalone quotes pass through.
                let next1 = chars.peek().copied();
                if next1 == Some('"') {
                    let mut clone = chars.clone();
                    clone.next();
                    if clone.peek() == Some(&'"') {
                        out.push_str("\\\"");
                        continue;
                    }
                }
                out.push('"');
            }
            '\r' => {} // CRs are normalised away; we re-emit LF only.
            c if c == '\n' || c == '\t' => out.push(c),
            c if c.is_control() => write!(out, "\\u{:04X}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// True if `s` is a legal bare TOML key (`[A-Za-z0-9_-]+`).
fn is_bare_toml_key(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn toml_bare_or_quoted_key(s: &str) -> String {
    if is_bare_toml_key(s) {
        s.to_string()
    } else {
        toml_quote(s)
    }
}

fn comment_kind_str(k: CommentKind) -> &'static str {
    match k {
        CommentKind::Plate => "plate",
        CommentKind::Eol => "eol",
        CommentKind::Pre => "pre",
        CommentKind::Post => "post",
        CommentKind::Repeatable => "repeatable",
        CommentKind::Decompiler => "decompiler",
    }
}

/// Write the contents of every [`PendingFile`] to disk under their paths,
/// creating parent directories as needed.
pub fn flush_to_disk(files: &[PendingFile]) -> Result<()> {
    for pf in files {
        if let Some(parent) = pf.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&pf.path, &pf.contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_from_name_rules() {
        assert_eq!(class_from_name("Foo__bar").as_deref(), Some("Foo"));
        assert_eq!(
            class_from_name("WormEntity__HandleMessage").as_deref(),
            Some("WormEntity"),
        );
        // Multi-level: only the leading segment is taken.
        assert_eq!(
            class_from_name("CWnd__NetChoiceDlg__sub_4A9070").as_deref(),
            Some("CWnd"),
        );
        // Names with no `__` → unclassified.
        assert_eq!(class_from_name("g_GameInfo"), None);
        assert_eq!(class_from_name("FUN_004FE070"), None);
        // Leading `__` is reserved (e.g. mangled MSVC names) — unclassified.
        assert_eq!(class_from_name("__stdcall_thunk"), None);
        // Non-alphanumeric chars in prefix → unclassified.
        assert_eq!(class_from_name("WINSPOOL.DRV::ClosePrinter"), None);
        assert_eq!(class_from_name("std::vector__push_back"), None);
    }

    /// Round-trip a small ReFile through write → parse, confirming both
    /// directions agree.
    #[test]
    fn round_trips_function_with_storage() {
        let f = Function {
            va: 0x004FE070,
            name: "WorldEntity__TryMovePosition".into(),
            calling_convention: Some("__thiscall".into()),
            plate_comment: Some("Generic move-and-collide.\nUsed by 50+ subclasses.".into()),
            no_return: false,
            signature: Some(Signature {
                returns: "int".into(),
                return_storage: None,
            }),
            param: vec![
                Param {
                    name: "this".into(),
                    ty: "WorldEntity *".into(),
                    storage: Some("ECX".into()),
                },
                Param {
                    name: "dx".into(),
                    ty: "Fixed".into(),
                    storage: Some("stack:0x4".into()),
                },
            ],
            local: vec![],
            comment: vec![InlineComment {
                va: 0x004FE0A4,
                kind: CommentKind::Decompiler,
                text: "piVar8 dual-view alias".into(),
            }],
        };
        let file = ReFile {
            function: vec![f],
            ..Default::default()
        };
        let text = write_re_file(&file);
        eprintln!("--- emitted ---\n{text}--- end ---");

        let parsed: ReFile = toml::from_str(&text).expect("emitter must produce parseable TOML");
        assert_eq!(parsed.function.len(), 1);
        let p = &parsed.function[0];
        assert_eq!(p.va, 0x004FE070);
        assert_eq!(p.name, "WorldEntity__TryMovePosition");
        assert_eq!(p.calling_convention.as_deref(), Some("__thiscall"));
        assert_eq!(p.param.len(), 2);
        assert_eq!(p.param[0].storage.as_deref(), Some("ECX"));
        assert_eq!(p.param[1].storage.as_deref(), Some("stack:0x4"));
        assert_eq!(p.comment.len(), 1);
        assert!(p.plate_comment.as_deref().unwrap().contains("subclasses"));
    }

    #[test]
    fn round_trips_struct_and_enum() {
        let s = Struct {
            name: "BaseEntity".into(),
            namespace: None,
            size: 0xA8,
            plate_comment: None,
            field: vec![
                Field {
                    offset: 0x00,
                    name: "vtable".into(),
                    ty: "void * *".into(),
                    type_namespace: None,
                    size: Some(4),
                    comment: None,
                },
                Field {
                    offset: 0x04,
                    name: "world".into(),
                    ty: "GameWorld *".into(),
                    type_namespace: None,
                    size: Some(4),
                    comment: Some("Owning world.".into()),
                },
            ],
        };
        let mut variants = indexmap::IndexMap::new();
        variants.insert("ProjectileImpact".into(), 0x76);
        variants.insert("MissileTickAfterFire".into(), 0x77);
        let e = Enum {
            name: "EntityMessage".into(),
            namespace: None,
            size: 4,
            variant: variants,
        };
        let file = ReFile {
            r#struct: vec![s],
            r#enum: vec![e],
            ..Default::default()
        };
        let text = write_re_file(&file);
        let parsed: ReFile = toml::from_str(&text).unwrap();
        assert_eq!(
            parsed.r#struct[0].field[1].comment.as_deref(),
            Some("Owning world.")
        );
        assert_eq!(
            parsed.r#enum[0].variant.get("ProjectileImpact"),
            Some(&0x76)
        );
    }

    #[test]
    fn handles_quotes_and_newlines_in_text() {
        let f = Function {
            va: 0x500,
            name: "x".into(),
            calling_convention: None,
            plate_comment: Some("she said \"hi\"\nthen left".into()),
            no_return: false,
            signature: None,
            param: vec![],
            local: vec![],
            comment: vec![],
        };
        let text = write_re_file(&ReFile {
            function: vec![f],
            ..Default::default()
        });
        let parsed: ReFile = toml::from_str(&text).unwrap();
        assert_eq!(
            parsed.function[0].plate_comment.as_deref(),
            Some("she said \"hi\"\nthen left\n")
        );
    }
}
