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
use openwa_re_data::{resolve, validate, xml_in};
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

    /// Read a Ghidra XML dump and write re/*.toml shards. (TODO)
    Export {
        /// Path to a Ghidra `XmlExporter` dump (e.g. C:/tmp/wa_export.xml).
        xml: PathBuf,
        /// Initial bootstrap mode: shard functions by VA range.
        #[arg(long)]
        bootstrap: bool,
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
        Cmd::Export { xml, bootstrap } => cmd_export(&re, &xml, bootstrap),
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

fn cmd_export(_re: &std::path::Path, xml: &std::path::Path, _bootstrap: bool) -> Result<()> {
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

    if !prog.comments.is_empty() {
        eprintln!("first 10 orphan comments:");
        for c in prog.comments.iter().take(10) {
            let preview: String = c.text.chars().take(60).collect();
            eprintln!("  0x{:08X} {:?} {preview:?}", c.va, c.kind);
        }
        let fn_lo = prog.functions.first().map(|f| f.va).unwrap_or(0);
        let fn_hi = prog.functions.last().map(|f| f.va).unwrap_or(0);
        eprintln!("function VA range: 0x{:08X}..=0x{:08X}", fn_lo, fn_hi);
    }

    anyhow::bail!("export: stats-only prototype; TOML emission not yet implemented")
}

fn cmd_import(_re: &std::path::Path, _out: &std::path::Path) -> Result<()> {
    anyhow::bail!("import: not yet implemented")
}

fn cmd_diff(_re: &std::path::Path, _xml: &std::path::Path) -> Result<()> {
    anyhow::bail!("diff: not yet implemented")
}
