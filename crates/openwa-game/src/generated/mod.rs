//! Build-time code generated from `re/**/*.toml` by `openwa-re-codegen`.
//!
//! These submodules are produced by `crates/openwa-game/build.rs` and live in
//! `$OUT_DIR/`. Do not edit by hand — edit the source TOML instead and rebuild.

pub mod addresses {
    #![allow(non_upper_case_globals, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/generated_addresses.rs"));
}
