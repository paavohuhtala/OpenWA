pub mod gfx_dir;
pub mod landscape;
pub mod palette;
pub mod queue;
pub mod spr;
pub mod sprite;
pub mod sprite_id;
pub mod turn_order;

pub use landscape::{DirtyRect, PCLandscape};
pub use queue::{
    DrawBitmapGlobalCmd, DrawCrosshairCmd, DrawLineStripHeader, DrawPixelCmd, DrawPolygonHeader,
    DrawRectCmd, DrawSpriteCmd, DrawSpriteOffsetCmd, DrawTextboxLocalCmd, RenderQueue,
};
pub use spr::{ParsedSprite, SprError, SprHeader};
pub use sprite::{Sprite, SpriteFrame, SpriteId};
pub use turn_order::{
    AnimatedItemList, TurnOrderAllianceGroup, TurnOrderTeamEntry, TurnOrderWidget,
};

/// Render table entry (0x14 = 20 bytes).
///
/// 14 entries live at DDGame+0x73B0 (stride 0x14). Only the first u32
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
