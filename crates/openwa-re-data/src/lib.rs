//! Source-of-truth schema for OpenWA reverse-engineering metadata.
//!
//! Files under `re/**/*.toml` are the canonical store. This crate parses,
//! validates, and renders that store to a single JSON `manifest` consumed
//! by `ghidra_scripts/OpenWAImport.java`, which applies every entry via
//! Ghidra's Java API. See the plan at
//! `C:\Users\Paavo\.claude\plans\this-project-has-a-cheerful-russell.md`.

pub mod emit;
pub mod filter;
pub mod manifest;
pub mod model;
pub mod repo;
pub mod resolve;
pub mod toml_io;
pub mod validate;
pub mod xml_in;

pub use model::*;
