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
//!
//! ## Variable-length commands
//!
//! Commands with variable-length data (LineStrip, Polygon) allocate
//! their vertex arrays separately in the arena via
//! `RenderQueue::alloc_aux()` and store an explicit pointer in the
//! enum variant. This avoids layout coupling between the command and
//! its auxiliary data.

use crate::bitgrid::DisplayBitGrid;
use crate::render::display::vtable::TiledBitmapSource;
use crate::render::sprite::sprite_op::SpriteOp;
use openwa_core::fixed::Fixed;

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

    /// Replaces legacy type 0 (`DRAW_RECT`).
    FillRect {
        color: u32,
        x1: Fixed,
        y1: Fixed,
        x2: Fixed,
        y2: Fixed,
        /// Perspective clip reference Z (`cmd[7]` in legacy format).
        ref_z: i32,
    },

    /// Replaces legacy type 0xB (`DRAW_CROSSHAIR`).
    ///
    /// `ref_z` is always 0 in all known producers — omitted here,
    /// the dispatcher passes 0 directly.
    Crosshair {
        color_fg: u32,
        color_bg: u32,
        x: Fixed,
        y: Fixed,
    },

    /// Replaces legacy type 0xD (`DRAW_TILED_BITMAP`).
    TiledBitmap {
        x: Fixed,
        y: Fixed,
        source: *const TiledBitmapSource,
        /// Bit 0 forces destination X to 0.
        flags: u8,
    },

    /// Replaces legacy type 0xE (`DRAW_TILED_TERRAIN`).
    ///
    /// `mode` (cmd[2]), `ref_z` (cmd[5]) and `flags` (cmd[8]) are constants
    /// (0/0/1) in the only known producer (`LandEntity::RenderLandscape` →
    /// `RQ_EnqueueTiledTerrain` at 0x005422A0) — omitted here.
    TiledTerrain { x: Fixed, y: Fixed, count: i32 },

    /// Replaces legacy type 6 (`DRAW_SPRITE_OFFSET`).
    ///
    /// `ref_z` (first Z reference) is always 0 in all known producers —
    /// omitted here, the dispatcher passes 0 directly.
    SpriteOffset {
        flags: u32,
        x: Fixed,
        y: Fixed,
        /// Second Z reference for perspective clip (`cmd[6]` in legacy format).
        ref_z_2: i32,
        sprite: SpriteOp,
        palette: u32,
    },

    /// Replaces legacy type 1 (`DRAW_BITMAP_GLOBAL`).
    ///
    /// `src_x` is always 0 in all known producers — omitted here.
    BitmapGlobal {
        x: Fixed,
        y: Fixed,
        bitmap: *mut DisplayBitGrid,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        flags: u32,
    },

    /// Replaces legacy type 2 (`DRAW_TEXTBOX_LOCAL`).
    ///
    /// `mode`, `ref_z`, `ref_z_2`, `src_x`, `src_y` are always 0 in
    /// all known producers — omitted here.
    TextboxLocal {
        x: Fixed,
        y: Fixed,
        bitmap: *mut DisplayBitGrid,
        src_w: i32,
        src_h: i32,
        flags: u32,
    },

    /// Replaces legacy type 8 (`DRAW_LINE_STRIP`).
    ///
    /// Vertex data is allocated separately via `RenderQueue::alloc_aux()`
    /// and referenced by the explicit `vertices` pointer.
    /// Each vertex is `[x: i32, y: i32, z: i32]` in Fixed16.
    LineStrip {
        count: u32,
        color: u32,
        /// Pointer to `count` vertices in the arena. Allocated via
        /// `alloc_aux()`, valid for the current frame only.
        vertices: *const [i32; 3],
    },

    /// Replaces legacy type 9 (`DRAW_POLYGON`).
    ///
    /// Vertex data is allocated separately via `RenderQueue::alloc_aux()`
    /// and referenced by the explicit `vertices` pointer.
    /// Each vertex is `[x: i32, y: i32, z: i32]` in Fixed16.
    Polygon {
        count: u32,
        color1: u32,
        color2: u32,
        /// Pointer to `count` vertices in the arena. Allocated via
        /// `alloc_aux()`, valid for the current frame only.
        vertices: *const [i32; 3],
    },
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

// Canary for buffer pressure — if this fires, revisit the size budget
// note in the plan (consider #[repr(u8)] discriminant or variant splitting).
const _: () = assert!(core::mem::size_of::<TypedRenderCmd>() <= 48);
