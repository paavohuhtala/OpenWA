//! Vtable patches for [`FireEffect`](openwa_game::engine::fire_effect::FireEffect).
//!
//! Replaces the destructor, tick, and apply-palette slots with their pure
//! Rust ports. Slot 3 (`Display__sub_524480`, 0x00524480) is left WA-side
//! because it has no known callers and hasn't been reverse-engineered.

use openwa_game::address::va;
use openwa_game::engine::fire_effect::{FireEffect, FireEffectVtable};
use openwa_game::vtable_replace;

pub fn install() -> Result<(), String> {
    vtable_replace!(FireEffectVtable, va::FIRE_EFFECT_VTABLE, {
        destructor    => FireEffect::destructor,
        tick          => FireEffect::tick,
        apply_palette => FireEffect::apply_palette,
    })?;
    Ok(())
}
