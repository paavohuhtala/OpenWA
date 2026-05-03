//! Incremental port of `WormEntity::HandleMessage` (0x00510B40, vtable slot 2).
//!
//! Unported messages fall through to the original WA function (saved into
//! [`ORIGINAL_HANDLE_MESSAGE`] by `vtable_replace!`). WA runs two pre-switches
//! before the main switch; ported handlers in `0x1E..=0x33` must call
//! [`pre_switch_a`] / [`pre_switch_b`] with the same per-message gates WA
//! uses, otherwise behavior diverges silently.
//!
//! See `project_worm_handle_message_re.md` (memory) for full RE state.

use core::sync::atomic::{AtomicU32, Ordering};
use openwa_core::fixed::Fixed;
use openwa_core::weapon::KnownWeaponId;

use super::base::BaseEntity;
use super::game_task::WorldEntity;
use super::worm::{WormEntity, WormState};
use crate::address::va;
use crate::engine::EntityActivityQueue;
use crate::engine::team_arena::TeamArena;
use crate::engine::world::GameWorld;
use crate::game::message::{
    SelectArmingMessage, SelectCursorMessage, SelectWeaponMessage, WeaponReleasedMessage,
    WormMovedMessage,
};
use crate::game::{EntityMessage, weapon_fire};
use crate::rebase::rb;

/// Saved original `WormEntity::HandleMessage` (0x00510B40), populated by
/// `vtable_replace!` at install time.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

// Rebased helper addresses, initialized by `init_addrs()`. Read by the naked
// bridge trampolines below — must be statics, not inline values, so the
// trampolines can `mov eax, [addr]` without leaking them through registers
// the helpers expect to be set up.
static mut ENTITY_ACTIVITY_QUEUE_RESET_RANK_ADDR: u32 = 0;
static mut WORM_NOTIFY_MOVED_ADDR: u32 = 0;
static mut WORM_COMMIT_PENDING_HEALTH_ADDR: u32 = 0;
static mut WORM_CANCEL_ACTIVE_WEAPON_ADDR: u32 = 0;
static mut WORM_APPLY_DAMAGE_ADDR: u32 = 0;
static mut WORM_SELECT_WEAPON_ADDR: u32 = 0;
static mut WORM_START_FIRING_ADDR: u32 = 0;
static mut WORM_CLEAR_WEAPON_STATE_ADDR: u32 = 0;
static mut TEAM_ARENA_SET_ACTIVE_WORM_ADDR: u32 = 0;
static mut WORM_FINISH_TURN_CLEANUP_ADDR: u32 = 0;
static mut WORM_BROADCAST_WEAPON_NAME_ADDR: u32 = 0;
static mut WORM_BROADCAST_WEAPON_SETTINGS_ADDR: u32 = 0;
static mut WORM_SELECT_FUSE_ADDR: u32 = 0;
static mut WORM_SELECT_BOUNCE_ADDR: u32 = 0;
static mut WORM_SELECT_HERD_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        ENTITY_ACTIVITY_QUEUE_RESET_RANK_ADDR = rb(va::ENTITY_ACTIVITY_QUEUE_RESET_RANK);
        WORM_NOTIFY_MOVED_ADDR = rb(va::WORM_ENTITY_NOTIFY_MOVED);
        WORM_COMMIT_PENDING_HEALTH_ADDR = rb(va::WORM_ENTITY_COMMIT_PENDING_HEALTH);
        WORM_CANCEL_ACTIVE_WEAPON_ADDR = rb(va::WORM_ENTITY_CANCEL_ACTIVE_WEAPON);
        WORM_APPLY_DAMAGE_ADDR = rb(va::WORM_ENTITY_APPLY_DAMAGE);
        WORM_SELECT_WEAPON_ADDR = rb(va::WORM_ENTITY_SELECT_WEAPON);
        WORM_START_FIRING_ADDR = rb(va::WORM_ENTITY_START_FIRING);
        WORM_CLEAR_WEAPON_STATE_ADDR = rb(va::WORM_ENTITY_CLEAR_WEAPON_STATE);
        TEAM_ARENA_SET_ACTIVE_WORM_ADDR = rb(va::TEAM_ARENA_SET_ACTIVE_WORM);
        WORM_FINISH_TURN_CLEANUP_ADDR = rb(va::WORM_FINISH_TURN_CLEANUP);
        WORM_BROADCAST_WEAPON_NAME_ADDR = rb(va::WORM_ENTITY_BROADCAST_WEAPON_NAME);
        WORM_BROADCAST_WEAPON_SETTINGS_ADDR = rb(va::WORM_ENTITY_BROADCAST_WEAPON_SETTINGS);
        WORM_SELECT_FUSE_ADDR = rb(va::WORM_ENTITY_SELECT_FUSE);
        WORM_SELECT_BOUNCE_ADDR = rb(va::WORM_ENTITY_SELECT_BOUNCE);
        WORM_SELECT_HERD_ADDR = rb(va::WORM_ENTITY_SELECT_HERD);
    }
}

/// `__usercall(EAX = queue, [stack] = slot)`, RET 0x4. Resets the entity's
/// rank to "newest" and ages up younger slots — does NOT free the slot
/// (genuine free is `FreeSlotById` at 0x00541860, used only by destructors).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_reset_activity_rank(
    _queue: *mut EntityActivityQueue,
    _slot: i32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "mov eax, dword ptr [esp+8]",
        "push dword ptr [esp+12]",
        "mov ebx, dword ptr [{addr}]",
        "call ebx",
        "pop ebx",
        "ret 8",
        addr = sym ENTITY_ACTIVITY_QUEUE_RESET_RANK_ADDR,
    );
}

/// `__usercall(ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_notify_moved(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 4",
        addr = sym WORM_NOTIFY_MOVED_ADDR,
    );
}

/// `__usercall(ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_commit_pending_health(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 4",
        addr = sym WORM_COMMIT_PENDING_HEALTH_ADDR,
    );
}

/// `__usercall(ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_cancel_active_weapon(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 4",
        addr = sym WORM_CANCEL_ACTIVE_WEAPON_ADDR,
    );
}

/// `__usercall(ESI = this, [stack] = arg1, arg2)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_apply_damage(_this: *mut WormEntity, _arg1: i32, _arg2: i32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "push dword ptr [esp+16]",
        "push dword ptr [esp+16]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 12",
        addr = sym WORM_APPLY_DAMAGE_ADDR,
    );
}

/// `__usercall(EAX = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_firing(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov edx, dword ptr [{addr}]",
        "call edx",
        "ret 4",
        addr = sym WORM_START_FIRING_ADDR,
    );
}

/// `__usercall(ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_clear_weapon_state(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 4",
        addr = sym WORM_CLEAR_WEAPON_STATE_ADDR,
    );
}

/// `__usercall(EAX = team_arena_base, EDX = team_idx, ESI = activate_value)`,
/// plain RET, no stack args. Caller must pass `world + 0x4628` (the
/// TeamArena base) as the first arg — WA reads it as a teams-array pointer.
/// `flag = 0` deactivates; non-zero is stored as the active-worm marker.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_set_active_worm(
    _team_arena: *mut TeamArena,
    _team_idx: i32,
    _flag: i32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov eax, dword ptr [esp+8]",
        "mov edx, dword ptr [esp+12]",
        "mov esi, dword ptr [esp+16]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop esi",
        "ret 12",
        addr = sym TEAM_ARENA_SET_ACTIVE_WORM_ADDR,
    );
}

/// `__fastcall(ECX = entity_owning_world, EDX = arg)`, plain RET. Wraps
/// the call as Rust stdcall(this, arg) for ergonomics.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_finish_turn_cleanup(_this: *mut BaseEntity, _arg: i32) {
    core::arch::naked_asm!(
        "mov ecx, dword ptr [esp+4]",
        "mov edx, dword ptr [esp+8]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "ret 8",
        addr = sym WORM_FINISH_TURN_CLEANUP_ADDR,
    );
}

/// `__usercall(EDI = this, [stack] = weapon, ammo)`, RET 0x8. NB: this
/// helper takes `this` in EDI, not ECX or ESI — only WA helper to do so.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_select_weapon(_this: *mut WormEntity, _weapon: u32, _ammo: i32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "push dword ptr [esp+16]",
        "push dword ptr [esp+16]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop edi",
        "ret 12",
        addr = sym WORM_SELECT_WEAPON_ADDR,
    );
}

/// `__thiscall(ECX = this, [stack] = name_str_ptr, flag)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_broadcast_weapon_name(
    _this: *mut WormEntity,
    _name_str: *const core::ffi::c_char,
    _flag: i32,
) {
    core::arch::naked_asm!(
        "mov ecx, dword ptr [esp+4]",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "ret 12",
        addr = sym WORM_BROADCAST_WEAPON_NAME_ADDR,
    );
}

/// `__fastcall(ECX = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_broadcast_weapon_settings(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "mov ecx, dword ptr [esp+4]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "ret 4",
        addr = sym WORM_BROADCAST_WEAPON_SETTINGS_ADDR,
    );
}

/// `__usercall(EDX = fuse_value, ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_select_fuse(_this: *mut WormEntity, _value: i32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov edx, dword ptr [esp+12]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 8",
        addr = sym WORM_SELECT_FUSE_ADDR,
    );
}

/// `__usercall(EAX = bounce_value, ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_select_bounce(_this: *mut WormEntity, _value: i32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "mov eax, dword ptr [esp+12]",
        "call ecx",
        "pop esi",
        "ret 8",
        addr = sym WORM_SELECT_BOUNCE_ADDR,
    );
}

/// `__usercall(EAX = herd_value, ESI = this)`, plain RET, no stack args.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_select_herd(_this: *mut WormEntity, _value: i32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "mov eax, dword ptr [esp+12]",
        "call ecx",
        "pop esi",
        "ret 8",
        addr = sym WORM_SELECT_HERD_ADDR,
    );
}

/// Inlines `WormEntity::IsActionState_Maybe` (0x0050E800). The function is
/// a 2-entry jumptable indexed by `byte[(state - 0x68) + 0x50e820]`: byte=0
/// returns 1 (action), byte=1 returns 0. From the data table at 0x50e820,
/// **action** states are `0x68..=0x72`, `0x74`, `0x75`, `0x8A`. Inverted
/// from the function name's apparent intent — `0x73` and `0x76..=0x89`
/// (the "weapon active / firing" states) all return 0.
fn is_action_state(state: u32) -> bool {
    matches!(state, 0x68..=0x72 | 0x74 | 0x75 | 0x8A)
}

/// Inlines `WormEntity::CanFireSubtype16` (0x00516930) — true for states
/// `{0x78, 0x7B, 0x7C, 0x7D}` (jetpack, AimingAngle, RopeSwinging, PreFire).
fn can_fire_subtype_16(state: u32) -> bool {
    matches!(state, 0x78 | 0x7B | 0x7C | 0x7D)
}

/// Inlines `WormEntity::DeactivateOnIdle_Maybe` (0x0050F7F0).
unsafe fn deactivate_on_idle(this: *mut WormEntity) {
    unsafe {
        if (*this).state() == WormState::Active as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
        }
        (*this).weapons_enabled = 0;
    }
}

/// Inlines `WormEntity::BeginThinkingHide` (0x00510370). When the
/// thinking animation is currently shown (state 1), transition it to the
/// fading-out state (2) and snapshot the worm's position into the
/// fade-out anchor — the chevrons sprite stays at this `(x, y)` while the
/// rest of the worm continues to move.
unsafe fn begin_thinking_hide(this: *mut WormEntity) {
    unsafe {
        if (*this).thinking_state == 1 {
            (*this).thinking_state = 2;
            (*this).thinking_anim_pos_x = (*this).base.pos_x;
            (*this).thinking_anim_pos_y = (*this).base.pos_y;
        }
    }
}

/// Applies unconditionally for msgs `0x1E,0x1F,0x22,0x23,0x24,0x25,0x26`,
/// gated on `weapons_enabled != 0` for `0x20/0x21`, on `turn_active != 0`
/// for `0x2F..=0x32`, and on `data[3] != 0 && turn_active != 0` for `0x33`.
unsafe fn pre_switch_a(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let queue = &raw mut (*world).entity_activity_queue;
        let slot = (*this).activity_rank_slot as i32;
        bridge_reset_activity_rank(queue, slot);
        (*this).stationary_frames = 0;
        bridge_notify_moved(this);
        (*this).aim_fade[1] = Fixed(0);
        (*this).aim_fade[3] = Fixed(0);
        (*this).aim_fade[5] = Fixed(0);
        (*this).aim_fade[7] = Fixed(0);
    }
}

/// Applies unconditionally for msgs `0x1E,0x1F,0x22,0x23,0x24,0x25`, gated
/// on `weapons_enabled != 0` for `0x20/0x21`.
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

/// `true` from a handler ⇒ WA `return;` (fully handled). `false` ⇒ WA
/// `break;` ⇒ we re-enter the saved original to run pre-switches +
/// `WorldEntity::HandleMessage` parent dispatch.
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
            EntityMessage::TeamVictory => msg_team_victory(this),
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
            EntityMessage::Surrender => msg_surrender(this),
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
            EntityMessage::PauseTurn => {
                msg_pause_turn(this);
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
            EntityMessage::SelectWeapon => {
                msg_select_weapon(this, data as *const SelectWeaponMessage);
                true
            }
            EntityMessage::SelectFuse => {
                msg_select_fuse(this, data as *const SelectArmingMessage);
                true
            }
            EntityMessage::SelectHerd => {
                msg_select_herd(this, data as *const SelectArmingMessage);
                true
            }
            EntityMessage::SelectBounce => {
                msg_select_bounce(this, data as *const SelectArmingMessage);
                true
            }
            EntityMessage::SelectCursor => {
                msg_select_cursor(this, data as *const SelectCursorMessage);
                true
            }
            EntityMessage::AdvanceWorm => {
                msg_advance_worm(this);
                true
            }
            EntityMessage::ShowDamage => {
                msg_show_damage(this);
                true
            }
            EntityMessage::WeaponClaimControl => {
                msg_weapon_claim_control(this);
                true
            }
            EntityMessage::Unknown129 => {
                msg_unknown_129(this, data);
                true
            }
            EntityMessage::FireWeapon => {
                msg_fire_weapon(this);
                true
            }
            EntityMessage::StartTurn => msg_start_turn(this),
            EntityMessage::FinishTurn => {
                msg_finish_turn(this);
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

unsafe fn msg_move_left(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).input_msg_move_left = 1;
    }
}

unsafe fn msg_move_right(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).input_msg_move_right = 1;
    }
}

unsafe fn msg_move_up(this: *mut WormEntity) {
    unsafe {
        if (*this).weapons_enabled != 0 {
            pre_switch_a(this);
            pre_switch_b(this);
        }
        (*this).input_msg_move_up = 1;
    }
}

unsafe fn msg_move_down(this: *mut WormEntity) {
    unsafe {
        if (*this).weapons_enabled != 0 {
            pre_switch_a(this);
            pre_switch_b(this);
        }
        (*this).input_msg_move_down = 1;
    }
}

unsafe fn msg_face_left(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).facing_direction_2 = -1;
    }
}

unsafe fn msg_face_right(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        (*this).facing_direction_2 = 1;
    }
}

/// In `Transitional` state the body only sets the flags and falls through
/// to the parent class. The `vf4` checks pin two states (`0x7B`/`0x7C`)
/// from being kicked back to Idle on later WA versions.
unsafe fn msg_team_victory(this: *mut WormEntity) -> bool {
    unsafe {
        (*this)._field_14c = 1;
        (*this)._field_140 = 1;
        let state = (*this).state();
        if state == WormState::Transitional as u32 {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let vf4 = (*world).version_flag_4;
        let suppress = (vf4 >= 5 && state == 0x7B) || (vf4 >= 9 && state == 0x7C);
        if !suppress {
            WormEntity::set_state_raw(this, WormState::Idle);
        }
        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        GameWorld::register_event_point_raw(world, pos_x, pos_y);
        true
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

unsafe fn msg_thinking_hide(this: *mut WormEntity) {
    unsafe { begin_thinking_hide(this) }
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

/// `kind` is `1` for KillWorm and `2` for KillWorm2 — read later by
/// `BehaviorTick` to choose between `SetState(0x82|0x84)`.
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

unsafe fn msg_freeze(this: *mut WormEntity) {
    unsafe {
        WormEntity::set_state_raw(this, WormState::Dead);
    }
}

unsafe fn msg_unknown_42(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).state() == WormState::Dead as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
            return true;
        }
        false
    }
}

/// Surrender (0x2B): drop to Idle iff the per-worm action-pending flag is
/// set or the worm is currently the active turn-holder. Otherwise no
/// state change and no parent dispatch.
unsafe fn msg_surrender(this: *mut WormEntity) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let arena: *const TeamArena = &raw const (*world).team_arena;
        let entry = TeamArena::team_worm(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        if (*entry)._field_98 != 0 || (*this).state() == WormState::Active as u32 {
            WormEntity::set_state_raw(this, WormState::Idle);
            return true;
        }
        false
    }
}

unsafe fn msg_bring_forward(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let queue = &raw mut (*world).entity_activity_queue;
        bridge_reset_activity_rank(queue, (*this).activity_rank_slot as i32);
    }
}

/// The state-set in `[0x78, 0x7B, 0x7C, 0x7D]` test inlines WA's
/// `CanFireSubtype16` (0x00516930). The decomp's `extraout_ECX` reading
/// `[ECX+0x24]` after that call is a Ghidra artifact: the helper only
/// touches EAX, and the caller's ECX remained loaded with `world` — not
/// "this returned via ECX". So `world.game_info` is read independently here.
unsafe fn msg_pause_turn(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        (*this).turn_paused = 1;

        let game_version = (*(*world).game_info).game_version;
        let outer_a = (*world).version_flag_4 == 0;
        let outer_b = if !outer_a {
            if game_version > 0x1E6 {
                let arena: *const TeamArena = &raw const (*world).team_arena;
                let entry = TeamArena::team_worm(
                    arena,
                    (*this).team_index as usize,
                    (*this).worm_index as usize,
                );
                (*entry).health < 1
            } else {
                false
            }
        } else {
            false
        };
        if !outer_a && !outer_b {
            return;
        }
        let state = (*this).state();
        if !matches!(state, 0x78 | 0x7B | 0x7C | 0x7D) {
            return;
        }
        let new_state =
            if game_version < 0x1E7 || WorldEntity::is_moving_raw(this as *const WorldEntity) {
                WormState::PostFire_Maybe
            } else {
                WormState::Idle
            };
        WormEntity::set_state_raw(this, new_state);
    }
}

unsafe fn msg_resume_turn(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        if (*this).selected_weapon != KnownWeaponId::Teleport {
            let pos_x = (*this).base.pos_x;
            let pos_y = (*this).base.pos_y;
            GameWorld::register_event_point_raw(world, pos_x, pos_y);
        }
        let queue = &raw mut (*world).entity_activity_queue;
        bridge_reset_activity_rank(queue, (*this).activity_rank_slot as i32);
        (*this).turn_paused = 0;
    }
}

unsafe fn msg_disable_weapons(this: *mut WormEntity) {
    unsafe { deactivate_on_idle(this) }
}

/// StartTurn (0x34). Initializes per-turn state for the worm whose turn
/// is starting. Returns `false` only when `world.game_info.game_version <
/// -1` (the never-hit fall-through path); always `true` for valid games.
unsafe fn msg_start_turn(this: *mut WormEntity) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;

        if (*this).state() == 0x8B {
            WormEntity::set_state_raw(this, WormState::Idle);
        }

        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        GameWorld::register_event_point_raw(world, pos_x, pos_y);

        let queue = &raw mut (*world).entity_activity_queue;
        bridge_reset_activity_rank(queue, (*this).activity_rank_slot as i32);

        (*this)._field_1bc = 0;
        (*this).shot_data_1 = 0;
        (*this).shot_data_2 = 0;
        (*this).aim_fade = [Fixed(0x10000); 8];
        (*this).weapons_enabled = 1;
        (*this).turn_active = 1;

        let team_arena: *mut TeamArena = &raw mut (*world).team_arena;
        bridge_set_active_worm(
            team_arena,
            (*this).team_index as i32,
            (*this).worm_index as i32,
        );

        let template = (*world).localized_template;
        let resolved = crate::wa::localized_template::resolve_split_array_raw(template, 0x69D)
            as *const core::ffi::c_char;
        bridge_broadcast_weapon_name(this, resolved, 1);

        if (*this).selected_weapon != KnownWeaponId::None {
            bridge_broadcast_weapon_settings(this);
        }

        (*this).stationary_frames = 0;
        (*this).turn_start_field_5d8 = (*world)._field_5d8;

        let game_version = (*(*world).game_info).game_version;
        if game_version < -1 {
            return false;
        }
        if game_version < 0x103 {
            let level_w = (*world).level_width as i32;
            let level_h = (*world).level_height as i32;
            (*this).weapon_param_1 = (level_w << 16) / 2;
            (*this).weapon_param_2 = (level_h << 16) / 2;
            (*this)._field_2e8 = -1;
            (*this).weapon_param_3 = 1;
        } else {
            (*this)._field_2e8 = 0;
        }
        true
    }
}

/// FinishTurn (0x37). End-of-turn cleanup. Kicks the worm out of any
/// "action" state, tears down the active weapon, clears
/// turn_active/paused, deactivates the worm in the team registry, then
/// optionally settles into Idle/Hurt depending on motion + scheme
/// version. The post-`CanFireSubtype16` gate uses WA's `cStack_831` flag
/// to track "took the dying-state path" (game_version >= 0x1E7 AND
/// health <= 0): alive worms in v3.5+ schemes (`version_flag_4 != 0`)
/// skip the SetState when still moving, while pre-v3.5 or dying worms
/// transition to Hurt.
unsafe fn msg_finish_turn(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;

        let state = (*this).state();
        if (state.wrapping_sub(0x68) < 0x23) && is_action_state(state) {
            WormEntity::set_state_raw(this, WormState::Idle);
        }

        if (*this).shot_data_1 == 0 && (*this)._unknown_2cc == 0 {
            bridge_cancel_active_weapon(this);
        } else {
            bridge_clear_weapon_state(this);
        }

        (*this).shot_data_1 = u32::MAX; // -1 as i32
        (*this).shot_data_2 = u32::MAX;

        deactivate_on_idle(this);
        begin_thinking_hide(this);

        (*this).aim_fade[5] = Fixed(0x10000);
        (*this).aim_fade[7] = Fixed(0x10000);
        (*this).aim_fade[1] = Fixed(0);
        (*this).aim_fade[3] = Fixed(0);
        (*this).turn_paused = 0;
        (*this).turn_active = 0;

        let team_arena: *mut TeamArena = &raw mut (*world).team_arena;
        bridge_set_active_worm(team_arena, (*this).team_index as i32, 0);

        let state = (*this).state();
        if can_fire_subtype_16(state) {
            let game_version = (*(*world).game_info).game_version;
            let arena: *const TeamArena = team_arena;
            let entry = TeamArena::team_worm(
                arena,
                (*this).team_index as usize,
                (*this).worm_index as usize,
            );
            let alive = game_version < 0x1E7 || (*entry).health > 0;
            let dying = !alive;
            let vf4 = (*world).version_flag_4;

            let new_state = if alive && vf4 > 3 {
                None
            } else if !WorldEntity::is_moving_raw(this as *const WorldEntity) {
                Some(WormState::Idle)
            } else if !dying && vf4 != 0 {
                None
            } else {
                Some(WormState::Hurt)
            };
            if let Some(s) = new_state {
                WormEntity::set_state_raw(this, s);
            }
        }

        bridge_finish_turn_cleanup(this as *mut BaseEntity, 0xE);

        if (*world).version_flag_4 == 0 {
            (*this)._field_250 = 0;
        }
        (*this)._field_258 = 0;
        let now = (*world)._field_5d8;
        (*this).turn_end_field_5d8 = now;
        let arena_mut: *mut TeamArena = team_arena;
        let entry_mut = TeamArena::team_worm_mut(
            arena_mut,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        let delta = now.wrapping_sub((*this).turn_start_field_5d8);
        let seconds = delta.wrapping_add(999) / 1000;
        (*entry_mut).turn_action_counter_Maybe = (*entry_mut)
            .turn_action_counter_Maybe
            .wrapping_add(seconds as i32);
    }
}

/// Falls through to the parent class on mismatch — non-matching worms
/// still want WorldEntity's default WormMoved handling.
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

/// `<< 16` matches WA's `00 00 XX 00` display-health layout (the high byte
/// of the low word is the visible value; full u16 is at +0x17A).
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

/// `_unknown_208` is the air-strike / pending-Teleport latch set when the
/// player armed a Teleport but didn't commit before turn end.
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

/// Inlines `WormEntity::ReleaseWeapon_Maybe` (0x0051C010).
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

/// Aim-snap inlines `WormEntity::QuantizeAimAngle_Maybe` (0x0051FD40):
/// snaps to `{0, 0x8000, 0x10000}` based on which 0x4000-quadrant the angle
/// falls in (the two end quadrants both go to 0x8000, not 0 — important).
unsafe fn msg_turn_started(this: *mut WormEntity) {
    unsafe {
        (*this).damage_stack_count = 0;
        (*this).cliff_fall_flag = 0;
        (*this).poison_source_mask = 0;
        (*this).facing_flag = 0;
        if (*this).saved_aim_flag != 0 {
            let aim = (*this).aim_angle;
            (*this).saved_aim_flag = 0;
            (*this).aim_angle = if aim < Fixed(0x4000) {
                Fixed(0x8000)
            } else if aim <= Fixed(0x7FFF) {
                Fixed(0)
            } else if aim <= Fixed(0xBFFF) {
                Fixed(0x10000)
            } else {
                Fixed(0x8000)
            };
        }
        (*this).poison_tick_accum = 0;
    }
}

/// `game_version - 0x4E < 5` is UNSIGNED wrapping in WA → range is exactly
/// `[0x4E..=0x52]`. A signed subtraction would falsely match `< 0x4E`
/// (caused a wa11g desync during slice 3d before the cast was added).
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

/// Resets aim-fade animation when *this* worm releases a Bungee
/// (fire_type=4 Special, subtype=15).
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
        let subtype = *((entry as *const u8).add(0x34) as *const i32);
        if subtype != 0xF {
            return false;
        }
        (*this).aim_fade = [Fixed(0x10000); 8];
        true
    }
}

/// Pre-switch A is conditional (see [`pre_switch_a`] doc); the inner if
/// can no-op (mismatched worm_index or network mode) but the function
/// still returns without parent dispatch.
unsafe fn msg_select_weapon(this: *mut WormEntity, message: *const SelectWeaponMessage) {
    unsafe {
        if message.is_null() {
            return;
        }
        if (*message).ammo_count != 0 && (*this).turn_active != 0 {
            pre_switch_a(this);
        }
        if (*message).worm_index == (*this).worm_index && (*this)._unknown_2cc == 0 {
            bridge_select_weapon(this, (*message).weapon_id, (*message).ammo_count);
        }
    }
}

unsafe fn msg_advance_worm(this: *mut WormEntity) {
    unsafe {
        bridge_apply_damage(this, 1, 1);
    }
}

unsafe fn msg_show_damage(this: *mut WormEntity) {
    unsafe {
        bridge_commit_pending_health(this);
    }
}

unsafe fn msg_weapon_claim_control(this: *mut WormEntity) {
    unsafe {
        bridge_cancel_active_weapon(this);
    }
}

/// Self-call into `WormEntity::GetEntityData` (vt[3]) with query 0x7D1.
/// Payload: `[i32 _, i32 worm_index, i16 x, i16 y, ...]`. The two i16 coords
/// are sign-extended into a `[i32; 2]` out-buffer that vt[3] reads/writes.
/// `WormStartFiring` is bridged (551 inst, cyclo 108 — too large to port).
unsafe fn msg_fire_weapon(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        bridge_start_firing(this);
    }
}

/// SelectFuse (0x2F). Pre-switch A applies when `turn_active != 0`. Body
/// gates on `worm_index` match + `_unknown_2cc == 0`, then accepts a
/// `data.value` from `[lo-1, hi-1]` where `(lo, hi) = (1, 5)` by default,
/// `(1, 9)` when scheme byte `0xD9D0` is set and `0xD9B1 <= 0x1A`, or
/// `(0, 9)` when scheme byte `0xD9D0` is set and `0xD9B1 > 0x1A`. The
/// range check is bypassed when `game_version < -1`. The bridged helper
/// reads the (possibly mutated) value through ESI=this/EDX=value.
unsafe fn msg_select_fuse(this: *mut WormEntity, message: *const SelectArmingMessage) {
    unsafe {
        if (*this).turn_active != 0 {
            pre_switch_a(this);
        }
        if message.is_null() {
            return;
        }
        if (*message).worm_index != (*this).worm_index || (*this)._unknown_2cc != 0 {
            return;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info as *const u8;
        let scheme_d9d0 = *game_info.add(0xD9D0);
        let scheme_d9b1 = *game_info.add(0xD9B1) as i8;
        let game_version = (*(*world).game_info).game_version;

        let mut hi: i32 = 5;
        let mut lo: i32 = 1;
        if scheme_d9d0 != 0 {
            hi = 9;
            if scheme_d9b1 > 0x1A {
                lo = 0;
            }
        }
        let mut value = (*message).value;
        let in_range_unsigned =
            (value.wrapping_sub(lo).wrapping_add(1) as u32) <= (hi.wrapping_sub(lo) as u32);
        if !(game_version < -1 || in_range_unsigned) {
            return;
        }
        if value == -1 && scheme_d9b1 > 0x1F {
            value = 0xFF;
        }
        bridge_select_fuse(this, value);
    }
}

/// SelectHerd (0x30). Same gate shape as SelectFuse but with a simpler
/// `value in [1, hi]` range — no `-1` mutation. `hi = 5` by default;
/// `9 + (1 if scheme byte 0xD9B1 > 0x1A else 0)` when scheme byte 0xD9D0
/// is set.
unsafe fn msg_select_herd(this: *mut WormEntity, message: *const SelectArmingMessage) {
    unsafe {
        if (*this).turn_active != 0 {
            pre_switch_a(this);
        }
        if message.is_null() {
            return;
        }
        if (*message).worm_index != (*this).worm_index || (*this)._unknown_2cc != 0 {
            return;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info as *const u8;
        let scheme_d9d0 = *game_info.add(0xD9D0);
        let scheme_d9b1 = *game_info.add(0xD9B1) as i8;
        let game_version = (*(*world).game_info).game_version;

        let hi: i32 = if scheme_d9d0 != 0 {
            9 + i32::from(scheme_d9b1 > 0x1A)
        } else {
            5
        };
        let in_range = ((*message).value.wrapping_sub(1) as u32) <= (hi - 1) as u32;
        if !(game_version < -1 || in_range) {
            return;
        }
        bridge_select_herd(this, (*message).value);
    }
}

/// SelectBounce (0x31). No range check — pure pass-through to the bridge.
unsafe fn msg_select_bounce(this: *mut WormEntity, message: *const SelectArmingMessage) {
    unsafe {
        if (*this).turn_active != 0 {
            pre_switch_a(this);
        }
        if message.is_null() {
            return;
        }
        if (*message).worm_index != (*this).worm_index || (*this)._unknown_2cc != 0 {
            return;
        }
        bridge_select_bounce(this, (*message).value);
    }
}

/// SelectCursor (0x32). Pre-switch A is conditional on `turn_active != 0`
/// (matches WA's pre-switch gate for msgs `0x2F..=0x32`); the body itself
/// also requires `worm_index` match and `_unknown_2cc == 0`. The
/// `direction` sign + `button_id` range gates only apply on
/// `game_version >= -1`.
unsafe fn msg_select_cursor(this: *mut WormEntity, message: *const SelectCursorMessage) {
    unsafe {
        if (*this).turn_active != 0 {
            pre_switch_a(this);
        }
        if message.is_null() {
            return;
        }
        if (*message).worm_index != (*this).worm_index || (*this)._unknown_2cc != 0 {
            return;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*(*world).game_info).game_version;
        if game_version >= -1 {
            if (*message).direction.unsigned_abs() != 1 {
                return;
            }
            let lower_bound = if game_version < 0x21
                || ((*this).shot_data_2 as i32) < 1
                || ((*this).shot_data_1 as i32) < 1
            {
                1
            } else {
                0
            };
            if (*message).button_id < lower_bound || (*message).button_id > 0x12 {
                return;
            }
        }
        (*this).weapon_param_1 = ((*message).coord_x as i32) << 16;
        (*this).weapon_param_2 = ((*message).coord_y as i32) << 16;
        (*this)._field_2e8 = (*message).direction;
        (*this).weapon_param_3 = (*message).button_id;
        if ((*this).shot_data_1 as i32) == ((*this).shot_data_2 as i32).wrapping_sub(1) {
            WormEntity::landing_check_raw(this);
        }
    }
}

unsafe fn msg_unknown_129(this: *mut WormEntity, data: *const u8) {
    unsafe {
        if data.is_null() {
            return;
        }
        let worm_index = *(data.add(4) as *const u32);
        if worm_index != (*this).worm_index || (*this).turn_active == 0 || (*this).turn_paused != 0
        {
            return;
        }
        let packed = *(data.add(8) as *const u32);
        let mut out = [(packed as i16) as i32, ((packed >> 16) as i16) as i32];
        WormEntity::get_entity_data_raw(this, 0x7D1, 0x394, out.as_mut_ptr() as *mut u32);
    }
}
