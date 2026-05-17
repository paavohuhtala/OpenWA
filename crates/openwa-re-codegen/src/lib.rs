//! Build-time codegen from the `re/**/*.toml` reverse-engineering catalog.
//!
//! Consumed by `crates/openwa-game/build.rs` (address constants + typed call
//! wrappers) and `crates/openwa-dll/build.rs` (usercall trampolines and hook
//! detour signature checks). Pure Rust string emission — no `syn`/`quote`,
//! no proc-macros.
//!
//! See `C:\Users\Paavo\.claude\plans\we-ve-recently-added-the-structured-snowflake.md`
//! for the full architecture.

pub use openwa_re_data::toml_io::Catalog;

pub mod emit_addresses;
pub mod emit_trampolines;
pub mod emit_wa_calls;
pub mod hook_map;
pub mod storage;
pub mod type_resolver;
