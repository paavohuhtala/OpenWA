//! Per-worm rendering (HUD, sprite, aim cursor, rope/bungee trail, etc.).
//!
//! These all run from `WormEntity::HandleMessage` case 0x3 (RenderScene)
//! during the per-frame draw pass.

pub mod trail;

pub use trail::draw_worm_trail;
