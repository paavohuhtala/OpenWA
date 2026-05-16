//! Single JSON document that fully describes the desired Ghidra state.
//!
//! `ReImport.java` consumes only this file — no XML — and applies every
//! piece via Ghidra's Java API. We tried Ghidra's XML importers first;
//! each major manager NPEs / IAEs / `.conflict`-spams on common edge
//! cases that the surface logging hides:
//!
//!   - `FunctionsXmlMgr.read:153` — NPE for every entry whose address
//!     already has a function with bodySize > 1 (`CreateFunctionCmd`
//!     never sets `newFunc`).
//!   - `SymbolTableXmlMgr.read:270` — NPE for PE IAT addresses where
//!     `getPrimarySymbol` is null and `createPreferredLabelOrFunction
//!     Symbol` returns null.
//!   - `DataTypesXmlMgr.processStructure` — every populated user struct
//!     becomes a `.conflict` because the importer creates an empty
//!     shell first, then `addDataType(shell, null)` sees the existing
//!     populated struct as non-equivalent.
//!
//! All fixable from the API side. The schema below is the entire
//! payload — types, typed globals, comments, symbols, function
//! metadata — with no XML carve-outs.

use crate::model::{self, CommentKind as ModelCommentKind};
use crate::toml_io::Catalog;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// User-defined DTM entries. Applied first so functions, typed
    /// globals, and parameters can resolve their type references.
    #[serde(default, skip_serializing_if = "Types::is_empty")]
    pub types: Types,
    /// Typed globals — applies a `DataType` at a VA.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub typed_globals: Vec<TypedGlobal>,
    /// Inline / plate comments on instructions and data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<Comment>,
    /// (VA, name) pairs for every function / global / label.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<Symbol>,
    /// Function-level overrides: signature, params (with optional custom
    /// storage), plate comment, calling convention, no-return.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<Function>,
}

// ─── Types section ───────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Types {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structs: Vec<Struct>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unions: Vec<Struct>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enums: Vec<Enum>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub typedefs: Vec<Typedef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_defs: Vec<FunctionDef>,
}

impl Types {
    fn is_empty(&self) -> bool {
        self.structs.is_empty()
            && self.unions.is_empty()
            && self.enums.is_empty()
            && self.typedefs.is_empty()
            && self.function_defs.is_empty()
    }
}

/// Shared shape for structs and unions. Union members live at offset 0.
#[derive(Debug, Serialize, Deserialize)]
pub struct Struct {
    pub name: String,
    /// Ghidra `CategoryPath` (`/`, `/OpenWA`, `/auto_structs`).
    pub namespace: String,
    pub size: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_comment: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Field {
    pub offset: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Namespace of the referenced datatype (omitted when root `/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_namespace: Option<String>,
    /// Explicit byte size. Load-bearing for pointer-to-incomplete types
    /// where Ghidra's `DataType.getLength()` returns -1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Enum {
    pub name: String,
    pub namespace: String,
    /// Underlying width in bytes (1/2/4/8).
    pub size: u32,
    pub values: Vec<EnumValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnumValue {
    pub name: String,
    pub value: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Typedef {
    pub name: String,
    pub namespace: String,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub namespace: String,
    pub returns: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<FunctionDefParam>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

// ─── Typed globals + comments ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct TypedGlobal {
    pub va: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Comment {
    pub va: String,
    pub kind: CommentKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommentKind {
    Plate,
    EndOfLine,
    Pre,
    Post,
    Repeatable,
    Decompiler,
}

impl From<ModelCommentKind> for CommentKind {
    fn from(k: ModelCommentKind) -> Self {
        match k {
            ModelCommentKind::Plate => CommentKind::Plate,
            ModelCommentKind::Eol => CommentKind::EndOfLine,
            ModelCommentKind::Pre => CommentKind::Pre,
            ModelCommentKind::Post => CommentKind::Post,
            ModelCommentKind::Repeatable => CommentKind::Repeatable,
            ModelCommentKind::Decompiler => CommentKind::Decompiler,
        }
    }
}

// ─── Symbols + functions ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Symbol {
    /// Absolute VA encoded as `0x004FE070`.
    pub va: String,
    pub name: String,
    /// Java-side dispatches: `function` → `Function.setName`; `data` /
    /// `label` → `SymbolTable.createLabel` / `Symbol.setName`.
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Data,
    Label,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    /// Absolute VA encoded as `0x004FE070`. ReImport decodes hex.
    pub va: String,
    /// `__stdcall` / `__cdecl` / `__thiscall` / `__fastcall` (or
    /// architecture-specific). Custom storage is signalled by per-param
    /// `storage`, independent of this string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calling_convention: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_return: bool,
    /// Mirror of Ghidra's `Function.hasCustomVariableStorage()`. When set,
    /// ReImport switches to `CUSTOM_STORAGE` mode so `param.storage` strings
    /// land verbatim instead of being recomputed by the calling convention.
    #[serde(default, skip_serializing_if = "is_false")]
    pub custom_storage: bool,
    /// Function-level plate / regular comment (shown in listing header).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_comment: Option<String>,
    /// Return type as a Ghidra type string. Omitted when no signature
    /// override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    /// Parameters in source order. Any non-null `storage` value flips
    /// the function to custom-variable-storage mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// `"ECX"`, `"EDX:EAX"`, `"stack:0x4"`, `"stack:0x10:4"`. Omitted
    /// for default-convention parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
}

// ─── Builder ─────────────────────────────────────────────────────────────────

pub fn build_from_catalog(cat: &Catalog) -> Manifest {
    Manifest {
        types: collect_types(cat),
        typed_globals: collect_typed_globals(cat),
        comments: collect_comments(cat),
        symbols: collect_symbols(cat),
        functions: collect_functions(cat),
    }
}

fn collect_types(cat: &Catalog) -> Types {
    let mut structs: Vec<Struct> = cat
        .structs
        .values()
        .map(|e| convert_struct(&e.value))
        .collect();
    structs.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut unions: Vec<Struct> = cat
        .unions
        .values()
        .map(|e| convert_union(&e.value))
        .collect();
    unions.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut enums: Vec<Enum> = cat.enums.values().map(|e| convert_enum(&e.value)).collect();
    enums.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut typedefs: Vec<Typedef> = cat
        .typedefs
        .values()
        .map(|e| convert_typedef(&e.value))
        .collect();
    typedefs.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    let mut function_defs: Vec<FunctionDef> = cat
        .function_defs
        .values()
        .map(|e| convert_function_def(&e.value))
        .collect();
    function_defs.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

    Types {
        structs,
        unions,
        enums,
        typedefs,
        function_defs,
    }
}

fn convert_struct(s: &model::Struct) -> Struct {
    Struct {
        name: s.name.clone(),
        namespace: ns(&s.namespace),
        size: s.size,
        plate_comment: s.plate_comment.clone(),
        fields: s.field.iter().map(convert_field).collect(),
    }
}

fn convert_union(u: &model::Union) -> Struct {
    Struct {
        name: u.name.clone(),
        namespace: ns(&u.namespace),
        size: u.size,
        plate_comment: u.plate_comment.clone(),
        fields: u.field.iter().map(convert_field).collect(),
    }
}

fn convert_field(f: &model::Field) -> Field {
    Field {
        offset: f.offset,
        name: f.name.clone(),
        ty: f.ty.clone(),
        type_namespace: f
            .type_namespace
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "/")
            .map(str::to_string),
        size: f.size,
        comment: f.comment.clone(),
    }
}

fn convert_enum(e: &model::Enum) -> Enum {
    Enum {
        name: e.name.clone(),
        namespace: ns(&e.namespace),
        size: e.size,
        values: e
            .variant
            .iter()
            .map(|(name, value)| EnumValue {
                name: name.clone(),
                value: *value,
            })
            .collect(),
    }
}

fn convert_typedef(t: &model::Typedef) -> Typedef {
    Typedef {
        name: t.name.clone(),
        namespace: ns(&t.namespace),
        target: t.target.clone(),
    }
}

fn convert_function_def(fd: &model::FunctionDef) -> FunctionDef {
    FunctionDef {
        name: fd.name.clone(),
        namespace: ns(&fd.namespace),
        returns: fd.returns.clone(),
        params: fd
            .param
            .iter()
            .map(|p| FunctionDefParam {
                name: p.name.clone(),
                ty: p.ty.clone(),
            })
            .collect(),
    }
}

fn ns(opt: &Option<String>) -> String {
    opt.as_deref()
        .filter(|s| !s.is_empty() && *s != "/")
        .map(str::to_string)
        .unwrap_or_else(|| "/".to_string())
}

fn collect_typed_globals(cat: &Catalog) -> Vec<TypedGlobal> {
    let mut out: Vec<TypedGlobal> = cat
        .globals
        .values()
        .filter_map(|e| {
            e.value.ty.as_ref().map(|ty| TypedGlobal {
                va: format!("0x{:08X}", e.value.va),
                ty: ty.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.va.cmp(&b.va));
    out
}

fn collect_comments(cat: &Catalog) -> Vec<Comment> {
    let mut out: Vec<Comment> = Vec::new();
    for entry in cat.functions.values() {
        for c in &entry.value.comment {
            out.push(Comment {
                va: format!("0x{:08X}", c.va),
                kind: c.kind.into(),
                text: c.text.clone(),
            });
        }
    }
    for entry in cat.globals.values() {
        if let Some(text) = &entry.value.comment {
            out.push(Comment {
                va: format!("0x{:08X}", entry.value.va),
                kind: CommentKind::Plate,
                text: text.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.va.cmp(&b.va));
    out
}

fn collect_symbols(cat: &Catalog) -> Vec<Symbol> {
    let mut out: Vec<Symbol> = Vec::new();
    for entry in cat.functions.values() {
        out.push(Symbol {
            va: format!("0x{:08X}", entry.value.va),
            name: entry.value.name.clone(),
            kind: SymbolKind::Function,
        });
    }
    for entry in cat.globals.values() {
        let g = &entry.value;
        out.push(Symbol {
            va: format!("0x{:08X}", g.va),
            name: g.name.clone(),
            kind: if g.ty.is_some() {
                SymbolKind::Data
            } else {
                SymbolKind::Label
            },
        });
    }
    for entry in cat.labels.values() {
        out.push(Symbol {
            va: format!("0x{:08X}", entry.value.va),
            name: entry.value.name.clone(),
            kind: SymbolKind::Label,
        });
    }
    out.sort_by(|a, b| a.va.cmp(&b.va).then(a.name.cmp(&b.name)));
    out
}

fn collect_functions(cat: &Catalog) -> Vec<Function> {
    let mut out: Vec<Function> = cat
        .functions
        .values()
        .filter_map(|f| build_function(&f.value))
        .collect();
    out.sort_by(|a, b| a.va.cmp(&b.va));
    out
}

fn build_function(f: &model::Function) -> Option<Function> {
    let has_override = f.calling_convention.is_some()
        || f.no_return
        || f.custom_storage
        || f.plate_comment.is_some()
        || f.signature.is_some()
        || !f.param.is_empty();
    if !has_override {
        return None;
    }
    Some(Function {
        va: format!("0x{:08X}", f.va),
        calling_convention: f.calling_convention.clone(),
        no_return: f.no_return,
        custom_storage: f.custom_storage,
        plate_comment: f.plate_comment.clone(),
        return_type: f.signature.as_ref().map(|s| s.returns.clone()),
        params: f
            .param
            .iter()
            .map(|p| Param {
                name: p.name.clone(),
                ty: p.ty.clone(),
                storage: p.storage.clone(),
            })
            .collect(),
    })
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Serialise to pretty JSON.
pub fn to_json(m: &Manifest) -> serde_json::Result<String> {
    serde_json::to_string_pretty(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Function as ModelFunction;
    use crate::toml_io::OwnedEntry;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn function_override_filter() {
        let mut functions: HashMap<u32, OwnedEntry<ModelFunction>> = HashMap::new();
        functions.insert(
            0x500,
            OwnedEntry {
                value: ModelFunction {
                    va: 0x500,
                    name: "default_call".into(),
                    calling_convention: None,
                    plate_comment: None,
                    no_return: false,
                    signature: None,
                    param: vec![],
                    local: vec![],
                    comment: vec![],
                },
                source: PathBuf::from("re/x.toml"),
            },
        );
        functions.insert(
            0x600,
            OwnedEntry {
                value: ModelFunction {
                    va: 0x600,
                    name: "usercall_fn".into(),
                    calling_convention: Some("__usercall".into()),
                    plate_comment: None,
                    no_return: false,
                    signature: None,
                    param: vec![],
                    local: vec![],
                    comment: vec![],
                },
                source: PathBuf::from("re/x.toml"),
            },
        );
        let cat = Catalog {
            functions,
            ..Default::default()
        };
        let m = build_from_catalog(&cat);
        assert_eq!(m.functions.len(), 1);
        assert_eq!(m.functions[0].va, "0x00000600");
        assert_eq!(
            m.functions[0].calling_convention.as_deref(),
            Some("__usercall")
        );
        assert_eq!(m.symbols.len(), 2);
    }
}
