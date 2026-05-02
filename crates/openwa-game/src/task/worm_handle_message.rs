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
//! Messages handled by Rust must call the relevant pre-switches explicitly
//! before doing per-message work. Pre-switch A and B helpers below mirror
//! WA's preambles 1:1; reusing them keeps newly-ported branches correct
//! without taking on the full function.
//!
//! See `project_worm_handle_message_re.md` (memory) for the full RE state.

use core::sync::atomic::{AtomicU32, Ordering};
use openwa_core::fixed::Fixed;
use openwa_core::weapon::KnownWeaponId;

use super::base::BaseEntity;
use super::worm::{WormEntity, WormState};
use crate::address::va;
use crate::engine::team_arena::TeamArena;
use crate::game::message::{WeaponReleasedMessage, WormMovedMessage};
use crate::game::{EntityMessage, weapon_fire};
use crate::rebase::rb;

/// Original WA `WormEntity::HandleMessage` (0x00510B40), populated by
/// `vtable_replace!` at install time. Called for any message branch not
/// yet ported to Rust.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

/// Address of `AnimQueue::ReleaseSlot_Maybe` resolved from `va::ANIM_QUEUE_RELEASE_SLOT`
/// at `init_addrs()` time. Read by the naked trampoline.
static mut ANIM_QUEUE_RELEASE_SLOT_ADDR: u32 = 0;

/// Address of `WormEntity::NotifyMoved_Maybe` (0x0050F730), the WA helper
/// invoked from Pre-switch A.
static mut WORM_NOTIFY_MOVED_ADDR: u32 = 0;

/// Resolve runtime addresses for the bridges; called once from the DLL hook
/// install path.
pub unsafe fn init_addrs() {
    unsafe {
        ANIM_QUEUE_RELEASE_SLOT_ADDR = rb(va::ANIM_QUEUE_RELEASE_SLOT);
        WORM_NOTIFY_MOVED_ADDR = rb(va::WORM_ENTITY_NOTIFY_MOVED);
    }
}

/// Bridge for `AnimQueue::ReleaseSlot_Maybe` (`__usercall(EAX = queue,
/// [stack] = slot)`, RET 0x4). Caller passes `queue = world + 0x600`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_anim_queue_release_slot(_queue: *mut u8, _slot: i32) {
    core::arch::naked_asm!(
        "push ebx",
        "mov eax, dword ptr [esp+8]",  // queue (1 save + ret = 8)
        "push dword ptr [esp+12]",     // slot
        "mov ebx, dword ptr [{addr}]",
        "call ebx",                     // RET 0x4 cleans the slot push
        "pop ebx",
        "ret 8",                        // we own the 2 stdcall stack args
        addr = sym ANIM_QUEUE_RELEASE_SLOT_ADDR,
    );
}

/// Bridge for `WormEntity::NotifyMoved_Maybe` (`__usercall(ESI = this)`,
/// plain RET, no stack args).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_notify_moved(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",  // this (1 save + ret = 8)
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 4",
        addr = sym WORM_NOTIFY_MOVED_ADDR,
    );
}

// ── Pre-switch helpers ────────────────────────────────────────────────
//
// WA's `HandleMessage` runs two preambles before its main switch. Both gate
// only on `msg_type` (with a per-message conditional we encode at each
// caller), so any Rust-side message handler in the relevant range must run
// the preamble before its body to stay behavior-equivalent with WA.

/// Pre-switch A body — applies for messages
/// `0x1E,0x1F,0x22,0x23,0x24,0x25,0x26` unconditionally and for `0x20/0x21`
/// when `weapons_enabled != 0`, `0x2F-0x32` when `turn_active != 0`, and
/// `0x33` when both `data[3] != 0` and `turn_active != 0`. Releases the
/// worm's anim-queue slot, clears the per-frame stationary counter, fires
/// `NotifyMoved`, then zeroes the four odd-indexed aim-fade values.
unsafe fn pre_switch_a(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let queue = (world as *mut u8).add(0x600);
        let slot = (*this).slot_id as i32;
        bridge_anim_queue_release_slot(queue, slot);
        (*this).stationary_frames = 0;
        bridge_notify_moved(this);
        (*this).aim_fade[1] = Fixed(0);
        (*this).aim_fade[3] = Fixed(0);
        (*this).aim_fade[5] = Fixed(0);
        (*this).aim_fade[7] = Fixed(0);
    }
}

/// Pre-switch B body — applies for messages `0x1E,0x1F,0x22,0x23,0x24,0x25`
/// unconditionally and for `0x20/0x21` when `weapons_enabled != 0`. Pure
/// Rust port of `WormEntity::LandingCheck_Maybe`.
unsafe fn pre_switch_b(this: *mut WormEntity) {
    unsafe { WormEntity::landing_check_raw(this) }
}

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
        let Ok(msg) = EntityMessage::try_from(msg_type) else {
            return fall_through(this, sender, msg_type, size, data);
        };

        // Each arm returns `true` when WA's body did `return;` (we handled it
        // fully); `false` when WA's body did `break;` (fall through to the
        // parent class via the saved original handler).
        let handled = match msg {
            EntityMessage::MoveLeft => {
                msg_move_left(this);
                true
            }
            EntityMessage::MoveRight => {
                msg_move_right(this);
                true
            }
            EntityMessage::MoveUp => {
                msg_move_up(this);
                true
            }
            EntityMessage::MoveDown => {
                msg_move_down(this);
                true
            }
            EntityMessage::FaceLeft => {
                msg_face_left(this);
                true
            }
            EntityMessage::FaceRight => {
                msg_face_right(this);
                true
            }
            EntityMessage::ThinkingShow => msg_thinking_show(this),
            EntityMessage::ThinkingHide => {
                msg_thinking_hide(this);
                true
            }
            EntityMessage::ReleaseWeapon => {
                msg_release_weapon(this);
                true
            }
            EntityMessage::Freeze => {
                msg_freeze(this);
                true
            }
            EntityMessage::Unknown42 => msg_unknown_42(this),
            EntityMessage::TurnStarted => {
                msg_turn_started(this);
                true
            }
            EntityMessage::TurnFinished => msg_turn_finished(this),
            EntityMessage::RetreatStarted => {
                msg_retreat_started(this);
                true
            }
            EntityMessage::RetreatFinished => {
                msg_retreat_finished(this);
                true
            }
            EntityMessage::KillWorm => {
                msg_kill_worm(this, 1);
                true
            }
            EntityMessage::KillWorm2 => {
                msg_kill_worm(this, 2);
                true
            }
            EntityMessage::EnableWeapons => {
                msg_enable_weapons(this);
                true
            }
            EntityMessage::DisableWeapons => {
                msg_disable_weapons(this);
                true
            }
            EntityMessage::ResumeTurn => {
                msg_resume_turn(this);
                true
            }
            EntityMessage::WormMoved => msg_worm_moved(this, data as *const WormMovedMessage),
            EntityMessage::WeaponReleased => {
                msg_weapon_released(this, data as *const WeaponReleasedMessage)
            }
            EntityMessage::ScalesOfJustice => {
                msg_scales_of_justice(this);
                true
            }
            EntityMessage::MoveSpecial => {
                msg_move_special(this);
                true
            }
            EntityMessage::TurnEndMaybe => msg_turn_end_maybe(this),
            EntityMessage::BringForward => {
                msg_bring_forward(this);
                true
            }
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

/// MoveLeft (0x1E): pre-switches A and B run unconditionally, then set the
/// edge-triggered "move left" input request.
unsafe fn msg_move_left(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).input_msg_move_left = 1;
    }
}

/// MoveRight (0x1F): same shape as `MoveLeft`.
unsafe fn msg_move_right(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).input_msg_move_right = 1;
    }
}

/// MoveUp (0x20): pre-switches A and B only run when `weapons_enabled != 0`
/// (gated by WA's preamble jumptable). The body always sets the
/// edge-triggered "move up" input request.
unsafe fn msg_move_up(this: *mut WormEntity) {
    unsafe {
        if (*this).weapons_enabled != 0 {
            pre_switch_a(this);
            pre_switch_b(this);
        }
        (*this).input_msg_move_up = 1;
    }
}

/// MoveDown (0x21): same shape as `MoveUp`.
unsafe fn msg_move_down(this: *mut WormEntity) {
    unsafe {
        if (*this).weapons_enabled != 0 {
            pre_switch_a(this);
            pre_switch_b(this);
        }
        (*this).input_msg_move_down = 1;
    }
}

/// FaceLeft (0x22): unconditional pre-switches, then sets the pending
/// facing direction to -1.
unsafe fn msg_face_left(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).facing_direction_2 = -1;
    }
}

/// FaceRight (0x23): mirror of `FaceLeft`, sets pending facing to +1.
unsafe fn msg_face_right(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).facing_direction_2 = 1;
    }
}

unsafe fn msg_thinking_show(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).thinking_state != 1 {
            (*this).thinking_anim = 0;
            (*this).thinking_state = 1;
            true
        } else {
            false
        }
    }
}

/// Inlines `WormEntity__CommitCursorPos_Maybe` (0x00510370): snapshot
/// pos into the cursor-marker draw fields when the animator is showing.
unsafe fn msg_thinking_hide(this: *mut WormEntity) {
    unsafe {
        if (*this).thinking_state == 1 {
            (*this).thinking_state = 2;
            (*this).thinking_anim_pos_x = (*this).base.pos_x;
            (*this).thinking_anim_pos_y = (*this).base.pos_y;
        }
    }
}

unsafe fn msg_retreat_started(this: *mut WormEntity) {
    unsafe {
        (*this).retreat_active = 1;
    }
}

unsafe fn msg_retreat_finished(this: *mut WormEntity) {
    unsafe {
        (*this).retreat_active = 0;
    }
}

/// `kind = 1` for plain kill (msg 0x40), `2` for the variant (msg 0x41).
/// Read by `WormEntity::BehaviorTick` when the worm becomes idle to fire
/// the kill `SetState(0x82|0x84)`.
unsafe fn msg_kill_worm(this: *mut WormEntity, kind: u32) {
    unsafe {
        (*this).kill_request = kind;
    }
}

unsafe fn msg_enable_weapons(this: *mut WormEntity) {
    unsafe {
        (*this).weapons_enabled = 1;
    }
}

unsafe fn msg_move_special(this: *mut WormEntity) {
    unsafe {
        (*this).detonate_crate_flag = 1;
    }
}

/// Freeze (0x29): worm enters the `Dead` state. Sent by the Freeze weapon
/// (special subtype 20).
unsafe fn msg_freeze(this: *mut WormEntity) {
    unsafe {
        WormEntity::set_state_raw(this, WormState::Dead);
    }
}

/// Unknown42 (0x2A): when the worm is in the `Dead` state, transition back
/// to `Idle`. Otherwise WA falls through to the parent class.
unsafe fn msg_unknown_42(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).state() == WormState::Dead as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
            return true;
        }
        false
    }
}

/// BringForward (0x58): release the worm's anim-queue slot. Single call,
/// no preamble (msg outside `0x1E..=0x33`).
unsafe fn msg_bring_forward(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let queue = (world as *mut u8).add(0x600);
        bridge_anim_queue_release_slot(queue, (*this).slot_id as i32);
    }
}

/// ResumeTurn (0x36): unless the selected weapon is Teleport, register an
/// effect-event point at the worm's position; release the worm's anim-queue
/// slot; clear `turn_paused`. Counterpart of `PauseTurn` (msg 0x35).
///
/// At the WA call site `RegisterEventPoint` is invoked as
/// `(ECX = pos.x, EDX = pos.y, [stack] = this)` — the x/y are the saved
/// `local_82c`/`local_830` snapshots from the function preamble (worm pos
/// at +0x84/+0x88), not any local case-only computation.
unsafe fn msg_resume_turn(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        if (*this).selected_weapon != KnownWeaponId::Teleport {
            let pos_x = (*this).base.pos_x.0;
            let pos_y = (*this).base.pos_y.0;
            crate::engine::world::GameWorld::register_event_point_raw(world, pos_x, pos_y);
        }
        let queue = (world as *mut u8).add(0x600);
        bridge_anim_queue_release_slot(queue, (*this).slot_id as i32);
        (*this).turn_paused = 0;
    }
}

/// DisableWeapons (0x46): inlines `WormEntity__DeactivateOnIdle` —
/// transition `Active` to `Idle` and clear the per-turn weapons-enabled
/// flag.
unsafe fn msg_disable_weapons(this: *mut WormEntity) {
    unsafe {
        if (*this).state() == WormState::Active as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
        }
        (*this).weapons_enabled = 0;
    }
}

/// WormMoved (0x47): matched worm sets `took_damage_flag`. Falls through
/// to parent class on mismatch.
unsafe fn msg_worm_moved(this: *mut WormEntity, message: *const WormMovedMessage) -> bool {
    unsafe {
        if message.is_null() {
            return false;
        }
        if (*message).team_index == (*this).team_index
            && (*message).worm_index == (*this).worm_index
        {
            (*this).took_damage_flag = 1;
            true
        } else {
            false
        }
    }
}

/// ScalesOfJustice (0x5E): snapshot the team-arena health entry into
/// `target_health_raw` as `(health as u32) << 16` to match WA's display
/// layout (`00 00 XX 00`).
unsafe fn msg_scales_of_justice(this: *mut WormEntity) {
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

/// TurnEndMaybe (0x75): when the air-strike / pending-action latch
/// (`_unknown_208`) is set, decrement Teleport ammo and clear the latch.
unsafe fn msg_turn_end_maybe(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this)._unknown_208 != 0 {
            let world = (*(this as *const BaseEntity)).world;
            let arena: *mut TeamArena = &raw mut (*world).team_arena;
            weapon_fire::subtract_ammo((*this).team_index, arena, KnownWeaponId::Teleport as u32);
            (*this)._unknown_208 = 0;
            true
        } else {
            false
        }
    }
}

/// ReleaseWeapon (0x27): inlines `WormEntity__ReleaseWeapon_Maybe`
/// (0x0051C010). For Projectile (fire_type=1) weapons being held in state
/// `0x68`, transition to `0x69`.
unsafe fn msg_release_weapon(this: *mut WormEntity) {
    unsafe {
        if (*this).selected_weapon == KnownWeaponId::None {
            return;
        }
        if (*this).weapons_enabled == 0 {
            return;
        }
        let entry = (*this).active_weapon_entry;
        if entry.is_null() {
            return;
        }
        if (*entry).fire_type == 1 && (*this).state() == WormState::ActiveVariant_Maybe as u32 {
            WormEntity::set_state_raw(this, WormState::Unknown_0x69);
        }
    }
}

/// TurnStarted (0x38): clear several per-turn accumulators and quantize
/// the worm's aim angle when the saved-aim flag is set.
///
/// Inlines `WormEntity__QuantizeAimAngle_Maybe` (0x0051FD40):
/// ```text
/// aim_angle bucket  →  snapped value
/// [0..=0x3FFF]      →  0x8000
/// [0x4000..=0x7FFF] →  0
/// [0x8000..=0xBFFF] →  0x10000
/// [0xC000..=0xFFFF] →  0x8000
/// ```
unsafe fn msg_turn_started(this: *mut WormEntity) {
    unsafe {
        (*this).damage_stack_count = 0;
        (*this).cliff_fall_flag = 0;
        (*this).poison_source_mask = 0;
        (*this).facing_flag = 0;
        if (*this).saved_aim_flag != 0 {
            let aim = (*this).aim_angle;
            (*this).saved_aim_flag = 0;
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
        (*this).poison_tick_accum = 0;
    }
}

/// TurnFinished (0x39): clear poison-source bitmask on a specific
/// game-version range, then fall through to the parent class.
///
/// WA's check is `(uint)(game_version - 0x4E) < 5` — UNSIGNED wrapping
/// subtraction, so the range is exactly `[0x4E..=0x52]`. Doing the
/// arithmetic on the signed `i32` would falsely match values below 0x4E.
unsafe fn msg_turn_finished(this: *mut WormEntity) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*(*world).game_info).game_version;
        if (game_version as u32).wrapping_sub(0x4E) < 5 {
            (*this).poison_source_mask = 0;
        }
        false
    }
}

/// WeaponReleased (0x49): when a Bungee weapon (fire_type=4 Special,
/// subtype=15) is released by *this* worm, reset the 8 aim-fade animation
/// values (+0x378..+0x394) to `1.0` Fixed.
unsafe fn msg_weapon_released(
    this: *mut WormEntity,
    message: *const WeaponReleasedMessage,
) -> bool {
    unsafe {
        if message.is_null() {
            return false;
        }
        if (*message).team_index != (*this).team_index
            || (*message).worm_index != (*this).worm_index
        {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let entries = (*(*world).weapon_table).entries.as_ptr();
        let entry = entries.add((*message).weapon.0 as usize);
        if (*entry).fire_type != 4 {
            return false;
        }
        // fire_subtype lives at +0x34 (just after fire_type at +0x30).
        let subtype = *((entry as *const u8).add(0x34) as *const i32);
        if subtype != 0xF {
            return false;
        }
        (*this).aim_fade = [Fixed(0x10000); 8];
        true
    }
}
