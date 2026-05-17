//! Post-parse resolution pass. Ghidra's XML scatters facts across multiple
//! sections — names live in `SYMBOL_TABLE`, applied types in `DATA`, comments
//! in `COMMENTS`, prototype data in `FUNCTIONS`. We collected everything
//! independently in `xml_in::parse_file`; this pass merges by VA so each
//! function / global owns the comments, names, and stack-frame data that
//! belong to it.
//!
//! Side effect: drops labels that overlap with functions (the function name
//! takes precedence) and globals that overlap with functions (a function
//! entry point is not a data slot).

use crate::model::*;
use crate::xml_in::{RawComment, XmlProgram};
use std::collections::HashMap;

/// Apply the resolve pass to an [`XmlProgram`] in place.
pub fn resolve(prog: &mut XmlProgram) -> ResolveStats {
    let mut stats = ResolveStats::default();

    // 1. Index function and global VAs for fast lookup.
    let function_vas: HashMap<Va, usize> = prog
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.va, i))
        .collect();

    // 2. Resolve global names from labels. DEFINED_DATA gave us VA+type;
    //    SYMBOL_TABLE gave us VA+name. The two intersect at typed-and-named
    //    globals; user-named-but-untyped globals show up only as labels.
    //
    //    Merge in two phases:
    //      (a) Walk DEFINED_DATA entries: if a label matches by VA, build a
    //          typed Global; if no name found, drop (untyped data is noise).
    //      (b) Walk leftover labels: promote any non-function, non-data label
    //          at a data-address-range VA to a name-only Global.
    let mut label_name_by_va: HashMap<Va, String> = HashMap::new();
    for l in prog.labels.iter() {
        label_name_by_va.insert(l.va, l.name.clone());
    }
    let mut named_global_vas: std::collections::HashSet<Va> = std::collections::HashSet::new();
    let mut kept_globals = Vec::with_capacity(prog.globals.len());
    for mut g in std::mem::take(&mut prog.globals) {
        if function_vas.contains_key(&g.va) {
            stats.globals_dropped_function_overlap += 1;
            continue;
        }
        match label_name_by_va.get(&g.va) {
            Some(n) => {
                g.name = n.clone();
                named_global_vas.insert(g.va);
                kept_globals.push(g);
                stats.globals_resolved += 1;
            }
            None => {
                stats.globals_dropped_unnamed += 1;
            }
        }
    }

    // 4. Partition labels: function-overlap (drop), global-overlap (drop —
    //    name already on the global), data-address label (promote to
    //    untyped global), code-address label (keep as Label).
    //
    //    Heuristic for code vs data: any label VA more than
    //    `MAX_FUNCTION_REACH` past the highest function entry is data, or
    //    below the lowest function entry (PE headers / pre-.text). Inside
    //    the padded range is code.
    let (text_lo, mut text_hi) = function_va_range(&prog.functions);
    text_hi = text_hi.saturating_add(MAX_FUNCTION_REACH);
    let mut kept_labels = Vec::with_capacity(prog.labels.len());
    for l in std::mem::take(&mut prog.labels) {
        if function_vas.contains_key(&l.va) {
            stats.labels_dropped_function_overlap += 1;
            continue;
        }
        if named_global_vas.contains(&l.va) {
            stats.labels_dropped_global_overlap += 1;
            continue;
        }
        if l.va < text_lo || l.va > text_hi {
            // Data section symbol — promote to untyped global.
            kept_globals.push(Global {
                va: l.va,
                name: l.name,
                ty: None,
                comment: None,
            });
            stats.globals_promoted_from_label += 1;
            continue;
        }
        kept_labels.push(l);
        stats.labels_kept += 1;
    }
    prog.globals = kept_globals;
    prog.labels = kept_labels;

    // 4. Route comments to whichever entity owns them.
    //
    //    - Address matches a function entry → function plate (kind=Plate).
    //    - Address falls inside a function body → function inline comment.
    //    - Address matches a global → global comment (Plate text concatenated;
    //      other kinds dropped — Ghidra doesn't surface non-plate comments on
    //      data lines in practice).
    //    - Anything else → orphan (most often section-header comments).
    let mut function_starts: Vec<(Va, usize)> = prog
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.va, i))
        .collect();
    function_starts.sort_unstable_by_key(|&(va, _)| va);

    let global_idx_by_va: HashMap<Va, usize> = prog
        .globals
        .iter()
        .enumerate()
        .map(|(i, g)| (g.va, i))
        .collect();

    let mut remaining: Vec<RawComment> = Vec::new();
    for c in std::mem::take(&mut prog.comments) {
        if let Some(&gi) = global_idx_by_va.get(&c.va) {
            if matches!(c.kind, CommentKind::Plate)
                && prog.globals[gi].comment.is_none()
                && prog.functions.iter().all(|f| f.va != c.va)
            {
                prog.globals[gi].comment = Some(c.text);
                stats.comments_routed += 1;
                continue;
            }
            // Non-plate comment on a global, or plate already set — drop.
            stats.comments_orphan += 1;
            remaining.push(c);
            continue;
        }
        match owning_function(&function_starts, c.va) {
            Some(idx) => {
                // Dedup: Ghidra emits plate comments both inside <FUNCTION>
                // (`REGULAR_CMT` → `plate_comment`) AND in <COMMENTS> as a
                // standalone `<COMMENT TYPE="plate" ADDRESS="va">`. Drop the
                // standalone duplicate when it matches the function's plate.
                if matches!(c.kind, CommentKind::Plate)
                    && prog.functions[idx].va == c.va
                    && prog.functions[idx].plate_comment.as_deref() == Some(c.text.as_str())
                {
                    stats.comments_routed += 1;
                    continue;
                }
                prog.functions[idx].comment.push(InlineComment {
                    va: c.va,
                    kind: c.kind,
                    text: c.text,
                });
                stats.comments_routed += 1;
            }
            None => {
                stats.comments_orphan += 1;
                remaining.push(c);
            }
        }
    }
    prog.comments = remaining;

    // 5. Sort everything by VA / name for deterministic downstream output.
    prog.functions.sort_unstable_by_key(|f| f.va);
    prog.globals.sort_unstable_by_key(|g| g.va);
    prog.labels.sort_unstable_by_key(|l| l.va);
    prog.structs.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    prog.unions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    prog.enums.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    prog.typedefs.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    prog.function_defs
        .sort_unstable_by(|a, b| a.name.cmp(&b.name));

    // Within each function, sort comments by VA.
    for f in prog.functions.iter_mut() {
        f.comment
            .sort_unstable_by_key(|c| (c.va, comment_kind_rank(c.kind)));
    }

    stats
}

/// Pick the function whose entry VA is the largest one ≤ `va` — that's the
/// function whose body covers it. Returns `None` if `va` is before the first
/// function or further than `MAX_FUNCTION_REACH` from the nearest entry.
const MAX_FUNCTION_REACH: u32 = 1 << 16; // 64 KB

fn owning_function(sorted: &[(Va, usize)], va: Va) -> Option<usize> {
    let pos = match sorted.binary_search_by_key(&va, |&(v, _)| v) {
        Ok(i) => i,
        Err(0) => return None,
        Err(i) => i - 1,
    };
    let (entry_va, idx) = sorted[pos];
    if va.saturating_sub(entry_va) <= MAX_FUNCTION_REACH {
        Some(idx)
    } else {
        None
    }
}

fn comment_kind_rank(k: CommentKind) -> u8 {
    match k {
        CommentKind::Plate => 0,
        CommentKind::Pre => 1,
        CommentKind::Eol => 2,
        CommentKind::Post => 3,
        CommentKind::Repeatable => 4,
        CommentKind::Decompiler => 5,
    }
}

#[derive(Debug, Default)]
pub struct ResolveStats {
    pub comments_routed: usize,
    pub comments_orphan: usize,
    pub globals_resolved: usize,
    pub globals_promoted_from_label: usize,
    pub globals_dropped_unnamed: usize,
    pub globals_dropped_function_overlap: usize,
    pub labels_kept: usize,
    pub labels_dropped_function_overlap: usize,
    pub labels_dropped_global_overlap: usize,
}

fn function_va_range(functions: &[Function]) -> (Va, Va) {
    if functions.is_empty() {
        return (Va::MAX, 0);
    }
    let mut lo = Va::MAX;
    let mut hi = 0;
    for f in functions {
        if f.va < lo {
            lo = f.va;
        }
        if f.va > hi {
            hi = f.va;
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_comments_to_owning_function() {
        let mut prog = XmlProgram {
            functions: vec![Function {
                va: 0x500000,
                name: "f".into(),
                calling_convention: None,
                plate_comment: None,
                no_return: false,
                custom_storage: false,
                signature: None,
                param: vec![],
                local: vec![],
                comment: vec![],
            }],
            comments: vec![
                RawComment {
                    va: 0x500004,
                    kind: CommentKind::Eol,
                    text: "inside".into(),
                },
                RawComment {
                    va: 0x900000,
                    kind: CommentKind::Eol,
                    text: "way past".into(),
                },
            ],
            ..Default::default()
        };
        let stats = resolve(&mut prog);
        assert_eq!(stats.comments_routed, 1);
        assert_eq!(stats.comments_orphan, 1);
        assert_eq!(prog.functions[0].comment.len(), 1);
        assert_eq!(prog.comments.len(), 1);
    }

    #[test]
    fn deduplicates_label_at_function_va_and_promotes_data_label() {
        let mut prog = XmlProgram {
            functions: vec![Function {
                va: 0x500000,
                name: "f".into(),
                calling_convention: None,
                plate_comment: None,
                no_return: false,
                custom_storage: false,
                signature: None,
                param: vec![],
                local: vec![],
                comment: vec![],
            }],
            labels: vec![
                Label {
                    va: 0x500000,
                    name: "f".into(),
                },
                Label {
                    va: 0x500100,
                    name: "code_label".into(),
                },
                Label {
                    // Outside function VA range — promoted to a global.
                    va: 0x800000,
                    name: "g_GameInfo".into(),
                },
            ],
            ..Default::default()
        };
        let stats = resolve(&mut prog);
        assert_eq!(stats.labels_dropped_function_overlap, 1);
        assert_eq!(stats.labels_kept, 1);
        assert_eq!(stats.globals_promoted_from_label, 1);
        assert_eq!(prog.labels.len(), 1);
        assert_eq!(prog.labels[0].name, "code_label");
        assert_eq!(prog.globals.len(), 1);
        assert_eq!(prog.globals[0].name, "g_GameInfo");
        assert_eq!(prog.globals[0].ty, None);
    }

    #[test]
    fn resolves_global_name_and_type_from_label() {
        let mut prog = XmlProgram {
            globals: vec![Global {
                va: 0x800000,
                name: String::new(),
                ty: Some("GameSession".into()),
                comment: None,
            }],
            labels: vec![Label {
                va: 0x800000,
                name: "g_GameSession".into(),
            }],
            ..Default::default()
        };
        let stats = resolve(&mut prog);
        assert_eq!(stats.globals_resolved, 1);
        assert_eq!(prog.globals[0].name, "g_GameSession");
        assert_eq!(prog.globals[0].ty.as_deref(), Some("GameSession"));
        // Label is dropped because it overlapped the global.
        assert_eq!(stats.labels_dropped_global_overlap, 1);
        assert_eq!(prog.labels.len(), 0);
    }
}
