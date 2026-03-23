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

#[macro_use]
pub mod macros;

pub mod audio;
pub mod display;
pub mod engine;
pub mod frontend;
pub mod game;
pub mod input;
pub mod render;

pub mod address;
pub mod fixed;
pub mod log;
pub mod mem;
pub mod rebase;
pub mod snapshot;
pub mod task;
pub mod vtable;
pub mod wa;
pub mod wa_alloc;
pub mod wa_call;
