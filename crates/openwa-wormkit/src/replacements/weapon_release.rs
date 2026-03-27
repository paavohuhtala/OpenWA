//! WeaponRelease hook (0x51C3D0).
//!
//! Orchestrates weapon firing: ammo sync, spawn offset calculation, network timing,
//! weapon stat counters, sound/visual effects, then delegates to FireWeapon (in weapon.rs).
//!
//! Convention: usercall(EAX=CTaskWorm*) + 4 stack params, RET 0x10.

use openwa_core::address::va;
use openwa_core::log::log_line;
use openwa_core::rebase::rb;
use openwa_core::task::worm::CTaskWorm;
use openwa_core::task::{CGameTask, SharedDataTable, Task};

use crate::hook::{self, usercall_trampoline};
use crate::replacements::{sound, weapon};

// ── Trampoline ──────────────────────────────────────────────

usercall_trampoline!(fn trampoline_weapon_release; impl_fn = weapon_release_impl;
    reg = eax; stack_params = 4; ret_bytes = "0x10");

// ── WeaponReleaseContext ────────────────────────────────────

/// The 0x2C-byte stack-local struct populated by WeaponRelease and passed to
/// FireWeapon as the `local_struct` (ECX) parameter.
#[repr(C)]
#[derive(Clone, Copy)]
struct WeaponReleaseContext {
    team_id: u32,
    worm_id: u32,
    param_1: u32,
    param_2: u32,
    spawn_offset_x: i32,
    spawn_offset_y: i32,
    ammo_per_turn: u32,
    ammo_per_slot: u32,
    _zero: u32,
    delay: u32,
    network_delay: i32,
}

const _: () = assert!(core::mem::size_of::<WeaponReleaseContext>() == 0x2C);

// ── Weapon category classifiers (pure Rust) ─────────────────

/// IsSuperWeapon (0x565960): returns true for "super weapon" IDs.
/// param_1 is the byte at DDGame+0x7E3F; for weapon 0x3B (SelectWorm) it returns param_1.
fn is_super_weapon(weapon_id: u32, ddgame_7e3f: u8) -> bool {
    match weapon_id {
        10 | 0x13 | 0x1D | 0x1E | 0x1F | 0x24 | 0x29 | 0x2A | 0x2D | 0x2E | 0x31 | 0x32
        | 0x33 | 0x36 | 0x37 | 0x38 | 0x3C | 0x3D => true,
        0x3B => ddgame_7e3f != 0,
        _ => false,
    }
}

/// FUN_005658C0: weapon category A — checks if weapon is in a specific set.
fn is_weapon_category_a(weapon_id: u32) -> bool {
    matches!(
        weapon_id,
        4 | 5 | 0x17 | 0x18 | 0x19 | 0x1A | 0x1F | 0x2D | 0x2E | 0x30 | 0x32 | 0x34 | 0x35
            | 0x36
    )
}

/// FUN_00565920: weapon category B — checks if weapon is in a specific set.
fn is_weapon_category_b(weapon_id: u32) -> bool {
    matches!(weapon_id, 0x1C | 0x2C | 0x2F | 0x32)
}

// ── Fixed-point multiply ────────────────────────────────────

/// Fixed-point 16.16 multiply: (a * b) >> 16, using SHRD like the original.
/// Matches the `IMUL + SHRD EAX,EDX,0x10` pattern in the disassembly.
#[inline(always)]
fn fixed_mul_shrd(a: i32, b: i32) -> i32 {
    let product = (a as i64) * (b as i64);
    // SHRD EAX,EDX,0x10: shift the full 64-bit result right by 16
    (product >> 16) as i32
}

// ── Main implementation ─────────────────────────────────────

unsafe extern "cdecl" fn weapon_release_impl(
    worm: *mut CTaskWorm,
    param_1: u32,
    param_2: u32,
    param_3: i32,
    param_4: i32,
) {
    let w = &*worm;

    // Initialize context struct to zero
    let mut ctx = WeaponReleaseContext {
        team_id: 0,
        worm_id: 0,
        param_1: 0,
        param_2: 0,
        spawn_offset_x: 0,
        spawn_offset_y: 0,
        ammo_per_turn: 0,
        ammo_per_slot: 0,
        _zero: 0,
        delay: 0,
        network_delay: 0,
    };

    // ── 1. Sync check ───────────────────────────────────────
    if w.fire_sync_frame_1 == w.fire_sync_frame_2 {
        let ddgame = w.ddgame() as *mut u8;
        *(ddgame.add(0x72E4) as *mut u32) = 0x0E;
        let mut offset = 0u32;
        while (offset as i32) < 0x118 {
            *(ddgame.add(0x73B0).add(offset as usize) as *mut u32) = 0;
            offset += 0x14;
        }
        *(ddgame.add(0x739C) as *mut u32) = 0;
    }

    // ── 2. Populate context fields ──────────────────────────
    let speed_x = w.base.pos_x.0;
    let speed_y = w.base.pos_y.0;
    ctx.team_id = w.team_index;
    ctx.worm_id = w.worm_index;
    ctx.ammo_per_turn = w.weapon_param_1 as u32;
    ctx.param_2 = param_2;
    ctx.param_1 = param_1;
    ctx.ammo_per_slot = w.weapon_param_2 as u32;

    let entry = w.active_weapon_entry;
    let fire_type = (*entry).fire_type;
    let fire_subtype_34 = (*entry).fire_subtype_34;
    let fire_subtype_38 = (*entry).fire_subtype_38;

    // ── 3. Spawn offset calculation ─────────────────────────
    let landscape_scale = w.landscape_scale;

    match fire_type {
        1 => match fire_subtype_38 {
            1 => {
                ctx.spawn_offset_x = param_3 * 0x18;
                ctx.spawn_offset_y = param_4 * 0x18;
            }
            2 => {
                // Falls through to type 3 (passthrough)
                ctx.spawn_offset_x = param_3;
                ctx.spawn_offset_y = param_4;
            }
            3 => {
                ctx.spawn_offset_x = fixed_mul_shrd(param_3, landscape_scale) * 0x18;
                ctx.spawn_offset_y = fixed_mul_shrd(param_4, landscape_scale) * 0x18;
            }
            4 => {
                ctx.spawn_offset_x = param_3 * 0x14;
                ctx.spawn_offset_y = param_4 * 0x14;
            }
            _ => {}
        },
        2 => {
            ctx.spawn_offset_x = fixed_mul_shrd(param_3, landscape_scale) * 0x18;
            ctx.spawn_offset_y = fixed_mul_shrd(param_4, landscape_scale) * 0x18;
            // Special Y adjustment for angle 0x79
            if w.state() == 0x79 {
                ctx.spawn_offset_y += w.base.speed_y.0;
            }
        }
        3 => {
            ctx.spawn_offset_x = param_3;
            ctx.spawn_offset_y = param_4;
        }
        4 => {
            if (fire_subtype_34 as u32).wrapping_sub(1) < 0x18 {
                ctx.spawn_offset_x = param_3;
                ctx.spawn_offset_y = param_4;
            }
        }
        _ => {}
    }

    // ── 4. Delay ────────────────────────────────────────────
    if w.difficulty_level == 0 {
        ctx.delay = 0x1E;
    } else if w.difficulty_level == 1 {
        ctx.delay = 0x3C;
    }

    // ── 5. Network timing ───────────────────────────────────
    let ddgame_ptr = w.ddgame();
    let ddgame_raw = ddgame_ptr as *const u8;
    let ptr_at_24 = *(ddgame_raw.add(0x24) as *const *const u8);
    let is_network = *ptr_at_24.add(0xD9D0);
    let fe_version = *ptr_at_24.add(0xD9B1);

    let mut adjust = 0i32;
    let max_clients = if is_network == 0 {
        5
    } else if fe_version < 0x1B {
        10
    } else {
        adjust = -1;
        10
    };

    let client_idx = w.network_client_index;
    if ((client_idx - adjust) as u32) < ((max_clients - adjust) as u32) {
        ctx.network_delay = (client_idx + 1) * 1000;
    }

    // ── 6. Weapon 0x22/0x24 special ─────────────────────────
    let weapon_id = w.selected_weapon;
    if (weapon_id == 0x22 || weapon_id == 0x24) && w.weapon_param_3 == 0 {
        (*worm).shot_data_1 = w.shot_data_2;
    }

    // ── 7. SharedData HandleMessage (msg 0x49) ──────────────
    let mut msg_buf = [0u8; 0x408];
    write_u32(&mut msg_buf, 0x00, w.team_index);
    write_u32(&mut msg_buf, 0x04, w.worm_index);
    write_u32(&mut msg_buf, 0x08, w.shot_data_1);
    write_u32(&mut msg_buf, 0x0C, w.shot_data_2);
    write_u32(&mut msg_buf, 0x10, w.fire_sync_frame_1 as u32);
    write_u32(&mut msg_buf, 0x14, w.fire_sync_frame_2 as u32);
    write_u32(
        &mut msg_buf,
        0x18,
        if w._unknown_2cc != 0 { 1 } else { 0 },
    );
    write_u32(&mut msg_buf, 0x1C, weapon_id);

    let table = SharedDataTable::from_task((*worm).as_task_ptr());
    let team_entity = table.lookup(0, 0x14);
    if !team_entity.is_null() {
        let vtable = *(team_entity as *const *const usize);
        let handle_msg: unsafe extern "thiscall" fn(
            *mut u8,
            *mut u8,
            u32,
            u32,
            *const u8,
        ) = core::mem::transmute(*vtable.add(2));
        handle_msg(
            team_entity as *mut u8,
            (*worm).as_task_ptr_mut() as *mut u8,
            0x49,
            0x408,
            msg_buf.as_ptr(),
        );
    }

    // ── 8. Weapon stat counters ─────────────────────────────
    let ddgame = (*worm).ddgame() as *mut u8;
    let team_id = (*worm).team_index;
    let worm_id = (*worm).worm_index;
    let current_weapon_byte = *ddgame.add(0x7E3F);

    if is_super_weapon(weapon_id, current_weapon_byte) {
        let counter = ddgame
            .add(0x40D8)
            .add((team_id * 0x51C) as usize)
            .add((worm_id * 0x9C) as usize) as *mut i32;
        *counter += 1;
    }

    // Range 0x3E..=0x46
    if weapon_id.wrapping_sub(0x3E) <= 8 {
        let counter = ddgame
            .add(0x40D4)
            .add((team_id * 0x51C) as usize)
            .add((worm_id * 0x9C) as usize) as *mut i32;
        *counter += 1;
    }

    if is_weapon_category_a(weapon_id) {
        let counter = ddgame
            .add(0x40D0)
            .add((team_id * 0x51C) as usize)
            .add((worm_id * 0x9C) as usize) as *mut i32;
        *counter += 1;
    }

    if is_weapon_category_b(weapon_id) {
        let counter = ddgame
            .add(0x40CC)
            .add((team_id * 0x51C) as usize)
            .add((worm_id * 0x9C) as usize) as *mut i32;
        *counter += 1;
    }

    // ── 9. Sound dispatch + 10. Visual effect ───────────────
    let task = worm as *mut CGameTask;
    let mut do_effect = false;
    let mut effect_state: u32 = 0x73;

    let w = &*worm; // re-borrow after mutation above
    let entry = w.active_weapon_entry;
    let fire_type = (*entry).fire_type;

    let play_worm_sound_addr = rb(va::PLAY_WORM_SOUND);
    let stop_worm_sound_addr = rb(va::STOP_WORM_SOUND);

    match fire_type {
        1 => {
            match (*entry).fire_subtype_34 {
                1 => {
                    if w.sound_handle == 0 {
                        call_play_worm_sound(worm, 0x1004E, 0x10000, play_worm_sound_addr);
                    }
                    do_effect = true;
                    effect_state = 0x73;
                }
                2 => {
                    sound::play_sound_local(task, 0x49, 3, 0x10000, 0x10000);
                    call_stop_worm_sound(worm, stop_worm_sound_addr);
                }
                3 | 7 | 0xB | 0xC => {
                    sound::play_sound_local(task, 0x4B, 3, 0x10000, 0x10000);
                    call_stop_worm_sound(worm, stop_worm_sound_addr);
                }
                4 => {
                    if w.sound_handle == 0 {
                        call_play_worm_sound(worm, 0x1004F, 0x10000, play_worm_sound_addr);
                    }
                    do_effect = true;
                    effect_state = 0x73;
                }
                5 => {
                    sound::play_sound_local(task, 0x50, 3, 0x10000, 0x10000);
                    do_effect = true;
                    effect_state = 0x75;
                }
                6 => {
                    sound::play_sound_local(task, 0x52, 3, 0x10000, 0x10000);
                    do_effect = true;
                    effect_state = 0x73;
                }
                10 => {
                    sound::play_sound_local(task, 0x1D, 3, 0x10000, 0x10000);
                    call_stop_worm_sound(worm, stop_worm_sound_addr);
                }
                _ => {}
            }
        }
        2 => {
            if w._unknown_2cc == 0 || w._unknown_2c8 == 1 {
                // Team sound from DDGame+0x7768 + team_id * 0xF0
                let ddgame = w.ddgame() as *const u8;
                let team_sound_offset = (team_id as usize) * 0xF0 + 0x7768;
                let team_sound = *(ddgame.add(team_sound_offset) as *const u32);
                sound::play_sound_local(task, team_sound, 3, 0x10000, 0x10000);
            }
        }
        // Type 3: no sound
        4 => {
            match (*entry).fire_subtype_34 {
                2 => {
                    sound::play_sound_local(task, 0x60, 3, 0x10000, 0x10000);
                }
                3 => {
                    let sound_id = (*entry).fire_subtype_38 as u32;
                    sound::play_sound_local(task, sound_id, 3, 0x10000, 0x10000);
                }
                4 => {
                    let sound_id = *((&raw const (*entry).fire_params as *const u8).add(4)
                        as *const u32);
                    sound::play_sound_local(task, sound_id, 3, 0x10000, 0x10000);
                }
                10 => {
                    if w._unknown_208 == 0 {
                        sound::play_sound_local(task, 0x43, 3, 0x10000, 0x10000);
                    }
                }
                0xB => {
                    if w.sound_handle == 0 {
                        call_play_worm_sound(worm, 0x10035, 0x10000, play_worm_sound_addr);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }

    // ── 10. Visual effect (if triggered by sound dispatch) ──
    if do_effect {
        let ddgame = &mut *(*worm).ddgame();
        let gfx_handler = *((&raw const *ddgame as *const u8).add(0x528) as *const *const u8);
        let palette = *(gfx_handler.add(0x22C) as *const u32);

        let rng1 = ddgame.advance_effect_rng();
        let rng2 = ddgame.advance_effect_rng();

        let rng2_offset = (rng2 & 0xFFFF) as i32 - 0x18000;
        let facing = (*worm).facing_direction_2;
        let rng_scaled = rng2_offset * facing;

        let rng1_offset = (rng1 & 0xFFFF) as i32 - 0x18000;

        let facing_flag: u32 = if facing < 1 { 0x40000 } else { 0 };
        let state_flag = facing_flag + effect_state;

        call_spawn_effect_full(
            worm, speed_x, speed_y, rng_scaled, rng1_offset, palette, state_flag, 0xA0000,
            0x1999, rb(va::SPAWN_EFFECT),
        );
    }

    // ── 11. Call FireWeapon ──────────────────────────────────
    let entry = (*worm).active_weapon_entry;
    let ctx_ptr = &ctx as *const WeaponReleaseContext as *const u8;
    weapon::fire_weapon_impl(entry, ctx_ptr, worm);

    let _ = log_line(&format!(
        "[WeaponRelease] worm=0x{:08X} weapon={} type={} sub34={} sub38={}",
        worm as u32, weapon_id, fire_type, fire_subtype_34, fire_subtype_38,
    ));
}

// ── Helper ──────────────────────────────────────────────────

#[inline(always)]
fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}

// ── Bridge functions ────────────────────────────────────────
// All bridges pass the runtime target address as the last cdecl parameter,
// matching the pattern used in weapon.rs (avoids sym + jmp indirection issues).

/// PlayWormSound (0x5150D0): usercall(EDI=worm) + stack(sound_handle_id, volume), RET 0x8.
#[unsafe(naked)]
unsafe extern "C" fn call_play_worm_sound(
    _worm: *mut CTaskWorm,
    _sound_id: u32,
    _volume: u32,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push edi",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov edi, [esp+8]",    // worm
        "mov eax, [esp+20]",   // addr
        "push [esp+16]",       // volume
        "push [esp+16]",       // sound_id (shifted +4)
        "call eax",
        "pop edi",
        "ret",
    );
}

/// StopWormSound (0x515180): usercall(ESI=worm), plain RET.
#[unsafe(naked)]
unsafe extern "C" fn call_stop_worm_sound(_worm: *mut CTaskWorm, _addr: u32) {
    core::arch::naked_asm!(
        "push esi",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov esi, [esp+8]",    // worm
        "mov eax, [esp+12]",   // addr
        "call eax",
        "pop esi",
        "ret",
    );
}

/// SpawnEffect (0x547C30): usercall(EAX=0x80000, ECX=speed_x, ESI=worm) + 7 stack, RET 0x1C.
#[unsafe(naked)]
unsafe extern "C" fn call_spawn_effect_full(
    _worm: *mut CTaskWorm,
    _speed_x: i32,
    _speed_y: i32,
    _rng_scaled: i32,
    _rng_offset: i32,
    _palette: u32,
    _state_flag: u32,
    _size: u32,
    _scale: u32,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "push ebx",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg
        // Args: +12=worm, +16=speed_x, +20=speed_y, +24=rng_scaled, +28=rng_offset,
        //       +32=palette, +36=state_flag, +40=size, +44=scale, +48=addr
        "mov esi, [esp+12]",   // worm → ESI
        "mov ecx, [esp+16]",   // speed_x → ECX
        "mov ebx, [esp+48]",   // addr
        "push [esp+44]",       // scale
        "push [esp+44]",       // size
        "push [esp+44]",       // state_flag
        "push [esp+44]",       // palette
        "push [esp+44]",       // rng_offset
        "push [esp+44]",       // rng_scaled
        "push [esp+44]",       // speed_y (first stack param)
        "mov eax, 0x80000",
        "call ebx",
        "pop ebx",
        "pop esi",
        "ret",
    );
}

// ── Hook installation ───────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "WeaponRelease",
            va::WEAPON_RELEASE,
            trampoline_weapon_release as *const (),
        )?;
    }
    Ok(())
}
