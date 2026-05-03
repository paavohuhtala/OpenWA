pub mod crosshair_line;
pub mod ddraw;
pub mod display;
pub mod landscape;
pub mod message;
pub mod opengl;
pub mod palette;
pub mod queue;
pub mod queue_dispatch;
pub mod sprite;
pub mod turn_order;
pub mod worm;

pub use ddraw::CompatRenderer;
pub use display::{
    DisplayBase, DisplayBaseVtable, DisplayGfx, DisplayGfxVtable, DrawScaledSpriteResult,
    FastcallResult, FrameCache, FrameCacheEntry, RenderContext, SpriteCache,
};
pub use landscape::{DirtyRect, Landscape};
pub use message::{COMMAND_TYPE_TYPED, RenderMessage, TypedRenderCmd};
pub use opengl::OpenGLState;
pub use queue::RenderQueue;
pub use sprite::{
    KnownSpriteId, ParsedSprite, Sprite, SpriteBank, SpriteFlags, SpriteFrame, SpriteOp,
};
pub use turn_order::{
    AnimatedItemList, TurnOrderAllianceGroup, TurnOrderTeamEntry, TurnOrderWidget,
};

/// One slot of the per-event-kind bbox table at `GameWorld+0x73B0`. 14 slots,
/// stride 0x14 bytes.
///
/// Written by `record_landing_event` (WA 0x00547D10): each call either inits
/// the slot to a single-point bbox (when `active == 0`) or expands the
/// existing bbox to include `(x, y)`. `WormEntity::landing_check_raw` (WA
/// 0x0050D450) is one writer; it dispatches to slot indices `{1, 2, 3, 4, 9, 11}`
/// based on worm position/state.
///
/// Only `active` is zeroed during construction; min/max are valid only when
/// `active != 0`. Frame-end render code clears `active` on the whole table —
/// see [`GameWorld::render_entries`](crate::engine::GameWorld::render_entries)
/// callers.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderEntry {
    /// 0x00: 0 = uninitialized; 1 = bbox below is valid.
    pub active: u32,
    /// 0x04: Bbox min X (Fixed16.16).
    pub min_x: i32,
    /// 0x08: Bbox min Y (Fixed16.16).
    pub min_y: i32,
    /// 0x0C: Bbox max X (Fixed16.16).
    pub max_x: i32,
    /// 0x10: Bbox max Y (Fixed16.16).
    pub max_y: i32,
}
const _: () = assert!(core::mem::size_of::<RenderEntry>() == 0x14);
