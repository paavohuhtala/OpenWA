//! Port of `MissileEntity::HandleMessage` case 2 (FrameFinish, 0x02).
//! WA 0x0050B400 case-2 body: 0x0050B656..0x0050BD16. Inner-tick dispatch
//! validated against the jump table at 0x0050BF88 — Ghidra/BN labels for
//! these per-type handlers were unreliable.

use openwa_core::fixed::Fixed;

use super::{MissileEntity, MissileType};
use crate::audio::sound_ops::play_sound_local;
use crate::audio::{KnownSoundId, SoundId};
use crate::engine::world::GameWorld;
use crate::entity::Entity;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::message::EntityMessage;
use crate::rebase::rb;

// ─── Bridge addresses ──────────────────────────────────────────────────────

static mut UPDATE_EFFECT_ADDR: u32 = 0;
static mut CHECK_FOR_ALARMED_WORM_ADDR: u32 = 0;
static mut INNER_TICK_HOMING_ADDR: u32 = 0;
static mut INNER_TICK_ANIMAL_ADDR: u32 = 0;
static mut INNER_TICK_DIGGER_ADDR: u32 = 0;
static mut INNER_TICK_CLUSTER_ADDR: u32 = 0;
static mut DETONATE_ADDR: u32 = 0;
static mut CREATE_BUBBLE_ADDR: u32 = 0;
static mut SPAWN_EFFECT_ADDR: u32 = 0;
static mut ALARM_TABLE_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        UPDATE_EFFECT_ADDR = rb(0x0050B240);
        CHECK_FOR_ALARMED_WORM_ADDR = rb(0x0050B110);
        INNER_TICK_HOMING_ADDR = rb(0x0050ABA0);
        INNER_TICK_ANIMAL_ADDR = rb(0x0050A7E0);
        INNER_TICK_DIGGER_ADDR = rb(0x0050A430);
        INNER_TICK_CLUSTER_ADDR = rb(0x0050A720);
        DETONATE_ADDR = rb(0x00509AC0);
        CREATE_BUBBLE_ADDR = rb(0x005472C0);
        SPAWN_EFFECT_ADDR = rb(0x00547C30);
        ALARM_TABLE_ADDR = rb(0x006AD288);
    }
}

// ─── WA bridges ────────────────────────────────────────────────────────────

/// `Task_Missile::update_effect` (0x0050B240) — `__usercall(ESI = this)`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_update_effect(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop esi",
        "ret 4",
        addr = sym UPDATE_EFFECT_ADDR,
    );
}

/// `Task_Missile::check_for_alarmed_worm` (0x0050B110) — `__usercall(EAX = this,
/// [stack] = sound_id, [stack] = threshold)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_check_for_alarmed_worm(
    _this: *mut MissileEntity,
    _sound_id: u32,
    _threshold: i32,
) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 12",
        addr = sym CHECK_FOR_ALARMED_WORM_ADDR,
    );
}

/// `Task_Missile::handle_homing` (0x0050ABA0) — `__usercall(EAX = this)`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_homing_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym INNER_TICK_HOMING_ADDR,
    );
}

/// `Task_Missile::handle_animal` (0x0050A7E0) — `__usercall(EAX = this)`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_animal_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym INNER_TICK_ANIMAL_ADDR,
    );
}

/// `Task_Missile::handle_digger` (0x0050A430) — stdcall(this), RET 0x4.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_digger_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym INNER_TICK_DIGGER_ADDR,
    );
}

/// `Task_Missile::cluster_crate_sweep` (0x0050A720) — `__usercall(ESI = this)`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_cluster_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop esi",
        "ret 4",
        addr = sym INNER_TICK_CLUSTER_ADDR,
    );
}

/// `Task_Missile::detonate` (0x00509AC0) — `__usercall(EAX = this,
/// [stack] = pos_x, [stack] = pos_y)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_detonate(
    _this: *mut MissileEntity,
    _pos_x: Fixed,
    _pos_y: Fixed,
) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 12",
        addr = sym DETONATE_ADDR,
    );
}

/// `GameTask::create_bubble_1` (0x005472C0) — `__usercall(EAX = pos_x,
/// ECX = pos_y, ESI = this, [stack] = zero, [stack] = kind)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_create_bubble(
    _this: *mut MissileEntity,
    _pos_x: Fixed,
    _pos_y: Fixed,
    _kind: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov eax, dword ptr [esp+12]",
        "mov ecx, dword ptr [esp+16]",
        "push dword ptr [esp+20]",
        "push 0",
        "mov edx, dword ptr [{addr}]",
        "call edx",
        "pop esi",
        "ret 16",
        addr = sym CREATE_BUBBLE_ADDR,
    );
}

/// `SpawnEffect` (0x00547C30) bridged with case-2's anim_kind = 0x80000
/// layout. `__usercall(EAX = anim_kind, ECX = pos_x, ESI = this)` + 7 stack
/// args (reverse-pushed): `pos_y, rng_dx, rng_dy, 0, pi_view_0, 0x10000,
/// state_flag`. RET 0x1C. Direct bridge — `weapon_release::spawn_effect`
/// writes a different anim_kind's permuted slot layout.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_spawn_effect(
    _this: *mut MissileEntity,
    _pos_x: Fixed,
    _pos_y: Fixed,
    _rng_dx: i32,
    _rng_dy: i32,
    _pi_view_0: u32,
    _state_flag: i32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [esp+12]",
        "mov eax, 0x80000",
        "push dword ptr [esp+0x20]",
        "push 0x10000",
        "push dword ptr [esp+0x24]",
        "push 0",
        "push dword ptr [esp+0x28]",
        "push dword ptr [esp+0x28]",
        "push dword ptr [esp+0x28]",
        "mov edx, dword ptr [{addr}]",
        "call edx",
        "pop esi",
        "ret 0x1C",
        addr = sym SPAWN_EFFECT_ADDR,
    );
}

// ─── Helpers ───────────────────────────────────────────────────────────────

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

/// Re-bound piVar8 view used inside the in-flight emit loop:
/// `+0x2F4` for normal flight, `+0x340` for homing-burn flight.
#[inline]
unsafe fn emit_pi_view(this: *const MissileEntity) -> [u32; 4] {
    unsafe {
        let base: *const u32 = if (*this).homing_engaged_latch != 0 {
            &(*this).impact_sound_id as *const u32
        } else {
            (*this)._render_data_08_0c.as_ptr()
        };
        [*base, *base.add(1), *base.add(2), *base.add(3)]
    }
}

// ─── Entry point ───────────────────────────────────────────────────────────

pub unsafe extern "thiscall" fn tick(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_version = (*this).game_version();

        if game_version >= 0x1D {
            (*this).super_animal_torque_accum = (*this)
                .super_animal_torque_accum
                .wrapping_add((*this).super_animal_torque_input);
            (*this).super_animal_torque_input = Fixed::ZERO;
        }

        WorldEntity::handle_message_raw(
            this as *mut WorldEntity,
            sender,
            EntityMessage::FrameFinish,
            size,
            data,
        );

        // Snapshot pos before update_effect mutates it; these are the values
        // WA caches at [ESP+0x10] / [ESP+0x14] for SpawnEffect /
        // RegisterEventPoint / detonate downstream.
        let pos_x_init = (*this).base.pos_x;
        let pos_y_init = (*this).base.pos_y;
        bridge_update_effect(this);

        let anim_kind = animation_rate_kind(this);
        let launch_seed = (*this).launch_seed;
        match anim_kind {
            4 => {
                let delta = launch_seed / 100;
                (*this).animation_phase = (*this).animation_phase.wrapping_add(delta);
            }
            5 => {
                let delta = launch_seed / 50;
                (*this).animation_phase = (*this).animation_phase.wrapping_add(delta);
            }
            6 => {
                let delta = launch_seed / 25;
                (*this).animation_phase = (*this).animation_phase.wrapping_add(delta);
            }
            _ => {}
        }

        if WorldEntity::is_moving_raw(this as *const WorldEntity)
            && (*this).base._field_b0 == 0
            && (*this).base._field_a4 == 0
        {
            let mass = (*this)._render_data_0e_0f[0] as i32;
            let rng_a = (*world).advance_rng();
            let dx_random = (((rng_a >> 8) & 0xFFFF) as i32).wrapping_sub(0x8000);
            let dx = mass.wrapping_mul(dx_random) / 100;
            let rng_b = (*world).advance_rng();
            let dy_random = (((rng_b >> 8) & 0xFFFF) as i32).wrapping_sub(0x8000);
            let dy = mass.wrapping_mul(dy_random) / 100;
            WorldEntity::add_impulse_raw(
                this as *mut WorldEntity,
                Fixed::from_raw(dx),
                Fixed::from_raw(dy),
                0,
            );
        }

        // POST-sway snapshot — splash-sound gate near the end requires this.
        let speed_y_pre = (*this).base.speed_y;

        if (*this).base._field_b0 == 0 {
            let fuse_timer = (*this).fuse_timer;
            if fuse_timer == 0 {
                let post_fuse_timer = (*this).post_fuse_terminate_timer;
                if post_fuse_timer != 0 {
                    let gate = (*this)._render_data_12_15[1];
                    let still_moving = if gate != 0 {
                        WorldEntity::is_moving_raw(this as *const WorldEntity)
                    } else {
                        false
                    };
                    if gate == 0 || !still_moving {
                        let new_pft = post_fuse_timer.wrapping_sub(0x14);
                        (*this).post_fuse_terminate_timer = new_pft;
                        if new_pft <= 0 {
                            (*this).post_fuse_terminate_timer = 0;
                        }
                        if (*this).post_fuse_sound_latched == 0 {
                            let sound_id = (*this)._render_data_12_15[2];
                            play_sound_local(
                                this as *mut WorldEntity,
                                SoundId(sound_id),
                                4,
                                Fixed::ONE,
                                Fixed::ONE,
                            );
                        }
                        (*this).post_fuse_sound_latched = 1;
                    }
                } else if matches!((*this).missile_type, MissileType::Animal)
                    && (*this).super_animal_walk_sprite != 0
                    && (*this).contact_phase == 1
                {
                    super::super_animal::finish_super_animal(this);
                } else {
                    MissileEntity::set_terminate_flag_raw(this, 1);
                }
            } else {
                let new_fuse = fuse_timer.wrapping_sub(0x14);
                (*this).fuse_timer = new_fuse;
                if new_fuse <= 0 {
                    (*this).fuse_timer = 0;
                    super::sound::stop_fuse_sound(this);
                }

                let fuse_now = (*this).fuse_timer;
                if fuse_now < 0x320
                    && (*this).weapon_data[2] != 0
                    && (*this).spawn_params.pellet_index == 0
                {
                    let mut alarm_table_size: u32 = 3;
                    if game_version >= 0x23 {
                        if game_version < 0x2C {
                            alarm_table_size = 4;
                        } else {
                            let fpt = (*this).fire_particle_trigger;
                            if fpt == 0x32 || fpt == 0x38 {
                                alarm_table_size = 4;
                            }
                        }
                    }
                    let rng = (*world).advance_rng();
                    let bucket = ((rng >> 16) & 0xFF) % alarm_table_size;
                    let sound_id = *(ALARM_TABLE_ADDR as *const u32).add(bucket as usize);
                    let threshold = (*this).weapon_data[6].wrapping_mul(2) as i32;
                    bridge_check_for_alarmed_worm(this, sound_id, threshold);
                    (*this).weapon_data[2] = 0;
                }
            }
        }

        // Round-toward-zero `level_width / 2` (matches WA's
        // `SUB EAX, sign(EAX); SAR EAX, 1`).
        let level_width = (*world).level_width as i32;
        let half_lw = (level_width - (level_width >> 31)) >> 1;
        let map_boundary_width = (*world).map_boundary_width as i32;
        let pos_x_int = pos_x_init.to_int();
        if (pos_x_int - half_lw).abs() >= map_boundary_width {
            MissileEntity::set_terminate_flag_raw(this, 1);
        }

        if (*this).contact_phase != 0 {
            GameWorld::register_event_point_raw(world, pos_x_init, pos_y_init);
        }

        if (*this).base._field_b0 != 0 && (*this).contact_phase == 1 {
            super::super_animal::finish_super_animal(this);
        }

        let mtype_now = (*this).missile_type;
        if (matches!(mtype_now, MissileType::Standard) || matches!(mtype_now, MissileType::Cluster))
            && !WorldEntity::is_moving_raw(this as *const WorldEntity)
            && (*this).ricochet_counter != 0
            && ((*this).ricochet_side_mask & 0x8) != 0
        {
            MissileEntity::set_terminate_flag_raw(this, 1);
        }

        // Same `0x80` Y-band shift HandleMessage applies to spawn-effect
        // emission while the homing burn is engaged.
        let pos_y_int = pos_y_init.to_int();
        let homing_y_bias: i32 = if (*this).homing_engaged_latch != 0 {
            0x80
        } else {
            0
        };
        let water_threshold = (*world).water_kill_y + homing_y_bias;

        if pos_y_int < water_threshold {
            match (*this).missile_type {
                MissileType::Homing => {
                    if (*this).base._field_b0 != 0 && ((*this).contact_face_mask & 0x400000) != 0 {
                        (*this).missile_type = MissileType::Zero;
                    } else {
                        bridge_inner_homing_tick(this);
                    }
                }
                MissileType::Animal => bridge_inner_animal_tick(this),
                MissileType::Digger => bridge_inner_digger_tick(this),
                MissileType::Cluster => bridge_inner_cluster_tick(this),
                MissileType::Zero | MissileType::Standard => {}
            }

            let owner_id = (*this).spawn_params.owner_id;
            if owner_id != 0 && (*this).detonate_response_mode != 0 {
                let team_entry_addr = (world as usize) + 0x4620 + (owner_id as usize) * 0x51C;
                if *(team_entry_addr as *const u32) == 0 {
                    if (*this).fuse_timer > 0xBB8 {
                        (*this).fuse_timer = 0xBB8;
                    }
                    (*this).textbox_visible_threshold = 0;
                    (*this).detonate_response_mode = 0;
                }
            }

            if (*this).base.subclass_data.terminate_flag == 0 {
                in_flight_body(this, world, pos_x_init, pos_y_init, speed_y_pre);
                return;
            }

            bridge_detonate(this, pos_x_init, pos_y_init);
        }

        set_world_activity_timer(world, 0xC);
        MissileEntity::free_raw(this, 1);
    }
}

/// In-flight body. All paths leave the missile alive — the caller does NOT
/// hit the detonate-and-free tail.
unsafe fn in_flight_body(
    this: *mut MissileEntity,
    world: *mut GameWorld,
    pos_x: Fixed,
    pos_y: Fixed,
    speed_y_pre: Fixed,
) {
    unsafe {
        let pos_y_int = pos_y.to_int();
        let pi_view = emit_pi_view(this);

        let underwater_or_blocked = (*this).base._field_b0 != 0 || (*this).base._field_a4 != 0;
        let bubble_path = underwater_or_blocked || pos_y_int >= (*world).water_level;

        if bubble_path {
            let inc = (pi_view[3] as i32).wrapping_shl(16) / 200;
            (*this).effect_emit_phase =
                (*this).effect_emit_phase.wrapping_add(Fixed::from_raw(inc));

            while (*this).effect_emit_phase >= Fixed::ONE {
                let rng = (*world).advance_effect_rng();
                let kind = ((rng >> 16) % 3) + 1;
                bridge_create_bubble(this, pos_x, pos_y, kind);
                (*this).effect_emit_phase = (*this).effect_emit_phase.wrapping_sub(Fixed::ONE);
            }
        } else {
            let inc = (pi_view[1] as i32).wrapping_shl(16) / 25;
            (*this).effect_emit_phase =
                (*this).effect_emit_phase.wrapping_add(Fixed::from_raw(inc));

            while (*this).effect_emit_phase >= Fixed::ONE {
                let rng_a = (*world).advance_effect_rng();
                let rng_b = (*world).advance_effect_rng();

                let rng_dx = ((rng_a & 0xFFFF) as i32).wrapping_sub(0x8000);
                let rng_dy = ((rng_b & 0xFFFF) as i32).wrapping_sub(0x8000);

                // Disasm SAR EDX,5 after IMUL by 0x51EB851F → divisor 200.
                // Ghidra mis-renders this as `/100`; BN agrees with disasm.
                let state_flag = (pi_view[2] as i32).wrapping_mul(0x147A) / 200;

                bridge_spawn_effect(this, pos_x, pos_y, rng_dx, rng_dy, pi_view[0], state_flag);

                (*this).effect_emit_phase = (*this).effect_emit_phase.wrapping_sub(Fixed::ONE);
            }
        }

        if (*this).base._field_a4 == 0 {
            (*this).splash_sound_latched = 0;
        } else {
            if (*this).splash_sound_latched == 0 && speed_y_pre.abs() > Fixed::ONE {
                play_sound_local(
                    this as *mut WorldEntity,
                    KnownSoundId::Splash,
                    5,
                    Fixed::ONE,
                    Fixed::ONE,
                );
            }
            (*this).splash_sound_latched = 1;
        }

        super::sound::check_fuse_sound(this);
        super::sound::check_dig_sound(this);

        if (*this).base._field_b0 != 0 && (*this).underwater_entry_latched == 0 {
            (*this).detonate_response_mode = 0;
            (*this).base.bucket_mask = 0x400000;
            super::sound::stop_fuse_sound(this);
            (*this).underwater_entry_latched = 1;
        }

        set_world_activity_timer(world, 0xC);

        let level_min_x = (*world).level_bound_min_x;
        let level_max_x = (*world).level_bound_max_x;
        let level_min_y = (*world).level_bound_min_y;

        let kind: u32 = if pos_x < level_min_x || pos_x > level_max_x || pos_y < level_min_y {
            if (*this).base._field_b0 == 0 { 8 } else { 0xA }
        } else if (*this).base._field_b0 == 0 {
            5
        } else {
            0xA
        };

        GameWorld::record_landing_event_raw(world, kind, pos_x, pos_y);
        (*this)._field_388 = 0;
    }
}

/// `GameTask::set_active` (0x00547ED0) — `__usercall(EDX = mode, ESI = this)`.
/// Refreshes both world activity-watchdog timers (`+0x5DC` and `+0x7E48`) to
/// `mode`, but only when each timer hasn't already decayed past `-mode`.
#[inline]
unsafe fn set_world_activity_timer(world: *mut GameWorld, mode: i32) {
    unsafe {
        if -mode <= (*world)._field_5dc {
            (*world)._field_5dc = mode;
        }
        if -mode <= (*world)._field_7e48 {
            (*world)._field_7e48 = mode;
        }
    }
}
