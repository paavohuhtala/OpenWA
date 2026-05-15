//! Stream-parse a Ghidra `XmlExporter` dump into our model.
//!
//! Drops the bulk of the file (DATA section, MARKUP, RELOCATION_TABLE, etc.)
//! and the Ghidra-built-in DTM (system headers, Demangler placeholders).
//! See `filter.rs` for the rules.
//!
//! The parser is a hand-written state machine over `quick_xml::Reader` —
//! orders of magnitude faster than serde-derive for a 44 MB file and gives
//! us precise control over which sections to walk and which to fast-skip.

use crate::filter;
use crate::model::*;
use anyhow::{Context, Result, anyhow, bail};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::path::Path;

/// The subset of a Ghidra XML dump we materialise. This is the
/// post-filter view — built-in types and auto-named symbols are already
/// dropped. Calling conventions / no-return are NOT populated here; those
/// come from the PyGhidra extras sidecar (see plan).
#[derive(Debug, Default)]
pub struct XmlProgram {
    pub functions: Vec<Function>,
    pub globals: Vec<Global>,
    pub labels: Vec<Label>,
    pub structs: Vec<Struct>,
    pub unions: Vec<Union>,
    pub enums: Vec<Enum>,
    pub typedefs: Vec<Typedef>,
    pub function_defs: Vec<FunctionDef>,
    /// One entry per (address, comment_kind). Routed into the owning function
    /// (if any) at a later pass; orphan comments stay attached to globals/labels.
    pub comments: Vec<RawComment>,
    /// Type names declared in DTM namespaces we filter out (Win32, MFC, CRT,
    /// `/PE`, `/Demangler`, anonymous `_struct_NN`, etc.). User TOML may
    /// reference these legitimately even though they're not defined in `re/`.
    pub external_types: Vec<String>,
    pub stats: ParseStats,
}

#[derive(Debug)]
pub struct RawComment {
    pub va: Va,
    pub kind: CommentKind,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct ParseStats {
    pub functions_kept: usize,
    pub functions_dropped_auto: usize,
    pub functions_dropped_library: usize,
    pub types_kept: usize,
    pub types_dropped_builtin: usize,
    pub types_dropped_anonymous: usize,
    pub types_dropped_placeholder: usize,
    pub symbols_kept: usize,
    pub symbols_dropped_auto: usize,
    pub comments_kept: usize,
}

pub fn parse_file(path: &Path) -> Result<XmlProgram> {
    let mut reader =
        Reader::from_file(path).with_context(|| format!("opening {}", path.display()))?;
    reader.config_mut().trim_text(true);
    // Resolve the inline DTD as no-op — Ghidra emits it but we don't need entity
    // expansion (no &foo; references in actual data).
    reader.config_mut().expand_empty_elements = false;

    let mut buf = Vec::with_capacity(64 * 1024);
    let mut prog = XmlProgram::default();

    loop {
        match reader
            .read_event_into(&mut buf)
            .with_context(|| format!("at offset {}", reader.buffer_position()))?
        {
            Event::Start(e) => match e.name().as_ref() {
                b"DATATYPES" => parse_datatypes(&mut reader, &mut prog)?,
                b"SYMBOL_TABLE" => parse_symbol_table(&mut reader, &mut prog)?,
                b"FUNCTIONS" => parse_functions(&mut reader, &mut prog)?,
                b"COMMENTS" => parse_comments(&mut reader, &mut prog)?,
                b"DATA" => parse_data(&mut reader, &mut prog)?,
                // Sections we skip entirely.
                b"MEMORY_MAP"
                | b"REGISTER_VALUES"
                | b"CODE"
                | b"EQUATES"
                | b"PROPERTIES"
                | b"BOOKMARKS"
                | b"PROGRAM_TREES"
                | b"PROGRAM_ENTRY_POINTS"
                | b"RELOCATION_TABLE"
                | b"MARKUP"
                | b"EXT_LIBRARY_TABLE"
                | b"PROCESSOR"
                | b"COMPILER"
                | b"INFO_SOURCE"
                | b"DESCRIPTION" => {
                    let name = e.name().as_ref().to_vec();
                    reader.read_to_end_into(quick_xml::name::QName(&name), &mut Vec::new())?;
                }
                _ => {}
            },
            Event::Empty(_)
            | Event::End(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_) => {}
            Event::Eof => break,
        }
        buf.clear();
    }

    Ok(prog)
}

// ─── DATATYPES section ───────────────────────────────────────────────────────

fn parse_datatypes<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let mut buf = Vec::with_capacity(8192);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) => match e.name().as_ref() {
                b"STRUCTURE" => parse_structure(reader, e, prog)?,
                b"UNION" => parse_union(reader, e, prog)?,
                b"ENUM" => parse_enum(reader, e, prog)?,
                b"FUNCTION_DEF" => parse_function_def(reader, e, prog)?,
                _ => {
                    let name = e.name().as_ref().to_vec();
                    reader.read_to_end_into(quick_xml::name::QName(&name), &mut Vec::new())?;
                }
            },
            Event::Empty(e) => match e.name().as_ref() {
                b"TYPE_DEF" => handle_typedef(&e, prog),
                b"ENUM" => handle_empty_enum(&e, prog),
                _ => {}
            },
            Event::End(e) if e.name().as_ref() == b"DATATYPES" => return Ok(()),
            Event::Eof => bail!("unexpected EOF inside <DATATYPES>"),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_structure<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    start: BytesStart<'_>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let name = required_attr(&start, b"NAME")?;
    let namespace = optional_attr(&start, b"NAMESPACE").unwrap_or_else(|| "/".to_string());
    let size = hex_attr(&start, b"SIZE")?;

    let mut keep =
        !filter::is_builtin_dtm_namespace(&namespace) && !filter::is_primitive_type_name(&name);

    // PlaceHolder Structure: size 0 with a marker REGULAR_CMT. Drop.
    let mut is_placeholder = size == 0;
    let mut fields: Vec<Field> = Vec::new();
    let mut plate: Option<String> = None;

    let mut buf = Vec::with_capacity(2048);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"MEMBER" => {
                if !keep {
                    continue;
                }
                let ty = required_attr(&e, b"DATATYPE")?;
                let mname_opt = optional_attr(&e, b"NAME");
                // Skip Ghidra's auto-fill: `undefined` size=1, no NAME. These
                // get regenerated on import from the parent SIZE attribute.
                if ty == "undefined" && mname_opt.is_none() {
                    continue;
                }
                let offset = hex_attr(&e, b"OFFSET")?;
                let mname = mname_opt.unwrap_or_else(|| format!("field_{offset:x}"));
                fields.push(Field {
                    offset,
                    name: mname,
                    ty,
                    comment: None,
                });
            }
            Event::Start(e) if e.name().as_ref() == b"MEMBER" => {
                // MEMBER with children (typically a REGULAR_CMT). Parse attrs
                // and any nested comment.
                let ty = required_attr(&e, b"DATATYPE")?;
                let offset = hex_attr(&e, b"OFFSET")?;
                let mname =
                    optional_attr(&e, b"NAME").unwrap_or_else(|| format!("field_{offset:x}"));
                let mut field_comment: Option<String> = None;
                let n = e.name().as_ref().to_vec();
                let mut inner = Vec::with_capacity(256);
                loop {
                    match reader.read_event_into(&mut inner)? {
                        Event::Start(ee) if ee.name().as_ref() == b"REGULAR_CMT" => {
                            let t = read_text(reader, b"REGULAR_CMT")?;
                            if !t.is_empty() {
                                field_comment = Some(t);
                            }
                        }
                        Event::End(ee) if ee.name().as_ref() == n.as_slice() => break,
                        Event::Eof => bail!("unexpected EOF inside <MEMBER>"),
                        _ => {}
                    }
                    inner.clear();
                }
                if keep && !(ty == "undefined" && field_comment.is_none()) {
                    fields.push(Field {
                        offset,
                        name: mname,
                        ty,
                        comment: field_comment,
                    });
                }
            }
            Event::Start(e) if e.name().as_ref() == b"REGULAR_CMT" => {
                let text = read_text(reader, b"REGULAR_CMT")?;
                if text == "PlaceHolder Structure" {
                    is_placeholder = true;
                    keep = false;
                } else if !text.is_empty() {
                    plate = Some(text);
                }
            }
            Event::End(e) if e.name().as_ref() == b"STRUCTURE" => break,
            Event::Eof => bail!("unexpected EOF inside <STRUCTURE>"),
            _ => {}
        }
        buf.clear();
    }

    if !keep {
        if is_placeholder {
            prog.stats.types_dropped_placeholder += 1;
            // Placeholder structs are referenced by name from other types;
            // keep the name so user references resolve.
            prog.external_types.push(name);
        } else {
            prog.stats.types_dropped_builtin += 1;
            prog.external_types.push(name);
        }
        return Ok(());
    }

    if filter::is_anonymous_type_name(&name) {
        prog.stats.types_dropped_anonymous += 1;
        prog.external_types.push(name);
        return Ok(());
    }

    prog.structs.push(Struct {
        name,
        namespace: normalise_namespace(namespace),
        size,
        plate_comment: plate,
        field: fields,
    });
    prog.stats.types_kept += 1;
    Ok(())
}

/// Map `/` (root) and empty to `None`; everything else passes through as `Some`.
fn normalise_namespace(ns: String) -> Option<String> {
    if ns.is_empty() || ns == "/" {
        None
    } else {
        Some(ns)
    }
}

fn parse_union<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    start: BytesStart<'_>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let name = required_attr(&start, b"NAME")?;
    let namespace = optional_attr(&start, b"NAMESPACE").unwrap_or_else(|| "/".to_string());
    let size = hex_attr(&start, b"SIZE")?;
    let keep =
        !filter::is_builtin_dtm_namespace(&namespace) && !filter::is_anonymous_type_name(&name);

    let mut fields: Vec<Field> = Vec::new();
    let mut plate: Option<String> = None;

    let mut buf = Vec::with_capacity(1024);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"MEMBER" => {
                if !keep {
                    continue;
                }
                let offset = hex_attr(&e, b"OFFSET")?;
                let mname =
                    optional_attr(&e, b"NAME").unwrap_or_else(|| format!("field_{offset:x}"));
                let ty = required_attr(&e, b"DATATYPE")?;
                fields.push(Field {
                    offset,
                    name: mname,
                    ty,
                    comment: None,
                });
            }
            Event::Start(e) if e.name().as_ref() == b"REGULAR_CMT" => {
                let text = read_text(reader, b"REGULAR_CMT")?;
                if !text.is_empty() {
                    plate = Some(text);
                }
            }
            Event::End(e) if e.name().as_ref() == b"UNION" => break,
            Event::Eof => bail!("unexpected EOF inside <UNION>"),
            _ => {}
        }
        buf.clear();
    }

    if !keep {
        if filter::is_anonymous_type_name(&name) {
            prog.stats.types_dropped_anonymous += 1;
        } else {
            prog.stats.types_dropped_builtin += 1;
        }
        prog.external_types.push(name);
        return Ok(());
    }

    prog.unions.push(Union {
        name,
        namespace: normalise_namespace(namespace),
        size,
        plate_comment: plate,
        field: fields,
    });
    prog.stats.types_kept += 1;
    Ok(())
}

fn parse_enum<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    start: BytesStart<'_>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let name = required_attr(&start, b"NAME")?;
    let namespace = optional_attr(&start, b"NAMESPACE").unwrap_or_else(|| "/".to_string());
    let size = hex_attr(&start, b"SIZE")?;
    let keep =
        !filter::is_builtin_dtm_namespace(&namespace) && !filter::is_anonymous_type_name(&name);

    let mut variants = indexmap::IndexMap::new();
    let mut buf = Vec::with_capacity(512);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"ENUM_ENTRY" => {
                if !keep {
                    continue;
                }
                let vname = required_attr(&e, b"NAME")?;
                let value = hex_attr_signed(&e, b"VALUE")?;
                variants.insert(vname, value);
            }
            Event::End(e) if e.name().as_ref() == b"ENUM" => break,
            Event::Eof => bail!("unexpected EOF inside <ENUM>"),
            _ => {}
        }
        buf.clear();
    }

    if !keep {
        if filter::is_anonymous_type_name(&name) {
            prog.stats.types_dropped_anonymous += 1;
        } else {
            prog.stats.types_dropped_builtin += 1;
        }
        prog.external_types.push(name);
        return Ok(());
    }

    prog.enums.push(Enum {
        name,
        namespace: normalise_namespace(namespace),
        size,
        variant: variants,
    });
    prog.stats.types_kept += 1;
    Ok(())
}

fn handle_empty_enum(e: &BytesStart<'_>, prog: &mut XmlProgram) {
    // Empty `<ENUM NAME=... SIZE=... />` — forward declaration. Drop iff filtered;
    // otherwise keep with no variants so name reference resolves.
    let Ok(name) = required_attr(e, b"NAME") else {
        return;
    };
    let namespace = optional_attr(e, b"NAMESPACE").unwrap_or_else(|| "/".to_string());
    if filter::is_builtin_dtm_namespace(&namespace) || filter::is_anonymous_type_name(&name) {
        prog.stats.types_dropped_builtin += 1;
        prog.external_types.push(name);
        return;
    }
    let size = hex_attr(e, b"SIZE").unwrap_or(4);
    prog.enums.push(Enum {
        name,
        namespace: normalise_namespace(namespace),
        size,
        variant: Default::default(),
    });
    prog.stats.types_kept += 1;
}

fn parse_function_def<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    start: BytesStart<'_>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let name = required_attr(&start, b"NAME")?;
    let namespace = optional_attr(&start, b"NAMESPACE").unwrap_or_else(|| "/".to_string());
    let keep = !filter::is_builtin_dtm_namespace(&namespace);

    let mut returns: TypeRef = "void".to_string();
    let mut params: Vec<FunctionDefParam> = Vec::new();

    let mut buf = Vec::with_capacity(1024);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"RETURN_TYPE" => {
                returns = required_attr(&e, b"DATATYPE")?;
            }
            Event::Empty(e) if e.name().as_ref() == b"PARAMETER" => {
                if !keep {
                    continue;
                }
                let pname = optional_attr(&e, b"NAME").unwrap_or_else(|| String::from("param"));
                let ty = required_attr(&e, b"DATATYPE")?;
                params.push(FunctionDefParam { name: pname, ty });
            }
            Event::Start(e) if e.name().as_ref() == b"REGULAR_CMT" => {
                read_text(reader, b"REGULAR_CMT")?;
            }
            Event::End(e) if e.name().as_ref() == b"FUNCTION_DEF" => break,
            Event::Eof => bail!("unexpected EOF inside <FUNCTION_DEF>"),
            _ => {}
        }
        buf.clear();
    }

    if !keep {
        prog.stats.types_dropped_builtin += 1;
        prog.external_types.push(name);
        return Ok(());
    }

    prog.function_defs.push(FunctionDef {
        name,
        namespace: normalise_namespace(namespace),
        returns,
        param: params,
    });
    prog.stats.types_kept += 1;
    Ok(())
}

fn handle_typedef(e: &BytesStart<'_>, prog: &mut XmlProgram) {
    let Ok(name) = required_attr(e, b"NAME") else {
        return;
    };
    let namespace = optional_attr(e, b"NAMESPACE").unwrap_or_else(|| "/".to_string());
    let Ok(target) = required_attr(e, b"DATATYPE") else {
        return;
    };
    if filter::is_builtin_dtm_namespace(&namespace) {
        prog.stats.types_dropped_builtin += 1;
        prog.external_types.push(name);
        return;
    }
    prog.typedefs.push(Typedef {
        name,
        namespace: normalise_namespace(namespace),
        target,
    });
    prog.stats.types_kept += 1;
}

// ─── SYMBOL_TABLE section ────────────────────────────────────────────────────

fn parse_symbol_table<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let mut buf = Vec::with_capacity(2048);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"SYMBOL" => {
                let source = optional_attr(&e, b"SOURCE_TYPE").unwrap_or_default();
                if source != "USER_DEFINED" {
                    prog.stats.symbols_dropped_auto += 1;
                    continue;
                }
                let primary = optional_attr(&e, b"PRIMARY").unwrap_or_default();
                if primary != "y" {
                    // Non-primary aliases — drop for now; round-trip can re-add via SYMBOL TYPE=label.
                    prog.stats.symbols_dropped_auto += 1;
                    continue;
                }
                let va = hex_attr(&e, b"ADDRESS")?;
                let name = required_attr(&e, b"NAME")?;
                if filter::is_auto_symbol_name(&name) {
                    prog.stats.symbols_dropped_auto += 1;
                    continue;
                }
                // We don't know yet whether this is a function, global, or
                // plain label. Functions are filled by <FUNCTIONS>; if a
                // SYMBOL VA collides with a FUNCTION VA later, the symbol is
                // dropped during catalog assembly. For now park everything
                // as a label and let the export pass route it.
                prog.labels.push(Label { va, name });
                prog.stats.symbols_kept += 1;
            }
            Event::Start(e) if e.name().as_ref() == b"SYMBOL" => {
                // Some symbol entries have nested elements; rare. Skip body.
                let n = e.name().as_ref().to_vec();
                reader.read_to_end_into(quick_xml::name::QName(&n), &mut Vec::new())?;
            }
            Event::End(e) if e.name().as_ref() == b"SYMBOL_TABLE" => return Ok(()),
            Event::Eof => bail!("unexpected EOF inside <SYMBOL_TABLE>"),
            _ => {}
        }
        buf.clear();
    }
}

// ─── FUNCTIONS section ───────────────────────────────────────────────────────

fn parse_functions<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let mut buf = Vec::with_capacity(8192);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) if e.name().as_ref() == b"FUNCTION" => parse_function(reader, e, prog)?,
            Event::End(e) if e.name().as_ref() == b"FUNCTIONS" => return Ok(()),
            Event::Eof => bail!("unexpected EOF inside <FUNCTIONS>"),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_function<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    start: BytesStart<'_>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let va = hex_attr(&start, b"ENTRY_POINT")?;
    let name = required_attr(&start, b"NAME")?;
    let library = optional_attr(&start, b"LIBRARY_FUNCTION").unwrap_or_default();

    let mut keep = true;
    if filter::is_auto_function_name(&name) {
        prog.stats.functions_dropped_auto += 1;
        keep = false;
    } else if library == "y" {
        prog.stats.functions_dropped_library += 1;
        keep = false;
    }

    let mut signature: Option<Signature> = None;
    let mut plate: Option<String> = None;
    let mut params: Vec<Param> = Vec::new();
    let mut locals: Vec<Local> = Vec::new();
    let mut custom_storage = false;

    let mut buf = Vec::with_capacity(8192);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"RETURN_TYPE" && keep => {
                let returns = required_attr(&e, b"DATATYPE")?;
                signature = Some(Signature {
                    returns,
                    return_storage: None,
                });
            }
            Event::Empty(e) if e.name().as_ref() == b"ADDRESS_RANGE" => {
                // Implicit from function body; nothing to retain.
            }
            Event::Start(e) if e.name().as_ref() == b"REGULAR_CMT" => {
                let text = read_text(reader, b"REGULAR_CMT")?;
                if keep && !text.is_empty() {
                    plate = Some(text);
                }
            }
            Event::Start(e) if e.name().as_ref() == b"TYPEINFO_CMT" => {
                // Display-only; derived from signature. Discard.
                read_text(reader, b"TYPEINFO_CMT")?;
            }
            Event::Start(e) if e.name().as_ref() == b"REPEATABLE_CMT" => {
                read_text(reader, b"REPEATABLE_CMT")?;
            }
            Event::Start(e) if e.name().as_ref() == b"STACK_FRAME" => {
                parse_stack_frame(reader, &mut params, &mut locals, keep)?;
            }
            Event::Empty(e) if e.name().as_ref() == b"REGISTER_VAR" => {
                if !keep {
                    continue;
                }
                custom_storage = true;
                let pname = required_attr(&e, b"NAME")?;
                let reg = required_attr(&e, b"REGISTER")?;
                let ty = optional_attr(&e, b"DATATYPE").unwrap_or_else(|| "void *".to_string());
                params.push(Param {
                    name: pname,
                    ty,
                    storage: Some(reg),
                });
            }
            Event::Start(e) if e.name().as_ref() == b"REGISTER_VAR" => {
                if keep {
                    custom_storage = true;
                    let pname = required_attr(&e, b"NAME")?;
                    let reg = required_attr(&e, b"REGISTER")?;
                    let ty = optional_attr(&e, b"DATATYPE").unwrap_or_else(|| "void *".to_string());
                    params.push(Param {
                        name: pname,
                        ty,
                        storage: Some(reg),
                    });
                }
                let n = b"REGISTER_VAR".to_vec();
                reader.read_to_end_into(quick_xml::name::QName(&n), &mut Vec::new())?;
            }
            Event::End(e) if e.name().as_ref() == b"FUNCTION" => break,
            Event::Eof => bail!("unexpected EOF inside <FUNCTION>"),
            _ => {}
        }
        buf.clear();
    }

    if !keep {
        return Ok(());
    }

    // Suppress the unused-variable lint; `custom_storage` is intentionally
    // observed but not acted on — we emit storage only where the XML had it.
    let _ = custom_storage;

    prog.functions.push(Function {
        va,
        name,
        calling_convention: None, // sidecar-supplied
        plate_comment: plate,
        no_return: false,
        signature,
        param: params,
        local: locals,
        comment: Vec::new(), // populated from COMMENTS section later
    });
    prog.stats.functions_kept += 1;
    Ok(())
}

fn parse_stack_frame<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    params: &mut Vec<Param>,
    locals: &mut Vec<Local>,
    keep: bool,
) -> Result<()> {
    let mut buf = Vec::with_capacity(1024);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"STACK_VAR" => {
                if !keep {
                    continue;
                }
                let offset = hex_attr_signed(&e, b"STACK_PTR_OFFSET")?;
                let pname_opt = optional_attr(&e, b"NAME");
                let ty = required_attr(&e, b"DATATYPE")?;
                let pname = pname_opt
                    .clone()
                    .unwrap_or_else(|| default_stack_name(offset));

                // Skip Ghidra defaults: auto-named (`local_NN`/`param_NN`)
                // with an `undefined*` type carry no information.
                if is_ghidra_default_stack_name(&pname) && is_undefined_type(&ty) {
                    continue;
                }

                if offset > 0 {
                    // Positive == above return address == param.
                    params.push(Param {
                        name: pname,
                        ty,
                        storage: None,
                    });
                } else {
                    locals.push(Local {
                        name: pname,
                        ty,
                        stack_offset: offset as i32,
                    });
                }
            }
            Event::Start(e) if e.name().as_ref() == b"STACK_VAR" => {
                let n = b"STACK_VAR".to_vec();
                reader.read_to_end_into(quick_xml::name::QName(&n), &mut Vec::new())?;
            }
            Event::End(e) if e.name().as_ref() == b"STACK_FRAME" => return Ok(()),
            Event::Eof => bail!("unexpected EOF inside <STACK_FRAME>"),
            _ => {}
        }
        buf.clear();
    }
}

// ─── COMMENTS section ────────────────────────────────────────────────────────

fn parse_comments<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    prog: &mut XmlProgram,
) -> Result<()> {
    let mut buf = Vec::with_capacity(2048);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(e) if e.name().as_ref() == b"COMMENT" => {
                let va = hex_attr(&e, b"ADDRESS")?;
                let kind_str = required_attr(&e, b"TYPE")?;
                let kind = match kind_str.as_str() {
                    "plate" => CommentKind::Plate,
                    "end-of-line" => CommentKind::Eol,
                    "pre" => CommentKind::Pre,
                    "post" => CommentKind::Post,
                    "repeatable" => CommentKind::Repeatable,
                    _ => continue,
                };
                let text = read_text(reader, b"COMMENT")?;
                if text.is_empty() {
                    continue;
                }
                if is_auto_generated_comment(&text, kind) {
                    continue;
                }
                prog.comments.push(RawComment { va, kind, text });
                prog.stats.comments_kept += 1;
            }
            Event::End(e) if e.name().as_ref() == b"COMMENTS" => return Ok(()),
            Event::Eof => bail!("unexpected EOF inside <COMMENTS>"),
            _ => {}
        }
        buf.clear();
    }
}

// ─── DATA section (user-typed globals only) ──────────────────────────────────

fn parse_data<R: std::io::BufRead>(reader: &mut Reader<R>, prog: &mut XmlProgram) -> Result<()> {
    let mut buf = Vec::with_capacity(2048);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(e) if e.name().as_ref() == b"DEFINED_DATA" => {
                let ty = required_attr(&e, b"DATATYPE")?;
                let ns =
                    optional_attr(&e, b"DATATYPE_NAMESPACE").unwrap_or_else(|| "/".to_string());

                // Filter rules: skip primitives, PE/DOS headers, and anything
                // whose base type is unnameable noise.
                let base = base_name(&ty);
                if filter::is_primitive_type_name(base) || filter::is_builtin_dtm_namespace(&ns) {
                    continue;
                }
                if base.is_empty() {
                    continue;
                }

                let va = hex_attr(&e, b"ADDRESS")?;
                prog.globals.push(Global {
                    va,
                    name: String::new(), // resolved against SYMBOL_TABLE in a follow-up pass
                    ty: Some(ty),
                    comment: None,
                });
            }
            Event::Start(e) if e.name().as_ref() == b"DEFINED_DATA" => {
                let n = b"DEFINED_DATA".to_vec();
                reader.read_to_end_into(quick_xml::name::QName(&n), &mut Vec::new())?;
            }
            Event::End(e) if e.name().as_ref() == b"DATA" => return Ok(()),
            Event::Eof => bail!("unexpected EOF inside <DATA>"),
            _ => {}
        }
        buf.clear();
    }
}

fn base_name(ty: &str) -> &str {
    let cut = ty.find(['*', '[']).unwrap_or(ty.len());
    ty[..cut].trim()
}

fn default_stack_name(offset: i64) -> String {
    if offset > 0 {
        format!("param_{offset:x}")
    } else {
        format!("local_{:x}", -offset)
    }
}

fn is_ghidra_default_stack_name(name: &str) -> bool {
    for prefix in ["local_", "param_", "Stack[", "local_res", "stack_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            // Tolerate trailing `]` for Stack[0x4].
            let rest = rest.trim_end_matches(']');
            if !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == 'x' || c == '_' || c == '-')
            {
                return true;
            }
        }
    }
    false
}

fn is_undefined_type(ty: &str) -> bool {
    matches!(
        ty.trim(),
        "undefined" | "undefined1" | "undefined2" | "undefined4" | "undefined8"
    )
}

/// True if a `<COMMENT>` element body looks Ghidra-generated rather than
/// user-authored. Three pattern families:
///   1. PE section markers (`.text`, `.rdata`, …)
///   2. MS Demangler RTTI dumps (`meta pointer for ::vftable`, `const ::vftable`)
///   3. Function ID Analyzer plates (`Library Function - …`)
fn is_auto_generated_comment(text: &str, kind: CommentKind) -> bool {
    // Catch-all patterns that fire on any comment kind.
    if matches!(
        text,
        "Export Function Pointers"
            | "Export Name Pointers"
            | "Export Ordinal Values"
            | "Export Library Name"
            | "TypeDescriptor.name"
            | ".text"
            | ".rdata"
            | ".data"
            | ".rsrc"
            | ".reloc"
            | ".idata"
            | ".pdata"
            | ".bss"
            | ".NET PDB Info"
            | "Padding for Alignment"
            | "const ::vftable"
            | "meta pointer for ::vftable"
            | "IMAGE_THUNK_DATA32"
            | "IMAGE_THUNK_DATA64"
            | "IMAGE_IMPORT_DESCRIPTOR"
            | "IMAGE_RESOURCE_DIRECTORY"
            | "IMAGE_RESOURCE_DIRECTORY_ENTRY"
            | "IMAGE_RESOURCE_DATA_ENTRY"
            | "IMAGE_RESOURCE_DIR_STRING_U"
    ) {
        return true;
    }
    if text.contains("::vftable")
        || text.contains("::RTTI ")
        || text.starts_with("ref to TypeDescriptor (RTTI ")
        || text.starts_with("PlaceHolder")
        || text.starts_with("Function Signature Data Type")
        // PE / DOS data-section structure-name plates (`IMAGE_*`, `_IMAGE_*`).
        // User-authored comments referencing these wouldn't be a bare type name.
        || (text.starts_with("IMAGE_") && text.len() < 64)
        || (text.starts_with("_IMAGE_") && text.len() < 64)
        // .rsrc resource-table auto-annotations from the PE-loader analyzer.
        || (text.starts_with("Rsrc_") && text.contains("Size of resource:"))
        // .rsrc icon / cursor / bitmap / dialog header auto-annotations.
        || text == "version (must be 1)"
        || text == "version (must be 0)"
        || text == "reserved (must be 0)"
        || text == "must be 1 for cursor / icon"
        || text == "must be 0 for icon / cursor"
        || text == "image type"
        || text == "number of images"
        || text == "style of dialog box"
        || text.starts_with("offset to image data")
        || text.starts_with("Size of image data")
        || text.starts_with("Rsrc String ID ")
    {
        return true;
    }
    if matches!(kind, CommentKind::Plate) && text.starts_with("Library Function -") {
        return true;
    }
    false
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn required_attr(e: &BytesStart<'_>, key: &[u8]) -> Result<String> {
    for a in e.attributes().with_checks(false) {
        let a = a?;
        if a.key.as_ref() == key {
            return Ok(String::from_utf8(
                a.unescape_value()?.into_owned().into_bytes(),
            )?);
        }
    }
    Err(anyhow!(
        "missing required attribute `{}` on <{}>",
        String::from_utf8_lossy(key),
        String::from_utf8_lossy(e.name().as_ref())
    ))
}

fn optional_attr(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for a in e.attributes().with_checks(false).flatten() {
        if a.key.as_ref() == key {
            return a.unescape_value().ok().map(|c| c.into_owned());
        }
    }
    None
}

fn hex_attr(e: &BytesStart<'_>, key: &[u8]) -> Result<u32> {
    let s = required_attr(e, key)?;
    parse_hex_u32(&s)
}

fn hex_attr_signed(e: &BytesStart<'_>, key: &[u8]) -> Result<i64> {
    let s = required_attr(e, key)?;
    let s = s.trim();
    let (neg, rest) = if let Some(r) = s.strip_prefix('-') {
        (true, r)
    } else {
        (false, s)
    };
    let n = if let Some(r) = rest.strip_prefix("0x") {
        i64::from_str_radix(r, 16)?
    } else if rest.chars().all(|c| c.is_ascii_hexdigit()) {
        // Ghidra often omits the `0x` prefix.
        i64::from_str_radix(rest, 16)?
    } else {
        rest.parse::<i64>()?
    };
    Ok(if neg { -n } else { n })
}

fn parse_hex_u32(s: &str) -> Result<u32> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("0x") {
        Ok(u32::from_str_radix(rest, 16)?)
    } else if s.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(u32::from_str_radix(s, 16)?)
    } else {
        Ok(s.parse::<u32>()?)
    }
}

fn read_text<R: std::io::BufRead>(reader: &mut Reader<R>, end: &[u8]) -> Result<String> {
    let mut out = String::new();
    let mut buf = Vec::with_capacity(512);
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Text(t) => out.push_str(&t.unescape()?),
            Event::CData(t) => out.push_str(std::str::from_utf8(t.as_ref())?),
            Event::End(e) if e.name().as_ref() == end => return Ok(out),
            Event::Eof => bail!(
                "unexpected EOF reading text inside <{}>",
                String::from_utf8_lossy(end)
            ),
            _ => {}
        }
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_small_synthetic() {
        let xml = r#"<?xml version="1.0" standalone="yes"?>
<PROGRAM>
  <DATATYPES>
    <STRUCTURE NAME="BaseEntity" NAMESPACE="/" SIZE="0x10">
      <MEMBER OFFSET="0x0" DATATYPE="void * *" NAME="vtable" SIZE="0x4" />
      <MEMBER OFFSET="0x4" DATATYPE="GameWorld *" NAME="world" SIZE="0x4" />
    </STRUCTURE>
    <STRUCTURE NAME="exception" NAMESPACE="/Demangler/std" SIZE="0x0">
      <REGULAR_CMT>PlaceHolder Structure</REGULAR_CMT>
    </STRUCTURE>
    <TYPE_DEF NAME="Fixed" NAMESPACE="/" DATATYPE="int" />
    <ENUM NAME="EntityMessage" NAMESPACE="/" SIZE="0x4">
      <ENUM_ENTRY NAME="ProjectileImpact" VALUE="0x76" COMMENT="" />
    </ENUM>
  </DATATYPES>
  <SYMBOL_TABLE>
    <SYMBOL ADDRESS="00500000" NAME="g_world" TYPE="global" SOURCE_TYPE="USER_DEFINED" PRIMARY="y" />
    <SYMBOL ADDRESS="00500100" NAME="LAB_00500100" TYPE="global" SOURCE_TYPE="USER_DEFINED" PRIMARY="y" />
  </SYMBOL_TABLE>
  <FUNCTIONS>
    <FUNCTION ENTRY_POINT="0052aaa0" NAME="GameRuntime__UpdateNetworkHudAnimations" LIBRARY_FUNCTION="n">
      <RETURN_TYPE DATATYPE="void" SIZE="0x0" />
      <REGULAR_CMT>__usercall: ESI=this.</REGULAR_CMT>
      <STACK_FRAME LOCAL_VAR_SIZE="0x4" PARAM_OFFSET="0x4" RETURN_ADDR_SIZE="0x0" BYTES_PURGED="20">
        <STACK_VAR STACK_PTR_OFFSET="0x4" NAME="chat_box_min_step" DATATYPE="int" SIZE="0x4" />
      </STACK_FRAME>
    </FUNCTION>
    <FUNCTION ENTRY_POINT="00401000" NAME="FUN_00401000" LIBRARY_FUNCTION="n">
      <RETURN_TYPE DATATYPE="void" SIZE="0x0" />
    </FUNCTION>
  </FUNCTIONS>
  <COMMENTS>
    <COMMENT ADDRESS="0052abc4" TYPE="end-of-line">stub release</COMMENT>
  </COMMENTS>
</PROGRAM>
"#;
        let tmp = std::env::temp_dir().join("openwa_re_test.xml");
        std::fs::write(&tmp, xml).unwrap();
        let prog = parse_file(&tmp).unwrap();

        assert_eq!(prog.structs.len(), 1);
        assert_eq!(prog.structs[0].name, "BaseEntity");
        assert_eq!(prog.structs[0].field.len(), 2);
        assert_eq!(prog.typedefs.len(), 1);
        assert_eq!(prog.enums.len(), 1);
        assert_eq!(prog.enums[0].variant.get("ProjectileImpact"), Some(&0x76));

        assert_eq!(prog.labels.len(), 1, "LAB_xxxxxxxx must be dropped");
        assert_eq!(prog.labels[0].name, "g_world");

        assert_eq!(prog.functions.len(), 1, "FUN_xxxxxxxx must be dropped");
        assert_eq!(prog.functions[0].va, 0x0052aaa0);
        assert_eq!(
            prog.functions[0].plate_comment.as_deref(),
            Some("__usercall: ESI=this.")
        );

        assert_eq!(prog.comments.len(), 1);
        assert!(matches!(prog.comments[0].kind, CommentKind::Eol));

        assert_eq!(prog.stats.functions_dropped_auto, 1);
        assert_eq!(prog.stats.types_dropped_placeholder, 1);
    }
}
