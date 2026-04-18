//! Cross-platform fundamentals for OpenWA.
//!
//! This crate contains idiomatic, portable Rust code that does not depend on
//! WA.exe's memory layout, the `i686-pc-windows-msvc` target, or any Windows
//! API. If a module needs `rb()` / `va::` / `registry` / MinHook / DirectX,
//! it belongs in `openwa-game` instead.
//!
//! Modules migrate here from `openwa-game` one at a time. See the root
//! `CLAUDE.md` for the current charter.

pub mod fixed;
pub mod img;
pub mod log;
pub mod rng;
pub mod scheme;
pub mod sprite_lzss;
pub mod trig;
pub mod weapon;
