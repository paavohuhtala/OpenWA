//! Per-worm rendering (HUD, sprite, aim cursor, attached-rope, etc.).
//!
//! These all run from `WormEntity::HandleMessage` case 0x3 (RenderScene)
//! during the per-frame draw pass.

pub mod rope;

pub use rope::draw_attached_rope;
