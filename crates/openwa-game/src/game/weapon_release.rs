//! WeaponRelease orchestration and spawn effect.
//!
//! Pure Rust reimplementation of WA.exe WeaponRelease (0x51C3D0) and
//! SpawnEffect (0x547C30). Called from hook trampolines in openwa-dll.

use crate::audio::{KnownSoundId, SoundId};
use crate::game::message::WeaponReleasedMessage;
use crate::game::{KnownWeaponId, is_super_weapon};
use crate::task::world_root::WorldRootEntity;
use crate::task::worm::{WormEntity, WormState};
use crate::task::{BaseEntity, Entity, SharedDataTable, WorldEntity};
use openwa_core::fixed::Fixed;
use openwa_core::log::log_line;

use crate::audio::sound_ops as sound;
use crate::game::weapon_fire::{self, WeaponReleaseContext};

// ── Weapon category classifiers (pure Rust) ─────────────────

/// FUN_005658C0: weapon category A — homing/animal/special projectile weapons.
pub fn is_weapon_category_a(weapon: KnownWeaponId) -> bool {
    use KnownWeaponId::*;
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
pub fn is_weapon_category_b(weapon: KnownWeaponId) -> bool {
    use KnownWeaponId::*;
    matches!(
        weapon,
        NapalmStrike | FlameThrower | PetrolBomb | SheepStrike
    )
}

// ── Main implementation ─────────────────────────────────────

pub unsafe fn weapon_release(
    worm: *mut WormEntity,
    spawn_x: u32,
    spawn_y: u32,
    aim_dir_x: Fixed,
    aim_dir_y: Fixed,
) {
    unsafe {
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
            let g = &mut *w.world();
            g.render_slot_count = 0x0E;
            for entry in &mut g.render_entries {
                entry.active = 0;
            }
            g.render_state_flag = 0;
        }

        // ── 2. Populate context fields ──────────────────────────
        let speed_x = w.base.pos_x;
        let speed_y = w.base.pos_y;
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

        use crate::game::weapon::{FireMethod, FireType};

        // ── 3. Spawn offset calculation ─────────────────────────
        let scale = w.landscape_scale;

        let (mut offset_x, mut offset_y) = (Fixed::ZERO, Fixed::ZERO);
        match FireType::try_from(fire_type) {
            Ok(FireType::Projectile) => match FireMethod::try_from(fire_method) {
                Ok(FireMethod::PlacedExplosive) => {
                    offset_x = aim_dir_x * 24;
                    offset_y = aim_dir_y * 24;
                }
                Ok(FireMethod::ProjectileFire) => {
                    // Falls through to Strike (passthrough)
                    offset_x = aim_dir_x;
                    offset_y = aim_dir_y;
                }
                Ok(FireMethod::CreateWeaponProjectile) => {
                    offset_x = aim_dir_x * scale * 24;
                    offset_y = aim_dir_y * scale * 24;
                }
                Ok(FireMethod::CreateArrow) => {
                    offset_x = aim_dir_x * 20;
                    offset_y = aim_dir_y * 20;
                }
                _ => {}
            },
            Ok(FireType::Placed) => {
                offset_x = aim_dir_x * scale * 24;
                offset_y = aim_dir_y * scale * 24;
                // Special Y adjustment for state 0x79
                if w.is_in_state(WormState::Unknown_0x79) {
                    offset_y += w.base.speed_y;
                }
            }
            Ok(FireType::Strike) => {
                offset_x = aim_dir_x;
                offset_y = aim_dir_y;
            }
            Ok(FireType::Special) if (special_subtype as u32).wrapping_sub(1) < 0x18 => {
                offset_x = aim_dir_x;
                offset_y = aim_dir_y;
            }
            _ => {}
        }
        ctx.spawn_offset_x = offset_x.0;
        ctx.spawn_offset_y = offset_y.0;

        // ── 4. Bounce-settle delay ──────────────────────────────
        // Worm's selected bounce flag (msg 0x31 SelectBounce) drives the
        // post-spawn settling delay: 30 frames (no bounce) or 60 frames
        // (bounce). Other values leave ctx.delay at 0.
        if w.selected_bounce_flag == 0 {
            ctx.delay = 0x1E;
        } else if w.selected_bounce_flag == 1 {
            ctx.delay = 0x3C;
        }

        // ── 5. Fuse timer (ms) ──────────────────────────────────
        // Worm's selected fuse value (msg 0x2F SelectFuse), bounded by
        // scheme: range is [0..4] offline, [0..9] online (with `adjust=-1`
        // for fe_version >= 0x1B unlocking a `-1` "no-fuse" sentinel).
        // Forwarded to FireWeapon as `(value + 1) * 1000` ms.
        let game_info = (*w.world()).game_info as *const u8;
        let is_network = *game_info.add(0xD9D0);
        let fe_version = *game_info.add(0xD9B1);

        let mut adjust = 0i32;
        let max_fuse = if is_network == 0 {
            5
        } else if fe_version < 0x1B {
            10
        } else {
            adjust = -1;
            10
        };

        let fuse = w.selected_fuse_value;
        if ((fuse - adjust) as u32) < ((max_fuse - adjust) as u32) {
            ctx.network_delay = (fuse + 1) * 1000;
        }

        // ── 6. Girder/GirderPack special ────────────────────────
        let weapon = w.selected_weapon;
        if matches!(weapon, KnownWeaponId::Girder | KnownWeaponId::GirderPack)
            && w.weapon_param_3 == 0
        {
            (*worm).shot_data_1 = w.shot_data_2;
        }

        // ── 7. SharedData HandleMessage (msg 0x49) ──────────────
        let team = weapon_fire::lookup_world_root(worm);
        if !team.is_null() {
            WorldRootEntity::handle_typed_message_raw(
                team,
                worm,
                WeaponReleasedMessage {
                    team_index: w.team_index,
                    worm_index: w.worm_index,
                    shot_data_1: w.shot_data_1,
                    shot_data_2: w.shot_data_2,
                    fire_sync_frame_1: w.fire_sync_frame_1,
                    fire_sync_frame_2: w.fire_sync_frame_2,
                    unknown_flag: if w._unknown_2cc != 0 { 1 } else { 0 },
                    weapon: weapon.into(),
                },
            );
        }

        // ── 8. Weapon stat counters ─────────────────────────────
        let g = &mut *{
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let team_id = (*worm).team_index;
        let worm_id = (*worm).worm_index;

        if is_super_weapon(weapon.into(), g.version_flag_3 != 0) {
            *g.weapon_stat_counter(team_id, worm_id, 0x40D8) += 1;
        }

        // Powerup/utility weapons (JetPack..=CrateShower)
        if (KnownWeaponId::JetPack..=KnownWeaponId::CrateShower).contains(&weapon) {
            *g.weapon_stat_counter(team_id, worm_id, 0x40D4) += 1;
        }

        if is_weapon_category_a(weapon) {
            *g.weapon_stat_counter(team_id, worm_id, 0x40D0) += 1;
        }

        if is_weapon_category_b(weapon) {
            *g.weapon_stat_counter(team_id, worm_id, 0x40CC) += 1;
        }

        // ── 9. Sound dispatch + 10. Visual effect ───────────────
        let task = worm as *mut WorldEntity;
        let mut do_effect = false;
        let mut effect_state: u32 = 0x73;

        let w = &*worm; // re-borrow after mutation above
        let entry = w.active_weapon_entry;

        match FireType::try_from((*entry).fire_type) {
            Ok(FireType::Projectile) => match (*entry).special_subtype {
                1 => {
                    if w.sound_handle == 0 {
                        sound::play_worm_sound(worm, SoundId(0x1004E), Fixed::ONE);
                    }
                    do_effect = true;
                    effect_state = 0x73;
                }
                2 => {
                    sound::play_sound_local(
                        task,
                        KnownSoundId::ThrowRelease,
                        3,
                        Fixed::ONE,
                        Fixed::ONE,
                    );
                    sound::stop_worm_sound(worm);
                }
                3 | 7 | 0xB | 0xC => {
                    sound::play_sound_local(
                        task,
                        KnownSoundId::RocketRelease,
                        3,
                        Fixed::ONE,
                        Fixed::ONE,
                    );
                    sound::stop_worm_sound(worm);
                }
                4 => {
                    if w.sound_handle == 0 {
                        sound::play_worm_sound(worm, SoundId(0x1004F), Fixed::ONE);
                    }
                    do_effect = true;
                    effect_state = 0x73;
                }
                5 => {
                    sound::play_sound_local(
                        task,
                        KnownSoundId::ShotgunFire,
                        3,
                        Fixed::ONE,
                        Fixed::ONE,
                    );
                    do_effect = true;
                    effect_state = 0x75;
                }
                6 => {
                    sound::play_sound_local(
                        task,
                        KnownSoundId::HandgunFire,
                        3,
                        Fixed::ONE,
                        Fixed::ONE,
                    );
                    do_effect = true;
                    effect_state = 0x73;
                }
                10 => {
                    sound::play_sound_local(
                        task,
                        KnownSoundId::LongbowRelease,
                        3,
                        Fixed::ONE,
                        Fixed::ONE,
                    );
                    sound::stop_worm_sound(worm);
                }
                _ => {}
            },
            Ok(FireType::Placed) if (w._unknown_2cc == 0 || w._unknown_2c8 == 1) => {
                let team_sound_raw = (*w.world()).team_sound_id(team_id);
                sound::play_sound_local(task, SoundId(team_sound_raw), 3, Fixed::ONE, Fixed::ONE);
            }
            // Type 3 (Strike): no sound
            Ok(FireType::Special) => {
                use crate::game::weapon::SpecialFireSubtype as S;
                match S::try_from((*entry).special_subtype) {
                    Ok(S::BaseballBat) => {
                        sound::play_sound_local(
                            task,
                            KnownSoundId::BaseballBatRelease,
                            3,
                            Fixed::ONE,
                            Fixed::ONE,
                        );
                    }
                    Ok(S::DragonBall) => {
                        sound::play_sound_local(
                            task,
                            SoundId((*entry).fire_method as u32),
                            3,
                            Fixed::ONE,
                            Fixed::ONE,
                        );
                    }
                    Ok(S::Kamikaze) => {
                        // Sound ID from fire_params.spread (polymorphic use of field)
                        sound::play_sound_local(
                            task,
                            SoundId((*entry).fire_params.spread as u32),
                            3,
                            Fixed::ONE,
                            Fixed::ONE,
                        );
                    }
                    Ok(S::Teleport) if w._unknown_208 == 0 => {
                        sound::play_sound_local(
                            task,
                            KnownSoundId::Teleport,
                            3,
                            Fixed::ONE,
                            Fixed::ONE,
                        );
                    }
                    Ok(S::Blowtorch) if w.sound_handle == 0 => {
                        sound::play_worm_sound(worm, SoundId(0x10035), Fixed::ONE);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        // ── 10. Visual effect (if triggered by sound dispatch) ──
        if do_effect {
            let world = &mut *{
                let this = worm as *const BaseEntity;
                (*this).world
            };
            let gfx_handler = world.game_state_stream as *const u8;
            let palette = *(gfx_handler.add(0x22C) as *const u32);

            let rng1 = world.advance_effect_rng();
            let rng2 = world.advance_effect_rng();

            let rng2_offset = (rng2 & 0xFFFF) as i32 - 0x18000;
            let facing = (*worm).facing_direction_2;
            let rng_scaled = rng2_offset * facing;

            let rng1_offset = (rng1 & 0xFFFF) as i32 - 0x18000;

            let facing_flag: u32 = if facing < 1 { 0x40000 } else { 0 };
            let state_flag = facing_flag + effect_state;

            spawn_effect(
                worm,
                0x80000,
                speed_x,
                speed_y,
                rng_scaled,
                rng1_offset,
                palette,
                state_flag,
                Fixed(0xA0000),
                Fixed(0x1999),
            );
        }

        // ── 11. Call FireWeapon ──────────────────────────────────
        let entry = (*worm).active_weapon_entry;
        weapon_fire::fire_weapon(entry, &ctx, worm);

        let _ = log_line(&format!(
            "[WeaponRelease] worm=0x{:08X} weapon={:?} type={} sub34={} sub38={}",
            worm as u32, weapon, fire_type, special_subtype, fire_method,
        ));
    }
}

// ── Helper ──────────────────────────────────────────────────

#[inline(always)]
fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}

// ── SpawnEffect (0x547C30) ──────────────────────────────────

/// Spawn a visual effect on the sprite anim entity. Pure Rust port of FUN_00547C30.
///
/// Builds a 0x408-byte message buffer from the params, looks up SpriteAnimEntity
/// via SharedData (entity type 0x1A), and sends HandleMessage(0x56).
///
/// Called directly from `weapon_release` and via the hook trampoline
/// for all other WA callers (17 call sites).
pub unsafe fn spawn_effect(
    worm: *mut WormEntity,
    constant: u32,
    speed_x: Fixed,
    speed_y: Fixed,
    rng_scaled: i32,
    rng_offset: i32,
    palette: u32,
    state_flag: u32,
    size: Fixed,
    scale: Fixed,
) {
    unsafe {
        // Build the 0x408-byte message buffer. ESI (worm/task) is NOT stored
        // in the buffer — it's passed to SharedData__Lookup as the task
        // context and used as the sender for HandleMessage(0x56). The first
        // data slot at [0x00] holds EAX (`constant`), not the task ptr.
        let mut buf = [0u8; 0x408];
        write_u32(&mut buf, 0x00, constant);
        write_u32(&mut buf, 0x04, speed_x.0 as u32);
        write_u32(&mut buf, 0x08, speed_y.0 as u32);
        write_u32(&mut buf, 0x0C, rng_scaled as u32);
        write_u32(&mut buf, 0x10, rng_offset as u32);
        // [0x14] = 0 (already zeroed)
        write_u32(&mut buf, 0x18, palette);
        write_u32(&mut buf, 0x1C, state_flag);
        // [0x20] = 0 (already zeroed)
        write_u32(&mut buf, 0x24, size.0 as u32);
        write_u32(&mut buf, 0x28, scale.0 as u32);

        // SharedData lookup for entity type 0x1A (SpriteAnimEntity)
        let table = SharedDataTable::from_task(worm as *const BaseEntity);
        let entity = table.lookup(0, 0x1A);
        if !entity.is_null() {
            let vtable = *(entity as *const *const usize);
            let handle_msg: unsafe extern "thiscall" fn(*mut u8, *mut u8, u32, u32, *const u8) =
                core::mem::transmute(*vtable.add(2));
            handle_msg(
                entity as *mut u8,
                worm as *mut u8,
                0x56,
                0x408,
                buf.as_ptr(),
            );
        }
    }
}
