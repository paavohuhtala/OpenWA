//! The "extras" sidecar — metadata Ghidra's XML DTD cannot carry.
//!
//! Ghidra's XML schema has no `CALLING_CONVENTION` attribute on `<FUNCTION>`
//! (literally a `<!-- TODO -->` in the DTD), no `no-return` flag, and no
//! `inline` flag. After `<analyzer> -import desired.xml` runs, a small
//! PyGhidra script reads `desired_extras.json` and applies these via
//! `Function.setCallingConvention(...)` / `setNoReturn(true)` / etc.
//!
//! JSON keeps the sidecar trivially readable for the PyGhidra side: 30 lines
//! of `json.load` + a loop, no schema library needed.

use crate::toml_io::Catalog;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Extras {
    pub functions: Vec<FunctionExtras>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionExtras {
    /// Absolute VA encoded as `0x004FE070` for human inspection; on JSON
    /// parse this comes through as a string and we decode hex on the
    /// Ghidra side.
    pub va: String,
    /// `__stdcall` / `__cdecl` / `__thiscall` / `__fastcall` / `__usercall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calling_convention: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_return: bool,
}

pub fn build_from_catalog(cat: &Catalog) -> Extras {
    let mut functions: Vec<FunctionExtras> = cat
        .functions
        .values()
        .filter_map(|f| {
            let f = &f.value;
            if f.calling_convention.is_some() || f.no_return {
                Some(FunctionExtras {
                    va: format!("0x{:08X}", f.va),
                    calling_convention: f.calling_convention.clone(),
                    no_return: f.no_return,
                })
            } else {
                None
            }
        })
        .collect();
    functions.sort_by(|a, b| a.va.cmp(&b.va));
    Extras { functions }
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
