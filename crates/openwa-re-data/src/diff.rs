//! Compute changes between a fresh Ghidra [`XmlProgram`] and the committed
//! TOML [`Catalog`]. Pure: no I/O, no mutation; the apply path consumes the
//! returned [`Change`] list.
//!
//! Scope (v1):
//!   - Functions: per-field updates against existing entries (wholesale per-field
//!     replace). Creates/deletes are reported as [`ChangeKind::NewFunction`] /
//!     [`ChangeKind::RemovedFunction`] but are NOT actionable — see
//!     [`Change::actionable`].
//!   - Labels: create / rename / delete, keyed by VA.
//!   - Globals: create / rename / retype / delete, keyed by VA.
//!   - Same-VA label↔global promotions surface as a `RemovedLabel`+`NewGlobal`
//!     (or `RemovedGlobal`+`NewLabel`) pair — the apply path edits both files
//!     independently. The renderer can fold them for display.
//!
//! Out of scope: structs / unions / enums / typedefs / function_defs /
//! external_types. Edit those directly in TOML.

use crate::emit;
use crate::manifest::ghidra_type;
use crate::model::*;
use crate::toml_io::Catalog;
use crate::xml_in::XmlProgram;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub va: Va,
    /// Owning file for updates and removals; destination file for additions.
    pub file: PathBuf,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    // Functions — only updates are actionable in v1.
    NewFunction {
        name: String,
    },
    RemovedFunction {
        name: String,
    },
    FunctionRename {
        old: String,
        new: String,
    },
    FunctionPlateComment {
        old: Option<String>,
        new: Option<String>,
    },
    FunctionReturns {
        old: Option<TypeRef>,
        new: Option<TypeRef>,
    },
    FunctionParams {
        old: Vec<Param>,
        new: Vec<Param>,
    },
    FunctionLocals {
        old: Vec<Local>,
        new: Vec<Local>,
    },
    FunctionComments {
        old: Vec<InlineComment>,
        new: Vec<InlineComment>,
    },
    FunctionCallingConvention {
        old: Option<String>,
        new: Option<String>,
    },
    FunctionNoReturn {
        old: bool,
        new: bool,
    },
    FunctionCustomStorage {
        old: bool,
        new: bool,
    },

    // Labels.
    NewLabel {
        name: String,
    },
    RemovedLabel {
        name: String,
    },
    LabelRename {
        old: String,
        new: String,
    },

    // Globals.
    NewGlobal {
        name: String,
        ty: Option<TypeRef>,
    },
    RemovedGlobal {
        name: String,
        ty: Option<TypeRef>,
    },
    GlobalRename {
        old: String,
        new: String,
    },
    GlobalRetype {
        old: Option<TypeRef>,
        new: Option<TypeRef>,
    },
}

impl Change {
    /// True if this change can be applied by incremental import. Function
    /// creates and deletes are reported but skipped — they require routing
    /// decisions we defer to a later phase.
    pub fn actionable(&self) -> bool {
        !matches!(
            self.kind,
            ChangeKind::NewFunction { .. } | ChangeKind::RemovedFunction { .. },
        )
    }
}

/// Compute the change list between `prog` and `cat`. `re_dir` is the
/// `re/` root and is only used to compute destination paths for additions.
pub fn diff(prog: &XmlProgram, cat: &Catalog, re_dir: &Path) -> Vec<Change> {
    let mut out = Vec::new();
    diff_functions(prog, cat, re_dir, &mut out);
    diff_labels(prog, cat, re_dir, &mut out);
    diff_globals(prog, cat, re_dir, &mut out);
    out.sort_by(|a, b| {
        a.va.cmp(&b.va)
            .then_with(|| change_rank(&a.kind).cmp(&change_rank(&b.kind)))
    });
    out
}

// ─── Functions ───────────────────────────────────────────────────────────────

fn diff_functions(prog: &XmlProgram, cat: &Catalog, re_dir: &Path, out: &mut Vec<Change>) {
    let xml_by_va: HashMap<Va, &Function> = prog.functions.iter().map(|f| (f.va, f)).collect();
    let xml_vas: HashSet<Va> = xml_by_va.keys().copied().collect();

    for (&va, x) in &xml_by_va {
        match cat.functions.get(&va) {
            None => out.push(Change {
                va,
                file: emit::destination_for_function(re_dir, &x.name, va),
                kind: ChangeKind::NewFunction {
                    name: x.name.clone(),
                },
            }),
            Some(entry) => diff_one_function(x, &entry.value, &entry.source, out),
        }
    }
    for (&va, entry) in &cat.functions {
        if !xml_vas.contains(&va) {
            out.push(Change {
                va,
                file: entry.source.clone(),
                kind: ChangeKind::RemovedFunction {
                    name: entry.value.name.clone(),
                },
            });
        }
    }
}

fn diff_one_function(x: &Function, t: &Function, file: &Path, out: &mut Vec<Change>) {
    let push = |out: &mut Vec<Change>, k: ChangeKind| {
        out.push(Change {
            va: x.va,
            file: file.to_path_buf(),
            kind: k,
        });
    };

    if x.name != t.name {
        push(
            out,
            ChangeKind::FunctionRename {
                old: t.name.clone(),
                new: x.name.clone(),
            },
        );
    }
    // Plate comments come from two sources with different trailing-whitespace
    // behaviour: quick_xml's `trim_text(true)` strips trailing newlines, while
    // TOML's `"""..."""` always carries one. Compare on the canonical form.
    if normalise_text(x.plate_comment.as_deref()) != normalise_text(t.plate_comment.as_deref()) {
        push(
            out,
            ChangeKind::FunctionPlateComment {
                old: t.plate_comment.clone(),
                new: x.plate_comment.clone(),
            },
        );
    }
    let x_returns = x.signature.as_ref().map(|s| s.returns.clone());
    let t_returns = t.signature.as_ref().map(|s| s.returns.clone());
    if !type_opt_eq(&x_returns, &t_returns) {
        push(
            out,
            ChangeKind::FunctionReturns {
                old: t_returns,
                new: x_returns,
            },
        );
    }
    if !params_equal_modulo_cv(&x.param, &t.param) {
        push(
            out,
            ChangeKind::FunctionParams {
                old: t.param.clone(),
                new: x.param.clone(),
            },
        );
    }
    if !locals_equal_modulo_cv(&x.local, &t.local) {
        push(
            out,
            ChangeKind::FunctionLocals {
                old: t.local.clone(),
                new: x.local.clone(),
            },
        );
    }
    if !inline_comments_equal(&x.comment, &t.comment) {
        push(
            out,
            ChangeKind::FunctionComments {
                old: t.comment.clone(),
                new: x.comment.clone(),
            },
        );
    }
    if x.calling_convention != t.calling_convention {
        push(
            out,
            ChangeKind::FunctionCallingConvention {
                old: t.calling_convention.clone(),
                new: x.calling_convention.clone(),
            },
        );
    }
    if x.no_return != t.no_return {
        push(
            out,
            ChangeKind::FunctionNoReturn {
                old: t.no_return,
                new: x.no_return,
            },
        );
    }
    if x.custom_storage != t.custom_storage {
        push(
            out,
            ChangeKind::FunctionCustomStorage {
                old: t.custom_storage,
                new: x.custom_storage,
            },
        );
    }
}

// ─── Labels ──────────────────────────────────────────────────────────────────

fn diff_labels(prog: &XmlProgram, cat: &Catalog, re_dir: &Path, out: &mut Vec<Change>) {
    let xml_by_va: HashMap<Va, &Label> = prog.labels.iter().map(|l| (l.va, l)).collect();
    let xml_vas: HashSet<Va> = xml_by_va.keys().copied().collect();

    for (&va, x) in &xml_by_va {
        match cat.labels.get(&va) {
            None => out.push(Change {
                va,
                file: emit::destination_for_label(re_dir, &x.name),
                kind: ChangeKind::NewLabel {
                    name: x.name.clone(),
                },
            }),
            Some(entry) => {
                if x.name != entry.value.name {
                    out.push(Change {
                        va,
                        file: entry.source.clone(),
                        kind: ChangeKind::LabelRename {
                            old: entry.value.name.clone(),
                            new: x.name.clone(),
                        },
                    });
                }
            }
        }
    }
    for (&va, entry) in &cat.labels {
        if !xml_vas.contains(&va) {
            out.push(Change {
                va,
                file: entry.source.clone(),
                kind: ChangeKind::RemovedLabel {
                    name: entry.value.name.clone(),
                },
            });
        }
    }
}

// ─── Globals ─────────────────────────────────────────────────────────────────

fn diff_globals(prog: &XmlProgram, cat: &Catalog, re_dir: &Path, out: &mut Vec<Change>) {
    let xml_by_va: HashMap<Va, &Global> = prog.globals.iter().map(|g| (g.va, g)).collect();
    let xml_vas: HashSet<Va> = xml_by_va.keys().copied().collect();

    for (&va, x) in &xml_by_va {
        match cat.globals.get(&va) {
            None => out.push(Change {
                va,
                file: emit::destination_for_global(re_dir, &x.name),
                kind: ChangeKind::NewGlobal {
                    name: x.name.clone(),
                    ty: x.ty.clone(),
                },
            }),
            Some(entry) => {
                if x.name != entry.value.name {
                    out.push(Change {
                        va,
                        file: entry.source.clone(),
                        kind: ChangeKind::GlobalRename {
                            old: entry.value.name.clone(),
                            new: x.name.clone(),
                        },
                    });
                }
                if !type_opt_eq(&x.ty, &entry.value.ty) {
                    out.push(Change {
                        va,
                        file: entry.source.clone(),
                        kind: ChangeKind::GlobalRetype {
                            old: entry.value.ty.clone(),
                            new: x.ty.clone(),
                        },
                    });
                }
            }
        }
    }
    for (&va, entry) in &cat.globals {
        if !xml_vas.contains(&va) {
            out.push(Change {
                va,
                file: entry.source.clone(),
                kind: ChangeKind::RemovedGlobal {
                    name: entry.value.name.clone(),
                    ty: entry.value.ty.clone(),
                },
            });
        }
    }
}

/// Canonicalise text for plate / inline-comment comparison. Both directions
/// of the round-trip differ in trailing whitespace handling — XML's
/// `trim_text` strips it; TOML's `"""..."""` preserves a trailing `\n` —
/// so an unchanged value would otherwise be flagged as a diff on every run.
fn normalise_text(s: Option<&str>) -> Option<String> {
    s.map(|t| t.trim_end().to_string())
}

fn inline_comments_equal(a: &[InlineComment], b: &[InlineComment]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| x.va == y.va && x.kind == y.kind && x.text.trim_end() == y.text.trim_end())
}

/// Type-string equality modulo C-style `const` / `volatile` qualifiers.
/// Ghidra strips these on export, so a TOML param spelled `const Foo *`
/// must compare equal to an XML param spelled `Foo *`. Without this the
/// importer would propose to overwrite every const-annotated TOML entry
/// after every Ghidra round trip.
fn types_equal_modulo_cv(a: &str, b: &str) -> bool {
    a == b || ghidra_type(a) == ghidra_type(b)
}

fn type_opt_eq(a: &Option<TypeRef>, b: &Option<TypeRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => types_equal_modulo_cv(x, y),
        _ => false,
    }
}

fn params_equal_modulo_cv(a: &[Param], b: &[Param]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.name == y.name && x.storage == y.storage && types_equal_modulo_cv(&x.ty, &y.ty)
    })
}

fn locals_equal_modulo_cv(a: &[Local], b: &[Local]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.name == y.name && x.stack_offset == y.stack_offset && types_equal_modulo_cv(&x.ty, &y.ty)
    })
}

/// Stable secondary sort key so changes for the same VA appear in a
/// predictable order in diff output.
fn change_rank(k: &ChangeKind) -> u8 {
    match k {
        ChangeKind::NewFunction { .. } => 0,
        ChangeKind::FunctionRename { .. } => 1,
        ChangeKind::FunctionReturns { .. } => 2,
        ChangeKind::FunctionParams { .. } => 3,
        ChangeKind::FunctionLocals { .. } => 4,
        ChangeKind::FunctionPlateComment { .. } => 5,
        ChangeKind::FunctionComments { .. } => 6,
        ChangeKind::FunctionCallingConvention { .. } => 7,
        ChangeKind::FunctionNoReturn { .. } => 8,
        ChangeKind::FunctionCustomStorage { .. } => 9,
        ChangeKind::RemovedFunction { .. } => 10,
        ChangeKind::NewLabel { .. } => 20,
        ChangeKind::LabelRename { .. } => 21,
        ChangeKind::RemovedLabel { .. } => 22,
        ChangeKind::NewGlobal { .. } => 30,
        ChangeKind::GlobalRename { .. } => 31,
        ChangeKind::GlobalRetype { .. } => 32,
        ChangeKind::RemovedGlobal { .. } => 33,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml_io::OwnedEntry;

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

    fn empty_cat() -> Catalog {
        Catalog::default()
    }

    fn re() -> &'static Path {
        Path::new("re")
    }

    #[test]
    fn function_rename_is_detected() {
        let xml = XmlProgram {
            functions: vec![make_fn(0x500, "new_name")],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x500,
            OwnedEntry {
                value: make_fn(0x500, "old_name"),
                source: PathBuf::from("re/x.toml"),
            },
        );
        let changes = diff(&xml, &cat, re());
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].kind,
            ChangeKind::FunctionRename {
                old: "old_name".into(),
                new: "new_name".into(),
            },
        );
        assert_eq!(changes[0].file, PathBuf::from("re/x.toml"));
    }

    #[test]
    fn function_local_retype_is_detected() {
        let mut x = make_fn(0x500, "f");
        x.local.push(Local {
            name: "entity_idx".into(),
            ty: "int".into(),
            stack_offset: -0x10,
        });
        let mut t = make_fn(0x500, "f");
        t.local.push(Local {
            name: "iVar3".into(),
            ty: "undefined4".into(),
            stack_offset: -0x10,
        });

        let xml = XmlProgram {
            functions: vec![x],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x500,
            OwnedEntry {
                value: t,
                source: PathBuf::from("re/x.toml"),
            },
        );
        let changes = diff(&xml, &cat, re());
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0].kind, ChangeKind::FunctionLocals { .. }));
    }

    #[test]
    fn cv_qualifier_only_param_difference_is_not_a_change() {
        // Ghidra exports cv-stripped types; TOML keeps `const Foo *` as
        // hand-annotated intent. The diff must treat them as equal so the
        // round trip doesn't constantly propose to delete the const.
        let mut x = make_fn(0x500, "f");
        x.calling_convention = Some("__stdcall".into());
        x.signature = Some(Signature {
            returns: "void".into(),
            return_storage: None,
        });
        x.param.push(Param {
            name: "p".into(),
            ty: "WeaponFireParams *".into(),
            storage: None,
        });
        let mut t = make_fn(0x500, "f");
        t.calling_convention = Some("__stdcall".into());
        t.signature = Some(Signature {
            returns: "void".into(),
            return_storage: None,
        });
        t.param.push(Param {
            name: "p".into(),
            ty: "const WeaponFireParams *".into(),
            storage: None,
        });

        let xml = XmlProgram {
            functions: vec![x],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x500,
            OwnedEntry {
                value: t,
                source: PathBuf::from("re/x.toml"),
            },
        );
        assert!(diff(&xml, &cat, re()).is_empty());
    }

    #[test]
    fn plate_comment_ignores_trailing_whitespace() {
        // The XML parser strips trailing whitespace; TOML's `"""..."""` always
        // carries a trailing newline. The diff must not see this as a change.
        let mut x = make_fn(0x500, "f");
        x.plate_comment = Some("Hello world.".into());
        let mut t = make_fn(0x500, "f");
        t.plate_comment = Some("Hello world.\n".into());

        let xml = XmlProgram {
            functions: vec![x],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x500,
            OwnedEntry {
                value: t,
                source: PathBuf::from("re/x.toml"),
            },
        );
        assert!(diff(&xml, &cat, re()).is_empty());
    }

    #[test]
    fn inline_comment_ignores_trailing_whitespace() {
        let mut x = make_fn(0x500, "f");
        x.comment.push(InlineComment {
            va: 0x520,
            kind: CommentKind::Eol,
            text: "stub release".into(),
        });
        let mut t = make_fn(0x500, "f");
        t.comment.push(InlineComment {
            va: 0x520,
            kind: CommentKind::Eol,
            text: "stub release\n".into(),
        });

        let xml = XmlProgram {
            functions: vec![x],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x500,
            OwnedEntry {
                value: t,
                source: PathBuf::from("re/x.toml"),
            },
        );
        assert!(diff(&xml, &cat, re()).is_empty());
    }

    #[test]
    fn new_label_lands_in_class_shard_when_classified() {
        let xml = XmlProgram {
            labels: vec![Label {
                va: 0x601000,
                name: "Foo__entry".into(),
            }],
            ..Default::default()
        };
        let changes = diff(&xml, &empty_cat(), re());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].file, PathBuf::from("re/Foo.toml"));
        assert!(matches!(changes[0].kind, ChangeKind::NewLabel { .. }));
    }

    #[test]
    fn new_label_falls_back_to_labels_toml() {
        let xml = XmlProgram {
            labels: vec![Label {
                va: 0x601000,
                name: "loop_top".into(),
            }],
            ..Default::default()
        };
        let changes = diff(&xml, &empty_cat(), re());
        assert_eq!(changes[0].file, PathBuf::from("re/labels.toml"));
    }

    #[test]
    fn global_retype_and_rename_are_separate_changes() {
        let xml = XmlProgram {
            globals: vec![Global {
                va: 0x800000,
                name: "g_world".into(),
                ty: Some("GameWorld *".into()),
                comment: None,
            }],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.globals.insert(
            0x800000,
            OwnedEntry {
                value: Global {
                    va: 0x800000,
                    name: "g_World".into(),
                    ty: Some("void *".into()),
                    comment: None,
                },
                source: PathBuf::from("re/globals.toml"),
            },
        );
        let changes = diff(&xml, &cat, re());
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::GlobalRename { .. }))
        );
        assert!(
            changes
                .iter()
                .any(|c| matches!(c.kind, ChangeKind::GlobalRetype { .. }))
        );
    }

    #[test]
    fn label_to_global_at_same_va_surfaces_as_two_changes() {
        // TOML has an untyped label; user gave it a type in Ghidra → resolve()
        // promoted it to a typed global at the same VA.
        let xml = XmlProgram {
            globals: vec![Global {
                va: 0x800000,
                name: "g_world".into(),
                ty: Some("GameWorld *".into()),
                comment: None,
            }],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.labels.insert(
            0x800000,
            OwnedEntry {
                value: Label {
                    va: 0x800000,
                    name: "g_world".into(),
                },
                source: PathBuf::from("re/labels.toml"),
            },
        );
        // The label and global live in separate files; apply needs to edit
        // both independently. Diff surfaces them as two raw changes; the
        // renderer is free to fold them for display.
        let changes = diff(&xml, &cat, re());
        assert_eq!(changes.len(), 2);
        let kinds: Vec<&ChangeKind> = changes.iter().map(|c| &c.kind).collect();
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, ChangeKind::RemovedLabel { name } if name == "g_world")),
        );
        assert!(
            kinds.iter().any(|k| matches!(
                k,
                ChangeKind::NewGlobal { name, ty } if name == "g_world" && ty.as_deref() == Some("GameWorld *"),
            )),
        );
    }

    #[test]
    fn function_create_and_delete_are_reported_but_not_actionable() {
        let xml = XmlProgram {
            functions: vec![make_fn(0x500, "new_fn")],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x600,
            OwnedEntry {
                value: make_fn(0x600, "old_fn"),
                source: PathBuf::from("re/x.toml"),
            },
        );
        let changes = diff(&xml, &cat, re());
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| !c.actionable()));
    }

    #[test]
    fn unchanged_function_produces_no_change() {
        let f = make_fn(0x500, "f");
        let xml = XmlProgram {
            functions: vec![f.clone()],
            ..Default::default()
        };
        let mut cat = empty_cat();
        cat.functions.insert(
            0x500,
            OwnedEntry {
                value: f,
                source: PathBuf::from("re/x.toml"),
            },
        );
        assert!(diff(&xml, &cat, re()).is_empty());
    }
}
