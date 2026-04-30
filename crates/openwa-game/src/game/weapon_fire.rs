//! Weapon fire dispatch, ammo management, and object creation.
//!
//! Pure Rust reimplementations of WA.exe weapon functions. Called from
//! hook trampolines in openwa-dll.
//!
//! Original WA functions:
//! - AddAmmo (0x522640), SubtractAmmo (0x522680), GetAmmo (0x5225E0)
//! - CountAliveWorms (0x5225A0)
//! - FireWeapon (0x51EE60): full dispatch
//! - CreateWeaponProjectile (0x51E0F0), ProjectileFire (0x51DFB0), CreateArrow (0x51ED90)

use crate::address::va;
use crate::audio::{KnownSoundId, SoundId};
use crate::engine::{GAME_PHASE_NORMAL_MIN, GAME_PHASE_SUDDEN_DEATH, TeamArena};
use crate::game::KnownWeaponId;
use crate::game::message::{
    ArmageddonMessage, FreezeMessage, NukeBlastMessage, PoisonWormMessage, RaiseWaterMessage,
    ScalesOfJusticeMessage, SelectWormMessage, SkipGoOrMailMineMoleMessage, SurrenderMessage,
    TaskMessageData,
};
use crate::game::weapon::{WeaponEntry, WeaponFireParams, WeaponSpawnData};
use crate::task::BaseEntity;
use crate::task::world_root::WorldRootEntity;
use crate::task::worm::{WormEntity, WormState};
use crate::wa::localized_template;
use core::ffi::c_char;
use openwa_core::fixed::Fixed;
use openwa_core::log::log_line;

// ── WeaponReleaseContext ────────────────────────────────────

/// The 0x2C-byte stack-local struct populated by WeaponRelease and passed to
/// FireWeapon as the `local_struct` (ECX) parameter.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WeaponReleaseContext {
    pub team_id: u32,
    pub worm_id: u32,
    pub spawn_x: u32,
    pub spawn_y: u32,
    pub spawn_offset_x: i32,
    pub spawn_offset_y: i32,
    pub ammo_per_turn: u32,
    pub ammo_per_slot: u32,
    pub _zero: u32,
    pub delay: u32,
    pub network_delay: i32,
}

const _: () = assert!(core::mem::size_of::<WeaponReleaseContext>() == 0x2C);

// ============================================================
// AddAmmo replacement (0x522640)
// ============================================================
// __usercall: EAX = team_index, EDX = amount, [ESP+4] = team_info_base, [ESP+8] = weapon_id
// RET 0x8

pub unsafe fn add_ammo(team_index: u32, amount: i32, arena: *mut TeamArena, weapon_id: u32) {
    unsafe {
        let (alliance, wid) = TeamArena::weapon_slot_key(arena, team_index as usize, weapon_id);
        let ammo = (*arena).get_ammo(alliance, wid);
        if ammo >= 0 {
            if amount < 0 {
                *(*arena).ammo_mut(alliance, wid) = -1;
            } else {
                *(*arena).ammo_mut(alliance, wid) = ammo + amount;
            }
        }
    }
}

// ============================================================
// SubtractAmmo replacement (0x522680)
// ============================================================
// __usercall: EAX = team_index, ECX = team_info_base, [ESP+4] = weapon_id
// RET 0x4

pub unsafe fn subtract_ammo(team_index: u32, arena: *mut TeamArena, weapon_id: u32) {
    unsafe {
        let (alliance, wid) = TeamArena::weapon_slot_key(arena, team_index as usize, weapon_id);
        let ammo = (*arena).get_ammo(alliance, wid);
        if ammo > 0 {
            *(*arena).ammo_mut(alliance, wid) = ammo - 1;
        }
    }
}

// ============================================================
// GetAmmo replacement (0x5225E0)
// ============================================================
// __usercall: EAX = team_index, ESI = team_info_base, EDX = weapon_id
// plain RET, returns EAX = ammo count

pub unsafe fn get_ammo(team_index: u32, arena: *mut TeamArena, weapon_id: u32) -> u32 {
    unsafe {
        let (alliance, wid) = TeamArena::weapon_slot_key(arena, team_index as usize, weapon_id);

        // Check weapon delay
        if (*arena).get_delay(alliance, wid) != 0 {
            if (*arena).game_mode_flag == 0 {
                return 0;
            }
            // In sudden death (phase >= 484), delayed weapons return 0
            // unless it's Teleport (weapon 0x28)
            if (*arena).game_phase >= GAME_PHASE_SUDDEN_DEATH
                && weapon_id != KnownWeaponId::Teleport as u32
            {
                return 0;
            }
        }

        // SelectWorm (0x3B) requires >1 alive worm on the team
        if (*arena).game_phase >= GAME_PHASE_NORMAL_MIN
            && weapon_id == KnownWeaponId::SelectWorm as u32
            && count_alive_worms(team_index, arena) == 0
        {
            return 0;
        }

        (*arena).get_ammo(alliance, wid) as u32
    }
}

// ============================================================
// CountAliveWorms replacement (0x5225A0)
// ============================================================
// __usercall: EAX = team_index, ECX = base
// plain RET, returns EAX = bool (1 if >1 worm alive on team)

pub unsafe fn count_alive_worms(team_index: u32, arena: *const TeamArena) -> u32 {
    unsafe {
        let header = TeamArena::team_header(arena, team_index as usize);
        let worm_count = (*header).worm_count;
        let mut alive = 0i32;
        for w in 1..=worm_count as usize {
            if (*TeamArena::team_worm(arena, team_index as usize, w)).health > 0 {
                alive += 1;
            }
        }
        if alive > 1 { 1 } else { 0 }
    }
}

// ============================================================
// FireWeapon (0x51EE60) — trapped, called directly from weapon_release
// ============================================================

/// FireWeapon dispatch — called directly by `weapon_release_impl`.
///
/// The WA function at 0x51EE60 is trapped via `install_trap!`; its only
/// caller (WeaponRelease) is ported and invokes this Rust function instead.
///
/// Dispatches to type-specific fire sub-functions based on `entry.fire_type`
/// and `entry.fire_method` / `entry.special_subtype`.
/// Sets the completion flag at worm+0x3C before and after dispatch.
pub unsafe fn fire_weapon(
    entry: *const WeaponEntry,
    ctx: *const WeaponReleaseContext,
    worm: *mut WormEntity,
) {
    unsafe {
        use crate::rebase::rb;

        let fire_type = (*entry).fire_type;
        let fire_method = (*entry).fire_method;
        let fire_params = &raw const (*entry).fire_params;
        // Log weapon fire
        let weapon = (*worm).selected_weapon;
        let _ = log_line(&format!(
            "[Weapon] FireWeapon: {:?} (id={}) type={} sub34={} sub38={}",
            weapon,
            weapon as u32,
            fire_type,
            (*entry).special_subtype,
            fire_method
        ));

        WormEntity::set_fire_complete_raw(worm, 0);

        use crate::game::weapon::{FireMethod, FireType};
        match FireType::try_from(fire_type) {
            Ok(FireType::Projectile) => match FireMethod::try_from(fire_method) {
                Ok(FireMethod::PlacedExplosive) => {
                    call_fire_placed_explosive(worm, fire_params, ctx, rb(0x51EC80))
                }
                Ok(FireMethod::ProjectileFire) => {
                    projectile_fire(worm, fire_params, ctx as *const WeaponSpawnData)
                }
                Ok(FireMethod::CreateWeaponProjectile) => {
                    create_weapon_projectile(worm, fire_params, ctx as *const u8)
                }
                Ok(FireMethod::CreateArrow) => create_arrow(worm, fire_params, ctx as *const u8),
                _ => {}
            },
            Ok(FireType::Rope) => match FireMethod::try_from(fire_method) {
                Ok(FireMethod::PlacedExplosive) => fire_rope_type_1(worm, fire_params, ctx),
                Ok(FireMethod::ProjectileFire) => {
                    create_weapon_projectile(worm, fire_params, ctx as *const u8)
                }
                Ok(FireMethod::CreateWeaponProjectile) => fire_rope_type_3(worm, fire_params, ctx),
                _ => {}
            },
            Ok(FireType::Strike) => {
                // StrikeFire takes a pointer to the subtype_34 field (reinterpreted as fire params)
                let subtype_34_ptr = &raw const (*entry).special_subtype as *const WeaponFireParams;
                call_fire_stdcall3(worm, subtype_34_ptr, ctx, rb(0x51E2C0));
            }
            Ok(FireType::Special) => {
                fire_weapon_special((*entry).special_subtype, entry, worm, ctx);
            }
            _ => {}
        }

        WormEntity::set_fire_complete_raw(worm, 1);
    }
}

// ── Sub-function bridges ────────────────────────────────────
// All bridges save/restore ESI+EDI, set ESI=EDI=worm, then call.
// This preserves LLVM's callee-saved registers while providing
// the usercall context that sub-functions expect.

/// Bridge: PlacedExplosive — usercall(ECX=local_struct, EDX=worm, [ESP+4]=fire_params), RET 0x4.
/// Args: (worm, fire_params, ctx, addr).
#[unsafe(naked)]
unsafe extern "C" fn call_fire_placed_explosive(
    _worm: *mut WormEntity,
    _fire_params: *const WeaponFireParams,
    _ctx: *const WeaponReleaseContext,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Set up usercall registers
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov edx, [esp+16]", // EDX = worm
        "mov ecx, [esp+24]", // ECX = local_struct
        "mov ebx, [esp+28]", // addr
        "push [esp+20]",     // fire_params (stack param)
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: Projectile/Rope/Grenade — stdcall(worm, fire_params, local_struct), RET 0xC.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_stdcall3(
    _worm: *mut WormEntity,
    _fire_params: *const WeaponFireParams,
    _ctx: *const WeaponReleaseContext,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov esi, [esp+16]", // worm (3 saves=12 + ret=4 = 16)
        "mov edi, [esp+16]",
        "mov ebx, [esp+28]", // addr (saves=12 + ret=4 + 3 args=12 = 28)
        "push [esp+24]",     // local_struct
        "push [esp+24]",     // params (shifted +4)
        "push [esp+24]",     // worm (shifted +8)
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: stdcall(fire_params, local_struct), RET 0x8.
/// Currently unused — was incorrectly used for Shotgun/Longbow (which are thiscall).
#[unsafe(naked)]
#[allow(dead_code)]
unsafe extern "C" fn call_fire_stdcall2(
    _worm: *mut WormEntity,
    _fire_params: *const WeaponFireParams,
    _ctx: *const WeaponReleaseContext,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov esi, [esp+16]", // worm
        "mov edi, [esp+16]",
        "mov ebx, [esp+28]", // addr
        "push [esp+24]",     // local_struct
        "push [esp+24]",     // params (shifted +4)
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Type 4 (special) weapon dispatch.
///
/// EAX at entry = weapon entry pointer (unchanged from type-4 switch).
/// Some handlers explicitly set EAX=worm or EAX=*(worm+0x44).
/// Handlers without explicit MOV EAX inherit the entry pointer.
pub unsafe fn fire_weapon_special(
    subtype: i32,
    entry: *const WeaponEntry,
    worm: *mut WormEntity,
    ctx: *const WeaponReleaseContext,
) {
    unsafe {
        use crate::game::weapon::SpecialFireSubtype as S;

        // Pointer to fire_method field, reinterpreted as fire params pointer for Girder
        let params_38_ptr = &raw const (*entry).fire_method as *const WeaponFireParams;

        match S::try_from(subtype) {
            Ok(S::FirePunch) => WormEntity::set_state_raw(worm, WormState::FirePunch),
            Ok(S::BaseballBat) => fire_drill(worm, ctx as *const u8),
            Ok(S::DragonBall) => fire_dragon_ball(worm, params_38_ptr, ctx as *const u8),
            Ok(S::Kamikaze) => WormEntity::set_state_raw(worm, WormState::Kamikaze),
            Ok(S::SuicideBomber) => WormEntity::set_state_raw(worm, WormState::SuicideBomber),
            Ok(S::Unknown6) => WormEntity::set_state_raw(worm, WormState::Unknown_0x70),
            Ok(S::PneumaticDrill) => WormEntity::set_state_raw(worm, WormState::PneumaticDrill),
            Ok(S::Prod) => fire_prod(worm, ctx as *const u8),
            Ok(S::Teleport) => fire_teleport(worm),
            Ok(S::Blowtorch) => WormEntity::set_state_raw(worm, WormState::Blowtorch),
            Ok(S::Parachute) => {} // TODO: parachute handler
            Ok(S::Surrender) => fire_surrender(worm),
            Ok(S::MailMineMole) => fire_mail_mine_mole(worm),
            Ok(S::NuclearTest) => {
                if can_fire_subtype16((*worm).state()) {
                    fire_nuclear_test(worm);
                } else {
                    WormEntity::set_state_raw(worm, WormState::TeleportCancelled_Maybe);
                }
            }
            Ok(S::Girder) => fire_girder(worm),
            Ok(S::Unknown18) => WormEntity::set_state_raw(worm, WormState::Unknown_0x72),
            Ok(S::SkipGo) => fire_skip_go(worm, entry),
            Ok(S::Freeze) => fire_freeze(worm),
            Ok(S::SelectWorm) => fire_select_worm(worm),
            Ok(S::ScalesOfJustice) => fire_scales_of_justice(worm),
            Ok(S::JetPack) => WormEntity::set_state_raw(worm, WormState::WeaponAimed_Maybe),
            Ok(S::Armageddon) => fire_armageddon(worm),
            _ => {}
        }
    }
}

/// Worm state validity check for subtype 16 — pure Rust port of 0x516930.
/// Used by Nuclear Test to gate firing.
pub fn can_fire_subtype16(state: u32) -> bool {
    state == WormState::WeaponAimed_Maybe as u32
        || (WormState::AimingAngle_Maybe as u32..=WormState::PreFire_Maybe as u32).contains(&state)
}

// ── Pure Rust fire handlers (no bridge needed) ──────────────

/// Convenience wrapper over [`crate::task::WorldRootEntity::from_shared_data`]
/// for the common worm call sites.
#[inline]
pub unsafe fn lookup_world_root(worm: *const WormEntity) -> *mut crate::task::WorldRootEntity {
    unsafe { crate::task::WorldRootEntity::from_shared_data(worm as *const BaseEntity) }
}

/// Send a typed message to `WorldRootEntity` for the worm's game tree, if the
/// SharedData lookup succeeds.
unsafe fn send_to_world_root<M: TaskMessageData>(worm: *mut WormEntity, msg: M) {
    unsafe {
        let team = lookup_world_root(worm);
        if team.is_null() {
            return;
        }
        WorldRootEntity::handle_typed_message_raw(team, worm, msg);
    }
}

/// Surrender (subtype 13) — dispatches `TaskMessage::Surrender` to
/// WorldRootEntity.
///
/// WorldRoot::HandleMessage (0x55DC00) delegates to TeamEntity (0x557310) for
/// the broadcast, then handles end-turn logic and surrender sound.
#[inline(never)]
unsafe fn fire_surrender(worm: *mut WormEntity) {
    unsafe {
        send_to_world_root(
            worm,
            SurrenderMessage {
                team_index: (*worm).team_index,
            },
        );
    }
}

/// Select Worm (subtype 21) — pure Rust replacement for 0x51EBE0.
unsafe fn fire_select_worm(worm: *mut WormEntity) {
    unsafe {
        send_to_world_root(
            worm,
            SelectWormMessage {
                unknown1: 8,
                team_index: (*worm).team_index,
            },
        );
    }
}

/// Skip Go (subtype 19) — pure Rust replacement for 0x51E8C0.
///
/// Toggles a bit in the team's `TeamHeader.turn_action_flags` (+0x7C).
/// Bit position comes from weapon entry's fire_params.
/// In game_version > 0x1C: toggles (set/clear). Otherwise: always sets.
unsafe fn fire_skip_go(worm: *const WormEntity, entry: *const WeaponEntry) {
    unsafe {
        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let game_version = (*(*world).game_info).game_version;
        let team_index = (*worm).team_index as usize;

        let bit_index = ((*entry).fire_params.shot_count & 0x1F) as u32;
        let bit = 1u32 << bit_index;

        let arena = &raw mut (*world).team_arena;
        let header = TeamArena::team_header_mut(arena, team_index);
        let flags = (*header).turn_action_flags;

        if game_version > 0x1C && (flags & bit) != 0 {
            (*header).turn_action_flags = flags & !bit;
        } else {
            (*header).turn_action_flags = flags | bit;
        }
    }
}

/// Freeze weapon (subtype 20) — pure Rust replacement for 0x51E600.
///
/// Sends `TaskMessage::Freeze` to WorldRootEntity, then increments
/// `WormEntry.turn_action_counter_Maybe` by 14 (0x0E).
unsafe fn fire_freeze(worm: *mut WormEntity) {
    unsafe {
        send_to_world_root(
            worm,
            FreezeMessage {
                team_index: (*worm).team_index,
            },
        );

        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let arena = &raw mut (*world).team_arena;
        let team_index = (*worm).team_index as usize;
        let worm_index = (*worm).worm_index as usize;
        let entry = TeamArena::team_worm_mut(arena, team_index, worm_index);
        (*entry).turn_action_counter_Maybe += 14;
    }
}

/// Mail/Mine/Mole (subtype 14) — pure Rust replacement for 0x51E670.
///
/// Conditionally calls worm->vtable[0xE](0x65) based on game version and worm state,
/// then sends message 0x28 to WorldRootEntity, then increments
/// WormEntry.turn_action_counter_Maybe by 7.
///
/// Version check logic (from disassembly at 0x51E670):
/// - version < 2: call vtable[0xE](0x65)
/// - 2 <= version < 5: skip vtable call
/// - version >= 5 && worm state == 0x7D: call vtable
/// - version >= 5 && worm state == 0x78 && version < 8: call vtable
/// - otherwise: skip
unsafe fn fire_mail_mine_mole(worm: *mut WormEntity) {
    unsafe {
        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let version = (*world).version_flag_4;
        let worm_state = (*worm).state();

        let should_call_vtable = version < 2
            || (version >= 5
                && (worm_state == WormState::PreFire_Maybe as u32
                    || (worm_state == WormState::WeaponAimed_Maybe as u32 && version < 8)));

        if should_call_vtable {
            WormEntity::set_state_raw(worm, WormState::Idle);
        }

        send_to_world_root(
            worm,
            SkipGoOrMailMineMoleMessage {
                team_index: (*worm).team_index,
            },
        );

        let arena = &raw mut (*world).team_arena;
        let team_index = (*worm).team_index as usize;
        let worm_index = (*worm).worm_index as usize;
        let entry = TeamArena::team_worm_mut(arena, team_index, worm_index);
        (*entry).turn_action_counter_Maybe += 7;
    }
}

/// Teleport (subtype 10) — pure Rust port of 0x51E710.
///
/// If WormEntity+0x208 == 0: set state to AirStrikePending_Maybe.
/// Otherwise: play teleport sound (via play_worm_sound_2), spawn visual
/// effect, update position, compute new state, clear action fields.
///
/// Convention: usercall(EAX=worm, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_teleport(worm: *mut WormEntity) {
    unsafe {
        use crate::audio::sound_ops as sound;
        use crate::rebase::rb;

        if (*worm)._unknown_208 == 0 {
            WormEntity::set_state_raw(worm, WormState::AirStrikePending_Maybe);
            return;
        }

        // Play teleport sound (0x36) on secondary sound handle
        sound::play_worm_sound_2(worm, SoundId(0x36), Fixed::ONE, 3);

        // Spawn visual effect: usercall(EAX=0x80000, ECX=x) + stdcall(y, 0, 0, 0, 600, 0x10000, 0x1999)
        let fire_x = (*worm).weapon_param_1;
        let fire_y = (*worm).weapon_param_2;
        call_spawn_effect(fire_x, fire_y, rb(0x547C30));

        // Temporarily swap fire_subtype_1 (+0x34) with _unknown_190, call position update, restore
        let saved_subtype1 = WormEntity::fire_subtype_1(worm);
        WormEntity::set_fire_subtype_1_raw(worm, (*worm)._unknown_190);
        call_position_update(worm, fire_x, fire_y, rb(0x4FE070));
        WormEntity::set_fire_subtype_1_raw(worm, saved_subtype1);

        // Compute new state: version < 455 → Idle (0x65), else → 0x8B
        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let game_version = (*(*world).game_info).game_version;
        let new_state = if game_version < 0x1C7 {
            WormState::Idle
        } else {
            WormState::Unknown_0x8B
        };
        WormEntity::set_state_raw(worm, new_state);

        // Clear action fields
        WormEntity::set_action_field_raw(worm, 0);
        (*worm)._unknown_208 = 0;
        (*worm)._unknown_198 = 0;
        (*worm)._unknown_19c = 0;
        (*worm).facing_direction_inv = 0;

        // Post-teleport landing check — records a "kind 3 / 4" event-bbox
        // entry into world.render_entries based on whether the new position
        // is inside the level scroll bbox.
        WormEntity::landing_check_raw(worm);

        // Debug log block (world+0x8144) omitted — only writes to debug file
    }
}

/// Nuclear Test (subtype 16) — pure Rust port of 0x51EB00.
///
/// Sends three messages to WorldRootEntity: RaiseWater (0x59), NukeBlast (0x5A),
/// PoisonWorm (0x51), and plays two sounds. Gated by can_fire_subtype16.
///
/// Convention: usercall(EAX=worm_state, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_nuclear_test(worm: *mut WormEntity) {
    unsafe {
        use crate::audio::sound_ops as sound;

        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let entry = &*(*worm).active_weapon_entry;
        let game = lookup_world_root(worm);

        if game.is_null() {
            return;
        }

        WorldRootEntity::handle_typed_message_raw(
            game,
            worm,
            RaiseWaterMessage {
                fire_method: entry.fire_method,
                unknown1: 8,
            },
        );

        WorldRootEntity::handle_typed_message_raw(game, worm, NukeBlastMessage { unknown1: 8 });

        sound::queue_sound(
            world,
            KnownSoundId::IndianAnthem.into(),
            8,
            Fixed::ONE,
            Fixed::ONE,
        );

        WorldRootEntity::handle_typed_message_raw(
            game,
            worm,
            PoisonWormMessage {
                unknown1: entry.fire_params.shot_count,
                unknown2: 2,
                team_index: (*worm).team_index,
            },
        );

        // PlaySoundGlobal(NukeFlash, 5, 0x10000, 0x10000)
        sound::queue_sound(
            world,
            KnownSoundId::NukeFlash.into(),
            5,
            Fixed::ONE,
            Fixed::ONE,
        );
    }
}

/// Bridge: FUN_00547C30 — usercall(EAX=0x80000, ECX=x) + stdcall(y, 0,0,0, 600, 0x10000, 0x1999).
#[unsafe(naked)]
unsafe extern "C" fn call_spawn_effect(_x: i32, _y: i32, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov ecx, [esp+8]",  // x
        "mov ebx, [esp+16]", // addr
        "push 0x1999",
        "push 0x10000",
        "push 0x258", // 600
        "push 0",
        "push 0",
        "push 0",
        "push [esp+36]", // y (8 + 4 + 6*4 = 36)
        "mov eax, 0x80000",
        "call ebx",
        "pop ebx",
        "ret",
    );
}

/// Bridge: FUN_004FE070 — usercall(ESI=worm, EDI=y) + stdcall(x). Plain RET.
#[unsafe(naked)]
unsafe extern "C" fn call_position_update(_worm: *mut WormEntity, _x: i32, _y: i32, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov esi, [esp+16]", // worm
        "mov edi, [esp+24]", // y
        "mov ebx, [esp+28]", // addr
        "push [esp+20]",     // x
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Scales of Justice (subtype 22) — pure Rust port of 0x51EC30.
///
/// Sends message 0x5E to WorldRootEntity, then plays a sound.
/// Convention: usercall(EAX=entry, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_scales_of_justice(worm: *mut WormEntity) {
    unsafe {
        use crate::rebase::rb;

        // Send message 0x5E to WorldRootEntity
        let game = lookup_world_root(worm);
        if !game.is_null() {
            WorldRootEntity::handle_typed_message_raw(game, worm, ScalesOfJusticeMessage);
        }

        // Play jet pack sound:
        // - LocalizedTemplate::ResolveSplitArray (token 0x6CB) — pure Rust now,
        //   returns a NULL-terminated `*mut *mut c_char` speech-bank array.
        // - FUN_005480F0: usercall(EAX=-21) + stdcall(worm, array, 0x17, &worm_name)
        //   randomly picks one entry and plays it.
        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let array = localized_template::resolve_split_array(
            (*world).localized_template,
            crate::wa::string_resource::res::GAME_SCALES_OF_JUSTICE_COMMENTS,
        );
        call_play_sound_usercall(
            worm,
            array,
            0x17,
            (*worm).worm_name.as_ptr() as *const c_char,
            rb(0x5480F0),
        );
    }
}

/// Bridge: usercall(EAX=-21) + stdcall(4 params). Plain RET (callee doesn't clean).
#[unsafe(naked)]
unsafe extern "C" fn call_play_sound_usercall(
    _worm: *mut WormEntity,
    _array: *mut *mut c_char,
    _param3: u32,
    _name: *const c_char,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov ebx, [esp+24]",   // addr (8 + 4*4 = 24)
        "push [esp+20]",       // name
        "push [esp+20]",       // param3 (shifted +4)
        "push [esp+20]",       // sound_val (shifted +8)
        "push [esp+20]",       // worm (shifted +12)
        "mov eax, 0xFFFFFFEB", // EAX = -21 (usercall param)
        "call ebx",
        "pop ebx",
        "ret",
    );
}

/// Armageddon (subtype 24) — pure Rust port of 0x51EA60.
///
/// Sends message 0x5B to WorldRootEntity with weapon/team info, then conditionally
/// sets a gravity center point via FUN_00547E70.
/// Convention: usercall(EAX=entry, ESI=worm, EDI=worm), plain RET.
///
unsafe fn fire_armageddon(worm: *mut WormEntity) {
    unsafe {
        // Send message 0x5B (Armageddon) to WorldRootEntity with weapon info buffer.
        //
        // The original allocates a 0x410-byte stack buffer, writes fields at offsets
        // 0x04/0x08/0x0C/0x10 from the buffer base, then passes (buffer_base + 4)
        // as the data pointer to HandleMessage (LEA ECX,[ESP+0x8] after one PUSH
        // was cleaned by the SharedData lookup). So HandleMessage sees:
        //   data[0x00] = 100 (0x64), data[0x04] = 166 (0xA6),
        //   data[0x08] = weapon_id,  data[0x0C] = team_index
        let team = lookup_world_root(worm);
        if !team.is_null() {
            WorldRootEntity::handle_typed_message_raw(
                team,
                worm,
                ArmageddonMessage {
                    unknown1: 100,
                    unknown2: 166,
                    selected_weapon: (*worm).selected_weapon as u32,
                    team_index: (*worm).team_index,
                },
            );
        }

        // Re-read GameWorld AFTER HandleMessage — the original does this (MOV EAX,[ESI+0x2C]
        // at 0x51EAB0 is after the CALL), and HandleMessage may modify game state.
        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let game_version = (*(*world).game_info).game_version;

        // If old game version or worm state 0x69, register an effect-event
        // point at the level center (formerly bridged as "set_gravity_center" —
        // see GameWorld::register_event_point_raw for the actual semantics).
        if game_version < 0x50 || (*worm).is_in_state(WormState::Unknown_0x69) {
            let level_w = (*world).level_width as i32;
            let level_h = (*world).level_height as i32;
            // SHL 16; CDQ; SUB EAX,EDX; SAR 1 — round-toward-zero divide by 2
            let half_x = ((level_w << 16) + (if level_w < 0 { 1 } else { 0 })) >> 1;
            let half_y = ((level_h << 16) + (if level_h < 0 { 1 } else { 0 })) >> 1;

            crate::engine::world::GameWorld::register_event_point_raw(world, half_x, half_y);
        }
    }
}

/// DragonBall (type 4 subtype 3) — pure Rust port of 0x51E350.
///
/// Allocates a GirderEntity (0xA4 bytes), copies 7 DWORDs from fire_params
/// as constructor arguments, and calls the constructor. The constructor handles
/// actually placing the girder on the landscape.
///
/// Convention: stdcall(worm, fire_params, local_struct), RET 0xC.
unsafe fn fire_dragon_ball(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        use crate::rebase::rb;
        use crate::task::SharedDataTable;
        use crate::wa_alloc::wa_malloc;

        // Look up parent task via SharedData (same key as CreateWeaponProjectile)
        let table = SharedDataTable::from_task(worm as *const BaseEntity);
        let parent = table.lookup(0, 0x19);

        // Allocate GirderEntity (0xA4 bytes), zero first 0x84
        let buffer = wa_malloc(0xA4);
        if buffer.is_null() {
            return;
        }
        core::ptr::write_bytes(buffer, 0, 0x84);

        // GirderEntity::Constructor — usercall(EAX=parent) +
        // stdcall(this, 7×fire_param DWORDs, local_struct), RET 0x24.
        // Copy 7 DWORDs from fire_params onto the stack via the naked bridge.
        call_girder_ctor(
            buffer,
            parent as *mut u8,
            fire_params,
            local_struct,
            rb(0x550890),
        );
    }
}

/// Bridge: GirderEntity::Constructor — usercall(EAX=parent) +
/// stdcall(this, 7 DWORDs from fire_params, local_struct), RET 0x24.
///
/// The original copies 7 DWORDs from fire_params onto the stack via REP MOVSD.
/// We replicate this by pushing them individually in reverse order.
#[unsafe(naked)]
unsafe extern "C" fn call_girder_ctor(
    _this: *mut u8,
    _parent: *mut u8,
    _fire_params: *const WeaponFireParams,
    _local_struct: *const u8,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        // Args: this=+16, parent=+20, fire_params=+24, local_struct=+28, addr=+32
        "mov esi, [esp+24]", // ESI = fire_params
        "mov eax, [esp+20]", // EAX = parent (usercall)
        "mov ebx, [esp+32]", // EBX = ctor address
        // Push constructor args in reverse: local_struct, 7 DWORDs, this
        "push [esp+28]", // local_struct (B+28 → stack pos 1)
        "push [esi+24]", // fire_params[6]
        "push [esi+20]", // fire_params[5]
        "push [esi+16]", // fire_params[4]
        "push [esi+12]", // fire_params[3]
        "push [esi+8]",  // fire_params[2]
        "push [esi+4]",  // fire_params[1]
        "push [esi]",    // fire_params[0] (B-28 → stack pos 8)
        "push [esp+48]", // this: B-32+48 = B+16 ✓
        "call ebx",      // RET 0x24 cleans 9 params
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Girder/GirderPack (subtype 17) — pure Rust port of 0x51E920.
///
/// Plays a sound, optionally creates a visual overlay on the
/// landscape and increments worm counters. The visual/counter path only runs
/// when WormEntity+0x2EC (weapon_param_3) is nonzero.
///
/// Convention: usercall(EAX=worm, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_girder(worm: *mut WormEntity) {
    unsafe {
        use crate::audio::sound_ops as sound;

        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let game_version = (*(*world).game_info).game_version;

        // Read girder position and sprite index from WormEntity fields
        let girder_x = (*worm).weapon_param_1;
        let girder_y = (*worm).weapon_param_2;
        let girder_sprite = (*worm).weapon_param_3;

        // Choose sound: 0x70 if girder has sprite or old version, else 0x73
        let sound_id: u32 = if girder_sprite != 0 || game_version < 0x21 {
            0x70
        } else {
            0x73
        };

        // Queue sound and set position to the girder location (local sound)
        if let Some(entry) = sound::queue_sound(world, SoundId(sound_id), 3, Fixed::ONE, Fixed::ONE)
        {
            (*entry).is_local = 1;
            (*entry).pos_x = girder_x as u32;
            (*entry).pos_y = girder_y as u32;
        }

        // If girder has a sprite, apply the visual overlay and update counters
        if girder_sprite != 0 {
            // Call Landscape vtable[5] to create girder overlay
            let landscape = (*world).landscape as *mut u8;
            let landscape_vt = *(landscape as *const *const usize);
            let girder_visual: unsafe extern "thiscall" fn(*mut u8, i32, i32, *mut u8, *mut u8) =
                core::mem::transmute(*landscape_vt.add(5));
            let sprite1 = (*world).sprite_cache_2[girder_sprite as usize];
            let sprite2 = (*world).sprite_cache_2[19 + girder_sprite as usize];
            girder_visual(landscape, girder_x >> 16, girder_y >> 16, sprite1, sprite2);

            // Increment WormEntry counters
            let arena = &raw mut (*world).team_arena;
            let team_index = (*worm).team_index as usize;
            let worm_index = (*worm).worm_index as usize;
            let entry = TeamArena::team_worm_mut(arena, team_index, worm_index);
            (*entry).turn_action_counter_Maybe += 3;
            (*entry).effect_counter_04_Maybe += 10;
        }
    }
}

// ── SpecialImpact wrapper ─────────────────────────────────

/// Typed wrapper for SpecialImpact (0x5193D0).
/// Convention: stdcall with 13 params (RET 0x34 = 52 bytes).
///
/// Used by Drill, Prod, and other melee/impact weapons. Applies damage in
/// a directional area from the worm's position.
#[allow(clippy::too_many_arguments)]
unsafe fn call_special_impact(
    worm: *mut WormEntity,
    x: i32,
    y: i32,
    radius_x: i32,
    radius_y: i32,
    weapon_type: i32,
    dx: i32,
    dy: i32,
    p8: i32,
    p9: i32,
    p10: i32,
    flags: u32,
    p12: i32,
) {
    unsafe {
        use crate::rebase::rb;
        type SpecialImpactFn = unsafe extern "stdcall" fn(
            *mut WormEntity,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            u32,
            i32,
        );
        let func: SpecialImpactFn = core::mem::transmute(rb(va::SPECIAL_IMPACT));
        func(
            worm,
            x,
            y,
            radius_x,
            radius_y,
            weapon_type,
            dx,
            dy,
            p8,
            p9,
            p10,
            flags,
            p12,
        );
    }
}

/// Compute version-dependent flags for SpecialImpact.
///
/// Pattern shared by Drill and Prod:
/// - Base flags OR 0x20 if version >= 2
/// - Base flags OR 0x10 if version >= 8
fn special_impact_version_flags(base: u32, version: u8) -> u32 {
    let mut flags = base;
    if version >= 2 {
        flags |= 0x20;
    }
    if version >= 8 {
        flags |= 0x10;
    }
    flags
}

/// BaseballBat (subtype 2) — pure Rust port of 0x51E3E0.
///
/// Calls SpecialImpact with facing-offset position and scaled direction.
/// The original is usercall(ECX=local_struct, ESI=worm) — the old bridge
/// did not set ECX, so this port also fixes a latent bug.
unsafe fn fire_drill(worm: *mut WormEntity, local_struct: *const u8) {
    unsafe {
        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let version = (*world).version_flag_4;
        let entry = &*(*worm).active_weapon_entry;
        let shot_count = entry.fire_params.shot_count;
        let weapon_type = entry.fire_method;
        let facing = (*worm).facing_direction_2;

        // Cast local_struct to WeaponSpawnData for field access (offsets match)
        let spawn = &*(local_struct as *const WeaponSpawnData);

        let x = facing * 0x1A_0000 + spawn.spawn_x.0;
        let y = spawn.spawn_y.0;
        let dx = (spawn.initial_speed_x.0 * shot_count) / 10;
        let dy = (spawn.initial_speed_y.0 * shot_count) / 10;
        let flags = special_impact_version_flags(0x21C4C, version);

        call_special_impact(
            worm,
            x,
            y,
            0x1A_0000,
            0x1E_0000,
            weapon_type,
            dx,
            dy,
            6,
            0x61,
            0x51,
            flags,
            1,
        );
    }
}

/// Prod (subtype 9) — pure Rust port of 0x51E480.
///
/// Like Drill but with trig interpolation on the spread angle.
/// Convention: usercall(EDI=worm) + 1 stack param (local_struct), RET 0x4.
unsafe fn fire_prod(worm: *mut WormEntity, local_struct: *const u8) {
    unsafe {
        use crate::rebase::rb;

        let world = {
            let this = worm as *const BaseEntity;
            (*this).world
        };
        let version = (*world).version_flag_4;
        let entry = &*(*worm).active_weapon_entry;
        let shot_count = entry.fire_params.shot_count;
        let spread = entry.fire_params.spread;
        let weapon_type = entry.fire_method;
        let facing = (*worm).facing_direction_2;

        let spawn = &*(local_struct as *const WeaponSpawnData);

        // Convert spread (degrees) to engine angle units: (spread << 16) / 360
        let angle = ((spread as u32) << 16) / 0x168;

        // Interpolated sin/cos lookup (same pattern as projectile_fire)
        let sin_table = rb(va::SIN_TABLE) as *const i32;
        let cos_table = sin_table.add(256);

        let table_index = ((angle >> 6) & 0x3FF) as usize;
        let frac = ((angle & 0x3F) << 10) as i32;

        let cos_base = *cos_table.add(table_index);
        let cos_next = *cos_table.add(table_index + 1);
        let cos_val = cos_base + fixed_mul(cos_next - cos_base, frac);

        let sin_base = *sin_table.add(table_index);
        let sin_next = *sin_table.add(table_index + 1);
        let sin_val = sin_base + fixed_mul(sin_next - sin_base, frac);

        // Scale trig results by shot_count and divide by 10.
        // In WA's coordinate system (Y increases downward), the vertical component is negated.
        // Original asm uses IMUL 0x99999999 (= -0x66666667) for dy, which negates the result.
        // dx = (sin_val * shot_count) / 10 * facing
        // dy = -(cos_val * shot_count) / 10
        let dx = ((sin_val * shot_count) / 10) * facing;
        let dy = -((cos_val * shot_count) / 10);

        let x = facing * 0x6_0000 + spawn.spawn_x.0;
        let y = spawn.spawn_y.0;
        let flags = special_impact_version_flags(0xC4C, version);

        call_special_impact(
            worm,
            x,
            y,
            0xC_0000,
            0xC_0000,
            weapon_type,
            dx,
            dy,
            0,
            0,
            0,
            flags,
            1,
        );
    }
}

// ── Naked asm bridges ───────────────────────────────────────

/// Bridge: ProjectileFire_Single — usercall(EDI=spawn_data, stack=[worm, fire_params]), RET 0x8.
/// Args: (spawn_data, worm, fire_params, addr).
#[unsafe(naked)]
unsafe extern "C" fn call_projectile_fire_single(
    _spawn_data: *const WeaponSpawnData,
    _worm: *mut WormEntity,
    _fire_params: *const WeaponFireParams,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov edi, [esp+16]", // EDI = spawn_data
        "mov ebx, [esp+28]", // addr
        "push [esp+24]",     // fire_params
        "push [esp+24]",     // worm (shifted +4)
        "call ebx",          // RET 0x8 cleans 2 params
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: MissileEntity::Constructor — thiscall(ECX=this, parent, fire_params, spawn_data), RET 0xC.
/// Args: (this, parent, fire_params, spawn_data, ctor_addr).
#[unsafe(naked)]
unsafe extern "C" fn call_missile_ctor(
    _this: *mut u8,
    _parent: *mut u8,
    _fire_params: *const WeaponFireParams,
    _spawn_data: *const u8,
    _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov ecx, [esp+16]", // ECX = this (buffer)
        "mov ebx, [esp+32]", // addr (16 + 4*4 args = 32)
        "push [esp+28]",     // spawn_data
        "push [esp+28]",     // fire_params (shifted +4)
        "push [esp+28]",     // parent (shifted +8)
        "call ebx",          // thiscall: RET 0xC cleans 3 params
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

// ── Object creation functions ───────────────────────────────

/// Rope variant 1 fire — Rust port of `FireWeapon__RopeType1` (WA 0x0051E1C0,
/// `__stdcall(worm, fire_params, local_struct)`, RET 0xC).
///
/// Looks up the parent task via SharedData, allocates a `MineEntity` (0x1BC
/// bytes), zeroes the first 0x19C, and forwards to `MineEntity::Constructor`
/// with the trailing `(0, 1)` tag. WA's MSVC SEH wrapper around the same body
/// is dropped — neither `wa_malloc` (returns null on failure) nor the C++
/// constructor throws in offline play.
unsafe fn fire_rope_type_1(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const WeaponReleaseContext,
) {
    unsafe {
        use crate::rebase::rb;
        use crate::task::SharedDataTable;
        use crate::wa_alloc::wa_malloc;

        let table = SharedDataTable::from_task(worm as *const BaseEntity);
        let parent = table.lookup(0, 0x19);

        let buffer = wa_malloc(0x1BC);
        if buffer.is_null() {
            return;
        }
        core::ptr::write_bytes(buffer, 0, 0x19C);

        type Ctor = unsafe extern "stdcall" fn(
            *mut u8,
            *mut u8,
            *const WeaponFireParams,
            *const WeaponReleaseContext,
            u32,
            u32,
        );
        let ctor: Ctor = core::mem::transmute(rb(va::MINE_ENTITY_CTOR));
        ctor(buffer, parent, fire_params, local_struct, 0, 1);
    }
}

/// Rope variant 3 fire — Rust port of `FireWeapon__RopeType3` (WA 0x0051E240,
/// `__stdcall(worm, fire_params, local_struct)`, RET 0xC).
///
/// Mirror of [`fire_rope_type_1`] for the canister payload: 0x17C-byte alloc,
/// zero the first 0x15C, hand off to `CanisterEntity::Constructor`. Same
/// SEH-elision rationale.
unsafe fn fire_rope_type_3(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const WeaponReleaseContext,
) {
    unsafe {
        use crate::rebase::rb;
        use crate::task::SharedDataTable;
        use crate::wa_alloc::wa_malloc;

        let table = SharedDataTable::from_task(worm as *const BaseEntity);
        let parent = table.lookup(0, 0x19);

        let buffer = wa_malloc(0x17C);
        if buffer.is_null() {
            return;
        }
        core::ptr::write_bytes(buffer, 0, 0x15C);

        type Ctor = unsafe extern "stdcall" fn(
            *mut u8,
            *mut u8,
            *const WeaponFireParams,
            *const WeaponReleaseContext,
        );
        let ctor: Ctor = core::mem::transmute(rb(va::CANISTER_ENTITY_CTOR));
        ctor(buffer, parent, fire_params, local_struct);
    }
}

/// Rust implementation of CreateWeaponProjectile.
///
/// Original: 0x51E0F0. Allocates MissileEntity (0x40C bytes), looks up
/// parent WorldRootEntity via SharedData, calls the original constructor.
pub unsafe fn create_weapon_projectile(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        use crate::rebase::rb;
        use crate::task::SharedDataTable;
        use crate::wa_alloc::wa_malloc;

        let world = &mut *{
            let this = worm as *const BaseEntity;
            (*this).world
        };

        // Pool capacity check: pool_count + 7 must be <= 700
        if world.object_pool_count + 7 > 700 {
            world.show_pool_overflow_warning();
            return;
        }

        // Look up parent WorldRootEntity via SharedData (key_esi=0, key_edi=0x19)
        let table = SharedDataTable::from_task(worm as *const BaseEntity);
        let parent = table.lookup(0, 0x19);

        // Allocate MissileEntity (0x40C bytes)
        let buffer = wa_malloc(0x40C);
        if buffer.is_null() {
            return;
        }

        // Zero bytes 0x00..0x3EC (the original only zeros 0x3EC of 0x40C)
        core::ptr::write_bytes(buffer, 0, 0x3EC);

        // Call original MissileEntity::Constructor
        // thiscall(ECX=buffer, parent, fire_params, local_struct), RET 0xC
        call_missile_ctor(
            buffer,
            parent,
            fire_params,
            local_struct,
            rb(va::MISSILE_ENTITY_CTOR),
        );

        let _ = log_line(&format!(
            "[Weapon] CreateWeaponProjectile: worm=0x{:08X} missile=0x{:08X}",
            worm as u32, buffer as u32,
        ));
    }
}

/// Rust implementation of ProjectileFire.
///
/// Algorithm (from disasm at 0x51DFB0):
/// 1. Copy 11 DWORDs from local_struct (spawn template) to local buffer
/// 2. Loop fire_params+0xC times:
///    a. Advance game RNG
///    b. Random spread angle from RNG × fire_params+0x10 / 360
///    c. Cos/sin table lookup with linear interpolation
///    d. 2D rotation matrix on template velocity
///    e. Write rotated velocity into local spawn data
///    f. Call ProjectileFire_Single(loop_counter, fire_params) with EDI=spawn_data
pub unsafe fn projectile_fire(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const WeaponSpawnData,
) {
    unsafe {
        use crate::rebase::rb;

        let params = &*fire_params;
        // collision_radius field is polymorphic — for ProjectileFire it holds the shot count
        let shot_count = params.collision_radius.0;
        if shot_count <= 0 {
            return;
        }

        // Copy spawn template from caller's stack buffer
        let mut spawn_data = *local_struct;

        // Read template velocity (will be rotated per-shot)
        let template_speed_x = spawn_data.initial_speed_x.0;
        let template_speed_y = spawn_data.initial_speed_y.0;

        // Trig table: sin at SIN_TABLE, cos at SIN_TABLE + 256*4
        let sin_table = rb(va::SIN_TABLE) as *const i32;
        let cos_table = sin_table.add(256); // cos = sin offset by 256 entries (quarter turn)

        let world = &mut *{
            let this = worm as *const BaseEntity;
            (*this).world
        };

        for _i in 0..shot_count {
            // Advance game RNG (same LCG as ADVANCE_GAME_RNG at 0x53F320)
            let rng = world.advance_rng();

            // Compute spread angle: ((rng_low16 - 0x8000) * spread_param) / 360
            // _fp_04 field is polymorphic — for ProjectileFire it holds spread angle
            let rng_centered = (rng & 0xFFFF) as i32 - 0x8000;
            let spread_param = params._fp_04;
            let angle = (rng_centered * spread_param) / 360;

            // Cos/sin table lookup with linear interpolation
            // Table index = (angle >> 6) & 0x3FF (1024 entries)
            // Fractional = (angle & 0x3F) << 10
            let table_index = ((angle >> 6) & 0x3FF) as usize;
            let frac = ((angle & 0x3F) << 10) as i32;

            let cos_base = *cos_table.add(table_index);
            let cos_next = *cos_table.add(table_index + 1);
            let cos_val = cos_base + fixed_mul(cos_next - cos_base, frac);

            let sin_base = *sin_table.add(table_index);
            let sin_next = *sin_table.add(table_index + 1);
            let sin_val = sin_base + fixed_mul(sin_next - sin_base, frac);

            // 2D rotation matrix:
            // speed_x = cos * template_x + sin * template_y
            // speed_y = -sin * template_x + cos * template_y
            let speed_x =
                fixed_mul(cos_val, template_speed_x) + fixed_mul(sin_val, template_speed_y);
            let speed_y =
                fixed_mul(-sin_val, template_speed_x) + fixed_mul(cos_val, template_speed_y);

            // Write rotated velocity into spawn data
            spawn_data.initial_speed_x = Fixed(speed_x);
            spawn_data.initial_speed_y = Fixed(speed_y);

            // Call ProjectileFire_Single(worm, fire_params) with EDI=&spawn_data
            call_projectile_fire_single(
                &raw const spawn_data,
                worm,
                fire_params,
                rb(va::PROJECTILE_FIRE_SINGLE),
            );
        }

        let _ = log_line(&format!(
            "[Weapon] ProjectileFire: worm=0x{:08X} shots={}",
            worm as u32, shot_count,
        ));
    }
}

/// Fixed-point 16.16 multiply: (a * b) >> 16, using full 64-bit intermediate.
#[inline(always)]
fn fixed_mul(a: i32, b: i32) -> i32 {
    openwa_core::fixed::Fixed(a)
        .mul_raw(openwa_core::fixed::Fixed(b))
        .0
}

/// Rust implementation of CreateArrow (0x51ED90).
///
/// Allocates a ArrowEntity (0x168 bytes), calls the original stdcall constructor.
/// Used by Shotgun and Longbow.
pub unsafe fn create_arrow(
    worm: *mut WormEntity,
    fire_params: *const WeaponFireParams,
    local_struct: *const u8,
) {
    unsafe {
        use crate::rebase::rb;
        use crate::task::SharedDataTable;
        use crate::wa_alloc::wa_malloc;

        let world = &mut *{
            let this = worm as *const BaseEntity;
            (*this).world
        };

        // Pool capacity check: pool_count + 2 must be <= 700
        if world.object_pool_count + 2 > 700 {
            world.show_pool_overflow_warning();
            return;
        }

        // Look up parent WorldRootEntity via SharedData (key 0, 0x19)
        let table = SharedDataTable::from_task(worm as *const BaseEntity);
        let parent = table.lookup(0, 0x19);

        // Allocate ArrowEntity (0x168 bytes)
        let buffer = wa_malloc(0x168);
        if buffer.is_null() {
            return;
        }
        core::ptr::write_bytes(buffer, 0, 0x148);

        // ArrowEntity::Constructor — stdcall(this, parent, fire_params, local_struct), RET 0x10
        let ctor: unsafe extern "stdcall" fn(*mut u8, *mut u8, *const WeaponFireParams, *const u8) =
            core::mem::transmute(rb(va::ARROW_ENTITY_CTOR));
        ctor(buffer, parent as *mut u8, fire_params, local_struct);

        let _ = log_line(&format!(
            "[Weapon] CreateArrow: worm=0x{:08X} arrow=0x{:08X}",
            worm as u32, buffer as u32,
        ));
    }
}
