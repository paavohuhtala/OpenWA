//! Build-time codegen for openwa-dll.
//!
//! Loads the same `re/**/*.toml` catalog used by openwa-game's build script,
//! joins it against this crate's `hooks/*.toml` files, and emits a single
//! `generated_trampolines.rs` containing:
//!
//! - One signature-check `const _CHECK_<wa_function>: <expected fn type> = <rust_impl>;`
//!   per declared hook — wrong impl signature errors at build time.
//! - For usercall hooks: a `#[unsafe(naked)] extern "C" fn tramp_<wa_function>()`
//!   that pulls register/stack params into cdecl order and forwards to the impl.
//! - For `save_original = true`: an `ORIG_<wa_function>: AtomicU32` and a
//!   typed `call_original_<wa_function>(...)` helper.
//! - A `pub unsafe fn install_<wa_function>() -> Result<(), String>` per hook.
//!
//! Hook files split per subsystem (engine.toml, entity.toml, sound.toml, …)
//! mirror `re/**/*.toml` and `crates/openwa-dll/src/replacements/*.rs`. See
//! the plan at
//! `C:\Users\Paavo\.claude\plans\we-ve-recently-added-the-structured-snowflake.md`.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root above crates/openwa-dll");
    let re_dir = workspace.join("re");
    let hooks_dir = manifest_dir.join("hooks");

    // Cargo rebuild triggers — re/ for catalog, hooks/ for our opt-in list.
    println!("cargo:rerun-if-changed={}", re_dir.display());
    for entry in walkdir::WalkDir::new(&re_dir)
        .into_iter()
        .filter_map(|r| r.ok())
    {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("toml") {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
    println!("cargo:rerun-if-changed={}", hooks_dir.display());
    if hooks_dir.exists() {
        for entry in fs::read_dir(&hooks_dir).expect("reading hooks/") {
            let p = entry.expect("hooks dir entry").path();
            if p.extension().and_then(|e| e.to_str()) == Some("toml") {
                println!("cargo:rerun-if-changed={}", p.display());
            }
        }
    }

    let cat = openwa_re_codegen::Catalog::load_from(&re_dir).expect("loading re/ catalog");
    let hook_map =
        openwa_re_codegen::hook_map::HookMap::load_from(&hooks_dir).expect("loading hooks/ map");

    let (source, stats) = openwa_re_codegen::emit_trampolines::generate(&cat, &hook_map);

    // Hard-fail on lookups that should have succeeded — these aren't
    // best-effort like wa_calls. If a hook is declared, it must build.
    if !stats.missing_in_catalog.is_empty() {
        panic!(
            "hook_map references unknown wa_function(s) — fix the typo or add to re/*.toml:\n  {}",
            stats.missing_in_catalog.join("\n  "),
        );
    }
    if !stats.missing_convention.is_empty() {
        panic!(
            "hook_map function(s) missing calling_convention in re/*.toml:\n  {}",
            stats.missing_convention.join("\n  "),
        );
    }
    if !stats.missing_storage.is_empty() {
        panic!(
            "hook_map function(s) custom_storage = true but some params lack storage =:\n  {}",
            stats.missing_storage.join("\n  "),
        );
    }
    if !stats.invalid_storage.is_empty() {
        panic!(
            "hook_map function(s) with invalid storage specs:\n  {}",
            stats.invalid_storage.join("\n  "),
        );
    }

    let out_dir: PathBuf = env::var_os("OUT_DIR").unwrap().into();
    fs::write(out_dir.join("generated_trampolines.rs"), source)
        .expect("writing generated_trampolines.rs");

    println!(
        "cargo:warning=trampolines: emitted {} default + {} custom",
        stats.emitted_default_storage, stats.emitted_custom_storage,
    );
}
