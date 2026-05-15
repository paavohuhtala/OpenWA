//! Source-of-truth schema for OpenWA reverse-engineering metadata.
//!
//! Files under `re/**/*.toml` are the canonical store. This crate is the
//! parser, validator, normaliser, and XML round-trip layer. See the plan
//! at `C:\Users\Paavo\.claude\plans\this-project-has-a-cheerful-russell.md`
//! for the design.

pub mod emit;
pub mod filter;
pub mod model;
pub mod repo;
pub mod resolve;
pub mod toml_io;
pub mod validate;
pub mod xml_in;

pub use model::*;
