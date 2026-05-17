//! Apply an incremental [`Change`] list to disk.
//!
//! Per-file: load the existing `ReFile` (or start empty), mutate the entries
//! the changes touch, then re-emit through [`crate::emit::write_re_file`] for
//! deterministic ordering and formatting. Writes go through a temp file +
//! rename so a crash mid-write can't leave a half-written shard on disk.
//!
//! Non-actionable changes (function create / delete, in this phase) are
//! counted and skipped — callers decide whether to surface them.

use crate::diff::{Change, ChangeKind};
use crate::emit::write_re_file;
use crate::model::*;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ApplyStats {
    /// Files whose contents changed (written to disk).
    pub files_written: usize,
    /// Files created by this run (didn't exist before).
    pub files_created: usize,
    /// Files removed by this run (became empty).
    pub files_removed: usize,
    /// Actionable changes applied to disk.
    pub changes_applied: usize,
    /// Non-actionable changes skipped (function create/delete).
    pub changes_skipped: usize,
}

/// Apply `changes` to TOML shards under `re_dir`. Returns counts; bails on
/// the first I/O / parse / model-inconsistency error.
pub fn apply(changes: &[Change], re_dir: &Path) -> Result<ApplyStats> {
    let mut stats = ApplyStats::default();

    let mut by_file: BTreeMap<PathBuf, Vec<&Change>> = BTreeMap::new();
    for c in changes {
        if !c.actionable() {
            stats.changes_skipped += 1;
            continue;
        }
        by_file.entry(c.file.clone()).or_default().push(c);
    }

    for (file, items) in by_file {
        apply_to_file(&file, &items, re_dir, &mut stats)?;
    }
    Ok(stats)
}

fn apply_to_file(
    file: &Path,
    items: &[&Change],
    re_dir: &Path,
    stats: &mut ApplyStats,
) -> Result<()> {
    let (mut rf, existed) = if file.is_file() {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        let parsed: ReFile =
            toml::from_str(&text).with_context(|| format!("parsing {}", file.display()))?;
        (parsed, true)
    } else {
        (ReFile::default(), false)
    };

    for c in items {
        apply_change(&mut rf, c)
            .with_context(|| format!("applying change at 0x{:08X} in {}", c.va, file.display()))?;
        stats.changes_applied += 1;
    }

    // Deterministic ordering for the re-emit. Functions/globals/labels by VA;
    // types by name (preserves the bootstrap convention).
    rf.function.sort_by_key(|f| f.va);
    rf.global.sort_by_key(|g| g.va);
    rf.label.sort_by_key(|l| l.va);
    rf.r#struct.sort_by(|a, b| a.name.cmp(&b.name));
    rf.union.sort_by(|a, b| a.name.cmp(&b.name));
    rf.r#enum.sort_by(|a, b| a.name.cmp(&b.name));
    rf.typedef.sort_by(|a, b| a.name.cmp(&b.name));
    rf.function_def.sort_by(|a, b| a.name.cmp(&b.name));

    if is_empty_re_file(&rf) {
        if existed {
            std::fs::remove_file(file)
                .with_context(|| format!("removing empty {}", file.display()))?;
            stats.files_removed += 1;
        }
        return Ok(());
    }

    let contents = write_re_file(&rf);

    // Avoid touching files that didn't actually change — useful when an
    // apply pass is dominated by no-op renames or compounded diffs.
    if existed {
        let current = std::fs::read_to_string(file).unwrap_or_default();
        if current == contents {
            return Ok(());
        }
    } else {
        stats.files_created += 1;
    }

    write_atomic(file, &contents, re_dir)?;
    stats.files_written += 1;
    Ok(())
}

fn apply_change(rf: &mut ReFile, c: &Change) -> Result<()> {
    use ChangeKind::*;
    match &c.kind {
        // Functions: locate by VA, replace the relevant field.
        FunctionRename { new, .. } => find_function_mut(rf, c.va)?.name = new.clone(),
        FunctionPlateComment { new, .. } => {
            find_function_mut(rf, c.va)?.plate_comment = new.clone()
        }
        FunctionReturns { new, .. } => {
            let f = find_function_mut(rf, c.va)?;
            f.signature = new.clone().map(|returns| Signature {
                returns,
                return_storage: f.signature.as_ref().and_then(|s| s.return_storage.clone()),
            });
        }
        FunctionParams { new, .. } => find_function_mut(rf, c.va)?.param = new.clone(),
        FunctionLocals { new, .. } => find_function_mut(rf, c.va)?.local = new.clone(),
        FunctionComments { new, .. } => find_function_mut(rf, c.va)?.comment = new.clone(),
        FunctionCallingConvention { new, .. } => {
            find_function_mut(rf, c.va)?.calling_convention = new.clone()
        }
        FunctionNoReturn { new, .. } => find_function_mut(rf, c.va)?.no_return = *new,
        FunctionCustomStorage { new, .. } => find_function_mut(rf, c.va)?.custom_storage = *new,

        // Labels.
        NewLabel { name } => rf.label.push(Label {
            va: c.va,
            name: name.clone(),
        }),
        RemovedLabel { .. } => rf.label.retain(|l| l.va != c.va),
        LabelRename { new, .. } => {
            for l in rf.label.iter_mut() {
                if l.va == c.va {
                    l.name = new.clone();
                }
            }
        }

        // Globals.
        NewGlobal { name, ty } => rf.global.push(Global {
            va: c.va,
            name: name.clone(),
            ty: ty.clone(),
            comment: None,
        }),
        RemovedGlobal { .. } => rf.global.retain(|g| g.va != c.va),
        GlobalRename { new, .. } => {
            for g in rf.global.iter_mut() {
                if g.va == c.va {
                    g.name = new.clone();
                }
            }
        }
        GlobalRetype { new, .. } => {
            for g in rf.global.iter_mut() {
                if g.va == c.va {
                    g.ty = new.clone();
                }
            }
        }

        NewFunction { .. } | RemovedFunction { .. } => {
            // Caller pre-filtered actionable changes; reaching here is a bug.
            anyhow::bail!(
                "internal: non-actionable change reached apply: {:?}",
                c.kind
            );
        }
    }
    Ok(())
}

fn find_function_mut(rf: &mut ReFile, va: Va) -> Result<&mut Function> {
    rf.function
        .iter_mut()
        .find(|f| f.va == va)
        .with_context(|| format!("function 0x{va:08X} not present in target file"))
}

fn is_empty_re_file(rf: &ReFile) -> bool {
    rf.function.is_empty()
        && rf.global.is_empty()
        && rf.label.is_empty()
        && rf.r#struct.is_empty()
        && rf.union.is_empty()
        && rf.r#enum.is_empty()
        && rf.typedef.is_empty()
        && rf.function_def.is_empty()
        && rf.external_types.is_empty()
}

/// Write `contents` to `path` via a sibling temp file + rename. The temp
/// goes inside the parent so the rename is same-filesystem (fast and atomic
/// on Windows + Unix).
fn write_atomic(path: &Path, contents: &str, re_dir: &Path) -> Result<()> {
    let parent = path.parent().unwrap_or(re_dir);
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating parent dir {}", parent.display()))?;
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("openwa-re");
    let tmp = parent.join(format!(".{stem}.tmp"));
    std::fs::write(&tmp, contents).with_context(|| format!("writing temp {}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(
            anyhow::Error::from(e).context(format!("renaming temp into {}", path.display()))
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::ChangeKind;
    use tempfile::TempDir;

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

    /// Write a ReFile to disk and round-trip it through apply.
    fn seed_file(dir: &Path, name: &str, rf: &ReFile) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, write_re_file(rf)).unwrap();
        p
    }

    fn load(p: &Path) -> ReFile {
        toml::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn function_rename_round_trips_to_disk() {
        let tmp = TempDir::new().unwrap();
        let re_dir = tmp.path();
        let rf = ReFile {
            function: vec![make_fn(0x500, "old_name")],
            ..Default::default()
        };
        let file = seed_file(re_dir, "x.toml", &rf);

        let changes = vec![Change {
            va: 0x500,
            file: file.clone(),
            kind: ChangeKind::FunctionRename {
                old: "old_name".into(),
                new: "new_name".into(),
            },
        }];
        let stats = apply(&changes, re_dir).unwrap();

        assert_eq!(stats.changes_applied, 1);
        assert_eq!(stats.files_written, 1);
        assert_eq!(load(&file).function[0].name, "new_name");
    }

    #[test]
    fn new_label_creates_destination_file_when_absent() {
        let tmp = TempDir::new().unwrap();
        let re_dir = tmp.path();
        let dest = re_dir.join("labels.toml");
        assert!(!dest.exists());

        let changes = vec![Change {
            va: 0x601000,
            file: dest.clone(),
            kind: ChangeKind::NewLabel {
                name: "loop_top".into(),
            },
        }];
        let stats = apply(&changes, re_dir).unwrap();

        assert_eq!(stats.files_created, 1);
        assert_eq!(stats.files_written, 1);
        let rf = load(&dest);
        assert_eq!(rf.label.len(), 1);
        assert_eq!(rf.label[0].name, "loop_top");
    }

    #[test]
    fn removing_last_entry_deletes_the_file() {
        let tmp = TempDir::new().unwrap();
        let re_dir = tmp.path();
        let rf = ReFile {
            label: vec![Label {
                va: 0x601040,
                name: "dead".into(),
            }],
            ..Default::default()
        };
        let file = seed_file(re_dir, "labels.toml", &rf);

        let changes = vec![Change {
            va: 0x601040,
            file: file.clone(),
            kind: ChangeKind::RemovedLabel {
                name: "dead".into(),
            },
        }];
        let stats = apply(&changes, re_dir).unwrap();
        assert_eq!(stats.files_removed, 1);
        assert!(!file.exists());
    }

    #[test]
    fn no_op_apply_does_not_touch_disk() {
        let tmp = TempDir::new().unwrap();
        let re_dir = tmp.path();
        let rf = ReFile {
            function: vec![make_fn(0x500, "f")],
            ..Default::default()
        };
        let file = seed_file(re_dir, "x.toml", &rf);
        let original_mtime = std::fs::metadata(&file).unwrap().modified().unwrap();

        // Apply a "rename" that's actually a no-op (old == new on disk).
        let changes = vec![Change {
            va: 0x500,
            file: file.clone(),
            kind: ChangeKind::FunctionRename {
                old: "f".into(),
                new: "f".into(),
            },
        }];
        let stats = apply(&changes, re_dir).unwrap();
        assert_eq!(stats.changes_applied, 1);
        assert_eq!(stats.files_written, 0);

        let after_mtime = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(original_mtime, after_mtime);
    }

    #[test]
    fn function_create_change_is_skipped_not_applied() {
        let tmp = TempDir::new().unwrap();
        let re_dir = tmp.path();
        let dest = re_dir.join("any.toml");

        let changes = vec![Change {
            va: 0x500,
            file: dest.clone(),
            kind: ChangeKind::NewFunction {
                name: "ignored".into(),
            },
        }];
        let stats = apply(&changes, re_dir).unwrap();
        assert_eq!(stats.changes_skipped, 1);
        assert_eq!(stats.files_written, 0);
        assert!(!dest.exists());
    }

    #[test]
    fn label_to_global_promotion_writes_both_files() {
        let tmp = TempDir::new().unwrap();
        let re_dir = tmp.path();
        let labels_file = seed_file(
            re_dir,
            "labels.toml",
            &ReFile {
                label: vec![Label {
                    va: 0x800000,
                    name: "g_world".into(),
                }],
                ..Default::default()
            },
        );
        let globals_file = re_dir.join("globals.toml");

        let changes = vec![
            Change {
                va: 0x800000,
                file: labels_file.clone(),
                kind: ChangeKind::RemovedLabel {
                    name: "g_world".into(),
                },
            },
            Change {
                va: 0x800000,
                file: globals_file.clone(),
                kind: ChangeKind::NewGlobal {
                    name: "g_world".into(),
                    ty: Some("GameWorld *".into()),
                },
            },
        ];
        let stats = apply(&changes, re_dir).unwrap();
        assert_eq!(stats.changes_applied, 2);

        // labels.toml became empty → deleted.
        assert!(!labels_file.exists());
        // globals.toml created with the new typed global.
        let g = load(&globals_file);
        assert_eq!(g.global.len(), 1);
        assert_eq!(g.global[0].ty.as_deref(), Some("GameWorld *"));
    }
}
