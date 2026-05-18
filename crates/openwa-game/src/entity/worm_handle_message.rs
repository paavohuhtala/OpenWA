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
use openwa_core::vec2::Vec2;
use openwa_core::weapon::{FireType, KnownWeaponId};

use super::base::BaseEntity;
use super::game_entity::WorldEntity;
use super::sound_emitter::SoundEmitter;
use super::worm::{KnownWormState, WormEntity, WormState};
use crate::audio::sound_ops as sound;
use crate::audio::{KnownSoundId, SoundId};
use crate::engine::team_arena::{TeamArena, WormEntry};
use crate::engine::world::GameWorld;
use crate::game::game_entity_message::{alliance_blocks_damage, world_entity_handle_message};
use crate::game::message::{
    CrateCollectedMessage, DamageWormsMessage, ExplosionMessage, PoisonWormMessage,
    SelectArmingMessage, SelectCursorMessage, SelectWeaponMessage, SpecialImpactMessage,
    Unknown129Message, WeaponReleasedMessage, WormMovedMessage,
};
use crate::game::{EntityMessage, weapon_fire};
use crate::generated::wa_calls;
use crate::rebase::rb;

/// Subtype on a [`FireType::Special`] weapon entry that triggers the
/// WeaponReleased aim-fade reset (msg 0x49). Empirically the Bungee
/// weapon — slot 15 has no [`openwa_core::weapon::SpecialFireSubtype`]
/// variant yet, so we keep it as a named constant here.
const BUNGEE_SPECIAL_SUBTYPE: i32 = 0xF;

/// Saved original `WormEntity::HandleMessage` (0x00510B40), populated by
/// `vtable_replace!` at install time.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

// `WormEntity__StepRopePhysics_Maybe` (0x005003D0) takes `this` on stack
// and a mode flag in AL — `core::arch::naked_asm!` can't bind a sub-register
// directly, so this one bridge stays hand-rolled. Every case-0x3 caller
// passes mode=0, so the trampoline just xor-zeroes AL before the call.
static mut WORM_STEP_ROPE_PHYSICS_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        WORM_STEP_ROPE_PHYSICS_ADDR = rb(0x005003D0);
    }
}

/// `__usercall(stdcall this on stack, AL = mode)`, RET 0x4. Per-frame rope
/// physics step. Bridge zeroes AL explicitly before the call (matching
/// WA's case-0x3 caller's `XOR AL,AL`).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_step_rope_physics(_this: *mut WormEntity) {
    core::arch::naked_asm!(
        "xor al, al",
        "push dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym WORM_STEP_ROPE_PHYSICS_ADDR,
    );
}

/// 5% of remaining gap, with a constant-step floor of `0x20C` so the
/// fade always reaches its target in finite time. Used by the two
/// `EaseAimVec` helpers below.
const AIM_FADE_RATE: Fixed = Fixed(0xCCC);
const AIM_FADE_MIN_STEP: Fixed = Fixed(0x20C);

/// 10% of remaining gap with the same fraction as a min-step floor —
/// matches WA's `GameTask__moveto` (0x00546a90) being called with
/// `EAX = 0x1999` from `EaseAuxValue`.
const AUX_VALUE_RATE: Fixed = Fixed(0x1999);
const AUX_VALUE_MIN_STEP: Fixed = Fixed(0x1999);

/// Rust port of `WormEntity__EaseAimVecA` (0x0050E630). Eases
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

/// Rust port of `WormEntity__EaseAimVecB` (0x0050E500). Eases
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

/// Rust port of `WormEntity__EaseAuxValue` (0x0050FB10). Eases
/// `_field_398` toward `_field_39c` via WA's `GameTask__moveto` primitive,
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

/// Inlines `WormEntity::IsActionState` (0x0050E800). The function is
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

/// Inlines `WormEntity::DeactivateOnIdle` (0x0050F7F0).
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
            (*this).thinking_anim_pos_x = (*this).base.pos.x;
            (*this).thinking_anim_pos_y = (*this).base.pos.y;
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
        wa_calls::EntityActivityQueue::ResetRank(queue, slot);
        (*this).stationary_frames = 0;
        wa_calls::WormEntity::NotifyMoved(this);
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
            EntityMessage::CrateCollected => {
                msg_crate_collected(this, data as *const CrateCollectedMessage)
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
            EntityMessage::FrameFinish => msg_frame_finish(this, sender, size, data),
            EntityMessage::FrameStart => msg_frame_start(this, sender, size, data),
            EntityMessage::RenderScene => msg_render_scene(this, sender, size, data),
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
/// `0x7` (CrateCollected). Antidote-crate path: when the picked-up
/// crate's `kind == 2` and the scheme's scope byte at `game_info+0xD9A4`
/// matches sender → receiver (per-worm / per-team / per-alliance), clears
/// the receiver's poison state and (game-version-gated) plays the cure
/// animation/sound through the worm's animator object.
unsafe fn msg_crate_collected(
    this: *mut WormEntity,
    message: *const CrateCollectedMessage,
) -> bool {
    unsafe {
        if (*message).kind != 2 {
            return true;
        }
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let scope = (*game_info)._scheme_d9a4;

        // Returns true if sender (per scope byte) matches receiver. Falls
        // through (returns false) for any other scope value — WA's switch
        // has no default arm so the entire body is skipped.
        let benefits = match scope {
            0 => {
                (*message).sender_worm == (*this).worm_index
                    && (*message).sender_team == (*this).team_index
            }
            1 => (*message).sender_team == (*this).team_index,
            2 => {
                // Compare alliance bytes: for team N, byte at
                // `game_info + N*0xBB8 - 0x765` = `team_records[N-1].name[12]`
                // approximately — WA reads the byte regardless of team
                // index, so faithfully mirror the address arithmetic.
                let game_info_b = game_info as *const u8;
                let sender_alliance =
                    *game_info_b.offset(((*message).sender_team as i32 * 0xBB8 - 0x765) as isize);
                let receiver_alliance =
                    *game_info_b.offset(((*this).team_index as i32 * 0xBB8 - 0x765) as isize);
                sender_alliance == receiver_alliance
            }
            _ => return true,
        };
        if !benefits {
            return true;
        }

        // Cure: gated on game_version >= 0x5F, and on game_version < 0x86
        // OR worm_state ∈ {Idle, Unknown_0x8B}, play the animator's vt[5]
        // (no args) + vt[7] (with sound id 0x80012D — likely the antidote
        // chime / "purified" sound).
        if (*this).poison_damage != 0 {
            let game_version = (*game_info).game_version;
            if game_version >= 0x5F {
                let state = (*this).state().0;
                let allowed = game_version < 0x86
                    || state == KnownWormState::Idle as u32
                    || state == KnownWormState::Unknown_0x8B as u32;
                if allowed {
                    let animator = (*this).animator;
                    let vt = *(animator as *const *const usize);
                    type Vt5Fn = unsafe extern "thiscall" fn(*mut u8);
                    type Vt7Fn = unsafe extern "thiscall" fn(*mut u8, u32);
                    let vt5: Vt5Fn = core::mem::transmute(*vt.add(5));
                    vt5(animator);
                    let vt7: Vt7Fn = core::mem::transmute(*vt.add(7));
                    vt7(animator, 0x80012D);
                }
            }
        }
        (*this).poison_damage = 0;
        (*this).poison_source_mask = 0;
        (*this).poison_tick_accum = 0;
        true
    }
}

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
        let pos_x = (*this).base.pos.x;
        let pos_y = (*this).base.pos.y;
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
        wa_calls::EntityActivityQueue::ResetRank(queue, (*this).activity_rank_slot as i32);
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
            let pos_x = (*this).base.pos.x;
            let pos_y = (*this).base.pos.y;
            GameWorld::register_event_point_raw(world, pos_x, pos_y);
        }
        let queue = &raw mut (*world).entity_activity_queue;
        wa_calls::EntityActivityQueue::ResetRank(queue, (*this).activity_rank_slot as i32);
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

        let pos_x = (*this).base.pos.x;
        let pos_y = (*this).base.pos.y;
        GameWorld::register_event_point_raw(world, pos_x, pos_y);

        let queue = &raw mut (*world).entity_activity_queue;
        wa_calls::EntityActivityQueue::ResetRank(queue, (*this).activity_rank_slot as i32);

        (*this)._field_1bc = 0;
        (*this).shot_data_1 = 0;
        (*this).shot_data_2 = 0;
        (*this).aim_fade = [Fixed::ONE; 8];
        (*this).weapons_enabled = 1;
        (*this).turn_active = 1;

        let team_arena: *mut TeamArena = &raw mut (*world).team_arena;
        wa_calls::TeamArena::SetActiveWorm(
            team_arena,
            (*this).team_index as i32,
            (*this).worm_index as i32,
        );

        let cache = (*world).localized_string_cache;
        let resolved = crate::wa::localized_string_cache::resolve_split_array_raw(cache, 0x69D)
            as *const core::ffi::c_char;
        wa_calls::WormEntity::BroadcastWeaponName(this, resolved as *mut c_char, 1);

        if (*this).selected_weapon != KnownWeaponId::None {
            wa_calls::WormEntity::BroadcastWeaponSettings(this);
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
            wa_calls::WormEntity::CancelActiveWeapon(this);
        } else {
            wa_calls::WormEntity::ClearWeaponState(this);
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
        wa_calls::TeamArena::SetActiveWorm(team_arena, (*this).team_index as i32, 0);

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

        wa_calls::GameTask::set_track(this as *mut BaseEntity, 0xE);

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

/// Inlines `WormEntity::ReleaseWeapon` (0x0051C010).
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

/// Aim-snap inlines `WormEntity__QuantizeAimAngle` (0x0051FD40):
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
            wa_calls::WormEntity::SelectWeapon(this, (*message).weapon_id, (*message).ammo_count);
        }
    }
}

unsafe fn msg_advance_worm(this: *mut WormEntity) {
    unsafe {
        wa_calls::WormEntity::ApplyDamage(this, 1, 1);
    }
}

unsafe fn msg_show_damage(this: *mut WormEntity) {
    unsafe {
        wa_calls::WormEntity::CommitPendingHealth(this);
    }
}

unsafe fn msg_weapon_claim_control(this: *mut WormEntity) {
    unsafe {
        wa_calls::WormEntity::CancelActiveWeapon(this);
    }
}

/// `WormStartFiring` is bridged (551 inst, cyclo 108 — too large to port).
unsafe fn msg_fire_weapon(this: *mut WormEntity) {
    unsafe {
        pre_switch_a(this);
        wa_calls::WormEntity::StartFiring(this);
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

        pre_switch_a(this);
        pre_switch_b(this);

        if gate_blocks {
            return true;
        }

        // State 0x73 (WeaponCharging) jumps to the function-tail firing-tick
        // block in WA — it's neither in the Idle group nor in the per-state
        // arms below, so handle it explicitly here.
        if state.is(KnownWormState::WeaponCharging_Maybe) {
            firing_tick(this);
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
                wa_calls::WormEntity::PlaySound(this, SoundId(0x1B), Fixed::ONE, 3);
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

/// `switchD_0051283d_caseD_73` in WA — the per-frame firing-tick block at
/// the function tail of `WormEntity::HandleMessage`, reached when the worm
/// is in state `0x73 (WeaponCharging)` and the case-0x24 (Jump) message
/// arrives.
///
/// Two paths share the trailing positional-sound emit:
/// - **Short path** (`game_version >= 0x31 && _field_258 != 0`): just
///   re-emits the 0x78 charging sound at the emitter's current position.
/// - **Full path** (everything else): validates the active weapon, runs
///   normal/special fire attempts, and on outright failure falls through
///   to the same trailing sound emit.
unsafe fn firing_tick(this: *mut WormEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;

        if (*game_info)._scheme_d964 == 0 {
            return;
        }

        let game_version = (*game_info).game_version;
        let mut emit_sound = false;

        if game_version >= 0x31 && (*this)._field_258 != 0 {
            // Short path: state 0x73 with a modern game_version + non-zero
            // `_field_258` skips the fire attempt and just chirps.
            emit_sound = true;
        } else {
            let saved_entry = (*this).active_weapon_entry;

            if game_version > 0x180 {
                if saved_entry.is_null() {
                    if game_version < 0x1ce {
                        // Show the "weapon descriptor missing" HUD message.
                        (*world).hud_status_code = 6;
                        (*world).hud_status_text = wa_calls::WA::LoadStringResource(0x710);
                        return;
                    }
                    // Modern null-descriptor: fall through to sound emit.
                    emit_sound = true;
                } else {
                    if (*this)._field_2e8 == 0
                        && wa_calls::WeaponSpawn::IsIndirect(saved_entry) != 0
                    {
                        wa_calls::GameTask::sub_546DB0(
                            this,
                            0x78,
                            8,
                            (*this).weapon_param_1,
                            (*this).weapon_param_2,
                        );
                        return;
                    }

                    if (*saved_entry).fire_type == 4 {
                        match (*saved_entry).special_subtype {
                            // Teleport — verify the destination is clear.
                            10 => {
                                let out_x: *mut i32 = &raw mut (*this).weapon_param_1;
                                let out_y: *mut i32 = &raw mut (*this).weapon_param_2;
                                if wa_calls::WormEntity::FindClearSpawnLocation(
                                    this, out_x, out_y, 0,
                                ) == 0
                                {
                                    wa_calls::GameTask::sub_546DB0(
                                        this,
                                        0x78,
                                        8,
                                        (*this).weapon_param_1,
                                        (*this).weapon_param_2,
                                    );
                                    return;
                                }
                            }
                            // Girder — verify the spawn area passes its
                            // overlap test (gated on a non-zero `_param3`).
                            17 => {
                                let arg = (*this).weapon_param_3;
                                if arg != 0 {
                                    let pos_x_lo =
                                        *((this as *const u8).add(0x2E2) as *const i16) as i32;
                                    let pos_y_lo =
                                        *((this as *const u8).add(0x2E6) as *const i16) as i32;
                                    if wa_calls::WormEntity::IsSpawnAreaValid(
                                        arg, this, pos_x_lo, pos_y_lo,
                                    ) == 0
                                    {
                                        wa_calls::GameTask::sub_546DB0(
                                            this,
                                            0x78,
                                            8,
                                            (*this).weapon_param_1,
                                            (*this).weapon_param_2,
                                        );
                                        return;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            if !emit_sound {
                if wa_calls::WormEntity::TryFireWeapon(this) != 0 {
                    wa_calls::WormEntity::LogWeaponFire(this, saved_entry);
                    return;
                }

                if (*game_info)._scheme_d964 > 1
                    && wa_calls::WormEntity::TryFireWeaponSpecial(this) != 0
                {
                    wa_calls::WormEntity::LogWeaponFire(this, saved_entry);
                    let v = game_version.wrapping_sub(0x29);
                    if (v as u32) < 0x2c {
                        (*this)._field_250 = 0;
                    }
                    return;
                }

                emit_sound = true;
            }
        }

        if !emit_sound {
            return;
        }

        // Trailing positional-sound emit shared by both paths. Mirrors the
        // gates inside `dispatch_local_sound`'s caller chain — bail when
        // sound is muted, the start-frame threshold isn't met, or no
        // active-sounds table is allocated.
        if (*game_info).sound_mute != 0 {
            return;
        }
        if (*world).frame_counter < (*game_info).sound_start_frame {
            return;
        }
        let table = (*world).active_sounds;
        if table.is_null() {
            return;
        }

        let emitter = &raw const (*(this as *const WorldEntity)).sound_emitter;
        let pos = SoundEmitter::get_position_raw(emitter);

        sound::dispatch_local_sound(
            table,
            Fixed::ONE,
            KnownSoundId::WarningBeep,
            8,
            pos,
            emitter as *mut SoundEmitter,
        );
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
        wa_calls::WormEntity::SelectFuse(value, this);
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
        wa_calls::WormEntity::SelectHerd((*message).value, this);
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
        wa_calls::WormEntity::SelectBounce((*message).value, this);
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
        wa_calls::BaseEntity::deliver(
            this as *mut core::ffi::c_void,
            0,
            this as *mut BaseEntity,
            0x14,
            0x48,
            0x408,
            payload.as_ptr() as *mut core::ffi::c_void,
        );

        let cache = (*world).localized_string_cache;
        let resolved = crate::wa::localized_string_cache::resolve_split_array_raw(cache, 0x6CF)
            as *mut *mut c_char;
        wa_calls::GameTask::comment_public(
            this,
            resolved,
            0x17,
            &raw const (*this).worm_name as *mut c_char,
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
            wa_calls::WormEntity::PlayImpactSound(this, 0x6A, Fixed::ONE);
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
            wa_calls::WormEntity::PlaySound(this, SoundId(sound_id), Fixed::ONE, 3);
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
        wa_calls::WormEntity::SpawnDamageParticles(
            damage_for_particles,
            this,
            (*this).base.pos.x,
            (*this).base.pos.y,
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

        if wa_calls::WormEntity::HitTestRopeLine(
            this,
            (*message).pos.x,
            (*message).damage,
            (*message).pos.y,
        ) != 0
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
            wa_calls::WormEntity::SpawnDamageParticles(
                halved,
                this,
                (*this).base.pos.x,
                (*this).base.pos.y,
                (*message).pos.x,
                (*message).pos.y,
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
        wa_calls::BaseEntity::deliver(
            this as *mut core::ffi::c_void,
            0,
            this as *mut BaseEntity,
            0x14,
            0x48,
            0x408,
            payload.as_ptr() as *mut core::ffi::c_void,
        );
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

/// Case `0x2 FrameFinish` — runs WA's parent dispatch then `BehaviorTick`,
/// followed by two book-keeping checks. Bridges `BehaviorTick` (still in
/// WA) and reuses the Rust port of `IsActionState_Maybe`. WA's switch ends
/// in `break;` (no fall-through), so this returns `true` either way.
unsafe fn msg_frame_finish(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) -> bool {
    unsafe {
        // Parent dispatch first — WorldEntity::HandleMessage clears the
        // owned sound handle for FrameFinish, then broadcasts to children.
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::FrameFinish,
            size,
            data,
        );

        if wa_calls::WormEntity::BehaviorTick(this) == 0 {
            return true;
        }

        let world = (*(this as *const BaseEntity)).world;
        let team_index = (*this).team_index;
        let worm_index = (*this).worm_index;
        let arena: *mut TeamArena = &raw mut (*world).team_arena;
        let entry = TeamArena::team_worm_mut(arena, team_index as usize, worm_index as usize);

        // Clear the per-turn action-pending flag on the worm entry when
        // either the state is outside the action range `[0x68..=0x8A]` or
        // `IsActionState` returns false, AND the entry slot is non-zero,
        // AND the worm's `_field_258` / `_unknown_2cc` gates are zero.
        let state = (*this).state();
        let state_out_of_range = state.0.wrapping_sub(0x68) > 0x22;
        if (state_out_of_range || !is_action_state(state))
            && (*entry)._field_98 != 0
            && (*this)._field_258 == 0
            && (*this)._unknown_2cc == 0
        {
            (*entry)._field_98 = 0;
        }

        // Per-team turn-action bit 4 + game_version >= 0x54 ⇒ advance
        // the global RNG (an extra LCG step that keeps the desync sync'd
        // with the bit-4 path WA reaches via the early `return`).
        let header = TeamArena::team_header(arena, team_index as usize);
        let game_version = (*(*world).game_info).game_version;
        if (*header).turn_action_flags & 4 != 0 && game_version > 0x53 {
            (*world).advance_rng();
        }

        true
    }
}

/// Case `0x1 FrameStart` — two independent scheme-gated blocks, then the
/// parent `WorldEntity::HandleMessage` dispatch. WA's case body falls
/// through to `default:` which calls the parent, so we always return `true`
/// after running the body + parent.
unsafe fn msg_frame_start(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let pos_x_snap = (*this).base.pos.x;
        let pos_y_snap = (*this).base.pos.y;

        // Block 1 — gated on `_scheme_d9ce`. Idle/Unknown_0x8B states get
        // their pos snapshotted into `_field_344/348` (subject to a
        // per-frame `_field_34c` counter override). Non-idle states with
        // `_field_bc != 0` only clear the byte. Non-idle with
        // `_field_bc == 0` skips the entire block.
        if (*game_info)._scheme_d9ce != 0 {
            let state = (*this).state();
            let in_idle = state.is(KnownWormState::Idle) || state.is(KnownWormState::Unknown_0x8B);
            if in_idle {
                let counter = (*this)._field_34c as i8;
                if counter < 1 || (*game_info)._scheme_d9b3 != 0 {
                    (*this)._field_344 = pos_x_snap;
                    (*this)._field_348 = pos_y_snap;
                }
                (*this)._field_34c = 0;
            } else if (*this).base._field_bc != 0 {
                (*this)._field_34c = 0;
            }
        }

        // Block 2 (LAB_00511e45) — drag/wind/impulse fold. Gated on the
        // three scheme drag/wind values, plus state != RopeSwinging. The
        // `_field_60 = ONE` (WormEntity+0x60) write happens whenever the
        // scheme/state gate passes, regardless of `_field_bc`; the
        // helper trio only runs when `_field_bc != 0`.
        let scheme_gate = (*game_info)._scheme_d9c5 != 0
            || (*game_info)._scheme_d9c0 != 0
            || (*game_info)._scheme_d9b8 != 0;
        if scheme_gate && !(*this).state().is(KnownWormState::RopeSwinging) {
            (*this).base.subclass_data._field_60 = Fixed::ONE;

            if (*this).base._field_bc != 0 {
                wa_calls::WormEntity::ApplyDragMods(this);
                // ApplyWind writes the X-axis impulse into `*out_x` and the
                // Y-axis impulse into `*out_y`; AccumulateImpulse then folds
                // them into `speed_x` (+0x90) and `speed_y` (+0x94). WA's
                // matching locals (local_83c / local_838) are pre-snapshotted
                // from speed_y / speed_x in the prologue but ApplyWind never
                // reads them — the slots are pure output buffers in this
                // path, so zero-init is faithful.
                let mut wind_dx = Fixed::ZERO;
                let mut wind_dy = Fixed::ZERO;
                wa_calls::WormEntity::ApplyWind(this, &raw mut wind_dx, &raw mut wind_dy, 0, 0);
                wa_calls::WormEntity::AccumulateImpulse(wind_dx, wind_dy, this);
            }
        }

        // Parent dispatch (WA's `default:` fall-through to
        // `WorldEntity::HandleMessage`).
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::FrameStart,
            size,
            data,
        );
        true
    }
}

// Globals used by case 0x3 (RenderScene) to hold the kamikaze pos/state
// swap. WA reads/writes these as plain memory; we mirror that with rebased
// pointers each frame. `KAMIKAZE_POS_SAVE_*_VA` are the Ghidra VAs.
const KAMIKAZE_POS_SAVE_X_VA: u32 = 0x007742F4;
const KAMIKAZE_POS_SAVE_Y_VA: u32 = 0x007742F8;
const KAMIKAZE_AUX_SAVE_VA: u32 = 0x008C1EB8;
const KAMIKAZE_CLIFF_FALL_SAVE_VA: u32 = 0x008C1EBC;

#[inline]
unsafe fn world_field_i32(world: *mut GameWorld, offset: usize) -> i32 {
    unsafe { *((world as *const u8).add(offset) as *const i32) }
}

#[inline]
unsafe fn world_field_byte(world: *mut GameWorld, offset: usize) -> u8 {
    unsafe { *((world as *const u8).add(offset)) }
}

/// Case `0x3 RenderScene` — the per-frame draw pass for a worm. WA's case
/// body has three sub-blocks (kamikaze prologue, draw block, kamikaze
/// epilogue) and **always** calls the parent `WorldEntity::HandleMessage`
/// before returning. Returns `true` so the dispatcher does not run
/// `fall_through`.
///
/// Most of the work is rendering side-effects we cannot validate
/// headlessly. The sim-relevant pieces are the kamikaze pos swap (the
/// `TryMovePosition` call inside Block A), the `world.field_7ea0` directional
/// nudge, the kamikaze state save/restore globals, and the parent
/// dispatch.
unsafe fn msg_render_scene(
    this: *mut WormEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) -> bool {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let state = (*this).state();
        let action_field = (*this).base.subclass_data.action_field;
        let fire_complete = (*this).fire_complete();

        let kamikaze_pos_save_x = rb(KAMIKAZE_POS_SAVE_X_VA) as *mut Fixed;
        let kamikaze_pos_save_y = rb(KAMIKAZE_POS_SAVE_Y_VA) as *mut Fixed;
        let kamikaze_aux_save = rb(KAMIKAZE_AUX_SAVE_VA) as *mut i32;
        let kamikaze_cliff_fall_save = rb(KAMIKAZE_CLIFF_FALL_SAVE_VA) as *mut u32;

        // Block A — kamikaze active path: action_field != 0 && state == 0x6D.
        // Saves the worm's current pos into the global swap slots, then 8-way
        // snaps the speed vector and forwards to TryMovePosition biased by
        // `snapped * render_interp_a * 16`.
        let kamikaze_active = action_field != 0 && state.is(KnownWormState::SuicideBomber);
        if kamikaze_active {
            *kamikaze_pos_save_x = (*this).base.pos.x;
            *kamikaze_pos_save_y = (*this).base.pos.y;

            let snapped = Vec2::new((*this).base.speed_x, (*this).base.speed_y).snap_to_8way();
            let interp_q4 = (*world).render_interp_a * 16;
            let pos = Vec2::new((*this).base.pos.x, (*this).base.pos.y);
            let new_pos = pos.wrapping_add(snapped.mul_raw(interp_q4));
            WorldEntity::try_move_position_raw(this as *mut WorldEntity, new_pos.x, new_pos.y);
        } else if fire_complete != 0 && action_field == 0 {
            // Block B — non-kamikaze path that gates on the worm having
            // just-completed firing. Save the kamikaze fields to the side
            // buffers and (when the frame is non-paused) drive the per-state
            // aim scroll for AimingAngle / RopeSwinging.
            *kamikaze_cliff_fall_save = (*this).cliff_fall_flag;
            bridge_step_rope_physics(this);
            if world_field_i32(world, 0x8150) != 0 {
                if state.is(KnownWormState::AimingAngle_Maybe) {
                    wa_calls::WormEntity::DrainInputBuffer(this);
                    wa_calls::WormEntity::ScrollAimX(this);
                } else if state.is(KnownWormState::RopeSwinging) {
                    *kamikaze_aux_save = (*this)._field_1e8;
                    wa_calls::WormEntity::DrainInputBuffer(this);
                    wa_calls::WormEntity::ScrollAimSmooth(this);
                }
            }
        }

        // Block C — main draw pass. Skipped entirely when the worm is in
        // the Dead state (0x64). Inside, an early `goto LAB_00511d57` skips
        // the draw + nudge when `_field_140 == 0` AND the per-team alliance
        // table has bit 3 set on the team's "spectator/hidden" byte.
        let mut skip_draw = false;
        if !state.is(KnownWormState::Dead) {
            let team_idx = (*this).team_index as usize;

            // Per-team skip-to-tail check (matches WA's `goto LAB_00511d57`).
            if (*this)._field_140 == 0 {
                let team_block_off = team_idx * 0x51c;
                let table_a = world_field_i32(world, 0x7E6C + team_idx * 4);
                let team_field_4618 = world_field_i32(world, 0x4618 + team_block_off);
                let team_byte_4628 = world_field_byte(world, 0x4628 + team_block_off);
                if table_a == 0 && team_field_4618 == 0 && (team_byte_4628 & 8) != 0 {
                    skip_draw = true;
                }
            }

            if !skip_draw {
                // Camera-bias nudge: ±1 per frame (or ±100 when this worm
                // holds the turn) into world.field_7ea0, signed by which
                // side of the viewport midpoint the worm is on.
                let step: i32 = if (*this).turn_active != 0 { 100 } else { 1 };
                let midpoint_x = world_field_i32(world, 0x8CEC);
                let nudge_slot = (world as *mut u8).add(0x7EA0) as *mut i32;
                if (*this).base.pos.x.0 < midpoint_x {
                    *nudge_slot = (*nudge_slot).wrapping_add(step);
                } else {
                    *nudge_slot = (*nudge_slot).wrapping_sub(step);
                }

                // Draw helpers — all rendering side-effects, bridged into WA.
                wa_calls::WormEntity::DrawAimingArrow_Maybe(this);
                wa_calls::WormEntity::DrawWormName_Maybe(this);
                wa_calls::WormEntity::DrawSprite_Maybe(this);
                wa_calls::WormEntity::DrawHealthLabel_Maybe(this);
                // DrawCrosshairLine has a Rust port; the worm pointer is the
                // receiver (the "WeaponAimEntity" overlay is similarly
                // synthetic — its offsets line up with WormEntity).
                crate::render::crosshair_line::draw_crosshair_line(
                    this as *const crate::entity::WeaponAimEntity,
                );

                // HUD/aim/rope sub-block: gated on `_field_b0 == 0`.
                if (*this).base._field_b0 == 0 {
                    if (*this)._unknown_208 == 0 {
                        wa_calls::WormEntity::DrawOffMapMarker_Maybe(this);
                    }
                    wa_calls::WormEntity::DrawHudLabels_Maybe(this);
                    wa_calls::WormEntity::DrawAimCursor_Maybe(this);
                    crate::render::worm::draw_turn_indicator(this);
                    let rope_style = (*world).gfx_color_table[8];
                    let rope_fill = (*world).gfx_color_table[6];
                    crate::render::worm::draw_attached_rope(this, rope_style, rope_fill);
                    wa_calls::WormEntity::DrawCursorMarker_Maybe(this);
                }
            }
        }

        // Tail (LAB_00511d57). Two terminal arms — kamikaze tail and the
        // non-kamikaze save/restore tail — both end with the parent
        // dispatch + return. WA re-reads the gating fields here rather than
        // using the prologue's snapshot, so we do too: helpers in the body
        // (TryMovePosition collision callbacks in Block A,
        // DrainInputBuffer / ScrollAim* in Block B) can mutate state.
        let tail_state = (*this).state();
        let tail_action_field = (*this).base.subclass_data.action_field;
        let tail_fire_complete = (*this).fire_complete();

        if tail_action_field != 0 && tail_state.is(KnownWormState::SuicideBomber) {
            let saved_x = *kamikaze_pos_save_x;
            let saved_y = *kamikaze_pos_save_y;
            WorldEntity::try_move_position_raw(this as *mut WorldEntity, saved_x, saved_y);
        } else if tail_fire_complete != 0 && tail_action_field == 0 {
            wa_calls::WormEntity::RestoreKamikazeState_Maybe(this);
            (*this).cliff_fall_flag = *kamikaze_cliff_fall_save;
            if world_field_i32(world, 0x8150) != 0 && tail_state.is(KnownWormState::RopeSwinging) {
                (*this)._field_1e8 = *kamikaze_aux_save;
            }
        }

        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::RenderScene,
            size,
            data,
        );
        true
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
