//! Loader for `crates/openwa-dll/hooks/*.toml`.
//!
//! Each TOML file declares a list of `[[hook]]` entries, one per WA function
//! the DLL replaces. The set is opt-in (not every catalogued function gets
//! hooked) — listing a function here is what makes `emit_trampolines` emit a
//! detour signature check, an optional `ORIG_*` atomic, and (for usercall
//! callees) a naked-asm trampoline.
//!
//! Files are split per subsystem (`engine.toml`, `entity.toml`, …) to mirror
//! `re/**/*.toml` and `crates/openwa-dll/src/replacements/*.rs`. They merge
//! into one in-memory list keyed by `wa_function`; duplicates across files
//! are a build error.
//!
//! Minimal schema:
//!
//! ```toml
//! [[hook]]
//! wa_function    = "GameRuntime__InitFrameDelay"
//! rust_impl      = "openwa_game::engine::main_loop::dispatch_frame::init_frame_delay_impl"
//! save_original  = false                  # optional, default false
//! preserve_registers = []                 # optional, default []. Special: "all".
//! ```
//!
//! Resolution of the TOML function's calling convention, params, and storage
//! happens later (at emit time) by joining `wa_function` against the
//! `Catalog`. The hook map itself is intentionally tiny.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookEntry {
    /// Matches a `[[function]].name` in `re/**/*.toml`. The codegen joins on
    /// this to recover calling convention, params, return type, custom_storage.
    pub wa_function: String,
    /// Fully-qualified Rust path to the impl function. For pure-convention
    /// hooks, this is the `extern "X" fn` detour itself. For usercall hooks,
    /// this is the `extern "cdecl"` impl the generated trampoline forwards
    /// to.
    pub rust_impl: String,
    /// When `true`, codegen emits a `static ORIG_<wa_function>: AtomicU32` and
    /// a typed `call_original_<wa_function>` wrapper. The install helper
    /// stores MinHook's returned trampoline pointer into the atomic.
    #[serde(default)]
    pub save_original: bool,
    /// Registers the WA caller may rely on across the call. The trampoline
    /// pushes/pops these around the cdecl impl call. Accepts either an array
    /// of lowercase reg names or the string `"all"` (= `["eax", "ecx",
    /// "edx"]`). Only meaningful for usercall hooks.
    #[serde(default)]
    pub preserve_registers: PreserveSpec,
}

/// `preserve_registers` accepts `["ecx", "edx"]` or `"all"`. Modeled as an
/// enum so the parser doesn't need a custom deserializer.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(untagged)]
pub enum PreserveSpec {
    #[default]
    None,
    All(AllToken),
    List(Vec<String>),
}

/// String marker — only the literal `"all"` deserializes. Anything else
/// surfaces as a parser error before reaching `PreserveSpec::List`.
#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct AllToken;

impl TryFrom<String> for AllToken {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        if s == "all" {
            Ok(AllToken)
        } else {
            Err(format!("expected \"all\", got {s:?}"))
        }
    }
}

impl PreserveSpec {
    /// Resolved list of register names (lowercase). `"all"` expands to
    /// `["eax", "ecx", "edx"]`.
    pub fn resolved(&self) -> Vec<String> {
        match self {
            PreserveSpec::None => Vec::new(),
            PreserveSpec::All(_) => vec!["eax".into(), "ecx".into(), "edx".into()],
            PreserveSpec::List(v) => v.iter().map(|s| s.to_lowercase()).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HookFile {
    #[serde(default)]
    hook: Vec<HookEntry>,
}

/// One entry tagged with its source path — same pattern as `Catalog::OwnedEntry`.
#[derive(Debug)]
pub struct OwnedHook {
    pub value: HookEntry,
    pub source: PathBuf,
}

#[derive(Debug, Default)]
pub struct HookMap {
    /// Keyed by `wa_function` for O(1) join against the RE catalog.
    pub hooks: HashMap<String, OwnedHook>,
}

impl HookMap {
    /// Load every `*.toml` directly under `dir` (non-recursive by design — a
    /// subdirectory in `hooks/` would be unusual; flat is greppable).
    pub fn load_from(dir: &Path) -> Result<Self> {
        let mut map = HookMap::default();
        if !dir.exists() {
            return Ok(map);
        }
        let entries =
            std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
        let mut paths: Vec<PathBuf> = Vec::new();
        for e in entries {
            let e = e?;
            let p = e.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("toml") {
                paths.push(p);
            }
        }
        // Deterministic order so generated output is stable across platforms.
        paths.sort();
        for p in paths {
            map.merge_file(&p)
                .with_context(|| format!("loading hook map {}", p.display()))?;
        }
        Ok(map)
    }

    fn merge_file(&mut self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)?;
        let file: HookFile = toml::from_str(&text)
            .with_context(|| format!("parsing hook map {}", path.display()))?;
        for h in file.hook {
            // Reject collisions immediately — co-locating hooks under one
            // subsystem prevents accidental double-declaration within a file,
            // but two files claiming the same WA function is a bug.
            if let Some(prev) = self.hooks.get(&h.wa_function) {
                bail!(
                    "duplicate hook for {:?}: declared in {} and {}",
                    h.wa_function,
                    prev.source.display(),
                    path.display(),
                );
            }
            self.hooks.insert(
                h.wa_function.clone(),
                OwnedHook {
                    value: h,
                    source: path.to_path_buf(),
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn loads_multi_file_and_merges() {
        let tmp = tempdir().unwrap();
        write(
            tmp.path(),
            "sound.toml",
            r#"
[[hook]]
wa_function = "IsSoundSuppressed"
rust_impl = "crate::replacements::sound::is_sound_suppressed_detour"
"#,
        );
        write(
            tmp.path(),
            "engine.toml",
            r#"
[[hook]]
wa_function = "GameRuntime__InitFrameDelay"
rust_impl = "openwa_game::engine::main_loop::dispatch_frame::init_frame_delay_impl"
preserve_registers = ["ecx"]
"#,
        );
        let map = HookMap::load_from(tmp.path()).unwrap();
        assert_eq!(map.hooks.len(), 2);
        assert!(map.hooks.contains_key("IsSoundSuppressed"));
        let init = &map.hooks["GameRuntime__InitFrameDelay"].value;
        assert_eq!(init.preserve_registers.resolved(), vec!["ecx"]);
    }

    #[test]
    fn duplicate_hook_across_files_is_error() {
        let tmp = tempdir().unwrap();
        write(
            tmp.path(),
            "a.toml",
            r#"
[[hook]]
wa_function = "Foo"
rust_impl = "x::foo"
"#,
        );
        write(
            tmp.path(),
            "b.toml",
            r#"
[[hook]]
wa_function = "Foo"
rust_impl = "y::foo"
"#,
        );
        let err = HookMap::load_from(tmp.path()).unwrap_err();
        let chain: String = err
            .chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(" / ");
        assert!(chain.contains("duplicate hook"), "got: {chain}");
    }

    #[test]
    fn preserve_all_expands() {
        let tmp = tempdir().unwrap();
        write(
            tmp.path(),
            "h.toml",
            r#"
[[hook]]
wa_function = "Foo"
rust_impl = "x"
preserve_registers = "all"
"#,
        );
        let map = HookMap::load_from(tmp.path()).unwrap();
        assert_eq!(
            map.hooks["Foo"].value.preserve_registers.resolved(),
            vec!["eax", "ecx", "edx"],
        );
    }

    #[test]
    fn missing_dir_yields_empty_map() {
        let tmp = tempdir().unwrap();
        let map = HookMap::load_from(&tmp.path().join("nope")).unwrap();
        assert!(map.hooks.is_empty());
    }
}
