//! The "extras" sidecar — every per-function override.
//!
//! Originally this only carried the few attributes Ghidra's XML DTD cannot
//! express (`CALLING_CONVENTION`, `no_return`). It now carries the full
//! function-override payload — plate comment, return type, parameters
//! (including custom storage for `__usercall`) — because Ghidra's
//! `FunctionsXmlMgr.read` NPEs on every `<FUNCTION>` element whose entry
//! point already has a function with bodySize > 1 (which is all of ours).
//! Root cause: `CreateFunctionCmd.handleExistingFunction` returns
//! `applyTo()=true` without ever setting `newFunc`, so `cmd.getFunction()`
//! at `FunctionsXmlMgr.java:153` is null. We bypass that path entirely by
//! applying every function override via `Function.*` API calls from
//! `ReImport.java`.

use crate::model::*;
use crate::toml_io::Catalog;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Extras {
    /// (VA, name) pairs for every function / global / label. Applied by
    /// `ReImport.java` via the Java API rather than `<SYMBOL_TABLE>` —
    /// Ghidra's `SymbolTableXmlMgr` NPEs on multiple obscure edge cases
    /// (PE IAT addresses, name-equal-to-existing-DEFAULT, etc.) without
    /// surfacing the failing entry. Applying ourselves lets us log per
    /// failure.
    pub symbols: Vec<SymbolExtras>,
    pub functions: Vec<FunctionExtras>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolExtras {
    /// Absolute VA encoded as `0x004FE070`.
    pub va: String,
    pub name: String,
    /// What the caller is naming. Java-side decides the right symbol-create
    /// path: `function` ⇒ `Function.setName` (or create then setName);
    /// `data` ⇒ `Listing.setComment`-friendly label; `label` ⇒ generic.
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
pub struct FunctionExtras {
    /// Absolute VA encoded as `0x004FE070`. ReImport decodes hex.
    pub va: String,
    /// `__stdcall` / `__cdecl` / `__thiscall` / `__fastcall` / `__usercall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calling_convention: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_return: bool,
    /// Function-level plate / regular comment (shown in listing header).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate_comment: Option<String>,
    /// Return type as a Ghidra type string. Omitted when no signature override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    /// Parameters in source order. If any param carries `storage`, the
    /// importer flips the function to custom-variable-storage and writes
    /// every storage slot explicitly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParamExtras>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParamExtras {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// `"ECX"`, `"EDX:EAX"`, `"stack:0x4"`, `"stack:0x10:4"`. Omitted for
    /// default-convention parameters where Ghidra computes storage from
    /// the chosen calling convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
}

pub fn build_from_catalog(cat: &Catalog) -> Extras {
    let mut functions: Vec<FunctionExtras> = cat
        .functions
        .values()
        .filter_map(|f| build_one(&f.value))
        .collect();
    functions.sort_by(|a, b| a.va.cmp(&b.va));

    let mut symbols: Vec<SymbolExtras> = Vec::new();
    for entry in cat.functions.values() {
        symbols.push(SymbolExtras {
            va: format!("0x{:08X}", entry.value.va),
            name: entry.value.name.clone(),
            kind: SymbolKind::Function,
        });
    }
    for entry in cat.globals.values() {
        let g = &entry.value;
        symbols.push(SymbolExtras {
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
        symbols.push(SymbolExtras {
            va: format!("0x{:08X}", entry.value.va),
            name: entry.value.name.clone(),
            kind: SymbolKind::Label,
        });
    }
    symbols.sort_by(|a, b| a.va.cmp(&b.va).then(a.name.cmp(&b.name)));

    Extras { symbols, functions }
}

fn build_one(f: &Function) -> Option<FunctionExtras> {
    let has_override = f.calling_convention.is_some()
        || f.no_return
        || f.plate_comment.is_some()
        || f.signature.is_some()
        || !f.param.is_empty();
    if !has_override {
        return None;
    }
    Some(FunctionExtras {
        va: format!("0x{:08X}", f.va),
        calling_convention: f.calling_convention.clone(),
        no_return: f.no_return,
        plate_comment: f.plate_comment.clone(),
        return_type: f.signature.as_ref().map(|s| s.returns.clone()),
        params: f
            .param
            .iter()
            .map(|p| ParamExtras {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Function;
    use crate::toml_io::OwnedEntry;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn collects_only_non_default_entries() {
        let mut functions: HashMap<u32, OwnedEntry<Function>> = HashMap::new();
        functions.insert(
            0x500,
            OwnedEntry {
                value: Function {
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
                value: Function {
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
        let extras = build_from_catalog(&cat);
        assert_eq!(extras.functions.len(), 1);
        assert_eq!(extras.functions[0].va, "0x00000600");
        assert_eq!(
            extras.functions[0].calling_convention.as_deref(),
            Some("__usercall")
        );
    }
}
