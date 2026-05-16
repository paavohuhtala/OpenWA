//! Interactive `openwa-re setup` wizard.
//!
//! Walks the user through per-machine configuration (WA install, Ghidra
//! install, Ghidra project, scratch dir), persists the result to
//! `<repo>/.openwa/setup.toml`, and installs `OpenWA{Export,Import}.java`
//! into `~/ghidra_scripts/` with their default scratch-path literals
//! rewritten to point at the chosen scratch dir.

use anyhow::{Context, Result, bail};
use dialoguer::{Confirm, Input, theme::ColorfulTheme};
use openwa_re_data::setup::{self, SetupConfig};
use std::path::{Path, PathBuf};

pub fn run(repo_root: &Path, force: bool) -> Result<()> {
    let setup_path = setup::setup_path(repo_root);
    if setup_path.is_file() && !force {
        eprintln!(
            "{} already exists. Pass --force to re-run the wizard.",
            setup_path.display(),
        );
        eprintln!("Loading existing config and refreshing Ghidra scripts only…");
        let cfg = SetupConfig::load(repo_root)?.unwrap_or_default();
        install_ghidra_scripts(repo_root, &cfg)?;
        return Ok(());
    }

    let theme = ColorfulTheme::default();

    eprintln!("OpenWA setup — per-machine configuration");
    eprintln!("Writes {}", setup_path.display());
    eprintln!();

    let existing = SetupConfig::load(repo_root)?.unwrap_or_default();
    let mut cfg = SetupConfig::default();

    // 1. Game install.
    cfg.game_dir = Some(prompt_game_dir(&theme, existing.game_dir.as_deref())?);
    save_partial(repo_root, &cfg)?;

    // 2. Ghidra install.
    cfg.ghidra_install = Some(prompt_ghidra_install(
        &theme,
        existing.ghidra_install.as_deref(),
    )?);
    save_partial(repo_root, &cfg)?;

    // 3. Ghidra project (parent dir + name).
    let (proj_dir, proj_name) = prompt_ghidra_project(
        &theme,
        existing.ghidra_project_dir.as_deref(),
        existing.ghidra_project_name.as_deref(),
    )?;
    cfg.ghidra_project_dir = Some(proj_dir);
    cfg.ghidra_project_name = Some(proj_name);
    save_partial(repo_root, &cfg)?;

    // 4. Scratch dir.
    cfg.scratch_dir = Some(prompt_scratch_dir(
        &theme,
        repo_root,
        existing.scratch_dir.as_deref(),
    )?);
    save_partial(repo_root, &cfg)?;

    // 5. Install Ghidra-side scripts.
    install_ghidra_scripts(repo_root, &cfg)?;

    eprintln!();
    eprintln!("Setup complete.");
    eprintln!("  {}", setup_path.display());
    eprintln!();
    eprintln!("Next steps:");
    eprintln!("  - In Ghidra, open Window → Script Manager, locate `OpenWAExport.java`");
    eprintln!("    and `OpenWAImport.java` (category: OpenWA), and tick the \"In Tool\"");
    eprintln!("    checkbox on both. Required for them to appear in the Tools menu.");
    eprintln!(
        "  - In Ghidra, run Tools → OpenWA → Export catalog (writes {}/wa_export.xml).",
        cfg.effective_scratch_dir(repo_root).display(),
    );
    eprintln!("  - `openwa-re import --bootstrap` to seed re/*.toml from the dump.");
    eprintln!("  - Edit re/, then `openwa-re export` to produce a manifest.");
    eprintln!("  - In Ghidra, run Tools → OpenWA → Import catalog to apply it.");

    Ok(())
}

fn prompt_game_dir(theme: &ColorfulTheme, prior: Option<&Path>) -> Result<PathBuf> {
    let detected = openwa_config::find_wa_dir();
    let initial = prior.map(|p| p.to_path_buf()).or_else(|| detected.clone());

    if let Some(d) = &detected {
        if prior.is_none() {
            eprintln!("Detected Worms Armageddon at {}", d.display());
        }
    }

    let default_text = initial
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    loop {
        let raw: String = Input::with_theme(theme)
            .with_prompt("Worms Armageddon install directory")
            .with_initial_text(&default_text)
            .interact_text()
            .context("reading game directory")?;
        let path = PathBuf::from(raw.trim());
        let exe = path.join("WA.exe");
        if !exe.is_file() {
            eprintln!(
                "  WA.exe not found at {} — pick the folder that contains WA.exe.",
                exe.display(),
            );
            continue;
        }
        return Ok(path);
    }
}

fn prompt_ghidra_install(theme: &ColorfulTheme, prior: Option<&Path>) -> Result<PathBuf> {
    let initial = prior.map(|p| p.display().to_string()).unwrap_or_default();
    loop {
        let raw: String = Input::with_theme(theme)
            .with_prompt("Ghidra install directory (contains ghidraRun.bat)")
            .with_initial_text(&initial)
            .interact_text()
            .context("reading Ghidra install directory")?;
        let path = PathBuf::from(raw.trim());
        match validate_ghidra_install(&path) {
            Ok(()) => return Ok(path),
            Err(e) => {
                eprintln!("  {e}");
                continue;
            }
        }
    }
}

fn validate_ghidra_install(path: &Path) -> Result<()> {
    if !path.is_dir() {
        bail!("not a directory: {}", path.display());
    }
    let candidates = [
        path.join("ghidraRun.bat"),
        path.join("ghidraRun"),
        path.join("support").join("analyzeHeadless.bat"),
        path.join("support").join("analyzeHeadless"),
    ];
    if !candidates.iter().any(|c| c.is_file()) {
        bail!(
            "doesn't look like a Ghidra install — none of ghidraRun(.bat) or \
             support/analyzeHeadless(.bat) found under {}",
            path.display(),
        );
    }
    Ok(())
}

fn prompt_ghidra_project(
    theme: &ColorfulTheme,
    prior_dir: Option<&Path>,
    prior_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    eprintln!();
    eprintln!("Ghidra project location:");
    eprintln!("  - <project_dir> contains the .gpr/.rep/.lock files");
    eprintln!("  - <project_name> is the file name without `.gpr`");

    let initial_dir = prior_dir
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let initial_name = prior_name.unwrap_or("WA").to_string();

    loop {
        let dir_raw: String = Input::with_theme(theme)
            .with_prompt("Ghidra project parent directory")
            .with_initial_text(&initial_dir)
            .interact_text()
            .context("reading Ghidra project dir")?;
        let dir = PathBuf::from(dir_raw.trim());
        if !dir.is_dir() {
            let create = Confirm::with_theme(theme)
                .with_prompt(format!("{} doesn't exist — create it?", dir.display()))
                .default(true)
                .interact()
                .context("create-dir confirmation")?;
            if create {
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("creating {}", dir.display()))?;
            } else {
                continue;
            }
        }

        let name: String = Input::with_theme(theme)
            .with_prompt("Ghidra project name")
            .with_initial_text(&initial_name)
            .interact_text()
            .context("reading Ghidra project name")?;
        let name = name.trim().to_string();
        if name.is_empty() {
            eprintln!("  project name cannot be empty");
            continue;
        }

        let gpr = dir.join(format!("{name}.gpr"));
        if gpr.is_file() {
            eprintln!("  Found existing project at {}.", gpr.display());
        } else {
            eprintln!(
                "  No {} yet — analyzeHeadless can create it later (not done by this wizard).",
                gpr.display(),
            );
        }
        return Ok((dir, name));
    }
}

fn prompt_scratch_dir(
    theme: &ColorfulTheme,
    repo_root: &Path,
    prior: Option<&Path>,
) -> Result<PathBuf> {
    let default_abs = repo_root.join(setup::DEFAULT_SCRATCH_REL);
    let initial = prior
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| default_abs.display().to_string());

    let raw: String = Input::with_theme(theme)
        .with_prompt("Scratch directory (shared between Ghidra and openwa-re)")
        .with_initial_text(&initial)
        .interact_text()
        .context("reading scratch dir")?;
    let path = PathBuf::from(raw.trim());
    std::fs::create_dir_all(&path)
        .with_context(|| format!("creating scratch dir {}", path.display()))?;
    Ok(path)
}

fn save_partial(repo_root: &Path, cfg: &SetupConfig) -> Result<()> {
    cfg.save(repo_root).map(|_| ())
}

/// Copy `ghidra_scripts/OpenWA{Export,Import}.java` from the repo into the
/// user's `~/ghidra_scripts/` directory, rewriting the hard-coded
/// `C:/tmp/wa_*.{xml,json}` defaults to point at the configured scratch dir.
fn install_ghidra_scripts(repo_root: &Path, cfg: &SetupConfig) -> Result<()> {
    let dest = ghidra_scripts_dir()?;
    std::fs::create_dir_all(&dest).with_context(|| format!("creating {}", dest.display()))?;

    let scratch = cfg.effective_scratch_dir(repo_root);
    let scratch_str = path_to_java_literal(&scratch);

    for (name, default_path) in [
        ("OpenWAExport.java", format!("{scratch_str}/wa_export")),
        ("OpenWAImport.java", format!("{scratch_str}/wa_import.json")),
    ] {
        let src = repo_root.join("ghidra_scripts").join(name);
        let dst = dest.join(name);
        let mut body =
            std::fs::read_to_string(&src).with_context(|| format!("reading {}", src.display()))?;
        body = rewrite_default_path(&body, &default_path);
        std::fs::write(&dst, body).with_context(|| format!("writing {}", dst.display()))?;
        eprintln!("  installed {}", dst.display());
    }
    Ok(())
}

/// Locate Ghidra's per-user scripts directory. Ghidra defaults to
/// `~/ghidra_scripts` regardless of install path.
fn ghidra_scripts_dir() -> Result<PathBuf> {
    let home = dirs_home()?;
    Ok(home.join("ghidra_scripts"))
}

fn dirs_home() -> Result<PathBuf> {
    if let Some(h) = std::env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(h));
    }
    if let Some(h) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(h));
    }
    bail!("can't locate home directory (neither USERPROFILE nor HOME set)")
}

/// Java string literals use forward slashes on every platform; backslashes
/// would need doubling. Normalize before substituting.
fn path_to_java_literal(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// Rewrite the single `C:/tmp/...` literal in the script body to the
/// supplied default. Both scripts contain exactly one occurrence in an
/// `else { prefix = "C:/tmp/..."; }` branch.
fn rewrite_default_path(src: &str, new_default: &str) -> String {
    // Be conservative: only replace the literal inside double quotes that
    // starts with `C:/tmp/wa_`. Avoids touching the comment header.
    let pattern_prefix = "\"C:/tmp/wa_";
    let mut out = String::with_capacity(src.len() + 32);
    let mut rest = src;
    while let Some(pos) = rest.find(pattern_prefix) {
        out.push_str(&rest[..pos]);
        // find the closing quote
        let after = &rest[pos + 1..];
        let Some(end_rel) = after.find('"') else {
            // malformed — keep going from after the opening quote
            out.push_str(&rest[pos..]);
            return out;
        };
        out.push('"');
        out.push_str(new_default);
        out.push('"');
        rest = &after[end_rel + 1..];
    }
    out.push_str(rest);
    out
}
