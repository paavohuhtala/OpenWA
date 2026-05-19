//! Port of `MissileEntity::HandleMessage` case 2 (FrameFinish, 0x02).
//! WA 0x0050B400 case-2 body: 0x0050B656..0x0050BD16. Inner-tick dispatch
//! validated against the jump table at 0x0050BF88 — Ghidra/BN labels for
//! these per-type handlers were unreliable.

use std::ptr::null;

use openwa_core::fixed::Fixed;
use openwa_core::vec2::Vec2;

use super::{MissileEntity, MissileType};
use crate::audio::sound_ops::{play_sound_local, queue_sound};
use crate::audio::{KnownSoundId, SoundId};
use crate::engine::world::GameWorld;
use crate::entity::Entity;
use crate::entity::base::BaseEntity;
use crate::entity::fire::{FireEntity, FireEntityInit, fire_entity_construct};
use crate::entity::game_entity::WorldEntity;
use crate::entity::shared_data::SharedDataTable;
use crate::game::message::{EntityMessage, WeaponHomingMessage};
use crate::generated::wa_calls;
use crate::rebase::rb;
use crate::wa_alloc::wa_malloc;

// ─── Bridge addresses ──────────────────────────────────────────────────────

static mut CREATE_BUBBLE_ADDR: u32 = 0;
static mut COLLECT_CRATE_ADDR: u32 = 0;
static mut ALARM_TABLE_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        CREATE_BUBBLE_ADDR = rb(0x005472C0);
        COLLECT_CRATE_ADDR = rb(0x00501340);
        ALARM_TABLE_ADDR = rb(0x006AD288);
    }
}

// ─── WA bridges ────────────────────────────────────────────────────────────

/// `GameCollisionTask::collect_crate` (0x00501340) — stdcall(this, owner_id,
/// pickup_class, &flag), RET 0x10. Returns the picked-up crate kind (1/2/4/5)
/// in EAX, or 0 if no crate was collected. Sets `*flag = 1` when the
/// collected crate's contents include weapon id 0x45 (Cluster Bomb).
unsafe extern "stdcall" fn bridge_collect_crate(
    this: *mut MissileEntity,
    owner_id: u32,
    pickup_class: u32,
    flag: *mut u8,
) -> i32 {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut MissileEntity, u32, u32, *mut u8) -> i32 =
            core::mem::transmute(COLLECT_CRATE_ADDR);
        f(this, owner_id, pickup_class, flag)
    }
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

// ─── Pure-Rust ports ───────────────────────────────────────────────────────

/// Pure-Rust port of `MissileEntity::update_animal_poison` (0x0050A820).
/// Drives the post-fuse poison-emission cycle for the (super-)animal weapon
/// family. Once the per-frame countdown stored in `_render_data_23_24[0]`
/// (+0x360) decrements to ≤ 0, sets the alt-render / poison-mode latch
/// `_unknown_3a4` (+0x3A4); thereafter, while latched and above water
/// (`underwater_entry_latched == 0`), spawns one [`GasEntity`] per frame
/// trailing behind the missile (Y+4.0 / X-4.0·direction) using the gas
/// sprite stored in `_render_data_23_24[1]` (+0x364) and the owning team.
///
/// [`GasEntity`]: WA's gas-cloud entity (vtable 0x00669E50, ctor 0x00554750)
unsafe fn update_animal_poison(this: *mut MissileEntity) {
    unsafe {
        if (*this)._unknown_3a4 != 0 && (*this).underwater_entry_latched == 0 {
            let pos_x = (*this).base.pos.x;
            let pos_y = (*this).base.pos.y;
            let parent = SharedDataTable::from_entity(this)
                .filter_physics()
                .unwrap_or(core::ptr::null_mut()) as *mut u8;

            let buf = wa_malloc(0x88);
            if !buf.is_null() {
                core::ptr::write_bytes(buf, 0, 0x68);
                // 4.0 fixed offset behind the animal (`direction << 18`),
                // 4.0 fixed offset below.
                let dx = Fixed::from_raw((*this).direction.wrapping_shl(18));
                let dy = Fixed::from_raw(0x40000);
                let sprite = (*this)._render_data_23_24[1];
                let owner = (*this).spawn_params.owner_id;
                wa_calls::GasEntity::Constructor(
                    buf as *mut core::ffi::c_void,
                    parent as *mut core::ffi::c_void,
                    pos_x - dx,
                    pos_y + dy,
                    sprite,
                    owner,
                );
            }
        }

        let timer = (*this)._render_data_23_24[0] as i32;
        if timer != 0 {
            let new_timer = timer.wrapping_sub(0x14);
            (*this)._render_data_23_24[0] = new_timer as u32;
            if new_timer <= 0 {
                (*this)._render_data_23_24[0] = 0;
                (*this)._unknown_3a4 = 1;
            }
        }
    }
}

/// Pure-Rust port of `MissileEntity::create_fire_1` (0x00509D70). Spawns one
/// incendiary [`FireEntity`] when an incendiary missile detonates
/// (`weapon_data[0x2D] == 2`). Mirrors the spawn-buffer the original builds
/// on the stack (kind=8, `_flag_18=1`, four `weapon_data[47..50]` slots,
/// owner team) and plays the OilDrumImpact sound (0x3B) at the explicit
/// detonation coordinates — queued globally then promoted in-place to a
/// positional local entry (no entity tracking, mirroring WA's manual sound
/// queue patch).
unsafe fn create_fire_1(this: *mut MissileEntity, pos_x: Fixed, pos_y: Fixed) {
    unsafe {
        let base = this as *mut BaseEntity;
        let fire_parent = SharedDataTable::from_entity(base)
            .filter_water()
            .unwrap_or(core::ptr::null_mut()) as *mut u8;

        let init = FireEntityInit {
            spawn_x: pos_x,
            spawn_y: pos_y,
            spawn_offset_x: Fixed::ZERO,
            spawn_offset_y: Fixed::from_raw(0x40000),
            _flag_10: 0,
            kind: 8,
            _flag_18: 1,
            fp_collision_radius: Fixed::from_raw((*this).weapon_data[49] as i32),
            fp_02: (*this).weapon_data[48] as i32,
            fp_spread: ((*this).weapon_data[47] as i32) / 2,
            fp_04: (*this).weapon_data[50] as i32,
            team_index: (*this).spawn_params.owner_id,
        };

        let buf = wa_malloc(0xD8);
        if !buf.is_null() {
            // WA only zeroes the first 0xB8 bytes; the trailing 0x20 bytes
            // are left untouched (the FireEntity ctor doesn't read them
            // before initialising).
            core::ptr::write_bytes(buf, 0, 0xB8);
            fire_entity_construct(buf as *mut FireEntity, fire_parent, &init, 0);
        }

        // Equivalent to `PlaySoundGlobal(this, 0x3B, 5, 1.0, 1.0)` followed by
        // the manual sound-queue patch the original applies to promote the
        // just-queued entry to a positional local sound at (pos_x, pos_y).
        // `emitter = null` ⇒ no entity ref tracking.
        let world = (*this).base.base.world;
        if let Some(entry) = queue_sound(world, KnownSoundId::Petrol, 5, Fixed::ONE, Fixed::ONE) {
            (*entry).is_local = 1;
            (*entry).emitter = null();
            (*entry).pos = Vec2::new(pos_x, pos_y);
        }
    }
}

/// Pure-Rust port of `Task_Missile::detonate` (0x00509AC0). Plays the
/// special-weapon explosion grunt for `fire_particle_trigger == 0x4A`,
/// computes the jittered explosion damage from the fuse-detonate render-data
/// slots (different slots than `OnContact` uses — see field doc), creates
/// the explosion via [`create_explosion`], then dispatches the
/// `weapon_data[0x2D]` follow-up:
/// 1, 3 → `create_clusters` (when not a sub-pellet);
/// 2    → `create_fire_1` (incendiary);
/// else → no follow-up.
///
/// [`create_explosion`]: crate::game::create_explosion::create_explosion
unsafe fn detonate(this: *mut MissileEntity, pos_x: Fixed, pos_y: Fixed) {
    unsafe {
        if (*this).fire_particle_trigger == 0x4A {
            play_sound_local(
                this as *mut WorldEntity,
                SoundId(0x40),
                5,
                Fixed::ONE,
                Fixed::ONE,
            );
        }

        // Damage sources differ from OnContact:
        //   OnContact uses explosion_damage / explosion_damage_pct (+0x354/+0x358)
        //   detonate  uses _render_data_01_05[2] / [3] (+0x2E0/+0x2E4)
        // (the fuse-expiry explosion uses different render_data slots than
        //  the contact explosion — same field-polymorphism pattern as the
        //  Homing missile subtype).
        let base_damage = (*this)._render_data_01_05[2];
        let damage_pct = (*this)._render_data_01_05[3];
        let damage = crate::game::missile_contact::explosion_damage_jitter(
            base_damage,
            this as *mut core::ffi::c_void,
            damage_pct,
            rb(crate::game::missile_contact::VA_EXPLOSION_DAMAGE_JITTER),
        );

        if damage != 0 {
            // Pos-Y offset = signed `(rd0 * rd2) / 200` promoted to Fixed.
            // Used to lift the explosion epicentre above the missile's
            // ground-contact point (visual / damage-radius tweak).
            let offset = ((*this)._render_data_01_05[0] as i32)
                .wrapping_mul((*this)._render_data_01_05[2] as i32);
            let pos_y_adj = pos_y.wrapping_add(Fixed::from_int(offset / 200));

            crate::game::create_explosion::create_explosion(
                pos_x,
                pos_y_adj,
                this as *mut BaseEntity,
                (*this)._render_data_01_05[1],
                damage,
                1,
                (*this).spawn_params.owner_id,
            );
        }

        match (*this).weapon_data[0x2D] {
            1 | 3 if (*this).spawn_params.pellet_index == 0 => {
                wa_calls::MissileEntity::create_clusters(this, pos_x, pos_y);
            }
            2 => {
                create_fire_1(this, pos_x, pos_y);
            }
            _ => {}
        }
    }
}

/// Pure-Rust port of `Task_Missile::update_effect` (0x0050B240). Drives the
/// secondary "trail" particle stream gated by `sprite_size & 0x40000000`:
/// each tick adds [`trail_emit_step`] to [`trail_emit_phase`] and emits one
/// `SpawnEffect` particle (anim_kind = `0xE0000`) per `Fixed::ONE` consumed.
/// The step then decays by `0xCCC` per frame, clamped to `[0, Fixed::ONE]`.
///
/// [`trail_emit_step`]: MissileEntity::trail_emit_step
/// [`trail_emit_phase`]: MissileEntity::trail_emit_phase
unsafe fn update_effect(this: *mut MissileEntity) {
    unsafe {
        if ((*this).sprite_size.to_raw() as u32 & 0x40000000) == 0 {
            return;
        }

        let step = (*this).trail_emit_step;
        (*this).trail_emit_phase = (*this).trail_emit_phase.wrapping_add(step);

        let world = (*(this as *const BaseEntity)).world;
        let pos_x = (*this).base.pos.x;
        let pos_y = (*this).base.pos.y;

        // High 16 bits non-zero ⇔ phase ≥ Fixed::ONE (phase is non-negative
        // here — step is clamped to [0, ONE] and phase only grows by adding
        // step, then decreases by ONE per emit).
        while ((*this).trail_emit_phase.to_raw() as u32 & 0xFFFF_0000) != 0 {
            // Four effect-RNG advances per emit. Their dst registers in WA:
            //   R1 (1st) → buf[0x10] offset; R2 → buf[0x0C]; R3 → buf[0x08] Y;
            //   R4 (4th) → buf[0x04] X.
            let r1 = (*world).advance_effect_rng();
            let r2 = (*world).advance_effect_rng();
            let r3 = (*world).advance_effect_rng();
            let r4 = (*world).advance_effect_rng();

            let dx = (r4 & 0xFFFF) as i32 * 4 - 0x20000;
            let dy = (r3 & 0xFFFF) as i32 * 4 - 0x20000;
            let rng_scaled = (r2 & 0xFFFF) as i32 - 0x8000;
            let rng_offset = (r1 & 0xFFFF) as i32 - 0x8000;

            crate::game::weapon_release::spawn_effect(
                this as *mut BaseEntity,
                0xE0000,
                Fixed::from_raw(pos_x.to_raw().wrapping_add(dx)),
                Fixed::from_raw(pos_y.to_raw().wrapping_add(dy)),
                rng_scaled,
                rng_offset,
                0,
                0x50,
                Fixed::ONE,
                Fixed::from_raw(0x51E),
            );

            (*this).trail_emit_phase = (*this).trail_emit_phase.wrapping_sub(Fixed::ONE);
        }

        let new_step = step.to_raw().wrapping_sub(0xCCC);
        (*this).trail_emit_step = if new_step < 0 {
            Fixed::ZERO
        } else if new_step > 0x10000 {
            Fixed::ONE
        } else {
            Fixed::from_raw(new_step)
        };
    }
}

/// Pure-Rust port of `MissileEntity::cluster_crate_sweep` (0x0050A720). Per-tick
/// in-flight crate-pickup sweep used by `MissileType::Cluster` and as the
/// first step of [`inner_animal_tick`]. Repeatedly invokes
/// `GameCollisionTask::collect_crate`; each successful pickup that reports the
/// "cluster-bomb contents" flag (weapon 0x45) extends the fuse by
/// `fuse_timer / pickup_count` (a diminishing time bonus). The pickup count is
/// bounded by [`GameInfo::crate_pickup_limit`]
/// (`game_info[+0xD9AF]`; 0 = unlimited). The loop continues while
/// `collect_crate` returned a non-zero kind AND the scheme is recent enough
/// (`game_version > 0x1B1`).
unsafe fn cluster_crate_sweep(this: *mut MissileEntity) {
    unsafe {
        let owner_id = (*this).spawn_params.owner_id;
        let pickup_class = (*this).spawn_params.owner_worm_id;
        if owner_id == 0 {
            return;
        }
        let world = (*this).base.base.world;

        loop {
            let mut flag: u8 = 0;
            let crate_kind = bridge_collect_crate(this, owner_id, pickup_class, &mut flag);

            let game_info = (*world).game_info;
            // game_version in [0xD2, 0x1E3] — historical pickup-extend window.
            let in_pickup_window = ((*game_info).game_version as u32).wrapping_sub(0xD2) < 0x112;
            if in_pickup_window && flag != 0 {
                let limit = (*game_info).crate_pickup_limit;
                if limit == 0 || (*this).crate_pickup_count < limit as u32 {
                    let old_fuse = (*this).fuse_timer;
                    (*this).crate_pickup_count = (*this).crate_pickup_count.wrapping_add(1);
                    let count = (*this).crate_pickup_count as i32;
                    (*this).fuse_timer = old_fuse.wrapping_add(old_fuse / count);
                }
            }

            if crate_kind == 0 || (*game_info).game_version <= 0x1B1 {
                return;
            }
        }
    }
}

// ─── Per-missile-type inner-tick dispatchers ───────────────────────────────

/// Pure-Rust port of `Task_Missile::handle_homing` (0x0050ABA0). Drives the
/// homing-missile state machine: lock-on countdown (`+0x354`) gates target
/// acquisition; once acquired, the burn timer (`+0x358`) gates active
/// `apply_direct_homing` / `apply_pigeon_homing` steering. When the lock-on
/// timer expires the missile broadcasts [`WeaponHomingMessage`] via the
/// world root.
unsafe fn inner_homing_tick(this: *mut MissileEntity) {
    unsafe {
        // For `MissileType::Homing`, `explosion_id` / `explosion_damage` /
        // `explosion_damage_pct` are repurposed (see field docs).
        let lock_timer = (*this).explosion_damage as i32;
        if lock_timer != 0 {
            let new_lock = lock_timer.wrapping_sub(0x14);
            (*this).explosion_damage = new_lock as u32;
            if new_lock < 1 {
                let owner_id = (*this).spawn_params.owner_id;
                if owner_id != 0 {
                    (*this).broadcast_via_world_root(WeaponHomingMessage {
                        team_index: owner_id,
                    });
                }
                (*this).explosion_damage = 0;
            }
            return;
        }

        if (*this).explosion_damage_pct == 0 {
            return;
        }

        match (*this).explosion_id {
            1 => wa_calls::MissileEntity::apply_direct_homing(this),
            2 => {
                wa_calls::MissileEntity::apply_direct_homing(this);
                wa_calls::MissileEntity::apply_pigeon_homing(this);
            }
            _ => {}
        }

        let new_burn = ((*this).explosion_damage_pct as i32).wrapping_sub(0x14);
        (*this).explosion_damage_pct = new_burn as u32;
        if new_burn < 1 {
            (*this).explosion_damage_pct = 0;
            (*this).homing_engaged_latch = 0;
        }
    }
}

/// Pure-Rust port of `Task_Missile::handle_animal` (0x0050A7E0). Sweeps for
/// nearby crates, dispatches the per-`contact_phase` body (jetpack vs.
/// walking), then trails poison gas if the animal is in alt-sprite mode.
unsafe fn inner_animal_tick(this: *mut MissileEntity) {
    unsafe {
        cluster_crate_sweep(this);
        if (*this).contact_phase == 1 {
            wa_calls::MissileEntity::handle_flying_animal(this);
        } else {
            wa_calls::MissileEntity::handle_walking_animal(this);
        }
        update_animal_poison(this);
    }
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
        let pos_x_init = (*this).base.pos.x;
        let pos_y_init = (*this).base.pos.y;
        update_effect(this);

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
                    wa_calls::MissileEntity::check_for_alarmed_worm(this, sound_id, threshold);
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
                        inner_homing_tick(this);
                    }
                }
                MissileType::Animal => inner_animal_tick(this),
                MissileType::Digger => wa_calls::MissileEntity::handle_digger(this),
                MissileType::Cluster => cluster_crate_sweep(this),
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

            detonate(this, pos_x_init, pos_y_init);
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

                // Anim_kind 0x80000 spark emit. SpawnEffect's slot layout is
                // shared across anim_kinds (offsets fixed by 0x547C30); the
                // weapon_release::spawn_effect param names reflect the
                // weapon-release caller's interpretation, so the case-2 args
                // map by slot:
                //   palette/slot_18      = 0
                //   state_flag/slot_1C   = pi_view[0]
                //   size/slot_24         = Fixed::ONE
                //   scale/slot_28        = state_flag (case-2 derived val)
                crate::game::weapon_release::spawn_effect(
                    this as *mut BaseEntity,
                    0x80000,
                    pos_x,
                    pos_y,
                    rng_dx,
                    rng_dy,
                    0,
                    pi_view[0],
                    Fixed::ONE,
                    Fixed::from_raw(state_flag),
                );

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
