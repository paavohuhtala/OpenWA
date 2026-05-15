//! `openwa-re` — repo-scoped Ghidra metadata tool.
//!
//! Subcommands:
//!   - `validate` — parse all `re/**/*.toml` and report schema/cross-ref errors
//!   - `export`   — TODO: read a Ghidra XML dump, write `re/*.toml`
//!   - `import`   — TODO: read `re/*.toml`, write Ghidra-compatible XML + extras sidecar
//!   - `diff`     — TODO: human-readable diff between `re/` and a given Ghidra XML

use anyhow::Result;
use clap::{Parser, Subcommand};
use openwa_re_data::repo::{find_repo_root, re_dir};
use openwa_re_data::toml_io::Catalog;
use openwa_re_data::{emit, resolve, validate, xml_in, xml_out};
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

    /// Read re/*.toml and emit Ghidra-importable XML + extras sidecar. (TODO)
    Import {
        /// Output path prefix: writes `<prefix>.xml` and `<prefix>_extras.json`.
        #[arg(long)]
        out: PathBuf,
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
        Cmd::Import { out } => cmd_import(&re, &out),
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

    let t1 = std::time::Instant::now();
    let rstats = resolve::resolve(&mut prog);
    let resolve_dt = t1.elapsed();

    eprintln!(
        "Parsed {} in {:.2}s, resolved in {:.2}s",
        xml.display(),
        parse_dt.as_secs_f64(),
        resolve_dt.as_secs_f64(),
    );
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

fn cmd_import(re: &std::path::Path, out: &std::path::Path) -> Result<()> {
    let t0 = std::time::Instant::now();
    let cat = Catalog::load_from(re)?;
    let load_dt = t0.elapsed();

    let report = validate::validate(&cat)?;
    if !report.ok() {
        anyhow::bail!(
            "refusing to emit XML: {} validation error(s). Run `openwa-re validate` for details.",
            report.errors.len(),
        );
    }

    let t1 = std::time::Instant::now();
    xml_out::write_to(out, &cat)?;
    let render_dt = t1.elapsed();

    let xml_path = {
        let mut p = out.to_path_buf();
        p.set_extension("xml");
        p
    };
    let extras_path = {
        let mut s = out.file_name().map(|n| n.to_owned()).unwrap_or_default();
        s.push("_extras.json");
        out.with_file_name(s)
    };
    let xml_bytes = std::fs::metadata(&xml_path)?.len();
    let extras_bytes = std::fs::metadata(&extras_path)?.len();

    eprintln!(
        "Loaded {} TOML file(s), {} entries in {:.2}s. Rendered XML in {:.2}s.",
        openwa_re_data::repo::enumerate_toml(re)?.len(),
        cat.total_entries(),
        load_dt.as_secs_f64(),
        render_dt.as_secs_f64(),
    );
    eprintln!(
        "  XML:     {} ({:.1} MiB)",
        xml_path.display(),
        xml_bytes as f64 / 1024.0 / 1024.0
    );
    eprintln!(
        "  Extras:  {} ({:.1} KiB)",
        extras_path.display(),
        extras_bytes as f64 / 1024.0
    );
    if !report.warnings.is_empty() {
        eprintln!(
            "  ({} validation warning(s) — non-fatal)",
            report.warnings.len()
        );
    }

    Ok(())
}

fn cmd_diff(_re: &std::path::Path, _xml: &std::path::Path) -> Result<()> {
    anyhow::bail!("diff: not yet implemented")
}
