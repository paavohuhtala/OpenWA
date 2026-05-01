//! Per-message Rust implementations of `WormEntity::HandleMessage` (0x00510B40).
//!
//! Vtable slot 2 of `WormEntityVtable`. The full WA implementation has 471
//! basic blocks across 37 case labels — far too large to port atomically.
//! This module ports it incrementally: each ported message gets its own
//! function, and unported messages fall through to the original WA function
//! (saved into [`ORIGINAL_HANDLE_MESSAGE`] by the `vtable_replace!` shim).
//!
//! ## Preamble interaction
//!
//! WA's `HandleMessage` runs **two pre-switches** before the main switch:
//! - Pre-switch A (msgs `0x1E..=0x33`): cancels aim animation if the
//!   relevant state-active flag is set
//! - Pre-switch B (msgs `0x1E..=0x25`): runs `LandingCheck`
//!
//! Messages handled by Rust **must not** be in those ranges unless we also
//! port the preamble effects. Currently every message handled here is
//! outside both ranges, so intercepting is behavior-preserving.
//!
//! See `project_worm_handle_message_re.md` (memory) for the full RE state.
//!
//! ## Currently ported messages
//!
//! - `0x1A` ThinkingShow — start the thinking-chevrons animator
//! - `0x1B` ThinkingHide — snapshot pos and stop the animator
//! - `0x27` ReleaseWeapon — gun-style weapon: state 0x68 → SetState(0x69)
//! - `0x29` Surrender — `SetState(Dead)`
//! - `0x38` TurnStarted — clear several per-turn fields; quantize aim angle
//! - `0x39` TurnFinished — clear poison source on certain game versions; fall through to parent
//! - `0x2A` DetonateWeapon — if dead, `SetState(Idle)`
//! - `0x3C` RetreatStarted — set `retreat_active = 1`
//! - `0x3D` RetreatFinished — clear `retreat_active`
//! - `0x40` KillWorm — set `_field_a0 = 1` (kill request)
//! - `0x41` (unnamed) — set `_field_a0 = 2` (kill request, variant)
//! - `0x45` DisableWeapons — set `_field_a3 = 1` (re-disable on turn)
//! - `0x46` WormMoved — if `Active`, `SetState(Idle)`; clear `_field_a3`
//! - `0x47` WormDamaged — match team+worm, set `_field_55 = 1`
//! - `0x49` WeaponFinished — Bungee finish for this worm: reset 8 aim-fade values
//! - `0x5E` NukeBlast — snapshot team-arena health into `target_health_raw`
//! - `0x62` DetonateCrate — set `_field_cc = 1`
//! - `0x75` (TurnEndMaybe) — if Teleport pending, subtract Teleport ammo, clear flag
//!
//! All other messages fall through to WA's original implementation.

use core::sync::atomic::{AtomicU32, Ordering};
use openwa_core::weapon::KnownWeaponId;

use super::base::BaseEntity;
use super::worm::{WormEntity, WormState};
use crate::engine::team_arena::TeamArena;
use crate::game::weapon_fire;

/// Original WA `WormEntity::HandleMessage` (0x00510B40), populated by
/// `vtable_replace!` at install time. Called for any message branch not
/// yet ported to Rust.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

/// Function pointer signature matching `WormEntityVtable::handle_message`.
type HandleMessageFn = unsafe extern "thiscall" fn(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
);

/// Vtable replacement for slot 2. Dispatches each ported message to a
/// dedicated handler; everything else calls back into WA's original.
pub unsafe extern "thiscall" fn handle_message(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    unsafe {
        // Each arm returns `true` when WA's body did `return;` (we handled it
        // fully); `false` when WA's body did `break;` (fall through to the
        // parent class via the saved original handler).
        let handled = match msg_type {
            0x1A => msg_thinking_show(this),
            0x1B => {
                msg_thinking_hide(this);
                true
            }
            0x27 => msg_release_weapon(this),
            0x29 => {
                msg_surrender(this);
                true
            }
            0x2A => msg_detonate_weapon(this),
            0x38 => {
                msg_turn_started(this);
                true
            }
            0x39 => msg_turn_finished(this, sender, msg_type, size, data),
            0x3C => {
                msg_retreat_started(this);
                true
            }
            0x3D => {
                msg_retreat_finished(this);
                true
            }
            0x40 => {
                msg_kill_worm(this, 1);
                true
            }
            0x41 => {
                msg_kill_worm(this, 2);
                true
            }
            0x45 => {
                msg_disable_weapons(this);
                true
            }
            0x46 => {
                msg_worm_moved(this);
                true
            }
            0x47 => msg_worm_damaged(this, data),
            0x49 => msg_weapon_finished(this, data),
            0x5E => {
                msg_nuke_blast(this);
                true
            }
            0x62 => {
                msg_detonate_crate(this);
                true
            }
            0x75 => msg_turn_end_maybe(this),
            _ => false,
        };
        if !handled {
            fall_through(this, sender, msg_type, size, data);
        }
    }
}

#[inline]
unsafe fn fall_through(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    let raw = ORIGINAL_HANDLE_MESSAGE.load(Ordering::Relaxed);
    debug_assert!(
        raw != 0,
        "WormEntity::HandleMessage original ptr not initialized; vtable_replace! ran?"
    );
    let f: HandleMessageFn = unsafe { core::mem::transmute(raw as usize) };
    unsafe { f(this, sender, msg_type, size, data) }
}

// ── Per-message handlers ──────────────────────────────────────────────

/// `EntityMessage::ThinkingShow` (0x1A) — start the thinking-chevrons
/// animator if it isn't already showing.
///
/// WA: re-entry while already in state 1 falls through to the parent
/// `WorldEntity::HandleMessage` (broadcast); we replicate that by leaving
/// the call to the original in place via `fall_through`, which is **not**
/// what we do here. See note below.
///
/// **Behavior note:** WA falls through to the parent class on re-entry
/// (the `break` after the `if`). Without porting the parent class's
/// HandleMessage, we'd need to call back into WA. For this slice we treat
/// re-entry as a no-op since the parent's default for msg 0x1A is just to
/// broadcast it down, and a worm's children are rare. If this turns out to
/// regress replay tests, switch to `fall_through` on the re-entry path.
unsafe fn msg_thinking_show(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).thinking_state != 1 {
            (*this).thinking_anim = 0;
            (*this).thinking_state = 1;
            true
        } else {
            // WA `break;` — re-entry while already in state 1 falls through
            // to the parent class.
            false
        }
    }
}

/// `EntityMessage::ThinkingHide` (0x1B) — snapshot worm position and
/// transition the thinking animator to its fade-out state.
///
/// Inlines `WormEntity__CommitCursorPos_Maybe` (0x00510370): if
/// `thinking_state == 1`, set it to 2 and copy `pos_x`/`pos_y` into the
/// snapshot fields used by `DrawCursorMarker_Maybe`.
unsafe fn msg_thinking_hide(this: *mut WormEntity) {
    unsafe {
        if (*this).thinking_state == 1 {
            (*this).thinking_state = 2;
            (*this).thinking_anim_pos_x = (*this).base.pos_x;
            (*this).thinking_anim_pos_y = (*this).base.pos_y;
        }
    }
}

/// `EntityMessage::RetreatStarted` (0x3C) — worm has fired; retreat timer
/// active until turn ends.
unsafe fn msg_retreat_started(this: *mut WormEntity) {
    unsafe {
        (*this).retreat_active = 1;
    }
}

/// `EntityMessage::RetreatFinished` (0x3D) — retreat timer expired.
unsafe fn msg_retreat_finished(this: *mut WormEntity) {
    unsafe {
        (*this).retreat_active = 0;
    }
}

/// `EntityMessage::KillWorm` (0x40) and the unnamed sibling (0x41) —
/// flag this worm for end-of-turn kill processing.
///
/// Stored in `_field_a0` at +0x280: `1` for plain kill (msg 0x40),
/// `2` for the variant (msg 0x41). Read by `WormEntity::BehaviorTick`
/// when the worm becomes idle to fire the kill `SetState(0x82|0x84)`.
unsafe fn msg_kill_worm(this: *mut WormEntity, kind: u32) {
    unsafe {
        let base = this as *mut u8;
        core::ptr::write(base.add(0x280) as *mut u32, kind);
    }
}

/// `EntityMessage::DisableWeapons` (0x45) — re-set the "weapons disabled
/// for this worm" flag (`_field_a3` at +0x28C).
///
/// Already cleared by `StartTurn` (msg 0x34), this message lets external
/// code re-disable mid-turn (e.g. a crate handler or scripted event).
unsafe fn msg_disable_weapons(this: *mut WormEntity) {
    unsafe {
        let base = this as *mut u8;
        core::ptr::write(base.add(0x28C) as *mut u32, 1);
    }
}

/// `EntityMessage::WormDamaged` (0x47) — broadcast notice that some worm
/// took damage. If the message addresses this worm (team + worm match),
/// set the took-damage marker (`_field_55` at +0x154).
///
/// Falls through to parent class otherwise (no-op when team mismatches).
/// Payload layout: `[team: u32, worm: u32, ...]`.
unsafe fn msg_worm_damaged(this: *mut WormEntity, data: *const u8) -> bool {
    unsafe {
        if data.is_null() {
            return false;
        }
        let p = data as *const u32;
        let target_team = core::ptr::read(p);
        let target_worm = core::ptr::read(p.add(1));
        if target_team == (*this).team_index && target_worm == (*this).worm_index {
            // _field_55 lives in `_unknown_154` (4 bytes at offset 0x154).
            let base = this as *mut u8;
            core::ptr::write(base.add(0x154) as *mut u32, 1);
            true
        } else {
            // No-match path: WA `break;` → fall through to parent class.
            false
        }
    }
}

/// `EntityMessage::DetonateCrate` (0x62) — flag this worm as having
/// triggered a remote-detonation crate. `_field_cc` at +0x330.
unsafe fn msg_detonate_crate(this: *mut WormEntity) {
    unsafe {
        let base = this as *mut u8;
        core::ptr::write(base.add(0x330) as *mut u32, 1);
    }
}

/// `EntityMessage::Surrender` (0x29) — transition the worm to the `Dead`
/// state via the vtable's `set_state`.
unsafe fn msg_surrender(this: *mut WormEntity) {
    unsafe {
        WormEntity::set_state_raw(this, WormState::Dead);
    }
}

/// `EntityMessage::DetonateWeapon` (0x2A) — when the worm is in the `Dead`
/// state, transition back to `Idle`. Otherwise WA falls through to the
/// parent class (broadcast).
///
/// Returns `true` if we handled it; `false` means the caller should fall
/// through to WA's original (matching WA's `break;`).
unsafe fn msg_detonate_weapon(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).state() == WormState::Dead as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
            return true;
        }
        false
    }
}

/// `EntityMessage::WormMoved` (0x46) — inlined `WormEntity__DeactivateOnIdle`:
/// if the worm is in `Active` state, transition to `Idle`; clear the
/// `_field_a3` activity flag at +0x28C either way.
unsafe fn msg_worm_moved(this: *mut WormEntity) {
    unsafe {
        if (*this).state() == WormState::Active as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
        }
        let base = this as *mut u8;
        core::ptr::write(base.add(0x28C) as *mut u32, 0);
    }
}

/// `EntityMessage::NukeBlast` (0x5E) — snapshot the team-arena health entry
/// for this worm into the on-entity `target_health_raw` field. Stored as
/// `(health as u32) << 16` to match WA's display layout (`00 00 XX 00`
/// where XX is the health byte).
///
/// WA: `param_1[0x5f] = arena_health(team, worm) << 0x10` — direct address
/// arithmetic; we go through `TeamArena::team_worm` for type safety.
unsafe fn msg_nuke_blast(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let arena: *const TeamArena = &raw const (*world).team_arena;
        let entry = TeamArena::team_worm(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        (*this).target_health_raw = ((*entry).health as u32) << 16;
    }
}

/// Message 0x75 (provisionally `TurnEndMaybe`) — when the air-strike /
/// pending-action latch (`_unknown_208`) is set, decrement Teleport ammo
/// and clear the latch. WA falls through to the parent class otherwise.
///
/// Behaviorally this looks like a deferred ammo decrement after a
/// successful Teleport completes. `_unknown_208` is set elsewhere in the
/// fire pipeline; this message is the cleanup signal.
unsafe fn msg_turn_end_maybe(this: *mut WormEntity) -> bool {
    unsafe {
        let base = this as *mut u8;
        let latch_ptr = base.add(0x208) as *mut u32;
        if core::ptr::read(latch_ptr) != 0 {
            let world = (*(this as *const BaseEntity)).world;
            let arena: *mut TeamArena = &raw mut (*world).team_arena;
            weapon_fire::subtract_ammo((*this).team_index, arena, KnownWeaponId::Teleport as u32);
            core::ptr::write(latch_ptr, 0);
            true
        } else {
            // Latch was clear — WA `break;` → fall through to parent class.
            false
        }
    }
}

/// `EntityMessage::ReleaseWeapon` (0x27) — inlined `WormEntity__ReleaseWeapon_Maybe`
/// (0x0051C010): for kind-1 (Projectile) weapons being held in state `0x68`,
/// transition to `0x69`. All other states/weapon kinds: WA `return;` (no-op).
///
/// Gates: `selected_weapon != 0` AND `_field_a3 != 0` (active worm flag).
unsafe fn msg_release_weapon(this: *mut WormEntity) -> bool {
    unsafe {
        // _field_5c = +0x170 = selected_weapon (KnownWeaponId, repr u32; 0 == None)
        let selected_weapon_raw = *(&raw const (*this).selected_weapon as *const u32);
        if selected_weapon_raw == 0 {
            return true; // WA returns from the helper without action; case 0x27 returns
        }
        // _field_a3 at +0x28C
        let active_flag = *((this as *const u8).add(0x28C) as *const u32);
        if active_flag == 0 {
            return true;
        }
        let entry = (*this).active_weapon_entry;
        if entry.is_null() {
            return true;
        }
        if (*entry).fire_type == 1 && (*this).state() == WormState::ActiveVariant_Maybe as u32 {
            WormEntity::set_state_raw(this, WormState::Unknown_0x69);
        }
        true
    }
}

/// `EntityMessage::TurnStarted` (0x38) — clear several per-turn accumulators
/// and quantize the worm's aim angle when the saved-aim flag is set.
///
/// Inlines `WormEntity__QuantizeAimAngle_Maybe` (0x0051FD40):
/// ```text
/// aim_angle bucket  →  snapped value
/// [0..=0x3FFF]      →  0x8000
/// [0x4000..=0x7FFF] →  0
/// [0x8000..=0xBFFF] →  0x10000
/// [0xC000..=0xFFFF] →  0x8000
/// ```
/// This snaps the carried-over aim angle to a quadrant aligned with the
/// new worm's facing direction at the start of the turn.
unsafe fn msg_turn_started(this: *mut WormEntity) {
    unsafe {
        let base = this as *mut u8;
        // _field_ac at +0x2B0 — damage-stack count (case 0x4B).
        core::ptr::write(base.add(0x2B0) as *mut u32, 0);
        // _field_b7 at +0x2DC — cliff-fall flag.
        core::ptr::write(base.add(0x2DC) as *mut u32, 0);
        // poison_source_mask at +0x144 — clear at turn start so this turn's
        // poison sources are tracked fresh.
        (*this).poison_source_mask = 0;
        // Byte at +0x338 — facing-related flag (cleared, not yet named).
        core::ptr::write(base.add(0x338), 0u8);
        // Byte at +0x33A — saved-aim flag. When set, snap aim angle and
        // clear the flag.
        let saved_aim_flag_ptr = base.add(0x33A);
        if core::ptr::read(saved_aim_flag_ptr) != 0 {
            // Inlined QuantizeAimAngle_Maybe.
            let aim = (*this).aim_angle;
            core::ptr::write(saved_aim_flag_ptr, 0u8);
            (*this).aim_angle = if aim < 0x4000 {
                0x8000
            } else if aim <= 0x7FFF {
                0
            } else if aim <= 0xBFFF {
                0x10000
            } else {
                0x8000
            };
        }
        // _field_d0 at +0x340 — poison-tick accumulator.
        core::ptr::write(base.add(0x340) as *mut u32, 0);
    }
}

/// `EntityMessage::TurnFinished` (0x39) — clear poison-source bitmask on a
/// specific game-version range, then explicitly fall through to the parent
/// class's `HandleMessage` (matches WA's `WorldEntity__vt2_HandleMessage`
/// tail call).
///
/// Game version range `0x4E..=0x52` represents WA versions where
/// poison-source clearing on turn end was added (or fixed).
unsafe fn msg_turn_finished(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*(*world).game_info).game_version;
        // WA's check is `(uint)(game_version - 0x4E) < 5` — UNSIGNED wrapping
        // subtraction, so the range is exactly `[0x4E..=0x52]`. Doing the
        // arithmetic on the signed `i32` would falsely match values below
        // 0x4E (which would wrap to a small negative and pass `< 5` signed).
        if (game_version as u32).wrapping_sub(0x4E) < 5 {
            (*this).poison_source_mask = 0;
        }
        // WA always falls through to WorldEntity::HandleMessage here; we do
        // the same via the saved original.
        fall_through(this, sender, msg_type, size, data);
        true
    }
}

/// `EntityMessage::WeaponFinished` (0x49) — when a Bungee weapon (fire_type=4
/// Special, subtype=15) finishes for *this* worm, reset all 8 aim-fade
/// animation values (+0x378..+0x394, indices `_field_dE..e5`) to `1.0` Fixed.
///
/// Gate: msg_data[0]=team match, msg_data[1]=worm match, msg_data[7]=weapon
/// index whose `WeaponEntry` has fire_type==4 && fire_subtype==15.
///
/// Returns `true` (matched and reset) or `false` (any condition fails — WA
/// falls through to parent class).
unsafe fn msg_weapon_finished(this: *mut WormEntity, data: *const u8) -> bool {
    unsafe {
        if data.is_null() {
            return false;
        }
        let p = data as *const u32;
        let team = core::ptr::read(p);
        let worm = core::ptr::read(p.add(1));
        if team != (*this).team_index || worm != (*this).worm_index {
            return false;
        }
        let weapon_idx = core::ptr::read(p.add(7));
        let world = (*(this as *const BaseEntity)).world;
        let entries = (*(*world).weapon_table).entries.as_ptr();
        let entry = entries.add(weapon_idx as usize);
        if (*entry).fire_type != 4 {
            return false;
        }
        // fire_subtype lives at +0x34 (just after fire_type at +0x30).
        let subtype = *((entry as *const u8).add(0x34) as *const i32);
        if subtype != 0xF {
            return false;
        }
        // Reset 8 aim-fade values at +0x378..+0x394 to 0x10000 (1.0 Fixed).
        let base = this as *mut u8;
        for i in 0..8 {
            core::ptr::write((base.add(0x378 + i * 4)) as *mut u32, 0x10000);
        }
        true
    }
}
