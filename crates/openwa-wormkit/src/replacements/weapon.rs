//! Weapon hooks.
//!
//! Replaces WA.exe functions that manage weapon ammo in the TeamArenaState area (DDGame + 0x4628):
//! - GetAmmo (0x5225E0): query ammo count with delay/phase checks
//! - AddAmmo (0x522640): add ammo to a weapon slot
//! - SubtractAmmo (0x522680): decrement ammo count
//! - CountAliveWorms (0x5225A0): check if >1 worm alive on team
//! - FireWeapon (0x51EE60): full Rust dispatch
//!
//! Passthrough hooks on fire sub-functions (log params, call original):
//! - CreateWeaponProjectile (0x51E0F0): thiscall(ECX=worm, fire_params, local_struct)
//! - ProjectileFire (0x51DFB0): stdcall(worm, fire_params, local_struct)
//! - StrikeFire (0x51E2C0): stdcall(worm, &subtype_34, local_struct) — AirStrike/NapalmStrike etc.
//! - PlacedExplosive (0x51EC80): usercall(ECX=local_struct, EDX=worm, fire_params)

use core::sync::atomic::{AtomicU32, Ordering};

use openwa_core::address::va;
use openwa_core::audio::KnownSoundId;
use openwa_core::engine::ddgame::{self, TeamArenaRef};
use openwa_core::fixed::Fixed;
use openwa_core::game::weapon::{WeaponEntry, WeaponFireParams, WeaponSpawnData};
use openwa_core::game::Weapon;
use openwa_core::log::log_line;
use openwa_core::task::turn_game::CTaskTurnGame;
use openwa_core::task::CTask;
use openwa_core::task::worm::{CTaskWorm, WormState};

use crate::hook::{self, usercall_trampoline};
use crate::replacements::weapon_release::WeaponReleaseContext;

// ============================================================
// AddAmmo replacement (0x522640)
// ============================================================
// __usercall: EAX = team_index, EDX = amount, [ESP+4] = team_info_base, [ESP+8] = weapon_id
// RET 0x8

unsafe extern "cdecl" fn add_ammo_impl(
    team_index: u32,
    amount: i32,
    arena: TeamArenaRef,
    weapon_id: u32,
) {
    let (alliance, wid) = arena.weapon_slot_key(team_index as usize, weapon_id);
    let state = arena.state_mut();
    let ammo = state.get_ammo(alliance, wid);
    if ammo >= 0 {
        if amount < 0 {
            *state.ammo_mut(alliance, wid) = -1;
        } else {
            *state.ammo_mut(alliance, wid) = ammo + amount;
        }
    }
}

usercall_trampoline!(fn trampoline_add_ammo; impl_fn = add_ammo_impl;
    regs = [eax, edx]; stack_params = 2; ret_bytes = "0x8");

// ============================================================
// SubtractAmmo replacement (0x522680)
// ============================================================
// __usercall: EAX = team_index, ECX = team_info_base, [ESP+4] = weapon_id
// RET 0x4

unsafe extern "cdecl" fn subtract_ammo_impl(team_index: u32, arena: TeamArenaRef, weapon_id: u32) {
    let (alliance, wid) = arena.weapon_slot_key(team_index as usize, weapon_id);
    let state = arena.state_mut();
    let ammo = state.get_ammo(alliance, wid);
    if ammo > 0 {
        *state.ammo_mut(alliance, wid) = ammo - 1;
    }
}

usercall_trampoline!(fn trampoline_subtract_ammo; impl_fn = subtract_ammo_impl;
    regs = [eax, ecx]; stack_params = 1; ret_bytes = "0x4");

// ============================================================
// GetAmmo replacement (0x5225E0)
// ============================================================
// __usercall: EAX = team_index, ESI = team_info_base, EDX = weapon_id
// plain RET, returns EAX = ammo count

unsafe extern "cdecl" fn get_ammo_impl(
    team_index: u32,
    arena: TeamArenaRef,
    weapon_id: u32,
) -> u32 {
    let (alliance, wid) = arena.weapon_slot_key(team_index as usize, weapon_id);
    let state = arena.state();

    // Check weapon delay
    if state.get_delay(alliance, wid) != 0 {
        if state.game_mode_flag == 0 {
            return 0;
        }
        // In sudden death (phase >= 484), delayed weapons return 0
        // unless it's Teleport (weapon 0x28)
        if state.game_phase >= ddgame::GAME_PHASE_SUDDEN_DEATH
            && weapon_id != Weapon::Teleport as u32
        {
            return 0;
        }
    }

    // SelectWorm (0x3B) requires >1 alive worm on the team
    if state.game_phase >= ddgame::GAME_PHASE_NORMAL_MIN && weapon_id == Weapon::SelectWorm as u32
        && count_alive_worms_impl(team_index, arena) == 0 {
            return 0;
        }

    state.get_ammo(alliance, wid) as u32
}

usercall_trampoline!(fn trampoline_get_ammo; impl_fn = get_ammo_impl;
    regs = [eax, esi, edx]);

// ============================================================
// CountAliveWorms replacement (0x5225A0)
// ============================================================
// __usercall: EAX = team_index, ECX = base
// plain RET, returns EAX = bool (1 if >1 worm alive on team)

unsafe extern "cdecl" fn count_alive_worms_impl(team_index: u32, arena: TeamArenaRef) -> u32 {
    let header = arena.team_header(team_index as usize);
    let worm_count = header.worm_count;
    let mut alive = 0i32;
    for w in 1..=worm_count as usize {
        if arena.team_worm(team_index as usize, w).health > 0 {
            alive += 1;
        }
    }
    if alive > 1 {
        1
    } else {
        0
    }
}

usercall_trampoline!(fn trampoline_count_alive_worms; impl_fn = count_alive_worms_impl;
    regs = [eax, ecx]);

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
pub(crate) unsafe fn fire_weapon(
    entry: *const WeaponEntry,
    ctx: *const WeaponReleaseContext,
    worm: *mut CTaskWorm,
) {
    use openwa_core::rebase::rb;

    let fire_type = (*entry).fire_type;
    let fire_method = (*entry).fire_method;
    let fire_params = &raw const (*entry).fire_params;
    // Log weapon fire
    let weapon = (*worm).selected_weapon;
    let _ = log_line(&format!(
        "[Weapon] FireWeapon: {:?} (id={}) type={} sub34={} sub38={}",
        weapon, weapon as u32, fire_type, (*entry).special_subtype, fire_method
    ));

    CTaskWorm::set_fire_complete_raw(worm, 0);

    use openwa_core::game::weapon::{FireType, FireMethod};
    match FireType::try_from(fire_type) {
        Ok(FireType::Projectile) => match FireMethod::try_from(fire_method) {
            Ok(FireMethod::PlacedExplosive) => call_fire_placed_explosive(worm, fire_params, ctx, rb(0x51EC80)),
            Ok(FireMethod::ProjectileFire) => call_fire_stdcall3(worm, fire_params, ctx, rb(0x51DFB0)),
            Ok(FireMethod::CreateWeaponProjectile) => call_fire_thiscall2(worm, fire_params, ctx, rb(0x51E0F0)),
            Ok(FireMethod::CreateArrow) => call_fire_thiscall2(worm, fire_params, ctx, rb(0x51ED90)),
            _ => {}
        },
        Ok(FireType::Rope) => match FireMethod::try_from(fire_method) {
            Ok(FireMethod::PlacedExplosive) => call_fire_stdcall3(worm, fire_params, ctx, rb(0x51E1C0)), // RopeType1
            Ok(FireMethod::ProjectileFire) => call_fire_thiscall2(worm, fire_params, ctx, rb(0x51E0F0)),
            Ok(FireMethod::CreateWeaponProjectile) => call_fire_stdcall3(worm, fire_params, ctx, rb(0x51E240)), // RopeType3
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

    CTaskWorm::set_fire_complete_raw(worm, 1);
}

// ── Sub-function bridges ────────────────────────────────────
// All bridges save/restore ESI+EDI, set ESI=EDI=worm, then call.
// This preserves LLVM's callee-saved registers while providing
// the usercall context that sub-functions expect.

/// Bridge: PlacedExplosive — usercall(ECX=local_struct, EDX=worm, [ESP+4]=fire_params), RET 0x4.
/// Args: (worm, fire_params, ctx, addr).
#[unsafe(naked)]
unsafe extern "C" fn call_fire_placed_explosive(
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _ctx: *const WeaponReleaseContext, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Set up usercall registers
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov edx, [esp+16]",  // EDX = worm
        "mov ecx, [esp+24]",  // ECX = local_struct
        "mov ebx, [esp+28]",  // addr
        "push [esp+20]",      // fire_params (stack param)
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
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _ctx: *const WeaponReleaseContext, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov esi, [esp+16]",  // worm (3 saves=12 + ret=4 = 16)
        "mov edi, [esp+16]",
        "mov ebx, [esp+28]",  // addr (saves=12 + ret=4 + 3 args=12 = 28)
        "push [esp+24]",      // local_struct
        "push [esp+24]",      // params (shifted +4)
        "push [esp+24]",      // worm (shifted +8)
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: CreateWeaponProjectile — thiscall(ECX=worm, fire_params, local_struct), RET 0x8.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_thiscall2(
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _ctx: *const WeaponReleaseContext, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov esi, [esp+16]",  // worm
        "mov edi, [esp+16]",
        "mov ecx, [esp+16]",  // ECX = worm (this)
        "mov ebx, [esp+28]",  // addr
        "push [esp+24]",      // local_struct
        "push [esp+24]",      // params (shifted +4)
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
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _ctx: *const WeaponReleaseContext, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov esi, [esp+16]",  // worm
        "mov edi, [esp+16]",
        "mov ebx, [esp+28]",  // addr
        "push [esp+24]",      // local_struct
        "push [esp+24]",      // params (shifted +4)
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
unsafe fn fire_weapon_special(
    subtype: i32, entry: *const WeaponEntry, worm: *mut CTaskWorm, ctx: *const WeaponReleaseContext,
) {
    use openwa_core::game::weapon::SpecialFireSubtype as S;
    use openwa_core::rebase::rb;

    // Pointer to fire_method field, reinterpreted as fire params pointer for Girder
    let params_38_ptr = &raw const (*entry).fire_method as *const WeaponFireParams;

    match S::try_from(subtype) {
        Ok(S::FirePunch) => CTaskWorm::set_state_raw(worm, WormState::FirePunch),
        Ok(S::BaseballBat) => fire_drill(worm, ctx as *const u8),
        Ok(S::DragonBall) => fire_dragon_ball(worm, params_38_ptr, ctx as *const u8),
        Ok(S::Kamikaze) => CTaskWorm::set_state_raw(worm, WormState::Kamikaze),
        Ok(S::SuicideBomber) => CTaskWorm::set_state_raw(worm, WormState::SuicideBomber),
        Ok(S::Unknown6) => CTaskWorm::set_state_raw(worm, WormState::Unknown_0x70),
        Ok(S::PneumaticDrill) => CTaskWorm::set_state_raw(worm, WormState::PneumaticDrill),
        Ok(S::Prod) => fire_prod(worm, ctx as *const u8),
        Ok(S::Teleport) => fire_teleport(worm),
        Ok(S::Blowtorch) => CTaskWorm::set_state_raw(worm, WormState::Blowtorch),
        Ok(S::Parachute) => {} // TODO: parachute handler
        Ok(S::Surrender) => fire_surrender(worm),
        Ok(S::MailMineMole) => fire_mail_mine_mole(worm),
        Ok(S::NuclearTest) => {
            if can_fire_subtype16((*worm).state()) {
                fire_nuclear_test(worm);
            } else {
                CTaskWorm::set_state_raw(worm, WormState::TeleportCancelled_Maybe);
            }
        }
        Ok(S::Girder) => fire_girder(worm),
        Ok(S::Unknown18) => CTaskWorm::set_state_raw(worm, WormState::Unknown_0x72),
        Ok(S::SkipGo) => fire_skip_go(worm, entry),
        Ok(S::Freeze) => fire_freeze(worm),
        Ok(S::SelectWorm) => fire_select_worm(worm),
        Ok(S::ScalesOfJustice) => fire_scales_of_justice(worm),
        Ok(S::JetPack) => CTaskWorm::set_state_raw(worm, WormState::WeaponAimed_Maybe),
        Ok(S::Armageddon) => call_fire_usercall(entry as *const (), worm, rb(0x51EA60)),
        _ => {}
    }
}

/// Worm state validity check for subtype 16 — pure Rust port of 0x516930.
/// Used by Nuclear Test to gate firing.
fn can_fire_subtype16(state: u32) -> bool {
    state == WormState::WeaponAimed_Maybe as u32
        || (WormState::AimingAngle_Maybe as u32..=WormState::PreFire_Maybe as u32).contains(&state)
}

// ── Pure Rust fire handlers (no bridge needed) ──────────────

/// Look up the CTaskTurnGame entity for a worm via SharedData.
///
/// The entity at key (0, 0x14) is a CTaskTurnGame (inherits CTaskTeam).
/// Returns null if not found.
pub(crate) unsafe fn lookup_turn_game(worm: *const CTaskWorm) -> *mut openwa_core::task::CTaskTurnGame {
    use openwa_core::task::SharedDataTable;

    let table = SharedDataTable::from_task(worm as *const CTask);
    table.lookup(0, 0x14) as *mut openwa_core::task::CTaskTurnGame
}

/// Surrender (subtype 13) — sends message 0x2B (TaskMessage::Surrender) to
/// CTaskTurnGame via vtable dispatch.
///
/// TurnGame::HandleMessage (0x55DC00) delegates to CTaskTeam (0x557310) for
/// the broadcast, then handles end-turn logic and surrender sound.
#[inline(never)]
unsafe fn fire_surrender(worm: *mut CTaskWorm) {
    let team = lookup_turn_game(worm);
    if team.is_null() {
        return;
    }

    let mut buf = [0u8; 0x40C];
    buf[0..4].copy_from_slice(&(*worm).team_index.to_ne_bytes());

    // Dispatch through vtable — hits CTaskTurnGame::HandleMessage (0x55DC00)
    // which delegates to CTaskTeam (0x557310) and handles end-turn/sound.
    CTaskTurnGame::handle_message_raw(
        team,
        worm as *mut openwa_core::task::CTask,
        0x2B,
        4,
        buf.as_ptr(),
    );
}

/// Send a message to CTaskTurnGame via SharedData lookup + vtable dispatch.
///
/// Shared pattern: look up CTaskTurnGame at key (0, 0x14), write team_index
/// into a 0x40C-byte buffer, and call HandleMessage (vtable slot 2).
///
/// The 0x40C-byte local buffer is passed as data pointer; team_index is written
/// at buf+0x00 to identify which team fired.
unsafe fn fire_send_team_message(worm: *mut CTaskWorm, msg_type: u32) {
    let team = lookup_turn_game(worm);
    if team.is_null() {
        return;
    }

    let mut buf = [0u8; 0x40C];
    let team_index = (*worm).team_index;
    buf[0..4].copy_from_slice(&team_index.to_ne_bytes());

    CTaskTurnGame::handle_message_raw(team,
        worm as *mut openwa_core::task::CTask,
        msg_type,
        4,
        buf.as_ptr(),
    );
}

/// Select Worm (subtype 21) — pure Rust replacement for 0x51EBE0.
///
/// Sends message 0x5D to CTaskTurnGame with buf = [8, team_index, ...].
unsafe fn fire_select_worm(worm: *mut CTaskWorm) {
    let team = lookup_turn_game(worm);
    if team.is_null() {
        return;
    }

    let mut buf = [0u8; 0x40C];
    buf[0..4].copy_from_slice(&8u32.to_ne_bytes());
    buf[4..8].copy_from_slice(&(*worm).team_index.to_ne_bytes());

    CTaskTurnGame::handle_message_raw(team,
        worm as *mut openwa_core::task::CTask,
        0x5D,
        0x408,
        buf.as_ptr(),
    );
}

/// Skip Go (subtype 19) — pure Rust replacement for 0x51E8C0.
///
/// Toggles a bit in the team's `TeamHeader.turn_action_flags` (+0x7C).
/// Bit position comes from weapon entry's fire_params.
/// In game_version > 0x1C: toggles (set/clear). Otherwise: always sets.
unsafe fn fire_skip_go(worm: *const CTaskWorm, entry: *const WeaponEntry) {
    use openwa_core::engine::ddgame::TeamArenaRef;
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let game_version = (*(*ddgame).game_info).game_version;
    let team_index = (*worm).team_index as usize;

    let bit_index = ((*entry).fire_params.shot_count & 0x1F) as u32;
    let bit = 1u32 << bit_index;

    let arena = TeamArenaRef::from_ptr(&raw mut (*ddgame).team_arena);
    let header = arena.team_header_mut(team_index);
    let flags = header.turn_action_flags;

    if game_version > 0x1C && (flags & bit) != 0 {
        header.turn_action_flags = flags & !bit;
    } else {
        header.turn_action_flags = flags | bit;
    }
}

/// Freeze weapon (subtype 20) — pure Rust replacement for 0x51E600.
///
/// Sends message 0x29 (TaskMessage::Freeze) to CTaskTurnGame, then increments
/// WormEntry.turn_action_counter_Maybe by 14 (0x0E).
unsafe fn fire_freeze(worm: *mut CTaskWorm) {
    use openwa_core::engine::ddgame::TeamArenaRef;

    fire_send_team_message(worm, 0x29);

    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let arena = TeamArenaRef::from_ptr(&raw mut (*ddgame).team_arena);
    let team_index = (*worm).team_index as usize;
    let worm_index = (*worm).worm_index as usize;
    let entry = arena.team_worm_mut(team_index, worm_index);
    entry.turn_action_counter_Maybe += 14;
}

/// Mail/Mine/Mole (subtype 14) — pure Rust replacement for 0x51E670.
///
/// Conditionally calls worm->vtable[0xE](0x65) based on game version and worm state,
/// then sends message 0x28 to CTaskTurnGame, then increments
/// WormEntry.turn_action_counter_Maybe by 7.
///
/// Version check logic (from disassembly at 0x51E670):
/// - version < 2: call vtable[0xE](0x65)
/// - 2 <= version < 5: skip vtable call
/// - version >= 5 && worm state == 0x7D: call vtable
/// - version >= 5 && worm state == 0x78 && version < 8: call vtable
/// - otherwise: skip
unsafe fn fire_mail_mine_mole(worm: *mut CTaskWorm) {
    use openwa_core::engine::ddgame::TeamArenaRef;
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let version = (*ddgame).version_flag_4;
    let worm_state = (*worm).state();

    let should_call_vtable = version < 2
        || (version >= 5
            && (worm_state == WormState::PreFire_Maybe as u32
                || (worm_state == WormState::WeaponAimed_Maybe as u32 && version < 8)));

    if should_call_vtable {
        CTaskWorm::set_state_raw(worm,WormState::Idle);
    }

    fire_send_team_message(worm, 0x28);

    let arena = TeamArenaRef::from_ptr(&raw mut (*ddgame).team_arena);
    let team_index = (*worm).team_index as usize;
    let worm_index = (*worm).worm_index as usize;
    let entry = arena.team_worm_mut(team_index, worm_index);
    entry.turn_action_counter_Maybe += 7;
}

/// Teleport (subtype 10) — pure Rust port of 0x51E710.
///
/// If CTaskWorm+0x208 == 0: set state to 0x6F (AirStrikePending_Maybe).
/// Otherwise: play sound, spawn visual effect, update position, compute
/// new state based on game version, clear action fields.
///
/// Convention: usercall(EAX=worm, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_teleport(worm: *mut CTaskWorm) {
    use openwa_core::rebase::rb;

    if (*worm)._unknown_208 == 0 {
        CTaskWorm::set_state_raw(worm,WormState::AirStrikePending_Maybe);
        return;
    }

    // Play air strike sound: usercall(EDI=worm) + stdcall(sound_id=0x36, volume=0x10000, flags=3)
    call_worm_play_sound(worm, 0x36, 0x10000, 3, rb(0x515020));

    // Spawn visual effect: usercall(EAX=0x80000, ECX=x) + stdcall(y, 0, 0, 0, 600, 0x10000, 0x1999)
    let fire_x = (*worm).weapon_param_1;
    let fire_y = (*worm).weapon_param_2;
    call_spawn_effect(fire_x, fire_y, rb(0x547C30));

    // Temporarily swap worm+0x34 with worm+0x190, call position update, restore
    let worm_raw = worm as *mut u8;
    let saved_34 = *(worm_raw.add(0x34) as *const i32);
    *(worm_raw.add(0x34) as *mut i32) = (*worm)._unknown_190;
    call_position_update(worm, fire_x, fire_y, rb(0x4FE070));
    *(worm_raw.add(0x34) as *mut i32) = saved_34;

    // Compute new state: version < 455 → Idle (0x65), else → 0x8B
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let game_version = (*(*ddgame).game_info).game_version;
    let new_state = if game_version < 0x1C7 {
        WormState::Idle
    } else {
        WormState::Unknown_0x8B
    };
    CTaskWorm::set_state_raw(worm,new_state);

    // Clear action fields
    *(worm_raw.add(0x48) as *mut i32) = 0;
    (*worm)._unknown_208 = 0;
    (*worm)._unknown_198 = 0;
    (*worm)._unknown_19c = 0;
    (*worm).facing_direction_inv = 0;

    // FUN_0050D450: usercall(ESI=worm), cleanup/landing check
    call_worm_landing_check(worm, rb(0x50D450));

    // Debug log block (ddgame+0x8144) omitted — only writes to debug file
}

/// Nuclear Test (subtype 16) — pure Rust port of 0x51EB00.
///
/// Sends three messages to CTaskTurnGame: RaiseWater (0x59), NukeBlast (0x5A),
/// PoisonWorm (0x51), and plays two sounds. Gated by can_fire_subtype16.
///
/// Convention: usercall(EAX=worm_state, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_nuclear_test(worm: *mut CTaskWorm) {
    use crate::replacements::sound;

    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let entry = &*(*worm).active_weapon_entry;
    let team = lookup_turn_game(worm);

    // Message 0x59 (RaiseWater): buf[0]=fire_method, buf[4]=8
    if !team.is_null() {
        let mut buf = [0u8; 0x40C];
        buf[0..4].copy_from_slice(&entry.fire_method.to_ne_bytes());
        buf[4..8].copy_from_slice(&8i32.to_ne_bytes());
        CTaskTurnGame::handle_message_raw(team,worm as *mut openwa_core::task::CTask, 0x59, 0x408, buf.as_ptr());
    }

    // Message 0x5A (NukeBlast): buf[0]=8
    let team = lookup_turn_game(worm);
    if !team.is_null() {
        let mut buf = [0u8; 0x40C];
        buf[0..4].copy_from_slice(&8i32.to_ne_bytes());
        CTaskTurnGame::handle_message_raw(team,worm as *mut openwa_core::task::CTask, 0x5A, 0x408, buf.as_ptr());
    }

    // PlaySoundGlobal(IndianAnthem, 8, 0x10000, 0x10000)
    sound::queue_sound(ddgame, KnownSoundId::IndianAnthem.into(), 8, Fixed::ONE, Fixed::ONE);

    // Message 0x51 (PoisonWorm): buf[0]=shot_count, buf[4]=2, buf[8]=team_index
    let team = lookup_turn_game(worm);
    if !team.is_null() {
        let mut buf = [0u8; 0x40C];
        buf[0..4].copy_from_slice(&entry.fire_params.shot_count.to_ne_bytes());
        buf[4..8].copy_from_slice(&2i32.to_ne_bytes());
        buf[8..12].copy_from_slice(&(*worm).team_index.to_ne_bytes());
        CTaskTurnGame::handle_message_raw(team,worm as *mut openwa_core::task::CTask, 0x51, 0x408, buf.as_ptr());
    }

    // PlaySoundGlobal(NukeFlash, 5, 0x10000, 0x10000)
    sound::queue_sound(ddgame, KnownSoundId::NukeFlash.into(), 5, Fixed::ONE, Fixed::ONE);
}

// ── Air Strike sub-function bridges ───────────────────────

/// Bridge: FUN_00515020 — usercall(EDI=worm) + stdcall(sound_id, volume, flags). RET 0xC.
#[unsafe(naked)]
unsafe extern "C" fn call_worm_play_sound(
    _worm: *mut CTaskWorm, _sound_id: u32, _volume: u32, _flags: u32, _addr: u32,
) {
    core::arch::naked_asm!(
        "push edi",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov edi, [esp+8]",    // worm
        "mov eax, [esp+24]",   // addr
        "push [esp+20]",       // flags
        "push [esp+20]",       // volume (shifted +4)
        "push [esp+20]",       // sound_id (shifted +8)
        "call eax",
        "pop edi",
        "ret",
    );
}

/// Bridge: FUN_00547C30 — usercall(EAX=0x80000, ECX=x) + stdcall(y, 0,0,0, 600, 0x10000, 0x1999).
#[unsafe(naked)]
unsafe extern "C" fn call_spawn_effect(_x: i32, _y: i32, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov ecx, [esp+8]",    // x
        "mov ebx, [esp+16]",   // addr
        "push 0x1999",
        "push 0x10000",
        "push 0x258",          // 600
        "push 0",
        "push 0",
        "push 0",
        "push [esp+36]",       // y (8 + 4 + 6*4 = 36)
        "mov eax, 0x80000",
        "call ebx",
        "pop ebx",
        "ret",
    );
}

/// Bridge: FUN_004FE070 — usercall(ESI=worm, EDI=y) + stdcall(x). Plain RET.
#[unsafe(naked)]
unsafe extern "C" fn call_position_update(
    _worm: *mut CTaskWorm, _x: i32, _y: i32, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov esi, [esp+16]",   // worm
        "mov edi, [esp+24]",   // y
        "mov ebx, [esp+28]",   // addr
        "push [esp+20]",       // x
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: FUN_0050D450 — usercall(ESI=worm). Plain RET.
#[unsafe(naked)]
unsafe extern "C" fn call_worm_landing_check(_worm: *mut CTaskWorm, _addr: u32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, [esp+8]",    // worm (1 save + ret = 8)
        "call [esp+12]",       // addr (8 + 4 = 12)
        "pop esi",
        "ret",
    );
}

/// Scales of Justice (subtype 22) — pure Rust port of 0x51EC30.
///
/// Sends message 0x5E to CTaskTurnGame, then plays a sound.
/// Convention: usercall(EAX=entry, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_scales_of_justice(worm: *mut CTaskWorm) {
    use openwa_core::rebase::rb;

    // Send message 0x5E to CTaskTurnGame
    let team = lookup_turn_game(worm);
    if !team.is_null() {
        CTaskTurnGame::handle_message_raw(team,
            worm as *mut openwa_core::task::CTask,
            0x5E,
            0,
            core::ptr::null(),
        );
    }

    // Play jet pack sound:
    // FUN_0053EC70: usercall(EDI=0x6CB) + stdcall(timer_obj)
    // FUN_005480F0: usercall(EAX=-21) + stdcall(worm, result, 0x17, &worm_name)
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let sound_val = call_get_sound_val((*ddgame).timer_obj, rb(0x53EC70));
    call_play_sound_usercall(worm, sound_val, 0x17, (*worm).worm_name.as_ptr(), rb(0x5480F0));
}

/// Bridge: FUN_0053EC70 — usercall(EDI=0x6CB) + stdcall(timer_obj). Returns EAX.
#[unsafe(naked)]
unsafe extern "C" fn call_get_sound_val(_timer_obj: *mut u8, _addr: u32) -> u32 {
    core::arch::naked_asm!(
        "push ebx",
        "push edi",
        // Stack: 2 saves(8) + ret(4) = 12 to first arg
        "mov edi, 0x6CB",
        "mov ebx, [esp+16]",   // addr
        "push [esp+12]",       // timer_obj
        "call ebx",
        "pop edi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: usercall(EAX=-21) + stdcall(4 params). Plain RET (callee doesn't clean).
#[unsafe(naked)]
unsafe extern "C" fn call_play_sound_usercall(
    _worm: *mut CTaskWorm, _sound_val: u32, _param3: u32, _name: *const u8, _addr: u32,
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
/// Sends message 0x5B to CTaskTurnGame with weapon/team info, then conditionally
/// sets a gravity center point via FUN_00547E70.
/// Convention: usercall(EAX=entry, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_armageddon(worm: *mut CTaskWorm) {
    use openwa_core::rebase::rb;

    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let game_version = (*(*ddgame).game_info).game_version;

    // Send message 0x5B to CTaskTurnGame with weapon info buffer
    let team = lookup_turn_game(worm);
    if !team.is_null() {
        let mut buf = [0u8; 0x40C];
        // buf[0x04] = 100 (0x64), buf[0x08] = 166 (0xA6), buf[0x0C] = weapon_id, buf[0x10] = team_index
        buf[0x04..0x08].copy_from_slice(&100i32.to_ne_bytes());
        buf[0x08..0x0C].copy_from_slice(&166i32.to_ne_bytes());
        buf[0x0C..0x10].copy_from_slice(&((*worm).selected_weapon as u32).to_ne_bytes());
        buf[0x10..0x14].copy_from_slice(&(*worm).team_index.to_ne_bytes());

        CTaskTurnGame::handle_message_raw(team,
            worm as *mut openwa_core::task::CTask,
            0x5B,
            0x408,
            buf.as_ptr(),
        );
    }

    // If old game version or worm state 0x69, set gravity center
    if game_version < 0x50 || (*worm).state() == 0x69 {
        // Compute half-level center from DDGame+0x77C0/0x77C4 (level dimensions)
        let ddgame_raw = ddgame as *const u8;
        let level_w = *(ddgame_raw.add(0x77C0) as *const i32);
        let level_h = *(ddgame_raw.add(0x77C4) as *const i32);
        // Convert to Fixed16.16 and halve: (value << 16) / 2
        // Original uses SHL 16; CDQ; SUB EAX,EDX; SAR 1 (round-toward-zero divide)
        let half_x = ((level_w << 16) + (if level_w < 0 { 1 } else { 0 })) >> 1;
        let half_y = ((level_h << 16) + (if level_h < 0 { 1 } else { 0 })) >> 1;

        // FUN_00547E70: usercall(ECX=half_x, EDX=half_y) + stdcall(worm), RET 0x4
        type SetGravityCenterFn = unsafe extern "stdcall" fn(*mut CTaskWorm);
        // Need naked bridge for ECX/EDX usercall params
        call_set_gravity_center(worm, half_x, half_y, rb(0x547E70));
    }
}

/// Bridge: usercall(ECX=half_x, EDX=half_y) + stdcall(worm), RET 0x4.
#[unsafe(naked)]
unsafe extern "C" fn call_set_gravity_center(
    _worm: *mut CTaskWorm, _half_x: i32, _half_y: i32, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        // Stack: 1 save(4) + ret(4) = 8 to first arg
        "mov ecx, [esp+12]",   // half_x
        "mov edx, [esp+16]",   // half_y
        "mov ebx, [esp+20]",   // addr
        "push [esp+8]",        // worm (shifted by 0 extra pushes before this)
        "call ebx",            // RET 0x4 cleans worm
        "pop ebx",
        "ret",
    );
}

/// DragonBall (type 4 subtype 3) — pure Rust port of 0x51E350.
///
/// Allocates a CTaskGirder (0xA4 bytes), copies 7 DWORDs from fire_params
/// as constructor arguments, and calls the constructor. The constructor handles
/// actually placing the girder on the landscape.
///
/// Convention: stdcall(worm, fire_params, local_struct), RET 0xC.
unsafe fn fire_dragon_ball(
    worm: *mut CTaskWorm, fire_params: *const WeaponFireParams, local_struct: *const u8,
) {
    use openwa_core::rebase::rb;
    use openwa_core::task::SharedDataTable;
    use openwa_core::wa_alloc::wa_malloc;

    // Look up parent task via SharedData (same key as CreateWeaponProjectile)
    let table = SharedDataTable::from_task(worm as *const CTask);
    let parent = table.lookup(0, 0x19);

    // Allocate CTaskGirder (0xA4 bytes), zero first 0x84
    let buffer = wa_malloc(0xA4);
    if buffer.is_null() {
        return;
    }
    core::ptr::write_bytes(buffer, 0, 0x84);

    // CTaskGirder::Constructor — usercall(EAX=parent) +
    // stdcall(this, 7×fire_param DWORDs, local_struct), RET 0x24.
    // Copy 7 DWORDs from fire_params onto the stack via the naked bridge.
    call_girder_ctor(buffer, parent as *mut u8, fire_params, local_struct, rb(0x550890));
}

/// Bridge: CTaskGirder::Constructor — usercall(EAX=parent) +
/// stdcall(this, 7 DWORDs from fire_params, local_struct), RET 0x24.
///
/// The original copies 7 DWORDs from fire_params onto the stack via REP MOVSD.
/// We replicate this by pushing them individually in reverse order.
#[unsafe(naked)]
unsafe extern "C" fn call_girder_ctor(
    _this: *mut u8, _parent: *mut u8, _fire_params: *const WeaponFireParams,
    _local_struct: *const u8, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        // Args: this=+16, parent=+20, fire_params=+24, local_struct=+28, addr=+32
        "mov esi, [esp+24]",  // ESI = fire_params
        "mov eax, [esp+20]",  // EAX = parent (usercall)
        "mov ebx, [esp+32]",  // EBX = ctor address
        // Push constructor args in reverse: local_struct, 7 DWORDs, this
        "push [esp+28]",      // local_struct (B+28 → stack pos 1)
        "push [esi+24]",      // fire_params[6]
        "push [esi+20]",      // fire_params[5]
        "push [esi+16]",      // fire_params[4]
        "push [esi+12]",      // fire_params[3]
        "push [esi+8]",       // fire_params[2]
        "push [esi+4]",       // fire_params[1]
        "push [esi]",         // fire_params[0] (B-28 → stack pos 8)
        "push [esp+48]",      // this: B-32+48 = B+16 ✓
        "call ebx",           // RET 0x24 cleans 9 params
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: usercall(EAX=eax_val, ESI=worm, EDI=worm), plain RET.
/// Kept for Low Gravity until the codegen UB is resolved.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_usercall(_eax: *const (), _worm: *mut CTaskWorm, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov eax, [esp+16]",
        "mov esi, [esp+20]",
        "mov edi, [esp+20]",
        "mov ebx, [esp+24]",
        "call ebx",
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
/// when CTaskWorm+0x2EC (weapon_param_3) is nonzero.
///
/// Convention: usercall(EAX=worm, ESI=worm, EDI=worm), plain RET.
unsafe fn fire_girder(worm: *mut CTaskWorm) {
    use openwa_core::rebase::rb;

    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let game_version = (*(*ddgame).game_info).game_version;

    // Read girder position and sprite index from CTaskWorm fields
    let girder_x = (*worm).weapon_param_1;
    let girder_y = (*worm).weapon_param_2;
    let girder_sprite = (*worm).weapon_param_3;

    // Choose sound: 0x70 if girder has sprite or old version, else 0x73
    let sound_id: u32 = if girder_sprite != 0 || game_version < 0x21 {
        0x70
    } else {
        0x73
    };

    // Call PlaySoundGlobal — thiscall(ECX=worm, sound_id, 3, 0x10000, 0x10000)
    type PlaySoundGlobalFn = unsafe extern "thiscall" fn(*mut CTaskWorm, u32, i32, i32, i32) -> i32;
    let play_sound: PlaySoundGlobalFn = core::mem::transmute(rb(va::PLAY_SOUND_GLOBAL));
    let result = play_sound(worm, sound_id, 3, 0x10000, 0x10000);

    // If sound was queued, set its position to the girder location
    if result != 0 {
        let queue_count = (*ddgame).sound_queue_count;
        if queue_count > 0 {
            let entry = &mut (*ddgame).sound_queue[(queue_count - 1) as usize];
            entry.is_local = 1;
            entry.secondary_vtable = 0;
            entry.pos_x = girder_x as u32;
            entry.pos_y = girder_y as u32;
        }
    }

    // If girder has a sprite, apply the visual overlay and update counters
    if girder_sprite != 0 {
        // Call PCLandscape vtable[5] to create girder overlay
        let landscape = (*ddgame).landscape as *mut u8;
        let landscape_vt = *(landscape as *const *const usize);
        let girder_visual: unsafe extern "thiscall" fn(*mut u8, i32, i32, *mut u8, *mut u8) =
            core::mem::transmute(*landscape_vt.add(5));
        let sprite1 = (*ddgame).sprite_cache_2[girder_sprite as usize];
        let sprite2 = (*ddgame).sprite_cache_2[19 + girder_sprite as usize];
        girder_visual(landscape, girder_x >> 16, girder_y >> 16, sprite1, sprite2);

        // Increment WormEntry counters
        let arena = openwa_core::engine::ddgame::TeamArenaRef::from_ptr(
            &raw mut (*ddgame).team_arena,
        );
        let team_index = (*worm).team_index as usize;
        let worm_index = (*worm).worm_index as usize;
        let entry = arena.team_worm_mut(team_index, worm_index);
        entry.turn_action_counter_Maybe += 3;
        entry.effect_counter_04_Maybe += 10;
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
    worm: *mut CTaskWorm,
    x: i32, y: i32,
    radius_x: i32, radius_y: i32,
    weapon_type: i32,
    dx: i32, dy: i32,
    p8: i32, p9: i32, p10: i32,
    flags: u32, p12: i32,
) {
    use openwa_core::rebase::rb;
    type SpecialImpactFn = unsafe extern "stdcall" fn(
        *mut CTaskWorm, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, u32, i32,
    );
    let func: SpecialImpactFn = core::mem::transmute(rb(va::SPECIAL_IMPACT));
    func(worm, x, y, radius_x, radius_y, weapon_type, dx, dy, p8, p9, p10, flags, p12);
}

/// Compute version-dependent flags for SpecialImpact.
///
/// Pattern shared by Drill and Prod:
/// - Base flags OR 0x20 if version >= 2
/// - Base flags OR 0x10 if version >= 8
fn special_impact_version_flags(base: u32, version: u8) -> u32 {
    let mut flags = base;
    if version >= 2 { flags |= 0x20; }
    if version >= 8 { flags |= 0x10; }
    flags
}

/// BaseballBat (subtype 2) — pure Rust port of 0x51E3E0.
///
/// Calls SpecialImpact with facing-offset position and scaled direction.
/// The original is usercall(ECX=local_struct, ESI=worm) — the old bridge
/// did not set ECX, so this port also fixes a latent bug.
unsafe fn fire_drill(worm: *mut CTaskWorm, local_struct: *const u8) {
    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let version = (*ddgame).version_flag_4;
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
        worm, x, y,
        0x1A_0000, 0x1E_0000,
        weapon_type, dx, dy,
        6, 0x61, 0x51,
        flags, 1,
    );
}

/// Prod (subtype 9) — pure Rust port of 0x51E480.
///
/// Like Drill but with trig interpolation on the spread angle.
/// Convention: usercall(EDI=worm) + 1 stack param (local_struct), RET 0x4.
unsafe fn fire_prod(worm: *mut CTaskWorm, local_struct: *const u8) {
    use openwa_core::rebase::rb;

    let ddgame = CTask::ddgame_raw(worm as *const CTask);
    let version = (*ddgame).version_flag_4;
    let entry = &*(*worm).active_weapon_entry;
    let shot_count = entry.fire_params.shot_count;
    let spread = entry.fire_params.spread;
    let weapon_type = entry.fire_method;
    let facing = (*worm).facing_direction_2;

    let spawn = &*(local_struct as *const WeaponSpawnData);

    // Convert spread (degrees) to engine angle units: (spread << 16) / 360
    let angle = ((spread as u32) << 16) / 0x168;

    // Interpolated sin/cos lookup (same pattern as projectile_fire_impl)
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
        worm, x, y,
        0xC_0000, 0xC_0000,
        weapon_type, dx, dy,
        0, 0, 0,
        flags, 1,
    );
}

// ── Naked asm bridges ───────────────────────────────────────

/// Bridge: ProjectileFire_Single — usercall(EDI=spawn_data, stack=[worm, fire_params]), RET 0x8.
/// Args: (spawn_data, worm, fire_params, addr).
#[unsafe(naked)]
unsafe extern "C" fn call_projectile_fire_single(
    _spawn_data: *const WeaponSpawnData, _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov edi, [esp+16]",  // EDI = spawn_data
        "mov ebx, [esp+28]",  // addr
        "push [esp+24]",      // fire_params
        "push [esp+24]",      // worm (shifted +4)
        "call ebx",           // RET 0x8 cleans 2 params
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Bridge: CTaskMissile::Constructor — thiscall(ECX=this, parent, fire_params, spawn_data), RET 0xC.
/// Args: (this, parent, fire_params, spawn_data, ctor_addr).
#[unsafe(naked)]
unsafe extern "C" fn call_missile_ctor(
    _this: *mut u8, _parent: *mut u8, _fire_params: *const WeaponFireParams, _spawn_data: *const u8, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        // Stack: 3 saves(12) + ret(4) = 16 to first arg
        "mov ecx, [esp+16]",  // ECX = this (buffer)
        "mov ebx, [esp+32]",  // addr (16 + 4*4 args = 32)
        "push [esp+28]",      // spawn_data
        "push [esp+28]",      // fire_params (shifted +4)
        "push [esp+28]",      // parent (shifted +8)
        "call ebx",           // thiscall: RET 0xC cleans 3 params
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}


// ============================================================
// Passthrough hooks — fire sub-functions
// ============================================================
// These hooks log parameters, then call the original WA function.
// They exist for RE discovery and validation — no behavior change.

static ORIG_CREATE_WEAPON_PROJECTILE: AtomicU32 = AtomicU32::new(0);
static ORIG_PROJECTILE_FIRE: AtomicU32 = AtomicU32::new(0);
static ORIG_STRIKE_FIRE: AtomicU32 = AtomicU32::new(0);
static ORIG_PLACED_EXPLOSIVE: AtomicU32 = AtomicU32::new(0);

/// Full replacement for CreateWeaponProjectile (0x51E0F0).
/// Convention: thiscall(ECX=worm, fire_params, local_struct), RET 0x8.
///
/// Allocates a CTaskMissile, calls the original constructor, and returns.
/// The constructor handles SharedData registration and pool management.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_create_weapon_projectile() {
    core::arch::naked_asm!(
        // thiscall: ECX=worm, stack=[ret, fire_params, local_struct]
        // Push cdecl args for Rust impl
        "push dword ptr [esp+8]",  // local_struct (ret=4 + param1=4 = 8)
        "push dword ptr [esp+8]",  // fire_params (shifted +4)
        "push ecx",                // worm (this)
        "call {impl_fn}",
        "add esp, 12",
        "ret 0x8",                 // clean 2 stack params
        impl_fn = sym create_weapon_projectile_impl,
    );
}

/// Rust implementation of CreateWeaponProjectile.
///
/// Original: 0x51E0F0. Allocates CTaskMissile (0x40C bytes), looks up
/// parent CTaskTurnGame via SharedData, calls the original constructor.
unsafe extern "cdecl" fn create_weapon_projectile_impl(
    worm: *mut CTaskWorm, fire_params: *const WeaponFireParams, local_struct: *const u8,
) {
    use openwa_core::rebase::rb;
    use openwa_core::task::SharedDataTable;
    use openwa_core::wa_alloc::wa_malloc;

    let ddgame = &mut *CTask::ddgame_raw(worm as *const CTask);

    // Pool capacity check: pool_count + 7 must be <= 700
    if ddgame.object_pool_count + 7 > 700 {
        ddgame.show_pool_overflow_warning();
        return;
    }

    // Look up parent CTaskTurnGame via SharedData (key_esi=0, key_edi=0x19)
    let table = SharedDataTable::from_task(worm as *const CTask);
    let parent = table.lookup(0, 0x19);

    // Allocate CTaskMissile (0x40C bytes)
    let buffer = wa_malloc(0x40C);
    if buffer.is_null() {
        return;
    }

    // Zero bytes 0x00..0x3EC (the original only zeros 0x3EC of 0x40C)
    core::ptr::write_bytes(buffer, 0, 0x3EC);

    // Call original CTaskMissile::Constructor
    // thiscall(ECX=buffer, parent, fire_params, local_struct), RET 0xC
    call_missile_ctor(
        buffer,
        parent as *mut u8,
        fire_params,
        local_struct,
        rb(va::CTASK_MISSILE_CTOR),
    );

    let _ = log_line(&format!(
        "[Weapon] CreateWeaponProjectile: worm=0x{:08X} missile=0x{:08X}",
        worm as u32, buffer as u32,
    ));
}

/// Full replacement for ProjectileFire (0x51DFB0).
/// Convention: stdcall(worm, fire_params, local_struct), RET 0xC.
///
/// Builds spawn data with RNG-randomized spread, then calls
/// ProjectileFire_Single per projectile. Used by Uzi, Handgun, Minigun.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_projectile_fire() {
    core::arch::naked_asm!(
        // stdcall: stack=[ret, worm, fire_params, local_struct]
        "push dword ptr [esp+12]",  // local_struct
        "push dword ptr [esp+12]",  // fire_params (shifted +4)
        "push dword ptr [esp+12]",  // worm (shifted +8)
        "call {impl_fn}",
        "add esp, 12",
        "ret 0xC",                  // clean 3 stack params
        impl_fn = sym projectile_fire_impl,
    );
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
unsafe extern "cdecl" fn projectile_fire_impl(
    worm: *mut CTaskWorm, fire_params: *const WeaponFireParams, local_struct: *const WeaponSpawnData,
) {
    use openwa_core::rebase::rb;

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

    let ddgame = &mut *CTask::ddgame_raw(worm as *const CTask);

    for _i in 0..shot_count {
        // Advance game RNG (same LCG as ADVANCE_GAME_RNG at 0x53F320)
        let rng = ddgame.advance_rng();

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
        let speed_x = fixed_mul(cos_val, template_speed_x)
            + fixed_mul(sin_val, template_speed_y);
        let speed_y = fixed_mul(-sin_val, template_speed_x)
            + fixed_mul(cos_val, template_speed_y);

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

/// Fixed-point 16.16 multiply: (a * b) >> 16, using full 64-bit intermediate.
#[inline(always)]
fn fixed_mul(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 16) as i32
}

/// Full replacement for CreateArrow (0x51ED90).
/// Convention: thiscall(ECX=worm, fire_params, local_struct), RET 0x8.
///
/// Allocates a CTaskArrow (0x168 bytes), calls the original stdcall constructor.
/// Used by Shotgun and Longbow.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_create_arrow() {
    core::arch::naked_asm!(
        "push dword ptr [esp+8]",
        "push dword ptr [esp+8]",
        "push ecx",
        "call {impl_fn}",
        "add esp, 12",
        "ret 0x8",
        impl_fn = sym create_arrow_impl,
    );
}

unsafe extern "cdecl" fn create_arrow_impl(
    worm: *mut CTaskWorm, fire_params: *const WeaponFireParams, local_struct: *const u8,
) {
    use openwa_core::rebase::rb;
    use openwa_core::task::SharedDataTable;
    use openwa_core::wa_alloc::wa_malloc;

    let ddgame = &mut *CTask::ddgame_raw(worm as *const CTask);

    // Pool capacity check: pool_count + 2 must be <= 700
    if ddgame.object_pool_count + 2 > 700 {
        ddgame.show_pool_overflow_warning();
        return;
    }

    // Look up parent CTaskTurnGame via SharedData (key 0, 0x19)
    let table = SharedDataTable::from_task(worm as *const CTask);
    let parent = table.lookup(0, 0x19);

    // Allocate CTaskArrow (0x168 bytes)
    let buffer = wa_malloc(0x168);
    if buffer.is_null() {
        return;
    }
    core::ptr::write_bytes(buffer, 0, 0x148);

    // CTaskArrow::Constructor — stdcall(this, parent, fire_params, local_struct), RET 0x10
    let ctor: unsafe extern "stdcall" fn(*mut u8, *mut u8, *const WeaponFireParams, *const u8) =
        core::mem::transmute(rb(va::CTASK_ARROW_CTOR));
    ctor(buffer, parent as *mut u8, fire_params, local_struct);

    let _ = log_line(&format!(
        "[Weapon] CreateArrow: worm=0x{:08X} arrow=0x{:08X}",
        worm as u32, buffer as u32,
    ));
}

/// Passthrough hook for StrikeFire (0x51E2C0).
/// Convention: stdcall(worm, &subtype_34, local_struct), RET 0xC.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_strike_fire() {
    core::arch::naked_asm!(
        "push eax",
        "push ecx",
        "push edx",
        "push dword ptr [esp+24]", // local_struct
        "push dword ptr [esp+24]", // subtype_34_ptr
        "push dword ptr [esp+24]", // worm
        "call {log_fn}",
        "add esp, 12",
        "pop edx",
        "pop ecx",
        "pop eax",
        "jmp [{orig}]",
        log_fn = sym log_strike_fire,
        orig = sym ORIG_STRIKE_FIRE,
    );
}

unsafe extern "cdecl" fn log_strike_fire(worm: u32, subtype_34_ptr: u32, local_struct: u32) {
    let _ = log_line(&format!(
        "[Weapon] StrikeFire: worm=0x{:08X} subtype_34=0x{:08X} local=0x{:08X}",
        worm, subtype_34_ptr, local_struct,
    ));
}

/// Passthrough hook for PlacedExplosive (0x51EC80).
/// Convention: usercall(ECX=local_struct, EDX=worm, [ESP+4]=fire_params), RET 0x4.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_placed_explosive() {
    core::arch::naked_asm!(
        // Save ALL registers — ECX and EDX are usercall params!
        "push eax",
        "push ecx",
        "push edx",
        "push ebx",
        "push esi",
        "push edi",
        "push ebp",
        // Log: push cdecl args
        "push dword ptr [esp+32]", // fire_params (7 saves=28 + ret=4 = 32)
        "push edx",                // worm (still valid, saved above)
        "push ecx",                // local_struct (still valid)
        "call {log_fn}",
        "add esp, 12",
        // Restore ALL (ECX and EDX restored for usercall!)
        "pop ebp",
        "pop edi",
        "pop esi",
        "pop ebx",
        "pop edx",
        "pop ecx",
        "pop eax",
        // Call original (usercall — ECX, EDX, stack all intact)
        "jmp [{orig}]",
        log_fn = sym log_placed_explosive,
        orig = sym ORIG_PLACED_EXPLOSIVE,
    );
}

unsafe extern "cdecl" fn log_placed_explosive(local_struct: u32, worm: u32, fire_params: u32) {
    let _ = log_line(&format!(
        "[Weapon] PlacedExplosive: worm=0x{:08X} local=0x{:08X} params=0x{:08X}",
        worm, local_struct, fire_params,
    ));
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install("AddAmmo", va::ADD_AMMO, trampoline_add_ammo as *const ())?;
        let _ = hook::install("GetAmmo", va::GET_AMMO, trampoline_get_ammo as *const ())?;
        let _ = hook::install("SubtractAmmo", va::SUBTRACT_AMMO, trampoline_subtract_ammo as *const ())?;
        let _ = hook::install("CountAliveWorms", va::COUNT_ALIVE_WORMS, trampoline_count_alive_worms as *const ())?;
        hook::install_trap!("FireWeapon", va::FIRE_WEAPON);

        // Passthrough hooks on fire sub-functions (log + call original)
        let t = hook::install("CreateWeaponProjectile", va::CREATE_WEAPON_PROJECTILE, trampoline_create_weapon_projectile as *const ())?;
        ORIG_CREATE_WEAPON_PROJECTILE.store(t as u32, Ordering::Relaxed);
        let t = hook::install("ProjectileFire", va::PROJECTILE_FIRE, trampoline_projectile_fire as *const ())?;
        ORIG_PROJECTILE_FIRE.store(t as u32, Ordering::Relaxed);
        let t = hook::install("StrikeFire", va::STRIKE_FIRE, trampoline_strike_fire as *const ())?;
        ORIG_STRIKE_FIRE.store(t as u32, Ordering::Relaxed);
        let t = hook::install("PlacedExplosive", va::PLACED_EXPLOSIVE, trampoline_placed_explosive as *const ())?;
        ORIG_PLACED_EXPLOSIVE.store(t as u32, Ordering::Relaxed);
        let _ = hook::install("CreateArrow", va::CREATE_ARROW, trampoline_create_arrow as *const ())?;
    }

    Ok(())
}
