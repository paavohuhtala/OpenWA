pub mod bungee_trail;
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

pub use ddraw::CompatRenderer;
pub use display::{
    DisplayBase, DisplayBaseVtable, DisplayGfx, DisplayGfxVtable, DrawScaledSpriteResult,
    FastcallResult, FrameCache, FrameCacheEntry, Palette, PaletteVtable, RenderContext,
    SpriteCache,
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

/// Render table entry (0x14 = 20 bytes).
///
/// 14 entries live at GameWorld+0x73B0 (stride 0x14). Only the first u32
/// is zeroed during construction; the rest is uninitialized/unknown.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderEntry {
    /// Active/state flag (zeroed on init).
    pub active: u32,
    /// Unknown data.
    pub _unknown: [u8; 16],
}
const _: () = assert!(core::mem::size_of::<RenderEntry>() == 0x14);
