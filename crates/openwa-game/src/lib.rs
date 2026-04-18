#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]
// FFI code frequently casts pointers for clarity even when redundant
#![allow(clippy::unnecessary_cast)]
// Transmute annotations are verbose for FFI function pointer casts
#![allow(clippy::missing_transmute_annotations)]
// Clamp-like patterns in FFI init code often have subtly different semantics
#![allow(clippy::manual_clamp)]
// Complex types are common in FFI function pointer signatures
#![allow(clippy::type_complexity)]
// Doc formatting lints — not worth the churn
#![allow(clippy::empty_line_after_doc_comments)]
#![allow(clippy::doc_lazy_continuation)]

// Allow proc macros to reference `openwa_game::registry` even when
// invoked from within openwa-game itself (where the crate name is `crate`).
extern crate self as openwa_game;

// Re-export inventory so the define_addresses! macro can reference it
// from any crate that depends on openwa-game.
pub use inventory;

// Re-export derive macros so users write `use openwa_game::FieldRegistry;`
pub use openwa_derive::FieldRegistry;
pub use openwa_derive::vtable;

#[macro_use]
pub mod macros;

pub mod asset;
pub mod audio;
pub mod bitgrid;
pub mod engine;
pub mod frontend;
pub mod game;
pub mod input;
pub mod render;

pub mod address;
pub mod field_format;
pub mod mem;
pub mod rebase;
pub mod registry;
pub mod snapshot;
pub mod task;
pub mod trig;
pub mod vtable;
pub mod wa;
pub mod wa_alloc;
pub mod wa_call;
