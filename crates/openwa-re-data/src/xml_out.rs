//! Emit a Ghidra-importable XML document from a [`Catalog`].
//!
//! We write the subset of `PROGRAM.DTD` Ghidra's `XmlImporter` consumes for
//! metadata overlay: `DATATYPES`, `DATA`, `COMMENTS`. We do NOT emit
//! `FUNCTIONS` or `SYMBOL_TABLE` — both Ghidra-side managers NPE on
//! various obscure edge cases (`FunctionsXmlMgr.read` for existing
//! functions with bodySize > 1; `SymbolTableXmlMgr.read` for PE IAT
//! addresses and DEFAULT-source conflicts). Every per-function override
//! and every (VA, name) pair lives in the extras sidecar instead, applied
//! by `ReImport.java` directly via the Java API with per-entry error
//! reporting.

use crate::model::*;
use crate::toml_io::Catalog;
use anyhow::Result;
use std::fmt::Write as FmtWrite;
use std::path::Path;

/// Produce the XML body. The caller writes it to disk.
pub fn render(cat: &Catalog) -> Result<String> {
    let mut w = String::with_capacity(2 * 1024 * 1024);

    writeln!(w, "<?xml version=\"1.0\" standalone=\"yes\"?>")?;
    writeln!(w, "<?program_dtd version=\"1\"?>")?;
    writeln!(w)?;
    writeln!(
        w,
        "<PROGRAM NAME=\"WA.exe\" EXE_FORMAT=\"Portable Executable (PE)\" IMAGE_BASE=\"00400000\">"
    )?;
    writeln!(w, "    <INFO_SOURCE TOOL=\"openwa-re\" />")?;
    writeln!(
        w,
        "    <PROCESSOR NAME=\"x86\" ENDIAN=\"little\" ADDRESS_MODEL=\"32-bit\" />"
    )?;

    render_datatypes(&mut w, cat)?;
    render_data(&mut w, cat)?;
    render_comments(&mut w, cat)?;

    writeln!(w, "</PROGRAM>")?;
    Ok(w)
}

/// Write the rendered XML and the extras sidecar to disk under `prefix`:
///   - `<prefix>.xml`            — the Ghidra import doc
///   - `<prefix>_extras.json`    — calling_convention + no_return per function
pub fn write_to(prefix: &Path, cat: &Catalog) -> Result<()> {
    let xml = render(cat)?;
    let xml_path = with_extension(prefix, "xml");
    std::fs::write(&xml_path, xml)?;

    let extras = crate::extras::build_from_catalog(cat);
    let json = serde_json::to_string_pretty(&extras)?;
    let extras_path = with_suffix(prefix, "_extras.json");
    std::fs::write(&extras_path, json)?;

    Ok(())
}

fn with_extension(prefix: &Path, ext: &str) -> std::path::PathBuf {
    let mut p = prefix.to_path_buf();
    p.set_extension(ext);
    p
}

fn with_suffix(prefix: &Path, suffix: &str) -> std::path::PathBuf {
    let mut s = prefix.file_name().map(|n| n.to_owned()).unwrap_or_default();
    s.push(suffix);
    prefix.with_file_name(s)
}

// ─── DATATYPES section ───────────────────────────────────────────────────────

fn render_datatypes(w: &mut String, cat: &Catalog) -> Result<()> {
    if cat.structs.is_empty()
        && cat.unions.is_empty()
        && cat.enums.is_empty()
        && cat.typedefs.is_empty()
        && cat.function_defs.is_empty()
        && cat.external_types.is_empty()
    {
        return Ok(());
    }
    writeln!(w, "    <DATATYPES>")?;

    let mut structs: Vec<&Struct> = cat.structs.values().map(|e| &e.value).collect();
    structs.sort_by(|a, b| a.name.cmp(&b.name));
    for s in structs {
        render_struct(w, s)?;
    }

    let mut unions: Vec<&Union> = cat.unions.values().map(|e| &e.value).collect();
    unions.sort_by(|a, b| a.name.cmp(&b.name));
    for u in unions {
        render_union(w, u)?;
    }

    let mut enums: Vec<&Enum> = cat.enums.values().map(|e| &e.value).collect();
    enums.sort_by(|a, b| a.name.cmp(&b.name));
    for e in enums {
        render_enum(w, e)?;
    }

    let mut typedefs: Vec<&Typedef> = cat.typedefs.values().map(|e| &e.value).collect();
    typedefs.sort_by(|a, b| a.name.cmp(&b.name));
    for t in typedefs {
        render_typedef(w, t)?;
    }

    let mut function_defs: Vec<&FunctionDef> =
        cat.function_defs.values().map(|e| &e.value).collect();
    function_defs.sort_by(|a, b| a.name.cmp(&b.name));
    for fd in function_defs {
        render_function_def(w, fd)?;
    }

    // external_types is intentionally NOT emitted. Earlier versions wrote
    // stub `<TYPE_DEF NAMESPACE="/openwa-re/external">` entries to make the
    // self round-trip lossless, but Ghidra's DataTypesXmlMgr actually creates
    // them as real types in the project, polluting the user's DB with 779
    // phantom typedefs. External-type names exist purely in `re/types.toml`
    // as validator hints; Ghidra resolves them via its built-in archives.

    writeln!(w, "    </DATATYPES>")?;
    Ok(())
}

fn render_struct(w: &mut String, s: &Struct) -> Result<()> {
    writeln!(
        w,
        "        <STRUCTURE NAME=\"{}\" NAMESPACE=\"{}\" SIZE=\"0x{:x}\">",
        xml_escape(&s.name),
        xml_escape(ns_or_root(&s.namespace)),
        s.size,
    )?;
    if let Some(plate) = &s.plate_comment {
        writeln!(
            w,
            "            <REGULAR_CMT>{}</REGULAR_CMT>",
            xml_escape(plate)
        )?;
    }
    for fld in &s.field {
        render_member(w, fld, fld.offset)?;
    }
    writeln!(w, "        </STRUCTURE>")?;
    Ok(())
}

/// Emit a `<MEMBER>` element with all attributes Ghidra's DTM-equality check
/// cares about: OFFSET, DATATYPE, DATATYPE_NAMESPACE (always `/` if unset),
/// NAME, SIZE (if known), optional inline REGULAR_CMT.
fn render_member(w: &mut String, fld: &Field, override_offset: u32) -> Result<()> {
    let mut attrs = String::new();
    use std::fmt::Write as _;
    write!(attrs, "OFFSET=\"0x{:x}\"", override_offset)?;
    write!(attrs, " DATATYPE=\"{}\"", xml_escape(&fld.ty))?;
    write!(
        attrs,
        " DATATYPE_NAMESPACE=\"{}\"",
        xml_escape(ns_or_root(&fld.type_namespace))
    )?;
    write!(attrs, " NAME=\"{}\"", xml_escape(&fld.name))?;
    if let Some(sz) = fld.size {
        write!(attrs, " SIZE=\"0x{sz:x}\"")?;
    }
    if let Some(comment) = &fld.comment {
        writeln!(w, "            <MEMBER {attrs}>")?;
        writeln!(
            w,
            "                <REGULAR_CMT>{}</REGULAR_CMT>",
            xml_escape(comment)
        )?;
        writeln!(w, "            </MEMBER>")?;
    } else {
        writeln!(w, "            <MEMBER {attrs} />")?;
    }
    Ok(())
}

fn render_union(w: &mut String, u: &Union) -> Result<()> {
    writeln!(
        w,
        "        <UNION NAME=\"{}\" NAMESPACE=\"{}\" SIZE=\"0x{:x}\">",
        xml_escape(&u.name),
        xml_escape(ns_or_root(&u.namespace)),
        u.size,
    )?;
    if let Some(plate) = &u.plate_comment {
        writeln!(
            w,
            "            <REGULAR_CMT>{}</REGULAR_CMT>",
            xml_escape(plate)
        )?;
    }
    for fld in &u.field {
        // Union members always live at offset 0 regardless of what the field
        // records (which may differ if it round-trips through the struct path).
        render_member(w, fld, 0)?;
    }
    writeln!(w, "        </UNION>")?;
    Ok(())
}

fn render_enum(w: &mut String, e: &Enum) -> Result<()> {
    writeln!(
        w,
        "        <ENUM NAME=\"{}\" NAMESPACE=\"{}\" SIZE=\"0x{:x}\">",
        xml_escape(&e.name),
        xml_escape(ns_or_root(&e.namespace)),
        e.size,
    )?;
    for (vname, value) in &e.variant {
        writeln!(
            w,
            "            <ENUM_ENTRY NAME=\"{}\" VALUE=\"0x{:x}\" COMMENT=\"\" />",
            xml_escape(vname),
            value,
        )?;
    }
    writeln!(w, "        </ENUM>")?;
    Ok(())
}

fn render_typedef(w: &mut String, t: &Typedef) -> Result<()> {
    writeln!(
        w,
        "        <TYPE_DEF NAME=\"{}\" NAMESPACE=\"{}\" DATATYPE=\"{}\" />",
        xml_escape(&t.name),
        xml_escape(ns_or_root(&t.namespace)),
        xml_escape(&t.target),
    )?;
    Ok(())
}

fn render_function_def(w: &mut String, fd: &FunctionDef) -> Result<()> {
    writeln!(
        w,
        "        <FUNCTION_DEF NAME=\"{}\" NAMESPACE=\"{}\">",
        xml_escape(&fd.name),
        xml_escape(ns_or_root(&fd.namespace)),
    )?;
    writeln!(
        w,
        "            <RETURN_TYPE DATATYPE=\"{}\" />",
        xml_escape(&fd.returns),
    )?;
    for (i, p) in fd.param.iter().enumerate() {
        writeln!(
            w,
            "            <PARAMETER ORDINAL=\"0x{i:x}\" DATATYPE=\"{}\" NAME=\"{}\" />",
            xml_escape(&p.ty),
            xml_escape(&p.name),
        )?;
    }
    writeln!(w, "        </FUNCTION_DEF>")?;
    Ok(())
}

// ─── DATA section (typed globals) ────────────────────────────────────────────

fn render_data(w: &mut String, cat: &Catalog) -> Result<()> {
    let typed: Vec<&Global> = cat
        .globals
        .values()
        .map(|e| &e.value)
        .filter(|g| g.ty.is_some())
        .collect();
    if typed.is_empty() {
        return Ok(());
    }
    let mut typed = typed;
    typed.sort_by_key(|g| g.va);

    writeln!(w, "    <DATA>")?;
    for g in typed {
        let ty = g.ty.as_deref().unwrap();
        writeln!(
            w,
            "        <DEFINED_DATA ADDRESS=\"{:08x}\" DATATYPE=\"{}\" />",
            g.va,
            xml_escape(ty),
        )?;
    }
    writeln!(w, "    </DATA>")?;
    Ok(())
}

// ─── COMMENTS section ────────────────────────────────────────────────────────

fn render_comments(w: &mut String, cat: &Catalog) -> Result<()> {
    // Collect inline comments from every function + global into one flat list.
    let mut all: Vec<(Va, CommentKind, &str)> = Vec::new();
    for entry in cat.functions.values() {
        for c in &entry.value.comment {
            all.push((c.va, c.kind, c.text.as_str()));
        }
    }
    for entry in cat.globals.values() {
        if let Some(c) = &entry.value.comment {
            all.push((entry.value.va, CommentKind::Plate, c.as_str()));
        }
    }
    if all.is_empty() {
        return Ok(());
    }
    all.sort_by_key(|&(va, _, _)| va);

    writeln!(w, "    <COMMENTS>")?;
    for (va, kind, text) in all {
        writeln!(
            w,
            "        <COMMENT ADDRESS=\"{:08x}\" TYPE=\"{}\">{}</COMMENT>",
            va,
            xml_comment_kind(kind),
            xml_escape(text),
        )?;
    }
    writeln!(w, "    </COMMENTS>")?;
    Ok(())
}

// ─── Escaping ────────────────────────────────────────────────────────────────

fn ns_or_root(ns: &Option<String>) -> &str {
    ns.as_deref().filter(|s| !s.is_empty()).unwrap_or("/")
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn xml_comment_kind(k: CommentKind) -> &'static str {
    match k {
        CommentKind::Plate => "plate",
        CommentKind::Eol => "end-of-line",
        CommentKind::Pre => "pre",
        CommentKind::Post => "post",
        CommentKind::Repeatable => "repeatable",
        CommentKind::Decompiler => "plate", // decompiler comments don't have a
                                            // distinct XML kind; fall back to
                                            // the plate slot.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml_io::OwnedEntry;
    use std::path::PathBuf;

    fn cat_with_function(f: Function) -> Catalog {
        let mut cat = Catalog::default();
        cat.functions.insert(
            f.va,
            OwnedEntry {
                value: f,
                source: PathBuf::from("test"),
            },
        );
        cat
    }

    #[test]
    fn emits_no_function_or_symbol_section() {
        let f = Function {
            va: 0x0052aaa0,
            name: "UpdateNetworkHudAnimations".into(),
            calling_convention: None,
            plate_comment: Some("plate".into()),
            no_return: false,
            signature: None,
            param: vec![],
            local: vec![],
            comment: vec![],
        };
        let xml = render(&cat_with_function(f)).unwrap();
        assert!(!xml.contains("<FUNCTION "));
        assert!(!xml.contains("<REGISTER_VAR"));
        assert!(!xml.contains("<SYMBOL_TABLE"));
        assert!(!xml.contains("<SYMBOL "));
    }

    #[test]
    fn xml_escape_handles_special_chars() {
        assert_eq!(xml_escape("Foo<&>"), "Foo&lt;&amp;&gt;");
        assert_eq!(xml_escape("\"hi\""), "&quot;hi&quot;");
    }
}
