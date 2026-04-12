//! Strongly-typed render queue messages.
//!
//! Incremental replacement for the legacy command-type byte format.
//! Each `RenderMessage` variant replaces one (or more) of the legacy
//! `DrawXxxCmd` structs from `queue.rs`. Typed messages are stored
//! inline in the existing `RenderQueue._buffer` arena via
//! [`TypedRenderCmd`], so construction is free — no heap allocation.
//!
//! ## Migration strategy
//!
//! Producers switch from `rq.alloc::<DrawXxxCmd>()` + field fill to
//! `rq.push_typed(layer, RenderMessage::Xxx { ... })` one at a time.
//! The dispatcher (`queue_dispatch.rs`) handles both legacy and typed
//! commands simultaneously until all producers are migrated.

use crate::fixed::Fixed;
use crate::render::sprite::sprite_op::SpriteOp;

/// Sentinel `command_type` value for [`TypedRenderCmd`]. Sits outside
/// the legacy 0..=0xE range so the existing per-case dispatcher arms
/// are unaffected.
pub const COMMAND_TYPE_TYPED: u32 = 0x100;

/// Strongly-typed render queue message.
///
/// One variant per logical draw operation. Payloads use the project's
/// typed primitives (`Fixed`, typed pointers) where possible; fields
/// whose semantics are still ambiguous stay as raw `u32` with a comment.
///
/// Stored inline inside `RenderQueue._buffer` via [`TypedRenderCmd`].
/// The buffer is reset by WA at the start of each frame, same as the
/// legacy byte-format commands.
#[derive(Debug, Clone, Copy)]
pub enum RenderMessage {
    /// Replaces legacy types 4 (`DRAW_SPRITE_GLOBAL`) and 5
    /// (`DRAW_SPRITE_LOCAL`).
    ///
    /// `local` selects screen-space (`true`, dispatcher applies
    /// `rq_translate_coordinates`) vs world-space (`false`, position
    /// passed through directly).
    Sprite {
        local: bool,
        x: Fixed,
        y: Fixed,
        sprite: SpriteOp,
        /// Palette context — passed to `blit_sprite` as the last arg.
        /// Semantics vary by producer (palette pointer, animation index, etc.).
        palette: u32,
    },
    // Future variants added here as producers migrate:
    // FillRect { ... }
    // Crosshair { ... }
    // TiledBitmap { ... }
    // etc.
}

/// Wrapper that carries a [`RenderMessage`] inline in the render queue
/// buffer. The `command_type` / `layer` prefix matches the legacy
/// command layout so the dispatcher's sort-by-layer step works unchanged.
#[repr(C)]
pub struct TypedRenderCmd {
    /// Always [`COMMAND_TYPE_TYPED`].
    pub command_type: u32,
    /// Render layer — same semantics as the legacy commands' `layer` field.
    pub layer: u32,
    /// The typed payload.
    pub message: RenderMessage,
}
