//! Build-time code generated from `crates/openwa-dll/hooks/*.toml` joined
//! against `re/**/*.toml`. Produced by `build.rs` via `openwa-re-codegen`.
//! Do not edit — change the TOML inputs and rebuild.
#![allow(non_upper_case_globals, non_snake_case, dead_code)]

include!(concat!(env!("OUT_DIR"), "/generated_trampolines.rs"));
