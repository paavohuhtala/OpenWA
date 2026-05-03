//! Incremental port of `WormEntity::HandleMessage` (0x00510B40, vtable slot 2).
//!
//! Unported messages fall through to the original WA function (saved into
//! [`ORIGINAL_HANDLE_MESSAGE`] by `vtable_replace!`). WA runs two pre-switches
//! before the main switch; ported handlers in `0x1E..=0x33` must call
//! [`pre_switch_a`] / [`pre_switch_b`] with the same per-message gates WA
//! uses, otherwise behavior diverges silently.
//!
//! See `project_worm_handle_message_re.md` (memory) for full RE state.

use core::ffi::c_char;
use core::sync::atomic::{AtomicU32, Ordering};
use openwa_core::fixed::Fixed;
use openwa_core::weapon::{FireType, KnownWeaponId};

use super::base::BaseEntity;
use super::game_entity::WorldEntity;
use super::worm::{KnownWormState, WormEntity, WormState};
use crate::address::va;
use crate::audio::SoundId;
use crate::audio::sound_ops as sound;
use crate::engine::EntityActivityQueue;
use crate::engine::team_arena::{TeamArena, WormEntry};
use crate::engine::world::GameWorld;
use crate::game::game_entity_message::world_entity_handle_message;
use crate::game::message::{
    DamageWormsMessage, ExplosionMessage, PoisonWormMessage, SelectArmingMessage,
    SelectCursorMessage, SelectWeaponMessage, SpecialImpactMessage, Unknown129Message,
    WeaponReleasedMessage, WormMovedMessage,
};
use crate::game::{EntityMessage, weapon_fire};
use crate::rebase::rb;

/// Subtype on a [`FireType::Special`] weapon entry that triggers the
/// WeaponReleased aim-fade reset (msg 0x49). Empirically the Bungee
/// weapon — slot 15 has no [`openwa_core::weapon::SpecialFireSubtype`]
/// variant yet, so we keep it as a named constant here.
const BUNGEE_SPECIAL_SUBTYPE: i32 = 0xF;

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
// FUN_00562EF0 — broadcasts a message via the SharedData parent observer.
// `__usercall(EAX = lookup_task)` + 5 stack args (sender, key_edi, msg_id,
// size, payload), RET 0x14. Caller must set ECX = key_esi (= 0 for the
// WorldRoot dispatch used by case 0x51 PoisonWorm).
static mut BROADCAST_VIA_SHARED_DATA_ADDR: u32 = 0;
// FUN_005480F0 — picks a random non-null entry from a string-pointer array
// and dispatches it through the SharedData random-text channel. Stdcall
// (this, name_array, count_kind, worm_name_ptr), RET 0x10.
static mut LOCALIZED_TEXT_RANDOM_PICK_ADDR: u32 = 0;
// PlayImpactSound_Maybe (0x004FF020) — usercall(EDI = this), 2 stack args
// (sound_id, mag), RET 0x8. Reads the sound emitter pointer from
// `[EDI+0xE0]`. Used by SpecialImpact (msg 0x4B) to play the corpse-hit
// sound during the Dead-state branch.
static mut WORM_PLAY_IMPACT_SOUND_ADDR: u32 = 0;
// WormEntity::PlaySound_Maybe (0x00515020) — usercall(EDI = this), 3 stack
// args (sound_id, vol, channel), RET 0xC. Stops the worm's currently-held
// sound handle (at `this+0x3B4`) before starting the new one.
static mut WORM_PLAY_SOUND_ADDR: u32 = 0;
// WormEntity::SpawnDamageParticles_Maybe (0x005108D0) —
// usercall(EAX = damage, ECX = this), 4 stack args (worm_x, worm_y, msg_x,
// msg_y), RET 0x10. Bails out when damage <= 2.
static mut WORM_SPAWN_DAMAGE_PARTICLES_ADDR: u32 = 0;
// WormEntity::HitTestRopeLine_Maybe (0x00501210) — fastcall(ECX = this,
// EDX = pos_x), 2 stack args (rope_param, pos_y), RET 0x8. Returns
// nonzero when an active rope at `this+0xBC` intersects the explosion at
// (pos_x, pos_y). The `rope_param` arg comes from the message's offset
// 0x10 (treated as a pre-multiplier `(arg + 2) << 17`).
static mut WORM_HIT_TEST_ROPE_LINE_ADDR: u32 = 0;

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
        BROADCAST_VIA_SHARED_DATA_ADDR = rb(0x00562EF0);
        LOCALIZED_TEXT_RANDOM_PICK_ADDR = rb(0x005480F0);
        WORM_PLAY_IMPACT_SOUND_ADDR = rb(0x004FF020);
        WORM_PLAY_SOUND_ADDR = rb(0x00515020);
        WORM_SPAWN_DAMAGE_PARTICLES_ADDR = rb(0x005108D0);
        WORM_HIT_TEST_ROPE_LINE_ADDR = rb(0x00501210);
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

/// `__usercall(EAX = lookup_task, ECX = key_esi)` + stdcall stack args
/// `(sender, key_edi, msg_id, size, payload)`, RET 0x14. The bridge sets
/// `ECX = 0` since every WormEntity caller routes through the WorldRoot
/// (key `(0, 0x14)`).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_broadcast_via_shared_data(
    _this: *mut WormEntity,
    _sender: *mut BaseEntity,
    _key_edi: u32,
    _msg_id: u32,
    _size: u32,
    _payload: *const u8,
) {
    core::arch::naked_asm!(
        "push ebx",
        "mov eax, dword ptr [esp+8]",
        "xor ecx, ecx",
        "push dword ptr [esp+28]",
        "push dword ptr [esp+28]",
        "push dword ptr [esp+28]",
        "push dword ptr [esp+28]",
        "push dword ptr [esp+28]",
        "mov ebx, dword ptr [{addr}]",
        "call ebx",
        "pop ebx",
        "ret 24",
        addr = sym BROADCAST_VIA_SHARED_DATA_ADDR,
    );
}

/// Plain stdcall tail-jump — args fall through unchanged.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_localized_text_random_pick(
    _this: *mut WormEntity,
    _name_array: *const *const c_char,
    _kind: u32,
    _worm_name_ptr: *const c_char,
) {
    core::arch::naked_asm!(
        "jmp dword ptr [{addr}]",
        addr = sym LOCALIZED_TEXT_RANDOM_PICK_ADDR,
    );
}

/// `__usercall(EDI = this, [stack] = sound_id, mag)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_play_impact_sound(
    _this: *mut WormEntity,
    _sound_id: u32,
    _mag: Fixed,
) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "push dword ptr [esp+16]",
        "push dword ptr [esp+16]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop edi",
        "ret 12",
        addr = sym WORM_PLAY_IMPACT_SOUND_ADDR,
    );
}

/// `__usercall(EDI = this, [stack] = sound_id, vol, channel)`, RET 0xC.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_play_sound(
    _this: *mut WormEntity,
    _sound_id: u32,
    _vol: Fixed,
    _channel: u32,
) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "push dword ptr [esp+20]",
        "push dword ptr [esp+20]",
        "push dword ptr [esp+20]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop edi",
        "ret 16",
        addr = sym WORM_PLAY_SOUND_ADDR,
    );
}

/// `__usercall(EAX = damage, ECX = this, [stack] = wx, wy, mx, my)`, RET 0x10.
/// The native function bails out when `damage <= 2`, so callers can pass
/// any damage value — the gate runs inside.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_spawn_damage_particles(
    _this: *mut WormEntity,
    _damage: i32,
    _wx: Fixed,
    _wy: Fixed,
    _mx: Fixed,
    _my: Fixed,
) {
    core::arch::naked_asm!(
        "mov ecx, dword ptr [esp+4]",
        "mov eax, dword ptr [esp+8]",
        "push dword ptr [esp+24]",
        "push dword ptr [esp+24]",
        "push dword ptr [esp+24]",
        "push dword ptr [esp+24]",
        "mov edx, dword ptr [{addr}]",
        "call edx",
        "ret 24",
        addr = sym WORM_SPAWN_DAMAGE_PARTICLES_ADDR,
    );
}

/// `__fastcall(ECX = this, EDX = pos_x, [stack] = rope_param, pos_y)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_hit_test_rope_line(
    _this: *mut WormEntity,
    _pos_x: Fixed,
    _rope_param: u32,
    _pos_y: Fixed,
) -> u32 {
    core::arch::naked_asm!(
        "mov ecx, dword ptr [esp+4]",
        "mov edx, dword ptr [esp+8]",
        "push dword ptr [esp+16]",
        "push dword ptr [esp+16]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "ret 16",
        addr = sym WORM_HIT_TEST_ROPE_LINE_ADDR,
    );
}

/// 5% of remaining gap, with a constant-step floor of `0x20C` so the
/// fade always reaches its target in finite time. Used by the two
/// `EaseAimVec` helpers below.
const AIM_FADE_RATE: Fixed = Fixed(0xCCC);
const AIM_FADE_MIN_STEP: Fixed = Fixed(0x20C);

/// 10% of remaining gap with the same fraction as a min-step floor —
/// matches WA's `FUN_00546a90` (0x00546a90) being called with
/// `EAX = 0x1999` from `EaseAuxValue`.
const AUX_VALUE_RATE: Fixed = Fixed(0x1999);
const AUX_VALUE_MIN_STEP: Fixed = Fixed(0x1999);

/// Rust port of `WormEntity::EaseAimVecA_Maybe` (0x0050E630). Eases
/// `aim_fade[4]` toward `aim_fade[5]` and `aim_fade[6]` toward
/// `aim_fade[7]`. The inlined ease primitive in WA is bit-identical to
/// [`Fixed::smooth_move_towards`].
unsafe fn ease_aim_vec_a(this: *mut WormEntity) {
    unsafe {
        let target_5 = (*this).aim_fade[5];
        (*this).aim_fade[4].smooth_move_towards(target_5, AIM_FADE_MIN_STEP, AIM_FADE_RATE);
        let target_7 = (*this).aim_fade[7];
        (*this).aim_fade[6].smooth_move_towards(target_7, AIM_FADE_MIN_STEP, AIM_FADE_RATE);
    }
}

/// Rust port of `WormEntity::EaseAimVecB_Maybe` (0x0050E500). Eases
/// `aim_fade[0]` toward `aim_fade[1]` and `aim_fade[2]` toward
/// `aim_fade[3]`.
unsafe fn ease_aim_vec_b(this: *mut WormEntity) {
    unsafe {
        let target_1 = (*this).aim_fade[1];
        (*this).aim_fade[0].smooth_move_towards(target_1, AIM_FADE_MIN_STEP, AIM_FADE_RATE);
        let target_3 = (*this).aim_fade[3];
        (*this).aim_fade[2].smooth_move_towards(target_3, AIM_FADE_MIN_STEP, AIM_FADE_RATE);
    }
}

/// Rust port of `WormEntity::EaseAuxValue_Maybe` (0x0050FB10). Eases
/// `_field_398` toward `_field_39c` via WA's `FUN_00546a90` primitive,
/// then — when the eased value is non-zero AND the worm holds the turn
/// — suppresses `aim_fade[5]` and `aim_fade[7]` so the aim arrow stops
/// re-targeting.
unsafe fn ease_aux_value(this: *mut WormEntity) {
    unsafe {
        let target = (*this)._field_39c;
        (*this)
            ._field_398
            .smooth_move_towards(target, AUX_VALUE_MIN_STEP, AUX_VALUE_RATE);
        if (*this)._field_398 != Fixed::ZERO && (*this).turn_active != 0 {
            (*this).aim_fade[5] = Fixed::ZERO;
            (*this).aim_fade[7] = Fixed::ZERO;
        }
    }
}

/// Inlines `WormEntity::IsActionState_Maybe` (0x0050E800). The function is
/// a 2-entry jumptable indexed by `byte[(state - 0x68) + 0x50e820]`: byte=0
/// returns 1 (action), byte=1 returns 0. From the data table at 0x50e820,
/// **action** states are `0x68..=0x72`, `0x74`, `0x75`, `0x8A`. Inverted
/// from the function name's apparent intent — `0x73` and `0x76..=0x89`
/// (the "weapon active / firing" states) all return 0.
fn is_action_state(state: WormState) -> bool {
    state.is_between(KnownWormState::ActiveVariant_Maybe..=KnownWormState::Unknown_0x72)
        || state.is_any_of(&[
            KnownWormState::TeleportCancelled_Maybe,
            KnownWormState::SuicideBomber,
            KnownWormState::Unknown_0x8A,
        ])
}

/// Inlines `WormEntity::CanFireSubtype16` (0x00516930) — true for states
/// `{0x78, 0x7B, 0x7C, 0x7D}` (jetpack, AimingAngle, RopeSwinging, PreFire).
fn can_fire_subtype_16(state: WormState) -> bool {
    state.is_any_of(&[
        KnownWormState::WeaponAimed_Maybe,
        KnownWormState::AimingAngle_Maybe,
        KnownWormState::RopeSwinging,
        KnownWormState::PreFire_Maybe,
    ])
}

/// Inlines `WormEntity::DeactivateOnIdle_Maybe` (0x0050F7F0).
unsafe fn deactivate_on_idle(this: *mut WormEntity) {
    unsafe {
        if (*this).state().is(KnownWormState::Active) {
            WormEntity::set_state_raw(this, KnownWormState::Idle);
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
        (*this).aim_fade[1] = Fixed::ZERO;
        (*this).aim_fade[3] = Fixed::ZERO;
        (*this).aim_fade[5] = Fixed::ZERO;
        (*this).aim_fade[7] = Fixed::ZERO;
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
            EntityMessage::Jump => msg_jump(this),
            EntityMessage::JumpUp => {
                msg_jump_up(this);
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
            EntityMessage::PoisonWorm => msg_poison_worm(this, data as *const PoisonWormMessage),
            EntityMessage::SpecialImpact => msg_special_impact(this, sender, size, data),
            EntityMessage::Explosion | EntityMessage::ProjectileImpact => {
                msg_explosion(this, sender, msg, size, data)
            }
            EntityMessage::DamageWorms => msg_damage_worms(this, data as *const DamageWormsMessage),
            EntityMessage::ApplyPoison => msg_apply_poison(this),
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
                msg_unknown_129(this, data as *const Unknown129Message);
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
            EntityMessage::UpdateNonCritical => {
                msg_update_non_critical(this);
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

/// `vf4` gates pin states `0x7B`/`0x7C` from being kicked back to Idle on
/// later WA versions.
unsafe fn msg_team_victory(this: *mut WormEntity) -> bool {
    unsafe {
        (*this)._field_14c = 1;
        (*this)._field_140 = 1;
        let state = (*this).state();
        if state.is(KnownWormState::Transitional) {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let vf4 = (*world).version_flag_4;
        let suppress = (vf4 >= 5 && state.is(KnownWormState::AimingAngle_Maybe))
            || (vf4 >= 9 && state.is(KnownWormState::RopeSwinging));
        if !suppress {
            WormEntity::set_state_raw(this, KnownWormState::Idle);
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
        WormEntity::set_state_raw(this, KnownWormState::Dead);
    }
}

unsafe fn msg_unknown_42(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).state().is(KnownWormState::Dead) {
            WormEntity::set_state_raw(this, KnownWormState::Idle);
            return true;
        }
        false
    }
}

unsafe fn msg_surrender(this: *mut WormEntity) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let arena: *const TeamArena = &raw const (*world).team_arena;
        let entry = TeamArena::team_worm(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        if (*entry)._field_98 != 0 || (*this).state().is(KnownWormState::Active) {
            WormEntity::set_state_raw(this, KnownWormState::Idle);
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

/// The decomp's `extraout_ECX` after `CanFireSubtype16` is a Ghidra artifact
/// — the helper only touches EAX; the caller's ECX still holds `world`.
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
        if !state.is_any_of(&[
            KnownWormState::WeaponAimed_Maybe,
            KnownWormState::AimingAngle_Maybe,
            KnownWormState::RopeSwinging,
            KnownWormState::PreFire_Maybe,
        ]) {
            return;
        }
        let new_state =
            if game_version < 0x1E7 || WorldEntity::is_moving_raw(this as *const WorldEntity) {
                KnownWormState::PostFire_Maybe
            } else {
                KnownWormState::Idle
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

/// Returns `false` only on `game_version < -1` — the never-hit
/// fall-through path. Always `true` for valid games.
unsafe fn msg_start_turn(this: *mut WormEntity) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;

        if (*this).state().is(KnownWormState::Unknown_0x8B) {
            WormEntity::set_state_raw(this, KnownWormState::Idle);
        }

        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        GameWorld::register_event_point_raw(world, pos_x, pos_y);

        let queue = &raw mut (*world).entity_activity_queue;
        bridge_reset_activity_rank(queue, (*this).activity_rank_slot as i32);

        (*this)._field_1bc = 0;
        (*this).shot_data_1 = 0;
        (*this).shot_data_2 = 0;
        (*this).aim_fade = [Fixed::ONE; 8];
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

/// The post-`CanFireSubtype16` gate uses WA's `cStack_831` flag to track
/// "took the dying-state path" (`game_version >= 0x1E7 && health <= 0`):
/// alive worms in v3.5+ schemes (`version_flag_4 != 0`) skip the SetState
/// when still moving; pre-v3.5 or dying worms transition to Hurt.
unsafe fn msg_finish_turn(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;

        let state = (*this).state();
        if state.is_between(KnownWormState::ActiveVariant_Maybe..=KnownWormState::Unknown_0x8A)
            && is_action_state(state)
        {
            WormEntity::set_state_raw(this, KnownWormState::Idle);
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

        (*this).aim_fade[5] = Fixed::ONE;
        (*this).aim_fade[7] = Fixed::ONE;
        (*this).aim_fade[1] = Fixed::ZERO;
        (*this).aim_fade[3] = Fixed::ZERO;
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
                Some(KnownWormState::Idle)
            } else if !dying && vf4 != 0 {
                None
            } else {
                Some(KnownWormState::Hurt)
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

/// Mismatched team/worm-index falls through so the parent's default
/// WormMoved handling still runs.
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
        if (*entry).fire_type == 1 && (*this).state().is(KnownWormState::ActiveVariant_Maybe) {
            WormEntity::set_state_raw(this, KnownWormState::Unknown_0x69);
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
                Fixed::ZERO
            } else if aim <= Fixed(0xBFFF) {
                Fixed::ONE
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
        let entry = &(*(*world).weapon_table).entries[(*message).weapon.0 as usize];
        if entry.fire_type != FireType::Special as i32 {
            return false;
        }
        if entry.special_subtype != BUNGEE_SPECIAL_SUBTYPE {
            return false;
        }
        (*this).aim_fade = [Fixed::ONE; 8];
        true
    }
}

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

/// `WormStartFiring` is bridged (551 inst, cyclo 108 — too large to port).
unsafe fn msg_fire_weapon(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        bridge_start_firing(this);
    }
}

/// The byte at `WormEntity+0x12A` is a per-worm input-restriction flag set
/// by the constructor from spawn init data byte 26 — semantics still TBD.
unsafe fn msg_jump_up(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        pre_switch_b(this);
        let restrict_jump = *(this as *const u8).add(0x12A);
        if restrict_jump != 0 {
            return;
        }
        let state = (*this).state();
        if state.is(KnownWormState::WeaponSelected_Maybe) && (*this)._field_29c == 0 {
            (*this)._field_2a0 = 1;
        }
        if state.is_any_of(&[
            KnownWormState::Idle,
            KnownWormState::IdleVariant_Maybe,
            KnownWormState::Active,
            KnownWormState::Unknown_0x88,
            KnownWormState::Unknown_0x8B,
        ]) {
            WormEntity::set_state_raw(this, KnownWormState::WeaponSelected_Maybe);
            (*this)._field_29c = 0;
            (*this)._field_2a0 = 0;
        }
    }
}

/// `0x24` (Jump). Mirror of `JumpUp` (msg 0x25) for the press edge.
///
/// The `WeaponCharging` (0x73) sub-arm jumps to the firing-tick block at the
/// function tail (`switchD_0051283d_caseD_73` in WA), which is not yet
/// ported. Returning `false` from that one branch hands the entire case off
/// to WA's saved original — the dispatcher then runs WA's pre-switches A+B
/// plus the firing block in one shot, with no double-execution.
///
/// Calling-convention note: WA dispatches `vt[0x44]` (slot 17 = AddImpulse)
/// in the `RopeSwinging` arm with literal `0` for `impulse_x`/`dz`; WA's
/// signed read of `Fixed` velocity here matches our `Fixed::add_impulse_raw`.
unsafe fn msg_jump(this: *mut WormEntity) -> bool {
    unsafe {
        let state = (*this).state();
        // WA snapshots `angle` at function entry into a stack slot the 0x7B
        // arm later reloads — capture it here too. (Ghidra's decomp wrongly
        // sourced this slot from `pos_x`; verified against the asm at
        // 0x0051293F: `MOV EAX, [ESP+0x20]` reads the saved-angle slot.)
        let angle_at_entry = (*this).base.angle.0;
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let game_version = (*game_info).game_version;

        // Input-restriction gate (WA: per-worm input-lock byte at +0x12A
        // AND the WorldEntity gate at +0xBC == 0). When tripped, only state
        // 0x78 / 0x7D bypass it on game_version >= 0x1E.
        let input_locked = *(this as *const u8).add(0x12A) != 0 && (*this).base._field_bc == 0;
        let gate_blocks = input_locked
            && (game_version < 0x1E
                || !state.is_any_of(&[
                    KnownWormState::WeaponAimed_Maybe,
                    KnownWormState::PreFire_Maybe,
                ]));

        // 0x73 falls into the firing-tick block at the function tail and is
        // not yet ported; hand it off to WA's saved original.
        if !gate_blocks && state.is(KnownWormState::WeaponCharging_Maybe) {
            return false;
        }

        pre_switch_a(this);
        pre_switch_b(this);

        if gate_blocks {
            return true;
        }

        let scheme_facing = (*game_info)._scheme_d926 as u32;

        if state.is_any_of(&[
            KnownWormState::Idle,
            KnownWormState::IdleVariant_Maybe,
            KnownWormState::Active,
            KnownWormState::Unknown_0x88,
            KnownWormState::Unknown_0x8B,
        ]) {
            (*this)._field_29c = 1;
            (*this)._field_2a0 = 0;
            WormEntity::set_state_raw(this, KnownWormState::WeaponSelected_Maybe);
            return true;
        }

        match state.0 as u8 {
            0x77 => {
                (*this)._field_29c = -1;
            }
            0x78 => {
                (*this)._field_15c = scheme_facing;
                bridge_play_sound(this, 0x1B, Fixed::ONE, 3);
                (*this).base.speed_x = Fixed(0);
                let to_post_fire = (*game_info)._scheme_d96e == 0
                    && WorldEntity::is_moving_raw(this as *const WorldEntity);
                let new_state = if to_post_fire {
                    KnownWormState::PostFire_Maybe
                } else {
                    KnownWormState::Idle
                };
                WormEntity::set_state_raw(this, new_state);
            }
            0x7B => {
                (*this)._field_15c = scheme_facing;
                WormEntity::set_state_raw(this, KnownWormState::WeaponCharging_Maybe);
                let raw = angle_at_entry.wrapping_sub(0x8000) as u32 & 0xFFFF;
                let clamped = if raw < 0x6666 {
                    0x6666
                } else if raw > 0x9999 {
                    0x9999
                } else {
                    raw
                };
                (*this)._field_264 = clamped;
            }
            0x7C => {
                (*this)._field_15c = scheme_facing;
                WorldEntity::add_impulse_raw(
                    this as *mut WorldEntity,
                    Fixed(0),
                    Fixed((*this)._field_1e8),
                    0,
                );
                WormEntity::set_state_raw(this, KnownWormState::WeaponCharging_Maybe);
            }
            0x7D => {
                (*this)._field_15c = scheme_facing;
                let new_state = if WorldEntity::is_moving_raw(this as *const WorldEntity) {
                    KnownWormState::PostFire_Maybe
                } else {
                    KnownWormState::Idle
                };
                WormEntity::set_state_raw(this, new_state);
            }
            _ => {}
        }
        true
    }
}

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
        let game_info = (*world).game_info;
        let scheme_d9d0 = (*game_info)._scheme_d9d0;
        let scheme_d9b1 = (*game_info)._scheme_d9b1;
        let game_version = (*game_info).game_version;

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
        let game_info = (*world).game_info;
        let scheme_d9d0 = (*game_info)._scheme_d9d0;
        let scheme_d9b1 = (*game_info)._scheme_d9b1;
        let game_version = (*game_info).game_version;

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

/// PoisonWorm (0x51). Accumulates `damage` into `worm.poison_damage` once
/// per `source_bit`. The alliance gate at `world + team_idx*0x51c + 0x462c`
/// reads the per-team alliance group and selects the friendly-fire
/// (game_info+0xD95C) or enemy-fire (game_info+0xD95D) scheme byte; values
/// > 2 block the application. Returns `false` to fall through to the
/// parent — the case body breaks out of WA's outer switch when its
/// preconditions don't hold, matching dispatcher fall-through semantics.
unsafe fn msg_poison_worm(this: *mut WormEntity, message: *const PoisonWormMessage) -> bool {
    unsafe {
        if message.is_null() {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*(*world).game_info).game_version;
        let mut source_bit = (*message).source_bit as u32;
        if game_version < 0x53 {
            source_bit = 1;
        }
        if (source_bit & (*this).poison_source_mask) != 0
            || (*this).state().is(KnownWormState::Dead)
        {
            return false;
        }
        if alliance_blocks_damage(world, (*message).sender_team_index, (*this).team_index) {
            return true; // WA's `return;` — handled, no parent dispatch
        }
        (*this).poison_damage = (*this).poison_damage.wrapping_add((*message).damage);
        (*this).poison_source_mask |= source_bit;

        let payload: [i32; 4] = [
            (*this).team_index as i32,
            (*this).worm_index as i32,
            (*message).damage,
            0,
        ];
        bridge_broadcast_via_shared_data(
            this,
            this as *mut BaseEntity,
            0x14,
            0x48,
            0x408,
            payload.as_ptr() as *const u8,
        );

        let template = (*world).localized_template;
        let resolved = crate::wa::localized_template::resolve_split_array_raw(template, 0x6CF)
            as *const *const c_char;
        bridge_localized_text_random_pick(
            this,
            resolved,
            0x17,
            &raw const (*this).worm_name as *const c_char,
        );
        true
    }
}

unsafe fn msg_special_impact(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) -> bool {
    unsafe {
        let message = data as *const SpecialImpactMessage;
        if message.is_null() {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let damage_kind = (*message).flag;

        if alliance_blocks_damage(world, (*message).source_team_index, (*this).team_index) {
            return true;
        }

        // Dead-state: bracket the parent dispatch with a save/restore of
        // `speed_x` so the impulse the parent applies to a corpse is undone
        // (modern schemes preserve the pre-impact speed; pre-499 schemes
        // zero it out instead).
        if (*this).state().is(KnownWormState::Dead) {
            let saved_speed_x = (*this).base.speed_x;
            bridge_play_impact_sound(this, 0x6A, Fixed::ONE);
            world_entity_handle_message(
                this as *mut WorldEntity,
                sender,
                EntityMessage::SpecialImpact,
                size,
                data,
            );
            (*this).base.speed_x = if (*game_info).game_version < 499 {
                Fixed::ZERO
            } else {
                saved_speed_x
            };
            return true;
        }

        if (*this).damage_lockout_flag != 0 {
            return true;
        }
        (*this).damage_lockout_flag = 1;
        if damage_kind == 6 {
            (*this).cliff_fall_flag = 1;
        }

        // States that ignore impact damage entirely — the lockout was set
        // above so subsequent SpecialImpacts this frame still no-op.
        let state = (*this).state();

        // TODO: This logic is duplicated at least twice, extract to a function when we understand it
        if state.is_any_of(&[
            KnownWormState::Dying1_Maybe,
            KnownWormState::Dying2_Maybe,
            KnownWormState::Unknown_0x85,
            KnownWormState::Kamikaze,
            KnownWormState::SuicideBomber,
        ]) {
            return true;
        }

        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::SpecialImpact,
            size,
            data,
        );

        let msg_damage = (*message).damage;
        if msg_damage != 0 {
            (*this).damage_event_accum = (*this).damage_event_accum.wrapping_add(msg_damage);
        }

        // Entry-guard facing-fade copy: `_scheme_d95b` enables the writeback,
        // gated additionally to skip damage_kind==10 in modern schemes.
        if (*game_info)._scheme_d95b != 0 && ((*game_info).game_version < 0x90 || damage_kind != 10)
        {
            (*this)._field_15c = (*game_info)._scheme_d926 as u32;
        }

        // The kind 2/7 branch shrinks the working damage by the new stack
        // count — the second chain-damage hit does ½ damage, the third ⅓,
        // etc. Subsequent steps (early-out, halving, particle spawn, scale,
        // drown accumulator) all use this post-divide value.
        let mut working_damage = msg_damage;
        match damage_kind {
            0 | 4 | 5 | 6 | 8 => {
                (*this)._field_15c = (*game_info)._scheme_d926 as u32;
            }
            1 => {
                (*this).drown_marker = Fixed::ONE;
            }
            2 | 7 => {
                (*this).damage_stack_count = (*this).damage_stack_count.wrapping_add(1);
                working_damage /= (*this).damage_stack_count as i32;
            }
            _ => {}
        }

        // Per-team rate-limited damage grunt: 24-frame cooldown.
        if ((*world).frame_counter - (*this).last_damage_sound_frame) > 0x18 {
            let rng = (*world).advance_rng();
            let sound_id = (*world).team_damage_grunt_id((*this).team_index, (rng & 0xFF) % 3);
            bridge_play_sound(this, sound_id, Fixed::ONE, 3);
            (*this).last_damage_sound_frame = (*world).frame_counter;
        }

        // Velocity-based Hurt/HurtAlt state pick — skipped when the
        // damage-halving flag is active.
        if (*this).base._field_a4 == 0 {
            let new_state = match damage_kind {
                1 => KnownWormState::Drowning,
                9 => KnownWormState::Dead1,
                _ => KnownWormState::Hurt,
            };
            WormEntity::set_state_raw(this, new_state);
        }

        // The kind != 10 / working_damage == 0 combination is a no-op past
        // this point; bail out to match WA's early-return.
        if working_damage == 0 && damage_kind != 10 {
            return true;
        }

        let damage_for_particles = if (*this).base._field_a4 != 0 {
            working_damage / 2 + 1
        } else {
            working_damage
        };
        bridge_spawn_damage_particles(
            this,
            damage_for_particles,
            (*this).base.pos_x,
            (*this).base.pos_y,
            (*message).pos_x,
            (*message).pos_y,
        );

        let arena: *mut TeamArena = &raw mut (*world).team_arena;
        let entry = TeamArena::team_worm_mut(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );

        if damage_kind != 10 {
            if damage_kind == 1 {
                (*this).drown_damage_accum = (*this)
                    .drown_damage_accum
                    .wrapping_add(damage_for_particles);
            }
            if (*world).terrain_pct_b != 0 {
                return true;
            }
            let scaled = ((*world)._field_5f0 as i32).wrapping_mul(damage_for_particles);
            apply_raw_damage_unchecked(this, entry, scaled);
        } else {
            // Percentage damage. The "health == 1 && msg.damage != 0"
            // special-case still funnels through the same helper as the
            // general path — WA spelled it out as separate code (one of
            // those LAB_005126f5 / LAB_00511409 fall-throughs).
            let entry_health = (*entry).health;
            let damage_amount = if entry_health == 1 && msg_damage != 0 {
                1
            } else {
                ((entry_health as i64).wrapping_mul(msg_damage as i64) / 100) as i32
            };
            if (*world).terrain_pct_b != 0 {
                return true;
            }
            apply_raw_damage_unchecked(this, entry, damage_amount);
        }
        true
    }
}

/// Shared body for `0x1C Explosion` and `0x76 ProjectileImpact`. WA's
/// `WormEntity::HandleMessage` switch falls both cases through to this
/// code; we dispatch them together and pass the original `msg_type` to the
/// parent so children of `WorldEntity::HandleMessage` see the right kind.
unsafe fn msg_explosion(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    msg_type: EntityMessage,
    size: u32,
    data: *const u8,
) -> bool {
    unsafe {
        let message = data as *const ExplosionMessage;
        if message.is_null() {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;

        // The message's `owner_id` field is empirically a team_index for
        // the alliance gate's purposes (matches WA `world + team * 0x51C +
        // 0x462C`).
        if alliance_blocks_damage(world, (*message).owner_id, (*this).team_index) {
            return true;
        }

        let state = (*this).state();
        if state.is_any_of(&[
            KnownWormState::Dying1_Maybe,
            KnownWormState::Dying2_Maybe,
            KnownWormState::Unknown_0x85,
            KnownWormState::Kamikaze,
            KnownWormState::SuicideBomber,
        ]) {
            return true;
        }

        if state.is(KnownWormState::Dead) {
            let saved_speed_x = (*this).base.speed_x;
            world_entity_handle_message(this as *mut WorldEntity, sender, msg_type, size, data);
            (*this).base.damage_accum = 0;
            (*this).base.speed_x = if (*game_info).game_version < 499 {
                Fixed::ZERO
            } else {
                saved_speed_x
            };
            return true;
        }

        if (*game_info)._scheme_d95b != 0 {
            (*this)._field_15c = (*game_info)._scheme_d926 as u32;
        }
        if (*game_info).game_version < 0xD9 && state.is(KnownWormState::AirStrikePending_Maybe) {
            WormEntity::set_state_raw(this, KnownWormState::Hurt);
        }

        world_entity_handle_message(this as *mut WorldEntity, sender, msg_type, size, data);
        let damage_accum = (*this).base.damage_accum;
        (*this).base.damage_accum = 0;

        if bridge_hit_test_rope_line(this, (*message).pos_x, (*message).damage, (*message).pos_y)
            != 0
        {
            WormEntity::set_state_raw(this, KnownWormState::WeaponCharging_Maybe);
        }

        if damage_accum != 0 {
            (*this).damage_event_accum = (*this).damage_event_accum.wrapping_add(damage_accum);
            let halved = if (*this).base._field_a4 != 0 {
                damage_accum / 2 + 1
            } else {
                damage_accum
            };
            bridge_spawn_damage_particles(
                this,
                halved,
                (*this).base.pos_x,
                (*this).base.pos_y,
                (*message).pos_x,
                (*message).pos_y,
            );
            if (*world).terrain_pct_b == 0 {
                let scaled = ((*world)._field_5f0 as i32).wrapping_mul(halved);
                let arena: *mut TeamArena = &raw mut (*world).team_arena;
                let entry = TeamArena::team_worm_mut(
                    arena,
                    (*this).team_index as usize,
                    (*this).worm_index as usize,
                );
                apply_raw_damage_unchecked(this, entry, scaled);
            }
            // Velocity-based Hurt/Dead1 state pick + facing direction set.
            // Skipped when the damage-halve flag is active.
            if (*this).base._field_a4 == 0 {
                let vx = (*this).base.speed_x.0;
                let vy = (*this).base.speed_y.0;
                let mag = vx.wrapping_abs().wrapping_add(vy.wrapping_abs());
                let new_state = if mag < 0x70000 {
                    KnownWormState::Hurt
                } else {
                    KnownWormState::Dead1
                };
                WormEntity::set_state_raw(this, new_state);
                if vx < -0x6666 {
                    (*this).facing_direction_2 = -1;
                } else if vx > 0x6666 {
                    (*this).facing_direction_2 = 1;
                }
            }
        }
        true
    }
}

/// DamageWorms (0x3B). Two paths gated on the sign of the message's
/// first i32. Positive: apply at most `msg_damage` HP but never kill —
/// the worm is left at ≥ 1 HP. Negative: damage to exactly 1 HP
/// (regardless of magnitude). Both end at the shared
/// `apply_raw_damage_unchecked` tail.
unsafe fn msg_damage_worms(this: *mut WormEntity, message: *const DamageWormsMessage) -> bool {
    unsafe {
        if message.is_null() {
            return false;
        }
        let world = (*(this as *const BaseEntity)).world;
        let arena: *mut TeamArena = &raw mut (*world).team_arena;
        let entry = TeamArena::team_worm_mut(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        let msg_damage = (*message).damage;
        let old_health = (*entry).health;
        let applied = if msg_damage >= 0 {
            let clamped_new = old_health.wrapping_sub(msg_damage).max(1);
            if old_health <= clamped_new {
                return true;
            }
            old_health - clamped_new
        } else {
            old_health.wrapping_sub(1)
        };
        if (*world).terrain_pct_b != 0 {
            return true;
        }
        apply_raw_damage_unchecked(this, entry, applied);
        true
    }
}

/// ApplyPoison (0x3E). Per-tick poison application using
/// `(poison_tick_accum - poison_damage)` as the running delta — keeps
/// the cumulative damage equal to `poison_damage` regardless of how
/// many ApplyPoisons fire. Like 0x3B path A, the new health is floored
/// at 1 (poison can't kill). The accumulator snapshot only updates on
/// game versions > 0x187.
unsafe fn msg_apply_poison(this: *mut WormEntity) -> bool {
    unsafe {
        if (*this).state() == KnownWormState::Dead.into() {
            return true;
        }
        let poison_damage = (*this).poison_damage;
        if poison_damage == 0 {
            return true;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let arena: *mut TeamArena = &raw mut (*world).team_arena;
        let entry = TeamArena::team_worm_mut(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        let old_health = (*entry).health;
        let candidate_new = ((*this).poison_tick_accum as i32)
            .wrapping_sub(poison_damage)
            .wrapping_add(old_health);
        if (*game_info).game_version > 0x187 {
            (*this).poison_tick_accum = poison_damage as u32;
        }
        let clamped_new = candidate_new.max(1);
        if old_health <= clamped_new {
            return true;
        }
        let applied = old_health - clamped_new;
        if (*world).terrain_pct_b != 0 {
            return true;
        }
        apply_raw_damage_unchecked(this, entry, applied);
        true
    }
}

/// Health-decrement + score broadcast tail shared by the damage paths
/// (cases 0x1C/0x76, 0x3B, 0x3E, 0x4B). Caller must have already cleared
/// the `world.terrain_pct_b` freeze gate. `raw_damage` is the pre-clamp
/// damage to apply; the helper consults `_scheme_d94a` to decide whether
/// `damage_taken_this_turn` accumulates the pre- or post-clamp value, and
/// broadcasts msg 0x48 (Damaged) via the SharedData (0, 0x14) lookup once
/// any health was actually subtracted.
unsafe fn apply_raw_damage_unchecked(
    this: *mut WormEntity,
    entry: *mut WormEntry,
    raw_damage: i32,
) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        if (*game_info)._scheme_d94a != 0 {
            (*this).damage_taken_this_turn =
                (*this).damage_taken_this_turn.wrapping_add(raw_damage);
        }
        if raw_damage < 1 {
            return;
        }
        let old_health = (*entry).health;
        if old_health < 1 {
            return;
        }
        let new_health = (old_health.wrapping_sub(raw_damage)).max(0);
        let applied = old_health - new_health;
        if (*game_info)._scheme_d94a == 0 {
            (*this).damage_taken_this_turn = (*this).damage_taken_this_turn.wrapping_add(applied);
        }
        (*entry).damage_event_score = (*entry).damage_event_score.wrapping_add(applied);
        (*entry).health = new_health;
        if (*this)._field_184 != 0 {
            (*this)._field_180 = 0;
        }
        if applied == 0 {
            return;
        }
        let is_not_kamikaze = if !(*this).state().is(KnownWormState::Kamikaze) {
            1i32
        } else {
            0i32
        };
        let payload: [i32; 5] = [
            (*this).team_index as i32,
            (*this).worm_index as i32,
            applied,
            is_not_kamikaze,
            0,
        ];
        bridge_broadcast_via_shared_data(
            this,
            this as *mut BaseEntity,
            0x14,
            0x48,
            0x408,
            payload.as_ptr() as *const u8,
        );
    }
}

/// Friendly/enemy fire scheme gate shared by all damage paths (msgs
/// 0x1C/0x76 ApplyDamage, 0x4B SpecialImpact, 0x51 PoisonWorm). A sender
/// of `0` (no source team) never blocks. Otherwise the receiver compares
/// its own `weapon_alliance` to the sender's: same-alliance reads
/// `friendly_fire_threshold`, cross-alliance reads `enemy_fire_threshold`.
/// Threshold values `> 2` block the damage.
unsafe fn alliance_blocks_damage(
    world: *const GameWorld,
    sender_team: u32,
    receiver_team: u32,
) -> bool {
    unsafe {
        if sender_team == 0 {
            return false;
        }
        let arena: *const TeamArena = &raw const (*world).team_arena;
        let sender_alliance =
            (*TeamArena::team_header(arena, sender_team as usize)).weapon_alliance;
        let receiver_alliance =
            (*TeamArena::team_header(arena, receiver_team as usize)).weapon_alliance;
        let game_info = (*world).game_info;
        let threshold = if sender_alliance == receiver_alliance {
            (*game_info).friendly_fire_threshold
        } else {
            (*game_info).enemy_fire_threshold
        };
        threshold > 2
    }
}

/// UpdateNonCritical (0x5). Per-frame easing + idle-sound emission.
/// Always handled (no parent dispatch). Two halves:
///
/// - Long-stationary worm (`stationary_frames > 499`) with a faded aim
///   (`aim_fade[0] == 0 || aim_fade[4] == 0`) and `CanIdleSound != 0`:
///   plays one of two team idle sounds (selection toggles every 31
///   frames), resets the four aim_fade slots {1, 3, 5, 7} to 1.0, and
///   sets `_field_3a0 = 1` either when no weapon is selected or when
///   `WeaponSpawn::DecodeDescriptor` reports both `out3` / `out4` as 0.
/// - Active turn (`turn_active != 0`) with `world._field_7640` having
///   changed since the last visit: resets aim_fade[1] / aim_fade[7] and
///   stores the new value into `_field_3a4`.
unsafe fn msg_update_non_critical(this: *mut WormEntity) {
    unsafe {
        ease_aim_vec_a(this);
        ease_aux_value(this);
        ease_aim_vec_b(this);

        let world = (*(this as *const BaseEntity)).world;

        if super::worm::worm_can_idle_sound_impl(this) != 0
            && (*this).stationary_frames > 499
            && ((*this).aim_fade[0] == Fixed::ZERO || (*this).aim_fade[4] == Fixed::ZERO)
        {
            if (*this).turn_paused == 0 && (*this).retreat_active == 0 {
                let parity = ((*world).frame_counter / 31) & 1;
                let sound_id = (*world).team_idle_sound_id((*this).team_index, parity as u32);
                sound::play_sound_local(
                    this as *mut WorldEntity,
                    SoundId(sound_id),
                    8,
                    Fixed::ONE,
                    Fixed::ONE,
                );
                (*this).stationary_frames = 0;
            }
            (*this).aim_fade[5] = Fixed::ONE;
            (*this).aim_fade[7] = Fixed::ONE;
            (*this).aim_fade[1] = Fixed::ONE;
            (*this).aim_fade[3] = Fixed::ONE;

            let no_aim_sprite = if (*this).selected_weapon == KnownWeaponId::None {
                true
            } else {
                let entry = &(*(*world).weapon_table).entries[(*this).selected_weapon as usize];
                let flags = crate::game::weapon_aim_flags::decode_weapon_aim_flags(entry);
                !flags.flag_d && !flags.flag_e
            };
            if no_aim_sprite {
                (*this)._field_3a0 = 1;
            }
        }

        if (*this).turn_active != 0 && (*world)._field_7640 != (*this)._field_3a4 {
            (*this).aim_fade[7] = Fixed::ONE;
            (*this).aim_fade[1] = Fixed::ONE;
            (*this)._field_3a4 = (*world)._field_7640;
        }
    }
}

unsafe fn msg_unknown_129(this: *mut WormEntity, message: *const Unknown129Message) {
    unsafe {
        if message.is_null() {
            return;
        }
        if (*message).worm_index != (*this).worm_index
            || (*this).turn_active == 0
            || (*this).turn_paused != 0
        {
            return;
        }
        let mut out = [(*message).coord_x(), (*message).coord_y()];
        WormEntity::get_entity_data_raw(this, 0x7D1, 0x394, out.as_mut_ptr() as *mut u32);
    }
}
