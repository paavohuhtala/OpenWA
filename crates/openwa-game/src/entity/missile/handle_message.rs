//! Incremental port of `MissileEntity::HandleMessage` (0x0050B400, vtable
//! slot 2). Ported cases run pure-Rust; the rest fall through to WA via
//! [`ORIGINAL_HANDLE_MESSAGE`].

use core::sync::atomic::{AtomicU32, Ordering};

use openwa_core::fixed::Fixed;

use super::super_animal::{finish_super_animal, start_super_animal};
use super::{MissileEntity, frame_finish, sound};
use crate::entity::Entity;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::entity::missile::MAX_STEERING_TORQUE;
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

pub static ORIGINAL_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

static mut STEP_ROPE_PHYSICS_ADDR: u32 = 0;
static mut RESTORE_KAMIKAZE_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        STEP_ROPE_PHYSICS_ADDR = rb(0x005003D0);
        RESTORE_KAMIKAZE_ADDR = rb(0x00500630);
    }
}

// ─── WA bridges ────────────────────────────────────────────────────────────

/// `WormEntity::StepRopePhysics_Maybe` (0x005003D0) — stdcall(this), RET 4.
unsafe fn bridge_step_rope_physics(this: *mut MissileEntity) {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut MissileEntity) =
            core::mem::transmute(STEP_ROPE_PHYSICS_ADDR as usize);
        f(this);
    }
}

/// `WormEntity::RestoreKamikazeState_Maybe` (0x00500630) — `__usercall(EAX = this)`.
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

/// `piVar8[2]` view at the top of WA's HandleMessage prologue.
#[inline]
unsafe fn animation_rate_kind(this: *const MissileEntity) -> u32 {
    unsafe {
        if (*this).homing_engaged_latch != 0 {
            (*this)._render_data_1a
        } else {
            (*this)._render_data_07
        }
    }
}

// ─── Per-case handlers ─────────────────────────────────────────────────────

/// Case `5` UpdateNonCritical.
unsafe fn msg_update_non_critical(this: *mut MissileEntity) {
    unsafe {
        if animation_rate_kind(this) != 3 {
            return;
        }
        let is_animal = matches!((*this).missile_type, super::MissileType::Animal);
        let underwater = (*this).base._field_b0 != 0;
        let super_animal_active = (*this).contact_phase == 1;
        if is_animal && (underwater || super_animal_active) {
            return;
        }

        let abs_sx = (*this).base.speed_x.wrapping_abs();
        let new = abs_sx
            .wrapping_div(Fixed(100))
            .wrapping_add(Fixed(0xCCC))
            .wrapping_add((*this).animation_phase)
            .fract();
        (*this).animation_phase = new;
    }
}

/// Case `0x2D` MoveWeaponLeft / `0x2E` MoveWeaponRight (`delta = ±0x5B0`).
unsafe fn msg_move_weapon_dir(this: *mut MissileEntity, msg: &MoveWeaponMessage, delta: Fixed) {
    unsafe {
        if msg.sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if (*this).contact_phase != 1 {
            return;
        }

        let game_version = (*this).game_version();

        if game_version < 0x1D {
            (*this).super_animal_torque_accum =
                (*this).super_animal_torque_accum.wrapping_add(delta);
        } else {
            let candidate = (*this).super_animal_torque_input.wrapping_add(delta);
            (*this).super_animal_torque_input =
                candidate.clamp(-MAX_STEERING_TORQUE, MAX_STEERING_TORQUE);
        }
    }
}

/// Case `0x1C` Explosion — forward to `WorldEntity::HandleMessage` when the
/// missile is configured to react.
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
            // WA's stack copy is 0x408 bytes (over-reads tail junk from a
            // larger buffer); copying the typed struct is equivalent since
            // the parent never reads past `ExplosionMessage`.
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

/// Case `0x2C` DetonateWeapon.
unsafe fn msg_detonate_weapon(this: *mut MissileEntity, msg: &DetonateWeaponMessage) {
    unsafe {
        if msg.team_index != (*this).spawn_params.owner_id {
            return;
        }
        if (*this).base._field_b0 != 0 {
            return;
        }

        let world = (*(this as *const BaseEntity)).world;

        if matches!((*this).missile_type, super::MissileType::Animal)
            && (*this).super_animal_walk_sprite != 0
        {
            match (*this).contact_phase {
                0 => {
                    start_super_animal(this);
                    return;
                }
                1 => {
                    let pos_y_int = (*this).base.pos_y.to_int();
                    if pos_y_int < (*world).water_kill_y {
                        finish_super_animal(this);
                        return;
                    }
                }
                _ => {}
            }
        }

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

/// Case `0x7A` (122) — sound-handle restore. A negative slot value with bit
/// `0x10000` set and low 16 bits `< 0x7F` is a deferred-retry sentinel
/// stashed by a previous failed `Task_Missile::start_*_sound`.
unsafe fn msg_sound_restore(this: *mut MissileEntity) {
    unsafe {
        let fuse_slot = (*this).fuse_sound_handle;
        if sound::is_deferred_sound_retry(fuse_slot) {
            sound::start_fuse_sound(this, fuse_slot.wrapping_neg());
        }
        let dig_slot = (*this).dig_sound_handle;
        if sound::is_deferred_sound_retry(dig_slot) {
            sound::start_dig_sound(this, dig_slot.wrapping_neg());
        }
    }
}

/// Case `0x03` RenderScene.
unsafe fn msg_render_scene(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let action_flag = (*this).base.subclass_data.action_flag;
        let digger_state_flag = (*this).base.subclass_data.digger_state_flag;
        let contact_phase = (*this).contact_phase;
        let kamikaze_proxy = (action_flag != 0 && digger_state_flag == 0) || contact_phase == 1;

        if kamikaze_proxy {
            bridge_step_rope_physics(this);
        }

        if contact_phase != 0 {
            let world = (*(this as *const BaseEntity)).world;
            let delta = (*world).viewport_coords[3]
                .center_x
                .wrapping_sub((*this).base.pos_x);
            (*world).field_7ea0 = (*world).field_7ea0.wrapping_add(delta.to_raw() as u32);
        }

        super::render::missile_render(this);

        if (*this).underwater_entry_latched == 0 {
            super::render::render_indicator(this);
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

/// Case `0x7E` (126) — fuse-timer modifier for `MissileType::Animal`.
///
/// NOTE: 32-bit IMUL/IDIV per Ghidra; if a desync surfaces here, WA's
/// machine code may be doing 64-bit math — switch to `i64`.
unsafe fn msg_homing_fuse_modifier(this: *mut MissileEntity, msg: &Unknown126Message) {
    unsafe {
        if msg.sender_id != (*this).spawn_params.owner_id {
            return;
        }
        if !matches!((*this).missile_type, super::MissileType::Animal) {
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
                frame_finish::tick(this, sender, size, data);
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
                msg_move_weapon_dir(
                    this,
                    payload::<MoveWeaponMessage>(data),
                    -MAX_STEERING_TORQUE,
                );
                true
            }
            EntityMessage::MoveWeaponRight => {
                msg_move_weapon_dir(
                    this,
                    payload::<MoveWeaponMessage>(data),
                    MAX_STEERING_TORQUE,
                );
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
