//! `openwa-re` — repo-scoped Ghidra metadata tool.
//!
//! Verbs are named from openwa-re's perspective: `import` brings data INTO
//! the catalog, `export` ships it OUT toward Ghidra. The Ghidra-side scripts
//! use the opposite verbs from Ghidra's perspective — they pair across the
//! boundary: `OpenWAExport.java` produces what `openwa-re import` consumes;
//! `openwa-re export` produces what `OpenWAImport.java` consumes.
//!
//! Subcommands:
//!   - `validate` — parse all `re/**/*.toml` and report schema/cross-ref errors
//!   - `import`   — read a Ghidra XML dump from a scratch dir, write `re/*.toml`
//!   - `export`   — read `re/*.toml`, write a Ghidra-bound manifest into a scratch dir
//!   - `diff`     — TODO: human-readable diff between `re/` and a given Ghidra XML
//!
//! Import/export both operate on a single "scratch dir" shared with Ghidra,
//! and use fixed file conventions inside it:
//!   - `wa_export.xml`         — XML dump produced by `OpenWAExport.java`
//!   - `wa_export_extras.json` — sidecar produced alongside the XML; carries
//!                               calling_convention, no_return, custom_storage
//!                               (attributes Ghidra's XML DTD cannot represent)
//!   - `wa_import.json`        — manifest consumed by `OpenWAImport.java`
//!
//! The extras sidecar is load-bearing: without it, custom-storage functions
//! lose their per-param ESI/EDI/stack assignments. We auto-pair it with the
//! XML on import and with the manifest on export, so users can never forget
//! to pass it.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use openwa_re_data::manifest;
use openwa_re_data::repo::{find_repo_root, re_dir};
use openwa_re_data::setup::SetupConfig;
use openwa_re_data::toml_io::Catalog;
use openwa_re_data::{apply, diff, emit, resolve, setup, validate, xml_in};
use std::path::{Path, PathBuf};

mod render;
mod wizard;

#[derive(Parser)]
#[command(name = "openwa-re", about = "OpenWA reverse-engineering metadata tool")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse all re/**/*.toml and check schema + cross-references.
    Validate,

    /// Read `<dir>/wa_export.xml` (+ `<dir>/wa_export_extras.json`) and
    /// write re/*.toml shards. Defaults `<dir>` from `.openwa/setup.toml`.
    Import {
        /// Scratch directory shared with Ghidra. Must contain
        /// `wa_export.xml` written by `OpenWAExport.java`; if a sibling
        /// `wa_export_extras.json` is present it's overlaid automatically.
        /// If omitted, defaults to the scratch dir recorded in
        /// `.openwa/setup.toml`.
        dir: Option<PathBuf>,
        /// Initial bootstrap mode: shard functions by VA range and OVERWRITE
        /// any existing files in `re/`. Required for the first dump; refuses
        /// to run later unless `--force` is passed.
        #[arg(long)]
        bootstrap: bool,
        /// Allow `--bootstrap` to overwrite a non-empty `re/` directory.
        #[arg(long)]
        force: bool,
        /// Don't write files; print the layout that would be created.
        #[arg(long)]
        dry_run: bool,
    },

    /// Read re/*.toml and emit `<dir>/wa_import.json` for `OpenWAImport.java`.
    /// Defaults `<dir>` from `.openwa/setup.toml`.
    Export {
        /// Scratch directory shared with Ghidra. The manifest is written to
        /// `<dir>/wa_import.json`; if `<dir>/wa_export_extras.json` is
        /// present, its calling_convention / no_return / custom_storage
        /// entries are overlaid onto the loaded TOML catalog before manifest
        /// emission. If omitted, defaults to the scratch dir recorded in
        /// `.openwa/setup.toml`.
        dir: Option<PathBuf>,
    },

    /// Diff committed re/ against a Ghidra XML dump. (TODO)
    Diff {
        /// Scratch directory containing `wa_export.xml`.
        dir: Option<PathBuf>,
    },

    /// Interactive per-machine setup wizard. Writes `.openwa/setup.toml` and
    /// copies the Ghidra-side scripts into `~/ghidra_scripts/`. When the
    /// configured Ghidra project doesn't exist yet, offers to bootstrap it
    /// from WA.exe via analyzeHeadless + an initial export/import round-trip.
    Setup {
        /// Re-run the wizard even if `.openwa/setup.toml` already exists.
        #[arg(long)]
        force: bool,
        /// Skip the bootstrap confirmation prompt; just run it. Useful for
        /// automation. Implies bootstrap will be attempted if the project is
        /// missing.
        #[arg(long)]
        bootstrap: bool,
        /// Inverse of `--bootstrap`: skip the bootstrap prompt entirely
        /// (config + script copy only).
        #[arg(long, conflicts_with = "bootstrap")]
        no_bootstrap: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cwd = std::env::current_dir()?;
    let root = find_repo_root(&cwd)?;
    let re = re_dir(&root);

    match cli.cmd {
        Cmd::Validate => cmd_validate(&re),
        Cmd::Import {
            dir,
            bootstrap,
            force,
            dry_run,
        } => {
            let dir = resolve_scratch_dir(&root, dir.as_deref())?;
            cmd_import(&re, &dir, bootstrap, force, dry_run)
        }
        Cmd::Export { dir } => {
            let dir = resolve_scratch_dir(&root, dir.as_deref())?;
            cmd_export(&re, &dir)
        }
        Cmd::Diff { dir } => {
            let dir = resolve_scratch_dir(&root, dir.as_deref())?;
            cmd_diff(&re, &dir)
        }
        Cmd::Setup {
            force,
            bootstrap,
            no_bootstrap,
        } => {
            let mode = if bootstrap {
                wizard::BootstrapMode::Yes
            } else if no_bootstrap {
                wizard::BootstrapMode::No
            } else {
                wizard::BootstrapMode::Prompt
            };
            wizard::run(&root, &re, force, mode)
        }
    }
}

/// Resolve a scratch dir from (a) an explicit CLI arg, or (b) `.openwa/setup.toml`.
/// Bails with an actionable error if neither is available.
fn resolve_scratch_dir(repo_root: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p.to_path_buf());
    }
    match SetupConfig::load(repo_root)? {
        Some(cfg) => Ok(cfg.effective_scratch_dir(repo_root)),
        None => anyhow::bail!(
            "no scratch dir given and {} doesn't exist. \
             Pass a directory explicitly or run `openwa-re setup`.",
            setup::setup_path(repo_root).display(),
        ),
    }
}

/// Fixed filenames inside the scratch dir shared with Ghidra.
const XML_FILE: &str = "wa_export.xml";
const EXTRAS_FILE: &str = "wa_export_extras.json";
const MANIFEST_FILE: &str = "wa_import.json";

/// Verify the scratch dir exists. Auto-creates it on `export` (we're writing
/// into it) but requires it to already exist on `import` / `diff` (we're
/// reading a Ghidra dump from it).
fn require_scratch_dir(dir: &std::path::Path, must_exist: bool) -> Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if must_exist {
        anyhow::bail!(
            "scratch dir not found: {}. Run OpenWAExport.java first.",
            dir.display(),
        );
    }
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating scratch dir {}", dir.display()))?;
    Ok(())
}

fn cmd_validate(re: &std::path::Path) -> Result<()> {
    let cat = Catalog::load_from(re)?;
    let n_files = openwa_re_data::repo::enumerate_toml(re)?.len();
    let report = validate::validate(&cat)?;

    eprintln!(
        "Parsed {} TOML file(s), {} entries.",
        n_files,
        cat.total_entries(),
    );

    if !report.warnings.is_empty() {
        eprintln!("{} warning(s):", report.warnings.len());
        let preview = report.warnings.len().min(10);
        for w in report.warnings.iter().take(preview) {
            eprintln!("  - {w}");
        }
        if report.warnings.len() > preview {
            eprintln!("  ... and {} more", report.warnings.len() - preview);
        }
    }
    if report.ok() {
        eprintln!("OK.");
        Ok(())
    } else {
        eprintln!("{} validation error(s):", report.errors.len());
        for e in &report.errors {
            eprintln!("  - {e}");
        }
        std::process::exit(1);
    }
}

fn cmd_import(
    re: &std::path::Path,
    dir: &std::path::Path,
    bootstrap: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    require_scratch_dir(dir, true)?;
    if !bootstrap {
        return cmd_import_incremental(re, dir, force, dry_run);
    }

    let xml = dir.join(XML_FILE);
    if !xml.is_file() {
        anyhow::bail!(
            "{} not found in scratch dir {}. Run OpenWAExport.java first.",
            XML_FILE,
            dir.display(),
        );
    }
    let extras_path = dir.join(EXTRAS_FILE);

    let t0 = std::time::Instant::now();
    let mut prog = xml_in::parse_file(&xml)?;
    let parse_dt = t0.elapsed();

    let extras_applied = apply_extras_sidecar(&mut prog, &extras_path)?;

    let t1 = std::time::Instant::now();
    let rstats = resolve::resolve(&mut prog);
    let resolve_dt = t1.elapsed();

    eprintln!(
        "Parsed {} in {:.2}s, resolved in {:.2}s",
        xml.display(),
        parse_dt.as_secs_f64(),
        resolve_dt.as_secs_f64(),
    );
    match extras_applied {
        Some(n) => eprintln!(
            "  extras sidecar: {} applied {} attribute set(s)",
            extras_path.display(),
            n,
        ),
        None => eprintln!(
            "  extras sidecar: {} not found — no calling-convention or no-return data",
            extras_path.display(),
        ),
    }
    eprintln!(
        "  functions: {} kept, {} dropped (auto), {} dropped (library)",
        prog.stats.functions_kept,
        prog.stats.functions_dropped_auto,
        prog.stats.functions_dropped_library,
    );
    eprintln!(
        "  types:     {} kept, {} dropped (builtin), {} dropped (placeholder), {} dropped (anonymous)",
        prog.stats.types_kept,
        prog.stats.types_dropped_builtin,
        prog.stats.types_dropped_placeholder,
        prog.stats.types_dropped_anonymous,
    );
    eprintln!(
        "  symbols:   {} kept, {} dropped (auto)",
        prog.stats.symbols_kept, prog.stats.symbols_dropped_auto,
    );
    eprintln!("  comments:  {} kept", prog.stats.comments_kept);
    eprintln!(
        "resolve: routed {} comments ({} orphan); resolved {} globals \
         ({} unnamed, {} fn-overlap); kept {} labels ({} fn-overlap, {} global-overlap)",
        rstats.comments_routed,
        rstats.comments_orphan,
        rstats.globals_resolved,
        rstats.globals_dropped_unnamed,
        rstats.globals_dropped_function_overlap,
        rstats.labels_kept,
        rstats.labels_dropped_function_overlap,
        rstats.labels_dropped_global_overlap,
    );
    eprintln!(
        "post-resolve inventory: {} fns, {} structs, {} unions, {} enums, {} typedefs, {} fn-defs, {} globals, {} labels, {} orphan comments",
        prog.functions.len(),
        prog.structs.len(),
        prog.unions.len(),
        prog.enums.len(),
        prog.typedefs.len(),
        prog.function_defs.len(),
        prog.globals.len(),
        prog.labels.len(),
        prog.comments.len(),
    );

    // Build the file plan in memory.
    let t2 = std::time::Instant::now();
    let pending = emit::bootstrap_files(&prog, re);
    let emit_dt = t2.elapsed();
    let total_bytes: usize = pending.iter().map(|p| p.contents.len()).sum();
    eprintln!(
        "Generated {} file(s), {:.1} MiB total, in {:.2}s",
        pending.len(),
        total_bytes as f64 / 1024.0 / 1024.0,
        emit_dt.as_secs_f64(),
    );
    for pf in &pending {
        let rel = pf.path.strip_prefix(re).unwrap_or(&pf.path);
        eprintln!(
            "  {:<32} {:>5} entries  {:>8.1} KiB",
            rel.display(),
            pf.entries,
            pf.contents.len() as f64 / 1024.0,
        );
    }

    if dry_run {
        eprintln!("(dry-run: no files written)");
        return Ok(());
    }

    // Safety: refuse to overwrite an already-populated `re/` unless forced.
    let existing = openwa_re_data::repo::enumerate_toml(re)?;
    if !existing.is_empty() && !force {
        anyhow::bail!(
            "re/ already contains {} TOML file(s). Pass --force to overwrite, \
             or delete re/*.toml first.",
            existing.len()
        );
    }

    emit::flush_to_disk(&pending)?;
    eprintln!("Wrote {} file(s) under {}.", pending.len(), re.display());

    Ok(())
}

/// Incremental import: diff fresh Ghidra XML against the committed catalog,
/// then mutate touched TOML shards in place. Validate-gated: refuses to run
/// if `re/` is already broken (you'd be applying changes on top of a
/// known-bad state). Scope is function field updates + label and global
/// create/rename/retype/delete; function create/delete are reported only.
fn cmd_import_incremental(
    re: &std::path::Path,
    dir: &std::path::Path,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    if !force {
        check_export_staleness(re, dir)?;
    }
    let prog = load_ghidra_export(dir)?;
    let cat = Catalog::load_from(re)?;

    // Validate-gate: never apply on top of a broken catalog. Warnings are
    // fine; errors are not.
    let report = validate::validate(&cat)?;
    if !report.ok() {
        anyhow::bail!(
            "refusing to import: {} validation error(s) in re/. \
             Run `openwa-re validate` for details and fix them first.",
            report.errors.len(),
        );
    }

    let changes = diff::diff(&prog, &cat, re);
    let rendered = render::render(&changes, re, &cat);
    print!("{rendered}");

    if dry_run {
        eprintln!("(dry-run: no files written)");
        return Ok(());
    }
    if changes.iter().all(|c| !c.actionable()) {
        // Either nothing to do, or every change was report-only.
        return Ok(());
    }

    let stats = apply::apply(&changes, re)?;
    eprintln!(
        "Applied {} change(s) across {} file(s) ({} created, {} removed, {} skipped).",
        stats.changes_applied,
        stats.files_written,
        stats.files_created,
        stats.files_removed,
        stats.changes_skipped,
    );
    Ok(())
}

fn cmd_export(re: &std::path::Path, dir: &std::path::Path) -> Result<()> {
    require_scratch_dir(dir, false)?;
    let manifest_path = dir.join(MANIFEST_FILE);
    let extras_path = dir.join(EXTRAS_FILE);

    let t0 = std::time::Instant::now();
    let mut cat = Catalog::load_from(re)?;
    let load_dt = t0.elapsed();

    let extras_applied = if extras_path.is_file() {
        Some(apply_extras_to_catalog(&mut cat, &extras_path)?)
    } else {
        None
    };

    let report = validate::validate(&cat)?;
    if !report.ok() {
        anyhow::bail!(
            "refusing to emit manifest: {} validation error(s). Run `openwa-re validate` for details.",
            report.errors.len(),
        );
    }

    let t1 = std::time::Instant::now();
    let manifest = manifest::build_from_catalog(&cat);
    let json = manifest::to_json(&manifest)?;
    std::fs::write(&manifest_path, &json)?;
    let render_dt = t1.elapsed();
    let bytes = std::fs::metadata(&manifest_path)?.len();

    eprintln!(
        "Loaded {} TOML file(s), {} entries in {:.2}s. Rendered manifest in {:.2}s.",
        openwa_re_data::repo::enumerate_toml(re)?.len(),
        cat.total_entries(),
        load_dt.as_secs_f64(),
        render_dt.as_secs_f64(),
    );
    eprintln!(
        "  Manifest: {} ({:.1} KiB)",
        manifest_path.display(),
        bytes as f64 / 1024.0,
    );
    match extras_applied {
        Some(n) => eprintln!(
            "  Extras overlay: {} applied to {} function(s)",
            extras_path.display(),
            n,
        ),
        None => eprintln!(
            "  Extras overlay: {} not found — emitting without calling-convention / no-return / custom-storage attributes",
            extras_path.display(),
        ),
    }
    if !report.warnings.is_empty() {
        eprintln!(
            "  ({} validation warning(s) — non-fatal)",
            report.warnings.len()
        );
    }

    Ok(())
}

fn cmd_diff(re: &std::path::Path, dir: &std::path::Path) -> Result<()> {
    require_scratch_dir(dir, true)?;
    let prog = load_ghidra_export(dir)?;

    let cat = Catalog::load_from(re)?;

    let changes = diff::diff(&prog, &cat, re);
    let rendered = render::render(&changes, re, &cat);
    print!("{rendered}");
    Ok(())
}

/// Refuse incremental import if any TOML file under `re/` has been edited
/// more recently than the last `openwa-re export` (whose output sits in the
/// scratch dir as `wa_import.json`). The danger: those TOML edits haven't
/// been pushed to Ghidra yet, so the XML can't possibly reflect them, and
/// applying the diff would silently revert them.
///
/// Skipped when no prior export exists (first import on this machine).
/// Bypass with `--force` if you know the unpushed TOML edits are
/// intentionally out of scope of this import.
fn check_export_staleness(re: &std::path::Path, dir: &std::path::Path) -> Result<()> {
    let manifest = dir.join(MANIFEST_FILE);
    let Ok(export_at) = std::fs::metadata(&manifest).and_then(|m| m.modified()) else {
        return Ok(());
    };

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for p in openwa_re_data::repo::enumerate_toml(re)? {
        let m = std::fs::metadata(&p)
            .with_context(|| format!("stat {}", p.display()))?
            .modified()
            .with_context(|| format!("mtime of {}", p.display()))?;
        if newest.as_ref().is_none_or(|(t, _)| m > *t) {
            newest = Some((m, p));
        }
    }
    let Some((newest_mtime, newest_path)) = newest else {
        return Ok(());
    };
    if newest_mtime <= export_at {
        return Ok(());
    }

    let delta = newest_mtime
        .duration_since(export_at)
        .map(format_duration)
        .unwrap_or_else(|_| "?".to_string());
    let path = newest_path
        .strip_prefix(re)
        .map(|p| Path::new("re").join(p))
        .unwrap_or(newest_path);
    anyhow::bail!(
        "refusing to import: {} was edited {} after the last `openwa-re export`. \
         Ghidra doesn't know about those edits yet, so import would silently \
         revert them. Run `openwa-re export` and then OpenWAImport.java in \
         Ghidra to push the TOML state first, or pass --force to override.",
        path.display(),
        delta,
    );
}

fn format_duration(d: std::time::Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{}s", s / 60, s % 60)
    } else if s < 86400 {
        format!("{}h{}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}d{}h", s / 86400, (s % 86400) / 3600)
    }
}

/// Parse `<dir>/wa_export.xml`, overlay `<dir>/wa_export_extras.json` if
/// present, run [`resolve::resolve`], and return the post-resolve program.
/// Shared by `cmd_import` and `cmd_diff`.
fn load_ghidra_export(dir: &std::path::Path) -> Result<openwa_re_data::xml_in::XmlProgram> {
    let xml = dir.join(XML_FILE);
    if !xml.is_file() {
        anyhow::bail!(
            "{} not found in scratch dir {}. Run OpenWAExport.java first.",
            XML_FILE,
            dir.display(),
        );
    }
    let extras_path = dir.join(EXTRAS_FILE);

    let mut prog = xml_in::parse_file(&xml)?;
    let _ = apply_extras_sidecar(&mut prog, &extras_path)?;
    let _ = resolve::resolve(&mut prog);
    Ok(prog)
}

/// Overlay calling_convention + no_return from a sidecar onto the catalog
/// loaded from TOML. Functions in the sidecar that aren't in `re/` are
/// silently skipped (they're typically MFC/CRT methods we filter out).
fn apply_extras_to_catalog(
    cat: &mut openwa_re_data::toml_io::Catalog,
    path: &std::path::Path,
) -> Result<usize> {
    if !path.is_file() {
        anyhow::bail!("extras sidecar not found: {}", path.display());
    }
    #[derive(serde::Deserialize)]
    struct Sidecar {
        functions: Vec<ExtrasEntry>,
    }
    #[derive(serde::Deserialize)]
    struct ExtrasEntry {
        va: String,
        #[serde(default)]
        calling_convention: Option<String>,
        #[serde(default)]
        no_return: bool,
        #[serde(default)]
        custom_storage: bool,
    }
    let raw = std::fs::read_to_string(path)?;
    let sc: Sidecar = serde_json::from_str(&raw)?;
    let mut applied = 0;
    for e in sc.functions {
        let va = parse_hex_va(&e.va)?;
        let Some(entry) = cat.functions.get_mut(&va) else {
            continue;
        };
        if e.calling_convention.is_some() {
            entry.value.calling_convention = e.calling_convention;
        }
        if e.no_return {
            entry.value.no_return = true;
        }
        if e.custom_storage {
            entry.value.custom_storage = true;
        }
        applied += 1;
    }
    Ok(applied)
}

/// Overlay calling_convention and no_return from the sidecar onto matching
/// functions in `prog`. Silent when the sidecar is missing (the original
/// onboarding path produced XML only). Returns `Some(count)` when applied,
/// `None` when the sidecar isn't present.
fn apply_extras_sidecar(
    prog: &mut openwa_re_data::xml_in::XmlProgram,
    path: &std::path::Path,
) -> Result<Option<usize>> {
    if !path.is_file() {
        return Ok(None);
    }
    #[derive(serde::Deserialize)]
    struct Sidecar {
        functions: Vec<ExtrasEntry>,
    }
    #[derive(serde::Deserialize)]
    struct ExtrasEntry {
        va: String,
        #[serde(default)]
        calling_convention: Option<String>,
        #[serde(default)]
        no_return: bool,
        #[serde(default)]
        custom_storage: bool,
    }

    let raw = std::fs::read_to_string(path)?;
    let sc: Sidecar = serde_json::from_str(&raw)?;

    // Build a VA → index map once; the function list is up to ~5K entries.
    let mut by_va = std::collections::HashMap::with_capacity(prog.functions.len());
    for (i, f) in prog.functions.iter().enumerate() {
        by_va.insert(f.va, i);
    }

    let mut applied = 0;
    for e in sc.functions {
        let va = parse_hex_va(&e.va)?;
        let Some(&i) = by_va.get(&va) else { continue };
        let f = &mut prog.functions[i];
        if e.calling_convention.is_some() {
            f.calling_convention = e.calling_convention;
        }
        if e.no_return {
            f.no_return = true;
        }
        if e.custom_storage {
            f.custom_storage = true;
        }
        applied += 1;
    }
    Ok(Some(applied))
}

fn parse_hex_va(s: &str) -> Result<u32> {
    let body = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    Ok(u32::from_str_radix(body, 16)?)
}
