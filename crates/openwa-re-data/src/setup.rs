//! Per-machine configuration for the `openwa-re` tool.
//!
//! Persisted at `<repo>/.openwa/setup.toml`. The `openwa-re setup` wizard
//! writes it; `openwa-re import` / `openwa-re export` read it to default
//! their scratch-dir argument.
//!
//! `.openwa/` is gitignored — every contributor runs `openwa-re setup` once
//! per machine to record their local Ghidra install / project / scratch dir.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Relative path under the repo root.
pub const SETUP_DIR: &str = ".openwa";
pub const SETUP_FILE: &str = "setup.toml";
/// Default scratch-dir relative to repo root (used when wizard accepts the
/// default and when consumers want to discover it without loading `setup.toml`).
pub const DEFAULT_SCRATCH_REL: &str = ".openwa/scratch";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SetupConfig {
    /// Worms Armageddon install directory — the folder that contains `WA.exe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_dir: Option<PathBuf>,

    /// Ghidra installation root — the folder that contains `ghidraRun.bat`
    /// and `support/analyzeHeadless.bat`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghidra_install: Option<PathBuf>,

    /// Ghidra project location. Two parts because `analyzeHeadless` takes
    /// them separately: `<parent_dir> <project_name>` (no `.gpr` suffix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghidra_project_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghidra_project_name: Option<String>,

    /// Scratch directory shared with Ghidra — `OpenWA{Export,Import}.java`
    /// reads/writes `wa_export.xml` / `wa_export_extras.json` / `wa_import.json`
    /// here. Defaults to `<repo>/.openwa/scratch` (auto-created, gitignored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_dir: Option<PathBuf>,
}

impl SetupConfig {
    pub fn load(repo_root: &Path) -> Result<Option<Self>> {
        let path = setup_path(repo_root);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let cfg: SetupConfig =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(cfg))
    }

    pub fn save(&self, repo_root: &Path) -> Result<PathBuf> {
        let dir = repo_root.join(SETUP_DIR);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(SETUP_FILE);
        let mut raw = String::from(SETUP_HEADER);
        raw.push_str(&toml::to_string_pretty(self).with_context(|| "serialising setup.toml")?);
        std::fs::write(&path, raw).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }

    /// Effective scratch dir — explicit override falls back to the standard
    /// `<repo>/.openwa/scratch`. Resolves to an absolute path under
    /// `repo_root` (relative entries in `setup.toml` are repo-relative).
    pub fn effective_scratch_dir(&self, repo_root: &Path) -> PathBuf {
        match &self.scratch_dir {
            Some(p) if p.is_absolute() => p.clone(),
            Some(p) => repo_root.join(p),
            None => repo_root.join(DEFAULT_SCRATCH_REL),
        }
    }
}

pub fn setup_path(repo_root: &Path) -> PathBuf {
    repo_root.join(SETUP_DIR).join(SETUP_FILE)
}

/// Convenience for callers that want to require setup to exist with a
/// helpful error message.
pub fn require(repo_root: &Path) -> Result<SetupConfig> {
    match SetupConfig::load(repo_root)? {
        Some(cfg) => Ok(cfg),
        None => bail!(
            "no setup found at {}. Run `openwa-re setup` first.",
            setup_path(repo_root).display(),
        ),
    }
}

const SETUP_HEADER: &str = "\
# Per-machine configuration for the `openwa-re` tool.
# Written by `openwa-re setup`. Safe to hand-edit. Gitignored.

";
