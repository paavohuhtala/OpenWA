//! Port of `MissileEntity::HandleMessage` case 2 (FrameFinish, 0x02).
//!
//! Source: WA 0x0050B400 (HandleMessage), case-2 body spans 0x0050B656..0x0050BD16.
//! Inner-tick dispatch table at `0x0050BF88` was read directly to fix
//! mis-attributions — Ghidra's `handle_homing` / `handle_animal` /
//! `handle_digger` / `cluster_tick` labels are unreliable for this class; use
//! the address.
//!
//! ## SpawnEffect bridge (anim_kind = 0x80000)
//!
//! `weapon_release::spawn_effect` writes a different anim_kind's permuted
//! buffer layout (palette/state_flag/size/scale at the wrong slots for case
//! 2). We bridge `SpawnEffect` (0x00547C30) directly with the case-2 layout
//! — see `bridge_spawn_effect` below.

use openwa_core::fixed::Fixed;

use super::{MissileEntity, MissileType};
use crate::audio::sound_ops::play_sound_local;
use crate::audio::{KnownSoundId, SoundId};
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::game_entity_message::world_entity_handle_message;
use crate::game::message::EntityMessage;
use crate::rebase::rb;

// ─── Bridge addresses ──────────────────────────────────────────────────────

static mut UPDATE_EFFECT_ADDR: u32 = 0;
static mut CHECK_FOR_ALARMED_WORM_ADDR: u32 = 0;
static mut INNER_UNKNOWN1_TICK_ADDR: u32 = 0;
static mut INNER_HOMING_TICK_ADDR: u32 = 0;
static mut INNER_SHEEP_TICK_ADDR: u32 = 0;
static mut INNER_CLUSTER_TICK_ADDR: u32 = 0;
static mut DETONATE_ADDR: u32 = 0;
static mut CREATE_BUBBLE_ADDR: u32 = 0;
static mut SPAWN_EFFECT_ADDR: u32 = 0;
static mut ALARM_TABLE_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        UPDATE_EFFECT_ADDR = rb(0x0050B240);
        CHECK_FOR_ALARMED_WORM_ADDR = rb(0x0050B110);
        // Inner-tick dispatch verified against jump table at 0x0050BF88
        // (bytes feb95000 bfb95000 feb95000 efb95000 f8b95000 e4b95000)
        // and BN's case targets — Ghidra/brief had Homing/Sheep/Cluster mis-mapped.
        INNER_UNKNOWN1_TICK_ADDR = rb(0x0050ABA0); // missile_type = 1
        INNER_HOMING_TICK_ADDR = rb(0x0050A7E0); //   missile_type = 3
        INNER_SHEEP_TICK_ADDR = rb(0x0050A430); //    missile_type = 4 (stdcall)
        INNER_CLUSTER_TICK_ADDR = rb(0x0050A720); //  missile_type = 5
        DETONATE_ADDR = rb(0x00509AC0);
        CREATE_BUBBLE_ADDR = rb(0x005472C0);
        SPAWN_EFFECT_ADDR = rb(0x00547C30);
        ALARM_TABLE_ADDR = rb(0x006AD288);
    }
}

// ─── WA bridges ────────────────────────────────────────────────────────────

/// `__usercall(ESI = this)`, plain RET. ESI callee-saved.
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

/// `__usercall(EAX = this, [stack] = sound_id, [stack] = threshold)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_check_for_alarmed_worm(
    _this: *mut MissileEntity,
    _sound_id: u32,
    _threshold: i32,
) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",   // this
        "push dword ptr [esp+12]",      // threshold (deeper, pushed first)
        "push dword ptr [esp+12]",      // sound_id (was at +8, now at +12 after first push)
        "mov ecx, dword ptr [{addr}]",
        "call ecx",                      // WA RET 0x8 cleans 2 stack args
        "ret 12",                        // clean Rust 3 args
        addr = sym CHECK_FOR_ALARMED_WORM_ADDR,
    );
}

/// `__usercall(EAX = this)`, plain RET. Inner tick for missile_type = Unknown1.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_unknown1_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym INNER_UNKNOWN1_TICK_ADDR,
    );
}

/// `__usercall(EAX = this)`, plain RET. Inner tick for missile_type = Homing.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_homing_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym INNER_HOMING_TICK_ADDR,
    );
}

/// `__stdcall(this)`, RET 0x4. Inner tick for missile_type = Sheep.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_sheep_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push dword ptr [esp+4]",       // forward this onto WA stack
        "mov ecx, dword ptr [{addr}]",
        "call ecx",                      // WA RET 0x4 cleans the 1 stack arg
        "ret 4",                         // clean Rust caller's 1 arg
        addr = sym INNER_SHEEP_TICK_ADDR,
    );
}

/// `__usercall(ESI = this)`, plain RET. Inner tick for missile_type = Cluster.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_inner_cluster_tick(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop esi",
        "ret 4",
        addr = sym INNER_CLUSTER_TICK_ADDR,
    );
}

/// `__usercall(EAX = this, [stack] = pos_x, [stack] = pos_y)`, RET 0x8.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_detonate(
    _this: *mut MissileEntity,
    _pos_x: Fixed,
    _pos_y: Fixed,
) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",   // this
        "push dword ptr [esp+12]",      // pos_y
        "push dword ptr [esp+12]",      // pos_x (was at +8, now at +12)
        "mov ecx, dword ptr [{addr}]",
        "call ecx",                      // WA RET 0x8
        "ret 12",
        addr = sym DETONATE_ADDR,
    );
}

/// `__usercall(EAX = pos_x, ECX = pos_y, ESI = this, [stack] = zero, [stack] = kind)`,
/// RET 0x8. Same shape as MineEntity's `bridge_create_bubble`.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_create_bubble(
    _this: *mut MissileEntity,
    _pos_x: Fixed,
    _pos_y: Fixed,
    _kind: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",   // this
        "mov eax, dword ptr [esp+12]",  // pos_x
        "mov ecx, dword ptr [esp+16]",  // pos_y
        "push dword ptr [esp+20]",      // kind
        "push 0",                        // unknown leading zero
        "mov edx, dword ptr [{addr}]",
        "call edx",                      // WA RET 0x8
        "pop esi",
        "ret 16",
        addr = sym CREATE_BUBBLE_ADDR,
    );
}

/// `__usercall(EAX = 0x80000, ECX = pos_x, ESI = this) + 7 stack args`,
/// RET 0x1C. The 7 stack args (reverse-push order, last push = arg1) are:
/// `pos_y, rng_dx, rng_dy, 0, pi_view_0, 0x10000, state_flag`.
///
/// Args 6 and 4 (the literal `0x10000` and `0`) are immediates — they are not
/// Rust-caller arguments. Args 1, 2, 3, 5, 7 come from Rust callers, plus
/// pos_x (ECX) and this (ESI).
///
/// `RET 0x1C` cleans 28 bytes = 7 stack args. We `ret 0x1C` to clean the 7
/// Rust caller arguments (also 28 bytes). ESI is callee-saved per ABI.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_spawn_effect(
    _this: *mut MissileEntity, // ESP+4
    _pos_x: Fixed,             // ESP+8
    _pos_y: Fixed,             // ESP+12
    _rng_dx: i32,              // ESP+16
    _rng_dy: i32,              // ESP+20
    _pi_view_0: u32,           // ESP+24
    _state_flag: i32,          // ESP+28
) {
    core::arch::naked_asm!(
        "push esi",                              // save ESI; offsets shift +4
        "mov esi, dword ptr [esp+8]",            // this
        "mov ecx, dword ptr [esp+12]",           // pos_x
        "mov eax, 0x80000",                      // anim_kind
        "push dword ptr [esp+0x20]",             // arg7 = state_flag
        "push 0x10000",                          // arg6 (immediate)
        "push dword ptr [esp+0x24]",             // arg5 = pi_view_0
        "push 0",                                // arg4 (immediate)
        "push dword ptr [esp+0x28]",             // arg3 = rng_dy
        "push dword ptr [esp+0x28]",             // arg2 = rng_dx (offset shift +4)
        "push dword ptr [esp+0x28]",             // arg1 = pos_y  (offset shift +4 again)
        "mov edx, dword ptr [{addr}]",
        "call edx",                              // WA RET 0x1C cleans 7 stack args
        "pop esi",
        "ret 0x1C",                              // clean 7 Rust caller args
        addr = sym SPAWN_EFFECT_ADDR,
    );
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// HandleMessage prologue piVar8 view: piVar8[2] addresses
/// `_render_data_07` (single-shot) or `_render_data_1a` (cluster).
/// Match for the animation-rate dispatcher.
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

/// BLOCK A1 emit-pi-view: re-bound view used for the bubble/spark inner blocks.
/// Single-shot reads from `_render_data_08_0c` (4 dwords starting at +0x2F4);
/// cluster reads from `impact_sound_id`/`ricochet_side_mask`/`ricochet_chance_pct`/
/// `_render_data_1e` (4 dwords starting at +0x340). Returns the 4-element slice
/// as raw u32 reads.
#[inline]
unsafe fn emit_pi_view(this: *const MissileEntity) -> [u32; 4] {
    unsafe {
        let base: *const u32 = if (*this).is_cluster_pellet != 0 {
            // +0x340
            &(*this).impact_sound_id as *const u32
        } else {
            // +0x2F4
            (*this)._render_data_08_0c.as_ptr()
        };
        [*base, *base.add(1), *base.add(2), *base.add(3)]
    }
}

/// Round-toward-zero divide-by-N via WA's IMUL-by-magic-then-SAR-then-add-sign
/// pattern. Equivalent to Rust's signed integer division (`a / n`) for the
/// values in play here. Kept as a one-liner so the call sites read 1:1 with
/// the disasm.
#[inline]
fn div_round_to_zero(a: i32, n: i32) -> i32 {
    a / n
}

// ─── Entry point ───────────────────────────────────────────────────────────

/// Pure-Rust port of `MissileEntity::HandleMessage` case 2 (FrameFinish).
///
/// Returns `true` when the call ended via the "in-flight" `RecordLandingEvent`
/// branch (mirroring WA's early returns). Returns `false` when the call ended
/// via the detonate-and-free tail (which destroys the missile via
/// `MissileEntity::free_raw(this, 1)` before returning). Both paths are
/// handled correctly inside this function — the bool is informational only.
///
/// SAFETY: caller must guarantee `this` points to a live `MissileEntity` whose
/// vtable is `MissileEntityVtable`-shaped, and `data` is a 0x408-byte payload.
pub unsafe extern "thiscall" fn tick(
    this: *mut MissileEntity,
    sender: *mut BaseEntity,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let game_version = (*game_info).game_version;

        // ── BLOCK A0: torque fold (modern schemes only) ─────────────────
        if game_version >= 0x1D {
            (*this).super_animal_torque_accum = (*this)
                .super_animal_torque_accum
                .wrapping_add((*this).super_animal_torque_input as u32);
            (*this).super_animal_torque_input = 0;
        }

        // ── BLOCK A1: forward to WorldEntity::HandleMessage ─────────────
        // WA: sub_4ff280(this, sender, msg_type, size, data) — full 5-arg passthrough.
        world_entity_handle_message(
            this as *mut WorldEntity,
            sender,
            EntityMessage::FrameFinish,
            size,
            data,
        );
        // sender / msg_type are forwarded as-is. msg_type is always 2 here
        // (= EntityMessage::FrameFinish); the variable is kept to mirror WA.
        let _ = msg_type;

        // ── BLOCK A2: snapshot pos + update_effect ──────────────────────
        // pos_x_init / pos_y_init are TAKEN HERE — before the inner tick
        // mutates pos. These are the values WA caches at `[ESP+0x10]` /
        // `[ESP+0x14]` for the SpawnEffect / RegisterEventPoint / detonate
        // calls below.
        let pos_x_init = (*this).base.pos_x;
        let pos_y_init = (*this).base.pos_y;
        bridge_update_effect(this);

        // ── BLOCK A3: animation_phase update by HandleMessage piVar8[2] ──
        // The HandleMessage prologue sets piVar8 to +0x2E8 (single) / +0x334
        // (cluster); piVar8[2] is _render_data_07 / _render_data_1a. Three
        // animation-rate kinds (4/5/6) advance animation_phase by
        // launch_seed/100, /50, /25 respectively (round toward zero).
        let anim_kind = animation_rate_kind(this);
        let launch_seed = (*this).launch_seed as i32;
        match anim_kind {
            4 => {
                let delta = div_round_to_zero(launch_seed, 100);
                (*this).animation_phase = (*this).animation_phase.wrapping_add(delta as u32);
            }
            5 => {
                let delta = div_round_to_zero(launch_seed, 50);
                (*this).animation_phase = (*this).animation_phase.wrapping_add(delta as u32);
            }
            6 => {
                let delta = div_round_to_zero(launch_seed, 25);
                (*this).animation_phase = (*this).animation_phase.wrapping_add(delta as u32);
            }
            _ => {}
        }

        // ── BLOCK A4: random sway via vt[17] (apply_impulse) ────────────
        // Gated on IsMoving + above-water (both _field_b0 and _field_a4 zero).
        // Two RNG advances per call; impulse magnitude scaled by mass at
        // `_render_data_0e_0f[0]` (+0x30C). Round-toward-zero division by 100.
        if WorldEntity::is_moving_raw(this as *const WorldEntity)
            && (*this).base._field_b0 == 0
            && (*this).base._field_a4 == 0
        {
            let mass = (*this)._render_data_0e_0f[0] as i32;
            let rng_a = (*world).advance_rng();
            let dx_random = (((rng_a >> 8) & 0xFFFF) as i32).wrapping_sub(0x8000);
            let dx = div_round_to_zero(mass.wrapping_mul(dx_random), 100);
            let rng_b = (*world).advance_rng();
            let dy_random = (((rng_b >> 8) & 0xFFFF) as i32).wrapping_sub(0x8000);
            let dy = div_round_to_zero(mass.wrapping_mul(dy_random), 100);
            WorldEntity::add_impulse_raw(
                this as *mut WorldEntity,
                Fixed::from_raw(dx),
                Fixed::from_raw(dy),
                0,
            );
        }

        // ── BLOCK A5: speed_y_pre snapshot (POST-sway) ──────────────────
        // Used by the splash-sound gate near the end. MUST happen after the
        // apply_impulse call (otherwise we miss any sway-induced velocity).
        let speed_y_pre = (*this).base.speed_y;

        // ── BLOCK A6: above-water fuse handling ─────────────────────────
        if (*this).base._field_b0 == 0 {
            // not underwater
            let fuse_timer = (*this).fuse_timer;
            if fuse_timer == 0 {
                // ── A6a: fuse expired ──────────────────────────────────
                let post_fuse_timer = (*this).post_fuse_terminate_timer;
                if post_fuse_timer != 0 {
                    // Decay path: gated by `_render_data_12_15[1]` (+0x320)
                    // — when that gate is set, decay only when missile has
                    // come to rest (IsMoving == false).
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
                            // PlaySoundLocal(this, sound_id=_render_data_12_15[2],
                            //                flags=4, volume=1.0, pitch=1.0)
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
                } else {
                    // post_fuse_timer == 0: terminate via super-animal finish
                    // OR vt[14].
                    if matches!((*this).missile_type, MissileType::Homing)
                        && (*this).super_animal_walk_sprite != 0
                        && (*this).contact_phase == 1
                    {
                        super::handle_message::bridge_finish_super_animal(this);
                    } else {
                        MissileEntity::set_terminate_flag_raw(this, 1);
                    }
                }
            } else {
                // ── A6b: fuse_timer > 0 — countdown ────────────────────
                let new_fuse = fuse_timer.wrapping_sub(0x14);
                (*this).fuse_timer = new_fuse;
                if new_fuse <= 0 {
                    (*this).fuse_timer = 0;
                    super::sound::stop_fuse_sound(this);
                }

                // ── A6c: alarm gate ────────────────────────────────────
                // `weapon_data[2] != 0 && spawn_params.pellet_index == 0`
                // (NOT `_render_data_01_05[4]` / `explosion_id` as one
                // version of the brief said).
                let fuse_now = (*this).fuse_timer;
                if fuse_now < 0x320
                    && (*this).weapon_data[2] != 0
                    && (*this).spawn_params.pellet_index == 0
                {
                    // Alarm-table size depends on game_version + a
                    // version-gated `fire_particle_trigger` (+0x2EC) check.
                    // NOT `fuse_timer_initial` (+0x318) as one
                    // earlier note suggested.
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

        // ── BLOCK A7: pos_x out-of-water-X-bounds → vt[14] ──────────────
        // `if |pos_x_int - level_width/2| >= map_boundary_width then vt[14]`.
        // `level_width / 2` is the round-toward-zero half (`SAR EAX, 1` after
        // `SUB EAX, sign(EAX)`).
        let level_width = (*world).level_width as i32;
        let half_lw = (level_width - (level_width >> 31)) >> 1;
        let map_boundary_width = (*world).map_boundary_width as i32;
        let pos_x_int = pos_x_init.to_int();
        if (pos_x_int - half_lw).abs() >= map_boundary_width {
            MissileEntity::set_terminate_flag_raw(this, 1);
        }

        // ── BLOCK A8: register event point if contact_phase != 0 ────────
        if (*this).contact_phase != 0 {
            GameWorld::register_event_point_raw(world, pos_x_init, pos_y_init);
        }

        // ── BLOCK A9: contact_phase == 1 underwater → finish_super_animal
        if (*this).base._field_b0 != 0 && (*this).contact_phase == 1 {
            super::handle_message::bridge_finish_super_animal(this);
        }

        // ── BLOCK A10: type 2/5 + !IsMoving + ricochet → vt[14] ─────────
        // Standard / Cluster missiles that have stopped moving but still
        // have ricochet bounces queued AND the side-mask bit 0x8 is set
        // → terminate.
        let mtype_now = (*this).missile_type;
        if (matches!(mtype_now, MissileType::Standard) || matches!(mtype_now, MissileType::Cluster))
            && !WorldEntity::is_moving_raw(this as *const WorldEntity)
            && (*this).ricochet_counter != 0
            && ((*this).ricochet_side_mask & 0x8) != 0
        {
            MissileEntity::set_terminate_flag_raw(this, 1);
        }

        // ── BLOCK A11: water-threshold gate ─────────────────────────────
        // `pos_y_int < water_kill_y + (cluster_pellet ? 0x80 : 0)` → AIR
        // BRANCH; else → detonate-and-free tail.
        let pos_y_int = pos_y_init.to_int();
        let cluster_bias: i32 = if (*this).is_cluster_pellet != 0 {
            0x80
        } else {
            0
        };
        let water_threshold = (*world).water_kill_y + cluster_bias;

        if pos_y_int < water_threshold {
            // ── AIR BRANCH ────────────────────────────────────────────────
            // ── BLOCK A12: missile_type inner-tick dispatch ─────────────
            // Validated against jump table at 0x0050BF88 (read directly to
            // override the brief's mis-attributions).
            match (*this).missile_type {
                MissileType::Unknown1 => {
                    if (*this).base._field_b0 != 0 && ((*this).contact_face_mask & 0x400000) != 0 {
                        (*this).missile_type = MissileType::Zero;
                    } else {
                        bridge_inner_unknown1_tick(this);
                    }
                }
                MissileType::Homing => bridge_inner_homing_tick(this),
                MissileType::Sheep => bridge_inner_sheep_tick(this),
                MissileType::Cluster => bridge_inner_cluster_tick(this),
                MissileType::Zero | MissileType::Standard => {
                    // skip switch (case 0 / case 2 jump to merge)
                }
            }

            // ── BLOCK A13: cluster spawn cleanup ────────────────────────
            // When this missile has an owner_id, the detonate_response_mode
            // is set, AND the per-team game-info entry at
            // `world+0x4620+(owner_id*0x51C)` is zero — clamp the fuse and
            // clear the cluster cleanup flags.
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

            // ── BLOCK A14+ : terminate gate ─────────────────────────────
            // `arg1[0x11]` = WorldEntity subclass_data + 0x44 = terminate_flag.
            if (*this).base.subclass_data.terminate_flag == 0 {
                // ── In-flight body — runs until record_landing_event ─
                in_flight_body(this, world, pos_x_init, pos_y_init, speed_y_pre);
                return;
            }

            // terminate_flag set: detonate first, then fall through to
            // set_active + Free at the bottom.
            bridge_detonate(this, pos_x_init, pos_y_init);
        }

        // ── BLOCK A21: detonate-and-free tail ──────────────────────────
        // Reached when (pos_y_int >= water_threshold) OR (air branch +
        // terminate_flag set). Calls set_active(0xC) and vt[1]Free(this, 1)
        // to actually destroy the missile.
        set_world_activity_timer(world, 0xC);
        MissileEntity::free_raw(this, 1);
    }
}

/// In-flight body (BLOCKS A14..A20). Emits sparks/bubbles, plays splash
/// sound, runs the sound-handle pollers, fires the underwater-entry latch,
/// and finally records the landing-event (kind 5/8/10) before returning. All
/// paths through this function leave the missile alive — the caller does
/// NOT hit the detonate-and-free tail.
unsafe fn in_flight_body(
    this: *mut MissileEntity,
    world: *mut GameWorld,
    pos_x_init: Fixed,
    pos_y_init: Fixed,
    speed_y_pre: Fixed,
) {
    unsafe {
        let pos_y_int = pos_y_init.to_int();

        // Emit pi-view: re-bound to +0x2F4 (single) or +0x340 (cluster).
        // NOT the HandleMessage prologue piVar8 at +0x2E8 / +0x334 — that's
        // a different view (animation_rate_kind).
        let pi_view = emit_pi_view(this);

        // ── BLOCK A14/A15: emit phase advance + spark/bubble loop ──────
        let underwater_or_blocked = (*this).base._field_b0 != 0 || (*this).base._field_a4 != 0;
        let bubble_path = underwater_or_blocked || pos_y_int >= (*world).water_level;

        if bubble_path {
            // Bubble path: phase advance = (pi_view[3] << 16) / 200, round-zero.
            let inc = div_round_to_zero((pi_view[3] as i32).wrapping_shl(16), 200);
            (*this).effect_emit_phase =
                (*this).effect_emit_phase.wrapping_add(Fixed::from_raw(inc));

            while (*this).effect_emit_phase >= Fixed::ONE {
                let rng = (*world).advance_effect_rng();
                let kind = ((rng >> 16) % 3) + 1;
                bridge_create_bubble(this, pos_x_init, pos_y_init, kind);
                (*this).effect_emit_phase = (*this).effect_emit_phase.wrapping_sub(Fixed::ONE);
            }
        } else {
            // Spark / SpawnEffect path: phase advance = (pi_view[1] << 16) / 25.
            let inc = div_round_to_zero((pi_view[1] as i32).wrapping_shl(16), 25);
            (*this).effect_emit_phase =
                (*this).effect_emit_phase.wrapping_add(Fixed::from_raw(inc));

            while (*this).effect_emit_phase >= Fixed::ONE {
                // Two effect-RNG advances (the secondary RNG at world+0x45F0).
                let rng_a = (*world).advance_effect_rng();
                let rng_b = (*world).advance_effect_rng();

                let rng_dx = ((rng_a & 0xFFFF) as i32).wrapping_sub(0x8000);
                let rng_dy = ((rng_b & 0xFFFF) as i32).wrapping_sub(0x8000);

                // state_flag = (pi_view[2] * 0x147A) / 200, round-zero.
                // Disasm SAR EDX,5 after IMUL by 0x51EB851F → divisor 200.
                // Ghidra mis-renders this as / 100; BN agrees with disasm.
                let state_flag = div_round_to_zero((pi_view[2] as i32).wrapping_mul(0x147A), 200);

                bridge_spawn_effect(
                    this, pos_x_init, pos_y_init, rng_dx, rng_dy, pi_view[0], state_flag,
                );

                (*this).effect_emit_phase = (*this).effect_emit_phase.wrapping_sub(Fixed::ONE);
            }
        }

        // ── BLOCK A16: splash sound ────────────────────────────────────
        // When underwater (`_field_a4 != 0`), gate splash sound on
        // `|speed_y_pre| > 0x10000` AND `splash_sound_latched == 0`. Set
        // latch when above-water or after firing the splash.
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

        // ── BLOCK A17: sound-handle pollers ────────────────────────────
        super::sound::check_fuse_sound(this);
        super::sound::check_dig_sound(this);

        // ── BLOCK A18: underwater-entry latch ──────────────────────────
        // First frame underwater (`_field_b0 != 0` + latch zero):
        // - clear detonate_response_mode
        // - arm bucket_mask = 0x400000 (water bucket only)
        // - stop the fuse sound
        // - set the latch to 1
        if (*this).base._field_b0 != 0 && (*this).underwater_entry_latched == 0 {
            (*this).detonate_response_mode = 0;
            (*this).base.bucket_mask = 0x400000;
            super::sound::stop_fuse_sound(this);
            (*this).underwater_entry_latched = 1;
        }

        // ── BLOCK A19: refresh world activity timer ────────────────────
        set_world_activity_timer(world, 0xC);

        // ── BLOCK A20: record landing-event (kind 5/8/10) and reset gate
        // pos_x in-bounds AND pos_y above level top → kind 5 (above-water)
        //                                           OR fall-through to 10 (under)
        // pos_x out-of-bounds OR pos_y above level top → kind 8 (above-water)
        //                                            OR fall-through to 10 (under)
        // underwater (any bounds) → kind 10
        //
        // The Fixed comparison uses the raw 16.16 values (NOT pos_y_int).
        let level_min_x = (*world).level_bound_min_x.to_raw();
        let level_max_x = (*world).level_bound_max_x.to_raw();
        let level_min_y = (*world).level_bound_min_y.to_raw();
        let pos_x_raw = pos_x_init.to_raw();
        let pos_y_raw = pos_y_init.to_raw();

        let kind: u32 =
            if pos_x_raw < level_min_x || pos_x_raw > level_max_x || pos_y_raw < level_min_y {
                if (*this).base._field_b0 == 0 { 8 } else { 0xA }
            } else if (*this).base._field_b0 == 0 {
                5
            } else {
                0xA
            };

        GameWorld::record_landing_event_raw(world, kind, pos_x_raw, pos_y_raw);
        (*this)._field_388 = 0;
    }
}

/// Inline port of `GameTask::set_active` (0x00547ED0). `__usercall(EDX = mode,
/// ESI = this)`, plain RET. The function reads `world` from `this+0x2C` and
/// refreshes the two world-level activity-watchdog timers
/// (`+0x5DC` and `+0x7E48`) to `mode`, but only when each timer has not
/// already decayed past `-mode`.
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
