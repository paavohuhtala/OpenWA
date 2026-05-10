pub mod backend;
pub mod capture;
pub mod crosshair_line;
pub mod ddraw;
pub mod display;
pub mod dual_run;
pub mod landscape;
pub mod message;
pub mod opengl;
pub mod palette;
pub mod queue;
pub mod queue_dispatch;
pub mod sprite;
pub mod textbox;
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
use openwa_core::fixed::Fixed;
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
    /// 0x04: Bbox min X
    pub min_x: Fixed,
    /// 0x08: Bbox min Y
    pub min_y: Fixed,
    /// 0x0C: Bbox max X
    pub max_x: Fixed,
    /// 0x10: Bbox max Y
    pub max_y: Fixed,
}
const _: () = assert!(core::mem::size_of::<RenderEntry>() == 0x14);
