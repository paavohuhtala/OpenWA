//! Emit a Ghidra-importable XML document from a [`Catalog`].
//!
//! We write the subset of `PROGRAM.DTD` Ghidra's `XmlImporter` consumes for
//! metadata overlay: `DATATYPES`, `SYMBOL_TABLE`, `DATA`, `FUNCTIONS`,
//! `COMMENTS`. Everything else (memory map, code blocks, relocations,
//! markup) is derived from the binary at import time.
//!
//! Storage parsing: TOML strings like `"ECX"`, `"stack:0x4"`,
//! `"stack:0x10:4"`, `"EDX:EAX"` are routed to `<REGISTER_VAR>` or
//! `<STACK_VAR>` based on prefix. Split-register storage (`EDX:EAX` for a
//! 64-bit return) is passed through to Ghidra verbatim as a register name —
//! Ghidra knows the syntax.

use crate::model::*;
use crate::toml_io::Catalog;
use anyhow::{Result, bail};
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
    render_symbol_table(&mut w, cat)?;
    render_data(&mut w, cat)?;
    render_functions(&mut w, cat)?;
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

    render_external_types(w, cat)?;

    writeln!(w, "    </DATATYPES>")?;
    Ok(())
}

/// Emit each `external_types` entry as a stub `<TYPE_DEF>` in the synthetic
/// `/openwa-re/external` namespace. Ghidra's importer ignores these (they're
/// already in its built-in archives by the real name), and our own re-export
/// pass filters that namespace out — re-collecting the names into
/// `external_types` so the round-trip is lossless.
fn render_external_types(w: &mut String, cat: &Catalog) -> Result<()> {
    if cat.external_types.is_empty() {
        return Ok(());
    }
    let mut names: Vec<&String> = cat.external_types.iter().collect();
    names.sort();
    for n in names {
        writeln!(
            w,
            "        <TYPE_DEF NAME=\"{}\" NAMESPACE=\"/openwa-re/external\" DATATYPE=\"undefined\" />",
            xml_escape(n),
        )?;
    }
    Ok(())
}

fn render_struct(w: &mut String, s: &Struct) -> Result<()> {
    writeln!(
        w,
        "        <STRUCTURE NAME=\"{}\" NAMESPACE=\"/\" SIZE=\"0x{:x}\">",
        xml_escape(&s.name),
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
        if let Some(comment) = &fld.comment {
            writeln!(
                w,
                "            <MEMBER OFFSET=\"0x{:x}\" DATATYPE=\"{}\" NAME=\"{}\">",
                fld.offset,
                xml_escape(&fld.ty),
                xml_escape(&fld.name),
            )?;
            writeln!(
                w,
                "                <REGULAR_CMT>{}</REGULAR_CMT>",
                xml_escape(comment)
            )?;
            writeln!(w, "            </MEMBER>")?;
        } else {
            writeln!(
                w,
                "            <MEMBER OFFSET=\"0x{:x}\" DATATYPE=\"{}\" NAME=\"{}\" />",
                fld.offset,
                xml_escape(&fld.ty),
                xml_escape(&fld.name),
            )?;
        }
    }
    writeln!(w, "        </STRUCTURE>")?;
    Ok(())
}

fn render_union(w: &mut String, u: &Union) -> Result<()> {
    writeln!(
        w,
        "        <UNION NAME=\"{}\" NAMESPACE=\"/\" SIZE=\"0x{:x}\">",
        xml_escape(&u.name),
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
        writeln!(
            w,
            "            <MEMBER OFFSET=\"0x0\" DATATYPE=\"{}\" NAME=\"{}\" />",
            xml_escape(&fld.ty),
            xml_escape(&fld.name),
        )?;
    }
    writeln!(w, "        </UNION>")?;
    Ok(())
}

fn render_enum(w: &mut String, e: &Enum) -> Result<()> {
    writeln!(
        w,
        "        <ENUM NAME=\"{}\" NAMESPACE=\"/\" SIZE=\"0x{:x}\">",
        xml_escape(&e.name),
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
        "        <TYPE_DEF NAME=\"{}\" NAMESPACE=\"/\" DATATYPE=\"{}\" />",
        xml_escape(&t.name),
        xml_escape(&t.target),
    )?;
    Ok(())
}

fn render_function_def(w: &mut String, fd: &FunctionDef) -> Result<()> {
    writeln!(
        w,
        "        <FUNCTION_DEF NAME=\"{}\" NAMESPACE=\"/\">",
        xml_escape(&fd.name),
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

// ─── SYMBOL_TABLE section ────────────────────────────────────────────────────

fn render_symbol_table(w: &mut String, cat: &Catalog) -> Result<()> {
    writeln!(w, "    <SYMBOL_TABLE>")?;

    // Function names.
    let mut funcs: Vec<&Function> = cat.functions.values().map(|e| &e.value).collect();
    funcs.sort_by_key(|f| f.va);
    for f in funcs {
        writeln!(
            w,
            "        <SYMBOL ADDRESS=\"{:08x}\" NAME=\"{}\" NAMESPACE=\"\" TYPE=\"global\" SOURCE_TYPE=\"USER_DEFINED\" PRIMARY=\"y\" />",
            f.va,
            xml_escape(&f.name),
        )?;
    }

    // Globals.
    let mut globals: Vec<&Global> = cat.globals.values().map(|e| &e.value).collect();
    globals.sort_by_key(|g| g.va);
    for g in globals {
        writeln!(
            w,
            "        <SYMBOL ADDRESS=\"{:08x}\" NAME=\"{}\" NAMESPACE=\"\" TYPE=\"global\" SOURCE_TYPE=\"USER_DEFINED\" PRIMARY=\"y\" />",
            g.va,
            xml_escape(&g.name),
        )?;
    }

    // Code labels. Emitted with the same flavour as Ghidra exports user
    // labels (`TYPE="global"` `PRIMARY="y"`) so they survive a re-export
    // round-trip; Ghidra still treats them as plain labels because no
    // `<FUNCTION>` or `<DEFINED_DATA>` claims the VA.
    let mut labels: Vec<&Label> = cat.labels.values().map(|e| &e.value).collect();
    labels.sort_by_key(|l| l.va);
    for l in labels {
        writeln!(
            w,
            "        <SYMBOL ADDRESS=\"{:08x}\" NAME=\"{}\" NAMESPACE=\"\" TYPE=\"global\" SOURCE_TYPE=\"USER_DEFINED\" PRIMARY=\"y\" />",
            l.va,
            xml_escape(&l.name),
        )?;
    }

    writeln!(w, "    </SYMBOL_TABLE>")?;
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

// ─── FUNCTIONS section ───────────────────────────────────────────────────────

fn render_functions(w: &mut String, cat: &Catalog) -> Result<()> {
    if cat.functions.is_empty() {
        return Ok(());
    }
    writeln!(w, "    <FUNCTIONS>")?;
    let mut funcs: Vec<&Function> = cat.functions.values().map(|e| &e.value).collect();
    funcs.sort_by_key(|f| f.va);
    for f in funcs {
        render_function(w, f)?;
    }
    writeln!(w, "    </FUNCTIONS>")?;
    Ok(())
}

fn render_function(w: &mut String, f: &Function) -> Result<()> {
    writeln!(
        w,
        "        <FUNCTION ENTRY_POINT=\"{:08x}\" NAME=\"{}\" LIBRARY_FUNCTION=\"n\">",
        f.va,
        xml_escape(&f.name),
    )?;

    if let Some(sig) = &f.signature {
        writeln!(
            w,
            "            <RETURN_TYPE DATATYPE=\"{}\" />",
            xml_escape(&sig.returns),
        )?;
    }
    if let Some(plate) = &f.plate_comment {
        writeln!(
            w,
            "            <REGULAR_CMT>{}</REGULAR_CMT>",
            xml_escape(plate)
        )?;
    }

    // Split params into stack (→ STACK_VAR) and register (→ REGISTER_VAR).
    let mut stack_params: Vec<(&Param, ParsedStorage)> = Vec::new();
    let mut register_params: Vec<(&Param, ParsedStorage)> = Vec::new();
    let mut unanchored: Vec<&Param> = Vec::new();
    for p in &f.param {
        match p.storage.as_deref().map(parse_storage) {
            Some(Ok(ps @ ParsedStorage::Stack { .. })) => stack_params.push((p, ps)),
            Some(Ok(ps @ ParsedStorage::Register(_))) => register_params.push((p, ps)),
            Some(Ok(ps @ ParsedStorage::SplitRegister(..))) => register_params.push((p, ps)),
            Some(Err(e)) => {
                bail!(
                    "function 0x{:08x} param `{}` storage `{}`: {}",
                    f.va,
                    p.name,
                    p.storage.as_deref().unwrap_or(""),
                    e,
                );
            }
            None => unanchored.push(p),
        }
    }

    if !stack_params.is_empty() {
        writeln!(w, "            <STACK_FRAME>")?;
        for (p, ps) in &stack_params {
            let ParsedStorage::Stack { offset, .. } = ps else {
                unreachable!()
            };
            writeln!(
                w,
                "                <STACK_VAR STACK_PTR_OFFSET=\"0x{:x}\" NAME=\"{}\" DATATYPE=\"{}\" />",
                offset,
                xml_escape(&p.name),
                xml_escape(&p.ty),
            )?;
        }
        writeln!(w, "            </STACK_FRAME>")?;
    }

    for (p, ps) in &register_params {
        let reg = match ps {
            ParsedStorage::Register(r) => r.clone(),
            ParsedStorage::SplitRegister(hi, lo) => format!("{hi}:{lo}"),
            ParsedStorage::Stack { .. } => unreachable!(),
        };
        writeln!(
            w,
            "            <REGISTER_VAR NAME=\"{}\" REGISTER=\"{}\" DATATYPE=\"{}\" />",
            xml_escape(&p.name),
            xml_escape(&reg),
            xml_escape(&p.ty),
        )?;
    }

    // Unanchored params (no `storage` field): emit nothing — Ghidra computes
    // them from the calling convention (set via extras sidecar).
    let _ = unanchored;

    writeln!(w, "        </FUNCTION>")?;
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

// ─── Storage parsing ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum ParsedStorage {
    Register(String),
    SplitRegister(String, String),
    Stack { offset: i32, _size: Option<u32> },
}

fn parse_storage(s: &str) -> std::result::Result<ParsedStorage, &'static str> {
    if let Some(rest) = s.strip_prefix("stack:") {
        let mut parts = rest.split(':');
        let off = parts.next().ok_or("missing stack offset")?;
        let size = parts.next();
        if parts.next().is_some() {
            return Err("too many `:` in stack storage");
        }
        let offset = parse_signed_hex_or_dec(off).ok_or("bad stack offset")?;
        let _size = match size {
            Some(s) => Some(s.parse::<u32>().map_err(|_| "bad stack size")?),
            None => None,
        };
        Ok(ParsedStorage::Stack { offset, _size })
    } else if let Some((hi, lo)) = s.split_once(':') {
        if !is_register_name(hi) || !is_register_name(lo) {
            return Err("split register parts must be register names");
        }
        Ok(ParsedStorage::SplitRegister(hi.to_string(), lo.to_string()))
    } else if is_register_name(s) {
        Ok(ParsedStorage::Register(s.to_string()))
    } else {
        Err("storage must be a register, register pair, or `stack:0x…`")
    }
}

fn is_register_name(r: &str) -> bool {
    !r.is_empty()
        && r.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn parse_signed_hex_or_dec(s: &str) -> Option<i32> {
    let (sign, rest) = match s.strip_prefix('-') {
        Some(r) => (-1i32, r),
        None => (1, s),
    };
    let n = if let Some(h) = rest.strip_prefix("0x") {
        i32::from_str_radix(h, 16).ok()?
    } else {
        rest.parse::<i32>().ok()?
    };
    Some(sign * n)
}

// ─── Escaping ────────────────────────────────────────────────────────────────

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
    fn renders_function_with_register_var() {
        let f = Function {
            va: 0x0052aaa0,
            name: "UpdateNetworkHudAnimations".into(),
            calling_convention: Some("__usercall".into()),
            plate_comment: Some("ESI = this".into()),
            no_return: false,
            signature: Some(Signature {
                returns: "void".into(),
                return_storage: None,
            }),
            param: vec![
                Param {
                    name: "this".into(),
                    ty: "GameRuntime *".into(),
                    storage: Some("ESI".into()),
                },
                Param {
                    name: "chat_box_min_step".into(),
                    ty: "Fixed".into(),
                    storage: Some("stack:0x4".into()),
                },
            ],
            local: vec![],
            comment: vec![],
        };
        let xml = render(&cat_with_function(f)).unwrap();
        assert!(xml.contains(r#"<REGISTER_VAR NAME="this" REGISTER="ESI""#));
        assert!(xml.contains(r#"<STACK_VAR STACK_PTR_OFFSET="0x4""#));
        assert!(xml.contains(r#"<REGULAR_CMT>ESI = this</REGULAR_CMT>"#));
    }

    #[test]
    fn escapes_xml_special_chars() {
        let f = Function {
            va: 0x500,
            name: "Foo<&>".into(),
            calling_convention: None,
            plate_comment: Some("She said \"hi\"; then left".into()),
            no_return: false,
            signature: None,
            param: vec![],
            local: vec![],
            comment: vec![],
        };
        let xml = render(&cat_with_function(f)).unwrap();
        assert!(xml.contains("Foo&lt;&amp;&gt;"));
        assert!(xml.contains("She said &quot;hi&quot;"));
    }

    #[test]
    fn parse_storage_accepts_grammar() {
        assert!(matches!(
            parse_storage("ECX"),
            Ok(ParsedStorage::Register(_))
        ));
        assert!(matches!(
            parse_storage("EDX:EAX"),
            Ok(ParsedStorage::SplitRegister(_, _))
        ));
        assert!(matches!(
            parse_storage("stack:0x4"),
            Ok(ParsedStorage::Stack { offset: 4, .. })
        ));
        assert!(matches!(
            parse_storage("stack:0x10:4"),
            Ok(ParsedStorage::Stack { offset: 16, .. })
        ));
        assert!(parse_storage("eax").is_err());
        assert!(parse_storage("stack:abc").is_err());
    }
}
