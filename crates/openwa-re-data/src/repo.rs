//! Repo-root discovery and `re/` directory walking.
//!
//! `openwa-re` is repo-scoped: every subcommand operates on `<repo>/re/*.toml`
//! discovered relative to the current working directory. We locate the repo
//! root by walking up looking for the workspace `Cargo.toml` (same approach
//! `cargo` uses), then enumerate `re/*.toml`.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Walk up from `start` until a directory containing a Cargo workspace
/// `Cargo.toml` is found. Returns the repo root.
pub fn find_repo_root(start: &Path) -> Result<PathBuf> {
    let mut cur = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if is_workspace_root(&cur) {
            return Ok(cur);
        }
        match cur.parent() {
            Some(p) if p != cur => cur = p.to_path_buf(),
            _ => bail!(
                "openwa-re must run inside the OpenWA workspace; \
                 walked up from {} without finding a workspace Cargo.toml",
                start.display()
            ),
        }
    }
}

fn is_workspace_root(dir: &Path) -> bool {
    let cargo = dir.join("Cargo.toml");
    if !cargo.is_file() {
        return false;
    }
    // Cheap signature check — full TOML parse not needed.
    std::fs::read_to_string(&cargo)
        .map(|s| s.contains("[workspace]"))
        .unwrap_or(false)
}

/// Path to the `re/` directory under the repo root.
pub fn re_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("re")
}

/// Enumerate every `*.toml` under `re/`, sorted by path for determinism.
pub fn enumerate_toml(re_dir: &Path) -> Result<Vec<PathBuf>> {
    if !re_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    walk(re_dir, &mut out)
        .with_context(|| format!("enumerating TOML files under {}", re_dir.display()))?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            walk(&path, out)?;
        } else if ft.is_file() && path.extension().is_some_and(|e| e == "toml") {
            out.push(path);
        }
    }
    Ok(())
}
