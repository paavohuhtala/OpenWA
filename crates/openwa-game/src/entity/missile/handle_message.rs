//! Incremental port of `MissileEntity::HandleMessage` (0x0050B400, vtable
//! slot 2). The dispatcher handles a small set of bounded message cases
//! pure-Rust and falls through to WA's original for everything else.
//!
//! WA's HandleMessage contains a top-level early-bail
//! (`if (msg - 2 > 0x7C) return msg - 2`) and a per-case canned return
//! value indexed off `msg + 2`. The cases we port here all run to either
//! an early `return` or `break` in the original — neither path forwards
//! to the parent `WorldEntity::HandleMessage`. So a "handled" branch
//! simply suppresses fall-through; an "unhandled" branch defers to WA's
//! original via [`ORIGINAL_HANDLE_MESSAGE`].

use core::sync::atomic::{AtomicU32, Ordering};

use super::{MissileEntity, frame_finish};
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::game_entity_message::world_entity_handle_message;
use crate::game::message::{
    DetonateWeaponMessage, EntityMessage, ExplosionMessage, MoveWeaponMessage, Unknown126Message,
};
use crate::rebase::rb;

type HandleMessageFn = unsafe extern "thiscall" fn(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
);

/// Saved original `MissileEntity::HandleMessage` (0x0050B400), populated
/// by `vtable_replace!` at install time.
pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

// Rebased bridge addresses, initialized by [`init_addrs`].
//
// `Task_Missile::start_fuse_sound` (0x00508B50) — `__usercall(EDI = this,
// [stack] = sound_id)`, RET 4. Tries to start the fuse sound; if that
// fails, stashes -sound_id as a deferred-retry sentinel.
static mut START_FUSE_SOUND_ADDR: u32 = 0;
// `Task_Missile::start_dig_sound` (0x00508930) — same shape, slot 0x3E0.
static mut START_DIG_SOUND_ADDR: u32 = 0;
// `Task_Missile::start_super_animal` (0x0050AF40) — `__usercall(EAX = this)`,
// plain RET. Transitions a homing missile into super-animal control mode.
static mut START_SUPER_ANIMAL_ADDR: u32 = 0;
// `Task_Missile::finish_super_animal` (0x0050B020) — `__usercall(EAX = this)`,
// plain RET. Closes out super-animal mode (drains residual velocity into
// 1/3 carry-over and sets contact_phase = 2).
static mut FINISH_SUPER_ANIMAL_ADDR: u32 = 0;
// `WormEntity::StepRopePhysics_Maybe` (0x005003D0) — stdcall(this), RET 4.
// Generic per-frame collision/rope physics tick used by case 3 (RenderScene)
// when this missile is the kamikaze-owned worm proxy or in super-animal mode.
static mut STEP_ROPE_PHYSICS_ADDR: u32 = 0;
// `WormEntity::RestoreKamikazeState_Maybe` (0x00500630) — `__usercall(EAX = this)`,
// plain RET. Inverse of the rope-physics setup: restores the kamikaze-owner
// worm's pre-render state after the missile has been drawn.
static mut RESTORE_KAMIKAZE_ADDR: u32 = 0;
// `Task_Missile::render_indicator` (0x00508F90) — stdcall(this), RET 4.
// Draws the optional homing-target / fuse-direction overlay (sprite + textbox).
// Skipped when [`underwater_entry_latched`] is set (= the missile has already
// crossed under water).
static mut RENDER_INDICATOR_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        START_FUSE_SOUND_ADDR = rb(0x00508B50);
        START_DIG_SOUND_ADDR = rb(0x00508930);
        START_SUPER_ANIMAL_ADDR = rb(0x0050AF40);
        FINISH_SUPER_ANIMAL_ADDR = rb(0x0050B020);
        STEP_ROPE_PHYSICS_ADDR = rb(0x005003D0);
        RESTORE_KAMIKAZE_ADDR = rb(0x00500630);
        RENDER_INDICATOR_ADDR = rb(0x00508F90);
    }
}

// ─── WA bridges ────────────────────────────────────────────────────────────

/// `__usercall(EDI = this, [stack] = sound_id)`, RET 4. EDI is callee-saved
/// per the x86 ABI, so the trampoline preserves it across the call.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_fuse_sound(_this: *mut MissileEntity, _sound_id: i32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",   // this  (ret(4) + edi(4) = 8)
        "push dword ptr [esp+12]",      // sound_id (caller's stack arg)
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop edi",
        "ret 8",
        addr = sym START_FUSE_SOUND_ADDR,
    );
}

/// Same shape as [`bridge_start_fuse_sound`].
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_dig_sound(_this: *mut MissileEntity, _sound_id: i32) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, dword ptr [esp+8]",
        "push dword ptr [esp+12]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop edi",
        "ret 8",
        addr = sym START_DIG_SOUND_ADDR,
    );
}

/// `__usercall(EAX = this)`, plain RET. EAX is caller-saved.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_start_super_animal(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym START_SUPER_ANIMAL_ADDR,
    );
}

/// Same shape as [`bridge_start_super_animal`]. Re-exposed to the sibling
/// [`free`](super::free) module — the destructor invokes this when the
/// missile is mid-super-animal (`contact_phase == 1`).
#[unsafe(naked)]
pub(super) unsafe extern "stdcall" fn bridge_finish_super_animal(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym FINISH_SUPER_ANIMAL_ADDR,
    );
}

/// `WormEntity::StepRopePhysics_Maybe` (0x005003D0) — stdcall(this), RET 4.
unsafe fn bridge_step_rope_physics(this: *mut MissileEntity) {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut MissileEntity) =
            core::mem::transmute(STEP_ROPE_PHYSICS_ADDR as usize);
        f(this);
    }
}

/// `WormEntity::RestoreKamikazeState_Maybe` (0x00500630) — `__usercall(EAX = this)`,
/// plain RET (no stack args, no cleanup).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_restore_kamikaze_state(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym RESTORE_KAMIKAZE_ADDR,
    );
}

/// `Task_Missile::render_indicator` (0x00508F90) — stdcall(this), RET 4.
unsafe fn bridge_render_indicator(this: *mut MissileEntity) {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut MissileEntity) =
            core::mem::transmute(RENDER_INDICATOR_ADDR as usize);
        f(this);
    }
}

/// HandleMessage selects between two "discriminator" slots inside
/// [`MissileEntity`]'s render-data block based on
/// [`is_cluster_pellet`](MissileEntity::is_cluster_pellet) — single-shot
/// missiles read [`_render_data_07`](MissileEntity::_render_data_07);
/// cluster pellets read [`_render_data_1a`](MissileEntity::_render_data_1a).
/// This corresponds to the `piVar8` view set up at the top of WA's
/// HandleMessage, and `animation_rate_kind() == piVar8[2]`.
#[inline]
unsafe fn animation_rate_kind(this: *const MissileEntity) -> u32 {
    unsafe {
        if (*this).is_cluster_pellet != 0 {
            (*this)._render_data_1a
        } else {
            (*this)._render_data_07
        }
    }
}

// ─── Per-case handlers ─────────────────────────────────────────────────────

/// `5` UpdateNonCritical — animation-phase update.
///
/// When the animation-rate kind discriminator is `3` and the missile is
/// either non-homing OR homing-but-flying-straight (not underwater /
/// not in super-animal control), the animation phase gets bumped by
/// `|speed_x| / 100 + 0xCCC` (mod 0x10000).
///
/// Always handled — the gate-failed path is a no-op in WA.
unsafe fn msg_update_non_critical(this: *mut MissileEntity) {
    unsafe {
        if animation_rate_kind(this) != 3 {
            return;
        }
        let homing = matches!((*this).missile_type, super::MissileType::Homing);
        let underwater = (*this).base._field_b0 != 0;
        let super_animal_active = (*this).contact_phase == 1;
        if homing && (underwater || super_animal_active) {
            return;
        }

        let abs_sx = (*this).base.speed_x.to_raw().wrapping_abs() as u32;
        let new = abs_sx
            .wrapping_div(100)
            .wrapping_add(0xCCC)
            .wrapping_add((*this).animation_phase)
            & 0xFFFF;
        (*this).animation_phase = new;
    }
}

/// `0x2D` MoveWeaponLeft / `0x2E` MoveWeaponRight — super-animal steering.
///
/// Only acts when the message sender's id matches `spawn_params.owner_id`
/// AND the missile is in super-animal control mode (contact_phase == 1).
/// The torque delta is `-0x5B0` for Left, `+0x5B0` for Right.
///
/// Old schemes (`game_version < 0x1D`): unconditionally adds delta to the
/// running [`super_animal_torque_accum`] accumulator.
///
/// New schemes: clamps the per-frame [`super_animal_torque_input`] to
/// `[-0x5B0, +0x5B0]`. The FrameFinish tick later folds the input into the
/// accumulator.
///
/// Always handled — the gate-failed path is a no-op in WA.
///
/// [`super_animal_torque_accum`]: MissileEntity::super_animal_torque_accum
/// [`super_animal_torque_input`]: MissileEntity::super_animal_torque_input
unsafe fn msg_move_weapon_dir(this: *mut MissileEntity, msg: &MoveWeaponMessage, delta: i32) {
    unsafe {
        if msg.sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if (*this).contact_phase != 1 {
            return;
        }

        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*(*world).game_info).game_version;

        if game_version < 0x1D {
            (*this).super_animal_torque_accum =
                (*this).super_animal_torque_accum.wrapping_add(delta as u32);
        } else {
            let candidate = (*this).super_animal_torque_input.wrapping_add(delta);
            (*this).super_animal_torque_input = candidate.clamp(-0x5B0, 0x5B0);
        }
    }
}

/// `0x1C` Explosion — forward inbound explosion broadcasts to the parent
/// `WorldEntity::HandleMessage` (which applies physics impulse / damage),
/// gated on:
///
/// - **Old/unforced path** (`game_version < 0x4E && _scheme_d99f == 0`):
///   forward only when [`explosion_response_flag`] is non-zero, payload
///   unchanged.
/// - **Modern/forced path** (otherwise): forward when either
///   [`explosion_response_flag`] is non-zero OR `_scheme_d99f != 0`,
///   first making a local copy of the [`ExplosionMessage`] with
///   [`caller_flag`] zeroed.
///
/// Always handled — the gate-failed paths drop the message silently in WA
/// (case body falls through to `break` → bottom canned-value return; no
/// parent dispatch).
///
/// Mirrors `MineEntity::HandleMessage`'s case 0x1C in shape but without
/// the alliance gate and settling-anim-flag side effects (those are
/// mine-specific).
///
/// [`explosion_response_flag`]: MissileEntity::explosion_response_flag
/// [`caller_flag`]: ExplosionMessage::caller_flag
unsafe fn msg_explosion(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    size: u32,
    msg: &ExplosionMessage,
) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let game_version = (*game_info).game_version;
        let scheme_d99f = (*game_info)._scheme_d99f;
        let responds = (*this).explosion_response_flag != 0;

        if game_version < 0x4E && scheme_d99f == 0 {
            if responds {
                world_entity_handle_message(
                    this as *mut WorldEntity,
                    sender,
                    EntityMessage::Explosion,
                    size,
                    msg as *const ExplosionMessage as *const u8,
                );
            }
        } else if responds || scheme_d99f != 0 {
            // Modern path: copy the message and zero `caller_flag` before
            // forwarding. WA's actual copy is 0x408 bytes (presumed
            // tail-junk over-read inherited from a larger stack buffer);
            // the parent never reads past `ExplosionMessage`, so copying
            // just the typed struct is equivalent.
            let mut local = *msg;
            local.caller_flag = 0;
            world_entity_handle_message(
                this as *mut WorldEntity,
                sender,
                EntityMessage::Explosion,
                size,
                &local as *const ExplosionMessage as *const u8,
            );
        }
    }
}

/// `0x2C` DetonateWeapon — manual detonate / state-cycle request from
/// the firing worm.
///
/// Gated on:
/// - the message originating from this missile's own owner
///   (`*data == spawn_params.owner_id`), AND
/// - the missile being above water (`_field_b0 == 0`).
///
/// When the gate passes:
/// 1. **Super-animal transition** — for homing missiles whose render data
///    enables super-animal control ([`super_animal_walk_sprite`] != 0):
///     - `contact_phase == 0` → call `Task_Missile::start_super_animal`,
///       and return.
///     - `contact_phase == 1 && pos_y_int < world.water_kill_y` →
///       call `Task_Missile::finish_super_animal`, and return.
/// 2. **Detonate dispatch** — by [`detonate_response_mode`]:
///     - `1`: invoke vtable[14] (`set_terminate_flag`) with flag `2` if
///       `weapon_data[0x2D] == 3`, else flag `1`. The flag-`1` sub-branch
///       additionally sets [`_field_3d4`] = 1 when
///       `weapon_data[0x2D] == 1 && game_version < 0x1F0 &&
///       weapon_data[9] == 0x41`.
///     - `2`: zero `textbox_visible_threshold` and
///       `detonate_response_mode`, then `fuse_timer = (rng & 0xFFFF) %
///       500`.
///
/// Always handled — the gate-failed paths are no-ops in WA.
///
/// [`super_animal_walk_sprite`]: MissileEntity::super_animal_walk_sprite
/// [`detonate_response_mode`]: MissileEntity::detonate_response_mode
/// [`_field_3d4`]: MissileEntity::_field_3d4
unsafe fn msg_detonate_weapon(this: *mut MissileEntity, msg: &DetonateWeaponMessage) {
    unsafe {
        if msg.team_index != (*this).spawn_params.owner_id {
            return;
        }
        if (*this).base._field_b0 != 0 {
            // Underwater — silently drop.
            return;
        }

        let world = (*(this as *const BaseEntity)).world;

        // Super-animal transition for eligible homing missiles.
        if matches!((*this).missile_type, super::MissileType::Homing)
            && (*this).super_animal_walk_sprite != 0
        {
            match (*this).contact_phase {
                0 => {
                    bridge_start_super_animal(this);
                    return;
                }
                1 => {
                    let pos_y_int = (*this).base.pos_y.to_int();
                    if pos_y_int < (*world).water_kill_y {
                        bridge_finish_super_animal(this);
                        return;
                    }
                }
                _ => {}
            }
        }

        // Detonate response.
        match (*this).detonate_response_mode {
            1 => {
                let flag = if (*this).weapon_data[0x2D] == 3 { 2 } else { 1 };
                MissileEntity::set_terminate_flag_raw(this, flag);
                if flag == 1 {
                    let game_version = (*(*world).game_info).game_version;
                    if (*this).weapon_data[0x2D] == 1
                        && game_version < 0x1F0
                        && (*this).weapon_data[9] == 0x41
                    {
                        (*this)._field_3d4 = 1;
                    }
                }
            }
            2 => {
                (*this).textbox_visible_threshold = 0;
                (*this).detonate_response_mode = 0;
                let rng = (*world).advance_rng();
                (*this).fuse_timer = ((rng & 0xFFFF) % 500) as i32;
            }
            _ => {}
        }
    }
}

/// `0x7A` (122) — sound-handle restore. Sent on save/restore (and similar
/// sound-system reset events). Re-arms the missile's two sound slots when
/// they were previously stashed as `-sound_id` retry sentinels by a
/// failed `Task_Missile::start_*_sound` call.
///
/// Predicate (the inlined `Task_Missile::sub_508B90` / `sub_5088A0`): the
/// slot is a deferred retry iff `-slot` is non-negative, has bit `0x10000`
/// set, and the low 16 bits are `< 0x7F` (i.e. the original sound id was
/// `0x10000 ..= 0x1007E`, the music-style category).
///
/// Always handled — both sub-branches are conditional, and a no-op when
/// neither slot is in the retry state.
unsafe fn msg_sound_restore(this: *mut MissileEntity) {
    unsafe {
        let fuse_slot = (*this).fuse_sound_handle;
        if is_deferred_sound_retry(fuse_slot) {
            bridge_start_fuse_sound(this, fuse_slot.wrapping_neg());
        }
        let dig_slot = (*this).dig_sound_handle;
        if is_deferred_sound_retry(dig_slot) {
            bridge_start_dig_sound(this, dig_slot.wrapping_neg());
        }
    }
}

/// Inline port of `Task_Missile::sub_508B90` / `sub_5088A0` (12-instruction
/// predicates). Returns `true` when `slot` is a `-sound_id` retry sentinel
/// stashed by a previously-failed `start_*_sound` call.
#[inline]
fn is_deferred_sound_retry(slot: i32) -> bool {
    let neg = slot.wrapping_neg() as u32;
    (neg as i32) >= 0 && (neg & 0x10000) != 0 && (neg & 0xFFFE_FFFF) < 0x7F
}

/// `0x03` RenderScene — per-frame draw dispatch.
///
/// Sequence (matching WA's case 3 body):
/// 1. **Pre-physics gate** — when the missile is acting as a kamikaze
///    rope-attached worm proxy (`subclass.action_flag != 0 &&
///    subclass.sheep_state_flag == 0`) OR is mid-super-animal
///    (`contact_phase == 1`), invoke
///    `WormEntity::StepRopePhysics_Maybe`.
/// 2. **Camera nudge** — when `contact_phase != 0` (super-animal active
///    or closing), accumulate `world.field_7ea0 += viewport_coords[3].center_x - pos_x`.
///    The accumulator is consumed elsewhere as the screen-track delta
///    so the camera follows the missile during sheep-control.
/// 3. **Render** — `super::render::missile_render`.
/// 4. **Indicator overlay** — when [`underwater_entry_latched`] is `0`,
///    invoke `Task_Missile::render_indicator` (homing-target /
///    fuse-direction HUD).
/// 5. **Post-physics gate** — same predicate as step 1, invoking
///    `WormEntity::RestoreKamikazeState_Maybe`.
/// 6. **Parent dispatch** — forward to `WorldEntity::HandleMessage` so
///    children (sub-pellets, etc.) get the broadcast.
///
/// Always handled.
///
/// [`underwater_entry_latched`]: MissileEntity::underwater_entry_latched
unsafe fn msg_render_scene(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let action_flag = (*this).base.subclass_data.action_flag;
        let sheep_state_flag = (*this).base.subclass_data.sheep_state_flag;
        let contact_phase = (*this).contact_phase;
        let kamikaze_proxy = (action_flag != 0 && sheep_state_flag == 0) || contact_phase == 1;

        if kamikaze_proxy {
            bridge_step_rope_physics(this);
        }

        if contact_phase != 0 {
            // Camera follow accumulator: (viewport_coords[3].center_x - pos_x).
            let world = (*(this as *const BaseEntity)).world;
            let viewport_x = (*world).viewport_coords[3].center_x.to_raw();
            let pos_x = (*this).base.pos_x.to_raw();
            (*world).field_7ea0 =
                ((*world).field_7ea0 as i32).wrapping_add(viewport_x.wrapping_sub(pos_x)) as u32;
        }

        super::render::missile_render(this);

        if (*this).underwater_entry_latched == 0 {
            bridge_render_indicator(this);
        }

        if kamikaze_proxy {
            bridge_restore_kamikaze_state(this);
        }

        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::RenderScene,
            size,
            data,
        );
    }
}

/// `0x7E` (126) — homing fuse-timer modifier sent by the homing-control UI.
///
/// `data` layout: `(sender_id: u32, mul: i32, div: i32)`. When the sender
/// owns this missile AND it's a homing missile (`missile_type == 3`):
/// - `mul >= 0` → `fuse_timer = fuse_timer + (fuse_timer * mul) / div`
/// - `mul < 0`  → `fuse_timer = i32::MAX` (effectively disable expiry)
///
/// Always handled — the gate-failed path is a no-op in WA.
///
/// NOTE: the multiplication is done as 32-bit (Ghidra-decomp interpretation);
/// on overflow the truncated u32 is reinterpreted as i32 before division.
/// If a desync surfaces here, WA's actual machine code may be doing 64-bit
/// IMUL/IDIV — revisit and switch to `i64` math.
unsafe fn msg_homing_fuse_modifier(this: *mut MissileEntity, msg: &Unknown126Message) {
    unsafe {
        if msg.sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if !matches!((*this).missile_type, super::MissileType::Homing) {
            return;
        }

        let fuse = (*this).fuse_timer;

        if msg.mul >= 0 {
            let product = (fuse as u32).wrapping_mul(msg.mul as u32) as i32;
            let quotient = if msg.div != 0 { product / msg.div } else { 0 };
            (*this).fuse_timer = quotient.wrapping_add(fuse);
        } else {
            (*this).fuse_timer = i32::MAX;
        }
    }
}

// ─── Dispatcher ────────────────────────────────────────────────────────────

/// Reinterpret the `*const u8` payload as a typed message ref. The caller
/// must ensure the payload was sent with this message-type's expected
/// shape (always true for messages the project broadcasts itself; WA's
/// senders are observed to honour the same shapes).
#[inline]
unsafe fn payload<T>(data: *const u8) -> &'static T {
    unsafe { &*(data as *const T) }
}

pub unsafe extern "thiscall" fn handle_message(
    this: *mut MissileEntity,
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
            EntityMessage::FrameFinish => {
                frame_finish::tick(this, sender, msg_type, size, data);
                true
            }
            EntityMessage::RenderScene => {
                msg_render_scene(this, sender, size, data);
                true
            }
            EntityMessage::UpdateNonCritical => {
                msg_update_non_critical(this);
                true
            }
            EntityMessage::Explosion => {
                msg_explosion(this, sender, size, payload::<ExplosionMessage>(data));
                true
            }
            EntityMessage::DetonateWeapon => {
                msg_detonate_weapon(this, payload::<DetonateWeaponMessage>(data));
                true
            }
            EntityMessage::MoveWeaponLeft => {
                msg_move_weapon_dir(this, payload::<MoveWeaponMessage>(data), -0x5B0);
                true
            }
            EntityMessage::MoveWeaponRight => {
                msg_move_weapon_dir(this, payload::<MoveWeaponMessage>(data), 0x5B0);
                true
            }
            EntityMessage::Unknown122 => {
                msg_sound_restore(this);
                true
            }
            EntityMessage::Unknown126 => {
                msg_homing_fuse_modifier(this, payload::<Unknown126Message>(data));
                true
            }
            _ => false,
        };
        if !handled {
            fall_through(this, sender, msg_type, size, data);
        }
    }
}

unsafe fn fall_through(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    let raw = ORIGINAL_HANDLE_MESSAGE.load(Ordering::Relaxed);
    debug_assert!(
        raw != 0,
        "MissileEntity::HandleMessage original ptr not initialized; vtable_replace! ran?"
    );
    let f: HandleMessageFn = unsafe { core::mem::transmute(raw as usize) };
    unsafe { f(this, sender, msg_type, size, data) }
}
