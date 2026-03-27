//! WeaponRelease hook (0x51C3D0).
//!
//! Orchestrates weapon firing: ammo sync, spawn offset calculation, network timing,
//! weapon stat counters, sound/visual effects, then delegates to FireWeapon (in weapon.rs).
//!
//! Convention: usercall(EAX=CTaskWorm*) + 4 stack params, RET 0x10.

use openwa_core::address::va;
use openwa_core::audio::{KnownSoundId, SoundId};
use openwa_core::fixed::Fixed;
use openwa_core::game::Weapon;
use openwa_core::log::log_line;
use openwa_core::rebase::rb;
use openwa_core::task::worm::{CTaskWorm, WormState};
use openwa_core::task::{CGameTask, Task};

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
pub(crate) struct WeaponReleaseContext {
    team_id: u32,
    worm_id: u32,
    spawn_x: u32,
    spawn_y: u32,
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
/// For SelectWorm, returns the DDGame+0x7E3F version flag (mode-dependent).
fn is_super_weapon(weapon: Weapon, ddgame_7e3f: u8) -> bool {
    use Weapon::*;
    matches!(
        weapon,
        Earthquake
            | SuicideBomber
            | MailStrike
            | MineStrike
            | MoleSquadron
            | GirderPack
            | ScalesOfJustice
            | SuperBanana
            | SalvationArmy
            | MbBomb
            | MingVase
            | SheepStrike
            | CarpetBomb
            | Donkey
            | NuclearTest
            | Armageddon
            | Freeze
            | MagicBullet
    ) || (weapon == Weapon::SelectWorm && ddgame_7e3f != 0)
}

/// FUN_005658C0: weapon category A — homing/animal/special projectile weapons.
fn is_weapon_category_a(weapon: Weapon) -> bool {
    use Weapon::*;
    matches!(
        weapon,
        HomingPigeon
            | SheepLauncher
            | Sheep
            | SuperSheep
            | AquaSheep
            | MoleBomb
            | MoleSquadron
            | SalvationArmy
            | MbBomb
            | Skunk
            | SheepStrike
            | MadCow
            | OldWoman
            | Donkey
    )
}

/// FUN_00565920: weapon category B — fire/napalm weapons.
fn is_weapon_category_b(weapon: Weapon) -> bool {
    use Weapon::*;
    matches!(weapon, NapalmStrike | FlameThrower | PetrolBomb | SheepStrike)
}

// ── Main implementation ─────────────────────────────────────

unsafe extern "cdecl" fn weapon_release_impl(
    worm: *mut CTaskWorm,
    spawn_x: u32,
    spawn_y: u32,
    aim_dir_x: Fixed,
    aim_dir_y: Fixed,
) {
    let w = &*worm;

    // Initialize context struct to zero
    let mut ctx = WeaponReleaseContext {
        team_id: 0,
        worm_id: 0,
        spawn_x: 0,
        spawn_y: 0,
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
        let g = &mut *w.ddgame();
        g.render_slot_count = 0x0E;
        for entry in &mut g.render_entries {
            entry.active = 0;
        }
        g.render_state_flag = 0;
    }

    // ── 2. Populate context fields ──────────────────────────
    let speed_x = w.base.pos_x.0;
    let speed_y = w.base.pos_y.0;
    ctx.team_id = w.team_index;
    ctx.worm_id = w.worm_index;
    ctx.ammo_per_turn = w.weapon_param_1 as u32;
    ctx.spawn_x = spawn_x;
    ctx.spawn_y = spawn_y;
    ctx.ammo_per_slot = w.weapon_param_2 as u32;

    let entry = w.active_weapon_entry;
    let fire_type = (*entry).fire_type;
    let special_subtype = (*entry).special_subtype;
    let fire_method = (*entry).fire_method;

    use openwa_core::game::weapon::{FireType, FireMethod};

    // ── 3. Spawn offset calculation ─────────────────────────
    let scale = w.landscape_scale;

    let (mut offset_x, mut offset_y) = (Fixed::ZERO, Fixed::ZERO);
    match FireType::try_from(fire_type) {
        Ok(FireType::Projectile) => match FireMethod::try_from(fire_method) {
            Ok(FireMethod::PlacedExplosive) => {
                offset_x = aim_dir_x * 0x18;
                offset_y = aim_dir_y * 0x18;
            }
            Ok(FireMethod::ProjectileFire) => {
                // Falls through to Strike (passthrough)
                offset_x = aim_dir_x;
                offset_y = aim_dir_y;
            }
            Ok(FireMethod::CreateWeaponProjectile) => {
                offset_x = aim_dir_x * scale * 0x18;
                offset_y = aim_dir_y * scale * 0x18;
            }
            Ok(FireMethod::CreateArrow) => {
                offset_x = aim_dir_x * 0x14;
                offset_y = aim_dir_y * 0x14;
            }
            _ => {}
        },
        Ok(FireType::Rope) => {
            offset_x = aim_dir_x * scale * 0x18;
            offset_y = aim_dir_y * scale * 0x18;
            // Special Y adjustment for state 0x79
            if w.state() == WormState::Unknown_0x79 as u32 {
                offset_y += w.base.speed_y;
            }
        }
        Ok(FireType::Strike) => {
            offset_x = aim_dir_x;
            offset_y = aim_dir_y;
        }
        Ok(FireType::Special) => {
            if (special_subtype as u32).wrapping_sub(1) < 0x18 {
                offset_x = aim_dir_x;
                offset_y = aim_dir_y;
            }
        }
        _ => {}
    }
    ctx.spawn_offset_x = offset_x.0;
    ctx.spawn_offset_y = offset_y.0;

    // ── 4. Delay ────────────────────────────────────────────
    if w.difficulty_level == 0 {
        ctx.delay = 0x1E;
    } else if w.difficulty_level == 1 {
        ctx.delay = 0x3C;
    }

    // ── 5. Network timing ───────────────────────────────────
    let game_info = (*w.ddgame()).game_info as *const u8;
    let is_network = *game_info.add(0xD9D0);
    let fe_version = *game_info.add(0xD9B1);

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

    // ── 6. Girder/GirderPack special ────────────────────────
    let weapon = w.selected_weapon;
    if matches!(weapon, Weapon::Girder | Weapon::GirderPack) && w.weapon_param_3 == 0 {
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
    write_u32(&mut msg_buf, 0x1C, weapon as u32);

    let team = weapon::lookup_team_task(worm);
    if !team.is_null() {
        (*team).handle_message(
            (*worm).as_task_ptr_mut(),
            0x49,
            0x408,
            msg_buf.as_ptr(),
        );
    }

    // ── 8. Weapon stat counters ─────────────────────────────
    let g = &mut *(*worm).ddgame();
    let team_id = (*worm).team_index;
    let worm_id = (*worm).worm_index;

    if is_super_weapon(weapon, g.version_flag_3) {
        *g.weapon_stat_counter(team_id, worm_id, 0x40D8) += 1;
    }

    // Powerup/utility weapons (JetPack..=CrateShower)
    if (Weapon::JetPack..=Weapon::CrateShower).contains(&weapon) {
        *g.weapon_stat_counter(team_id, worm_id, 0x40D4) += 1;
    }

    if is_weapon_category_a(weapon) {
        *g.weapon_stat_counter(team_id, worm_id, 0x40D0) += 1;
    }

    if is_weapon_category_b(weapon) {
        *g.weapon_stat_counter(team_id, worm_id, 0x40CC) += 1;
    }

    // ── 9. Sound dispatch + 10. Visual effect ───────────────
    let task = worm as *mut CGameTask;
    let mut do_effect = false;
    let mut effect_state: u32 = 0x73;

    let w = &*worm; // re-borrow after mutation above
    let entry = w.active_weapon_entry;

    let play_worm_sound_addr = rb(va::PLAY_WORM_SOUND);
    let stop_worm_sound_addr = rb(va::STOP_WORM_SOUND);

    match FireType::try_from((*entry).fire_type) {
        Ok(FireType::Projectile) => {
            match (*entry).special_subtype {
                1 => {
                    if w.sound_handle == 0 {
                        call_play_worm_sound(worm, 0x1004E, 0x10000, play_worm_sound_addr);
                    }
                    do_effect = true;
                    effect_state = 0x73;
                }
                2 => {
                    sound::play_sound_local(task, KnownSoundId::ThrowRelease, 3, Fixed::ONE, Fixed::ONE);
                    call_stop_worm_sound(worm, stop_worm_sound_addr);
                }
                3 | 7 | 0xB | 0xC => {
                    sound::play_sound_local(task, KnownSoundId::RocketRelease, 3, Fixed::ONE, Fixed::ONE);
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
                    sound::play_sound_local(task, KnownSoundId::ShotgunFire, 3, Fixed::ONE, Fixed::ONE);
                    do_effect = true;
                    effect_state = 0x75;
                }
                6 => {
                    sound::play_sound_local(task, KnownSoundId::HandgunFire, 3, Fixed::ONE, Fixed::ONE);
                    do_effect = true;
                    effect_state = 0x73;
                }
                10 => {
                    sound::play_sound_local(task, KnownSoundId::LongbowRelease, 3, Fixed::ONE, Fixed::ONE);
                    call_stop_worm_sound(worm, stop_worm_sound_addr);
                }
                _ => {}
            }
        }
        Ok(FireType::Rope) => {
            if w._unknown_2cc == 0 || w._unknown_2c8 == 1 {
                let team_sound_raw = (*w.ddgame()).team_sound_id(team_id);
                sound::play_sound_local(task, SoundId(team_sound_raw), 3, Fixed::ONE, Fixed::ONE);
            }
        }
        // Type 3 (Strike): no sound
        Ok(FireType::Special) => {
            use openwa_core::game::weapon::SpecialFireSubtype as S;
            match S::try_from((*entry).special_subtype) {
                Ok(S::PneumaticDrill) => {
                    sound::play_sound_local(
                        task, KnownSoundId::BaseballBatRelease, 3, Fixed::ONE, Fixed::ONE,
                    );
                }
                Ok(S::Girder) => {
                    sound::play_sound_local(task, SoundId((*entry).fire_method as u32), 3, Fixed::ONE, Fixed::ONE);
                }
                Ok(S::BaseballBat) => {
                    // Sound ID from fire_params.spread (polymorphic use of field)
                    sound::play_sound_local(task, SoundId((*entry).fire_params.spread as u32), 3, Fixed::ONE, Fixed::ONE);
                }
                Ok(S::AirStrike) => {
                    if w._unknown_208 == 0 {
                        sound::play_sound_local(task, KnownSoundId::Teleport, 3, Fixed::ONE, Fixed::ONE);
                    }
                }
                Ok(S::ScalesOfJustice) => {
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
        let gfx_handler = ddgame.game_state_stream as *const u8;
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
    weapon::fire_weapon(entry, &ctx, worm);

    let _ = log_line(&format!(
        "[WeaponRelease] worm=0x{:08X} weapon={:?} type={} sub34={} sub38={}",
        worm as u32, weapon, fire_type, special_subtype, fire_method,
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
