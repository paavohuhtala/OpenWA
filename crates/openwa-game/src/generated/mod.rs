//! Build-time code generated from `re/**/*.toml` by `openwa-re-codegen`.
//!
//! These submodules are produced by `crates/openwa-game/build.rs` and live in
//! `$OUT_DIR/`. Do not edit by hand — edit the source TOML instead and rebuild.

pub mod addresses {
    #![allow(non_upper_case_globals, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/generated_addresses.rs"));
}

// The generated file already declares its own `pub mod wa_calls { ... }` so
// the `include!` lands at module scope.
#[allow(non_snake_case, dead_code)]
mod wa_calls_gen {
    include!(concat!(env!("OUT_DIR"), "/generated_wa_calls.rs"));
}
pub use wa_calls_gen::wa_calls;
