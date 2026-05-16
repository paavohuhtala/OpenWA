//! `openwa-re` — repo-scoped Ghidra metadata tool.
//!
//! Subcommands:
//!   - `validate` — parse all `re/**/*.toml` and report schema/cross-ref errors
//!   - `export`   — read a Ghidra XML dump, write `re/*.toml`
//!   - `import`   — read `re/*.toml`, write a Ghidra import manifest (JSON)
//!   - `diff`     — TODO: human-readable diff between `re/` and a given Ghidra XML

use anyhow::Result;
use clap::{Parser, Subcommand};
use openwa_re_data::manifest;
use openwa_re_data::repo::{find_repo_root, re_dir};
use openwa_re_data::toml_io::Catalog;
use openwa_re_data::{emit, resolve, validate, xml_in};
use std::path::PathBuf;

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

    /// Read a Ghidra XML dump and write re/*.toml shards.
    Export {
        /// Path to a Ghidra `XmlExporter` dump (e.g. C:/tmp/wa_export.xml).
        xml: PathBuf,
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

    /// Read re/*.toml and emit a Ghidra import manifest (JSON).
    Import {
        /// Output JSON path. ReImport.java consumes this directly.
        #[arg(long)]
        out: PathBuf,
        /// Optional `_extras.json` produced by `ReExport.java`. When given,
        /// its calling_convention / no_return entries are overlaid onto the
        /// loaded TOML catalog before manifest emission. Lets us inject
        /// these attributes without rewriting TOML (which is what a fresh
        /// `--bootstrap` would do).
        #[arg(long)]
        extras: Option<PathBuf>,
    },

    /// Diff committed re/ against a Ghidra XML dump. (TODO)
    Diff { xml: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let cwd = std::env::current_dir()?;
    let root = find_repo_root(&cwd)?;
    let re = re_dir(&root);

    match cli.cmd {
        Cmd::Validate => cmd_validate(&re),
        Cmd::Export {
            xml,
            bootstrap,
            force,
            dry_run,
        } => cmd_export(&re, &xml, bootstrap, force, dry_run),
        Cmd::Import { out, extras } => cmd_import(&re, &out, extras.as_deref()),
        Cmd::Diff { xml } => cmd_diff(&re, &xml),
    }
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

fn cmd_export(
    re: &std::path::Path,
    xml: &std::path::Path,
    bootstrap: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    if !bootstrap {
        anyhow::bail!(
            "export: only `--bootstrap` mode is implemented for now. \
             Pass --bootstrap to do the initial dump."
        );
    }

    let t0 = std::time::Instant::now();
    let mut prog = xml_in::parse_file(xml)?;
    let parse_dt = t0.elapsed();

    let extras_path = extras_sidecar_path(xml);
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

fn cmd_import(
    re: &std::path::Path,
    out: &std::path::Path,
    extras: Option<&std::path::Path>,
) -> Result<()> {
    let t0 = std::time::Instant::now();
    let mut cat = Catalog::load_from(re)?;
    let load_dt = t0.elapsed();

    let extras_applied = match extras {
        Some(p) => Some(apply_extras_to_catalog(&mut cat, p)?),
        None => None,
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
    let out_path = if out.extension().is_some() {
        out.to_path_buf()
    } else {
        out.with_extension("json")
    };
    std::fs::write(&out_path, &json)?;
    let render_dt = t1.elapsed();
    let bytes = std::fs::metadata(&out_path)?.len();

    eprintln!(
        "Loaded {} TOML file(s), {} entries in {:.2}s. Rendered manifest in {:.2}s.",
        openwa_re_data::repo::enumerate_toml(re)?.len(),
        cat.total_entries(),
        load_dt.as_secs_f64(),
        render_dt.as_secs_f64(),
    );
    eprintln!(
        "  Manifest: {} ({:.1} KiB)",
        out_path.display(),
        bytes as f64 / 1024.0,
    );
    if !report.warnings.is_empty() {
        eprintln!(
            "  ({} validation warning(s) — non-fatal)",
            report.warnings.len()
        );
    }
    if let Some(n) = extras_applied {
        eprintln!("  Extras overlay: {n} function(s) updated from sidecar");
    }

    Ok(())
}

fn cmd_diff(_re: &std::path::Path, _xml: &std::path::Path) -> Result<()> {
    anyhow::bail!("diff: not yet implemented")
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

/// `ReExport.java` writes the sidecar at `<xml_prefix>_extras.json` — where
/// `xml_prefix` is whatever the user passed as the prefix (the `.xml` then
/// gets appended). So for `C:/tmp/wa_export.xml`, the sidecar is at
/// `C:/tmp/wa_export_extras.json` (strip `.xml`, append `_extras.json`).
fn extras_sidecar_path(xml: &std::path::Path) -> PathBuf {
    let stem = xml.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = xml.parent().unwrap_or_else(|| std::path::Path::new(""));
    parent.join(format!("{stem}_extras.json"))
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
