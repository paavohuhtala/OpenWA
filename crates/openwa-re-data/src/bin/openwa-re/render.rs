//! Human-readable rendering of an incremental [`Change`] list.
//!
//! Grouped by destination file (one section per file), then by VA, so the
//! output reads like a per-file changelog. Function create/delete are split
//! into a `report-only` section since they aren't applied by `import` yet.

use openwa_re_data::diff::{Change, ChangeKind};
use openwa_re_data::model::{InlineComment, Local, Param, Va};
use openwa_re_data::toml_io::Catalog;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

pub fn render(changes: &[Change], re_dir: &Path, cat: &Catalog) -> String {
    if changes.is_empty() {
        return "No changes — `re/` is in sync with Ghidra.\n".to_string();
    }

    let mut out = String::new();
    let (actionable, report_only): (Vec<&Change>, Vec<&Change>) =
        changes.iter().partition(|c| c.actionable());

    // ─── Per-file actionable sections ────────────────────────────────────────
    let mut by_file: BTreeMap<&Path, Vec<&Change>> = BTreeMap::new();
    for c in &actionable {
        by_file.entry(c.file.as_path()).or_default().push(c);
    }

    for (file, items) in &by_file {
        let _ = writeln!(out, "=== {} ===", relative(file, re_dir).display());
        render_file_section(&mut out, items, cat);
        out.push('\n');
    }

    // ─── Report-only section (function creates/deletes) ──────────────────────
    if !report_only.is_empty() {
        out.push_str("=== report-only (not applied) ===\n");
        for c in &report_only {
            render_report_only(&mut out, c, re_dir);
        }
        out.push('\n');
    }

    render_summary(&mut out, changes);
    out
}

fn render_file_section(out: &mut String, items: &[&Change], cat: &Catalog) {
    // Group consecutive changes targeting the same (VA, entity-kind) so a
    // multi-field function update collapses under a single header.
    let mut i = 0;
    while i < items.len() {
        let head = items[i];
        let group_kind = entity_kind(&head.kind);
        let mut j = i + 1;
        while j < items.len() && items[j].va == head.va && entity_kind(&items[j].kind) == group_kind
        {
            j += 1;
        }
        render_group(out, &items[i..j], cat);
        i = j;
    }
}

#[derive(PartialEq, Eq)]
enum EntityKind {
    Function,
    Label,
    Global,
}

fn entity_kind(k: &ChangeKind) -> EntityKind {
    use ChangeKind::*;
    match k {
        NewFunction { .. }
        | RemovedFunction { .. }
        | FunctionRename { .. }
        | FunctionPlateComment { .. }
        | FunctionReturns { .. }
        | FunctionParams { .. }
        | FunctionLocals { .. }
        | FunctionComments { .. }
        | FunctionCallingConvention { .. }
        | FunctionNoReturn { .. }
        | FunctionCustomStorage { .. } => EntityKind::Function,
        NewLabel { .. } | RemovedLabel { .. } | LabelRename { .. } => EntityKind::Label,
        NewGlobal { .. } | RemovedGlobal { .. } | GlobalRename { .. } | GlobalRetype { .. } => {
            EntityKind::Global
        }
    }
}

fn render_group(out: &mut String, group: &[&Change], cat: &Catalog) {
    let head = group[0];
    let va = head.va;

    // Pick the header sigil + label per entity kind.
    let (sigil, kind_label, name) = header_for(group, cat);
    let _ = writeln!(out, "  {sigil} {kind_label} 0x{va:08X}  {name}");

    for c in group {
        render_change_detail(out, c);
    }
}

/// Pick a `(sigil, label, name)` for the group header.
///
/// Sigils: `+` add, `-` remove, `~` modify.
fn header_for(group: &[&Change], cat: &Catalog) -> (&'static str, &'static str, String) {
    use ChangeKind::*;
    // Special-case: a pure rename (no other field changes) gets the new name.
    let pure_rename = |group: &[&Change], pred: fn(&ChangeKind) -> bool| -> Option<String> {
        if group.len() != 1 {
            return None;
        }
        if !pred(&group[0].kind) {
            return None;
        }
        match &group[0].kind {
            FunctionRename { new, .. } | LabelRename { new, .. } | GlobalRename { new, .. } => {
                Some(new.clone())
            }
            _ => None,
        }
    };

    for c in group {
        match &c.kind {
            NewFunction { name } => return ("+", "fn", name.clone()),
            RemovedFunction { name } => return ("-", "fn", name.clone()),
            NewLabel { name } => return ("+", "label", name.clone()),
            RemovedLabel { name } => return ("-", "label", name.clone()),
            NewGlobal { name, .. } => return ("+", "global", name.clone()),
            RemovedGlobal { name, .. } => return ("-", "global", name.clone()),
            _ => {}
        }
    }
    // No create/remove → modification group.
    let kind = entity_kind(&group[0].kind);
    let label = match kind {
        EntityKind::Function => "fn",
        EntityKind::Label => "label",
        EntityKind::Global => "global",
    };
    let name = pure_rename(group, |k| {
        matches!(
            k,
            FunctionRename { .. } | LabelRename { .. } | GlobalRename { .. }
        )
    })
    .or_else(|| current_name_from_group(group))
    .unwrap_or_else(|| catalog_name_for(group[0].va, kind, cat).unwrap_or_default());
    ("~", label, name)
}

/// If the group contains a rename, prefer the new name. Used when the group
/// has other fields changing alongside the rename.
fn current_name_from_group(group: &[&Change]) -> Option<String> {
    use ChangeKind::*;
    for c in group {
        match &c.kind {
            FunctionRename { new, .. } | LabelRename { new, .. } | GlobalRename { new, .. } => {
                return Some(new.clone());
            }
            _ => {}
        }
    }
    None
}

/// Pull the current TOML-side name for a VA so plate-only or local-only
/// modification groups still get a meaningful header.
fn catalog_name_for(va: Va, kind: EntityKind, cat: &Catalog) -> Option<String> {
    match kind {
        EntityKind::Function => cat.functions.get(&va).map(|e| e.value.name.clone()),
        EntityKind::Label => cat.labels.get(&va).map(|e| e.value.name.clone()),
        EntityKind::Global => cat.globals.get(&va).map(|e| e.value.name.clone()),
    }
}

fn render_change_detail(out: &mut String, c: &Change) {
    use ChangeKind::*;
    match &c.kind {
        // Headers already carry the name for these — no detail line needed.
        NewFunction { .. } | RemovedFunction { .. } | NewLabel { .. } | RemovedLabel { .. } => {}

        NewGlobal { ty, .. } => {
            if let Some(t) = ty {
                let _ = writeln!(out, "      type: {t}");
            }
        }
        RemovedGlobal { ty, .. } => {
            if let Some(t) = ty {
                let _ = writeln!(out, "      (was type: {t})");
            }
        }
        FunctionRename { old, new } | LabelRename { old, new } | GlobalRename { old, new } => {
            let _ = writeln!(out, "      name: {old} → {new}");
        }
        GlobalRetype { old, new } => {
            let _ = writeln!(
                out,
                "      type: {} → {}",
                display_opt(old.as_deref()),
                display_opt(new.as_deref()),
            );
        }
        FunctionReturns { old, new } => {
            let _ = writeln!(
                out,
                "      returns: {} → {}",
                display_opt(old.as_deref()),
                display_opt(new.as_deref()),
            );
        }
        FunctionPlateComment { old, new } => {
            let _ = writeln!(
                out,
                "      plate: {} → {}",
                display_opt_quoted(old.as_deref()),
                display_opt_quoted(new.as_deref()),
            );
        }
        FunctionParams { old, new } => {
            if old.len() != new.len() {
                let _ = writeln!(out, "      params: {} → {} entries", old.len(), new.len());
            } else {
                let _ = writeln!(out, "      params:");
            }
            render_param_diff(out, old, new);
        }
        FunctionLocals { old, new } => {
            if old.len() != new.len() {
                let _ = writeln!(out, "      locals: {} → {} entries", old.len(), new.len());
            } else {
                let _ = writeln!(out, "      locals:");
            }
            render_local_diff(out, old, new);
        }
        FunctionComments { old, new } => {
            if old.len() != new.len() {
                let _ = writeln!(out, "      comments: {} → {} entries", old.len(), new.len());
            } else {
                let _ = writeln!(out, "      comments:");
            }
            render_comment_diff(out, old, new);
        }
        FunctionCallingConvention { old, new } => {
            let _ = writeln!(
                out,
                "      calling_convention: {} → {}",
                display_opt(old.as_deref()),
                display_opt(new.as_deref()),
            );
        }
        FunctionNoReturn { old, new } => {
            let _ = writeln!(out, "      no_return: {old} → {new}");
        }
        FunctionCustomStorage { old, new } => {
            let _ = writeln!(out, "      custom_storage: {old} → {new}");
        }
    }
}

/// Compact per-element diff between two `Param` lists. Skips identical
/// positions; flags trailing additions / removals.
fn render_param_diff(out: &mut String, old: &[Param], new: &[Param]) {
    let max = old.len().max(new.len());
    for i in 0..max {
        match (old.get(i), new.get(i)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                let _ = writeln!(out, "        [{i}] {} → {}", fmt_param(a), fmt_param(b),);
            }
            (None, Some(b)) => {
                let _ = writeln!(out, "        [{i}] + {}", fmt_param(b));
            }
            (Some(a), None) => {
                let _ = writeln!(out, "        [{i}] - {}", fmt_param(a));
            }
            (None, None) => {}
        }
    }
}

fn render_local_diff(out: &mut String, old: &[Local], new: &[Local]) {
    let max = old.len().max(new.len());
    for i in 0..max {
        match (old.get(i), new.get(i)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                let _ = writeln!(out, "        [{i}] {} → {}", fmt_local(a), fmt_local(b),);
            }
            (None, Some(b)) => {
                let _ = writeln!(out, "        [{i}] + {}", fmt_local(b));
            }
            (Some(a), None) => {
                let _ = writeln!(out, "        [{i}] - {}", fmt_local(a));
            }
            (None, None) => {}
        }
    }
}

fn render_comment_diff(out: &mut String, old: &[InlineComment], new: &[InlineComment]) {
    use std::collections::HashSet;
    let old_set: HashSet<(_, _, &str)> = old
        .iter()
        .map(|c| (c.va, c.kind, c.text.as_str()))
        .collect();
    let new_set: HashSet<(_, _, &str)> = new
        .iter()
        .map(|c| (c.va, c.kind, c.text.as_str()))
        .collect();
    for c in new {
        if !old_set.contains(&(c.va, c.kind, c.text.as_str())) {
            let _ = writeln!(
                out,
                "        + 0x{:08X} ({:?}) {}",
                c.va,
                c.kind,
                truncate(&c.text, 60),
            );
        }
    }
    for c in old {
        if !new_set.contains(&(c.va, c.kind, c.text.as_str())) {
            let _ = writeln!(
                out,
                "        - 0x{:08X} ({:?}) {}",
                c.va,
                c.kind,
                truncate(&c.text, 60),
            );
        }
    }
}

fn fmt_param(p: &Param) -> String {
    match &p.storage {
        Some(s) => format!("{}: {} @{s}", p.name, p.ty),
        None => format!("{}: {}", p.name, p.ty),
    }
}

fn fmt_local(l: &Local) -> String {
    format!("{}: {} @{:#x}", l.name, l.ty, l.stack_offset)
}

fn display_opt(s: Option<&str>) -> String {
    match s {
        Some(s) => s.to_string(),
        None => "<none>".to_string(),
    }
}

fn display_opt_quoted(s: Option<&str>) -> String {
    match s {
        Some(s) => format!("\"{}\"", truncate(s, 60)),
        None => "<none>".to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', "\\n");
    if s.chars().count() <= max {
        s
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

fn render_report_only(out: &mut String, c: &Change, re_dir: &Path) {
    use ChangeKind::*;
    let rel = relative(&c.file, re_dir);
    match &c.kind {
        NewFunction { name } => {
            let _ = writeln!(
                out,
                "  + fn 0x{:08X}  {name}  (would land in {})",
                c.va,
                rel.display(),
            );
        }
        RemovedFunction { name } => {
            let _ = writeln!(
                out,
                "  - fn 0x{:08X}  {name}  (currently in {})",
                c.va,
                rel.display(),
            );
        }
        _ => {} // only function create/delete are report-only today
    }
}

fn render_summary(out: &mut String, changes: &[Change]) {
    use ChangeKind::*;
    let mut fn_modified = 0usize;
    let mut fn_new = 0usize;
    let mut fn_removed = 0usize;
    let mut lbl_new = 0usize;
    let mut lbl_rename = 0usize;
    let mut lbl_removed = 0usize;
    let mut gbl_new = 0usize;
    let mut gbl_modified = 0usize;
    let mut gbl_removed = 0usize;

    // Count function modifications as one-per-VA, not one-per-field.
    let mut modified_function_vas = std::collections::HashSet::new();
    let mut modified_global_vas = std::collections::HashSet::new();

    for c in changes {
        match &c.kind {
            NewFunction { .. } => fn_new += 1,
            RemovedFunction { .. } => fn_removed += 1,
            FunctionRename { .. }
            | FunctionPlateComment { .. }
            | FunctionReturns { .. }
            | FunctionParams { .. }
            | FunctionLocals { .. }
            | FunctionComments { .. }
            | FunctionCallingConvention { .. }
            | FunctionNoReturn { .. }
            | FunctionCustomStorage { .. } => {
                if modified_function_vas.insert(c.va) {
                    fn_modified += 1;
                }
            }
            NewLabel { .. } => lbl_new += 1,
            RemovedLabel { .. } => lbl_removed += 1,
            LabelRename { .. } => lbl_rename += 1,
            NewGlobal { .. } => gbl_new += 1,
            RemovedGlobal { .. } => gbl_removed += 1,
            GlobalRename { .. } | GlobalRetype { .. } => {
                if modified_global_vas.insert(c.va) {
                    gbl_modified += 1;
                }
            }
        }
    }

    let files: std::collections::HashSet<&Path> =
        changes.iter().map(|c| c.file.as_path()).collect();

    out.push_str("Summary\n");
    let _ = writeln!(
        out,
        "  files affected: {}, changes: {}",
        files.len(),
        changes.len()
    );
    let _ = writeln!(
        out,
        "  functions:  {fn_modified} modified  ({fn_new} new, {fn_removed} removed — report-only)"
    );
    let _ = writeln!(
        out,
        "  labels:     {lbl_new} new, {lbl_rename} renamed, {lbl_removed} removed",
    );
    let _ = writeln!(
        out,
        "  globals:    {gbl_modified} modified, {gbl_new} new, {gbl_removed} removed"
    );
    if fn_new + fn_removed > 0 {
        out.push('\n');
        out.push_str("NOTE: function create/delete are reported but not applied in this phase.\n");
    }
}

fn relative(file: &Path, re_dir: &Path) -> PathBuf {
    match file.strip_prefix(re_dir) {
        Ok(p) => Path::new("re").join(p),
        Err(_) => file.to_path_buf(),
    }
}
