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
use openwa_core::engine::ddgame::{self, TeamArenaRef};
use openwa_core::fixed::Fixed;
use openwa_core::game::weapon::{WeaponEntry, WeaponFireParams, WeaponSpawnData};
use openwa_core::game::Weapon;
use openwa_core::log::log_line;
use openwa_core::task::Task;
use openwa_core::task::worm::CTaskWorm;

use crate::hook::{self, usercall_trampoline};

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
// FireWeapon replacement (0x51EE60)
// ============================================================
// Convention: usercall(EAX=worm, ECX=local_struct) + 1 stack(worm), RET 0x4.
// Note: EAX = *(CTaskWorm+0x36C) = worm self-pointer, so EAX == stack param.
//
// Weapon launch data offsets (relative to worm/EAX):
//   +0x30 = weapon type (1-4)
//   +0x34 = subtype for types 3,4
//   +0x38 = subtype for types 1,2
//   +0x3C = params base
//
// worm = CTaskWorm pointer (ESI in original).
// local_struct = stack-local buffer from WeaponRelease (ECX at call site).
//
// Sub-functions are usercall: ESI=worm, ECX=local_struct (for some).
// We capture all three in our naked trampoline.

static ORIG_FIRE_WEAPON: AtomicU32 = AtomicU32::new(0);

/// Naked trampoline for FireWeapon.
/// Must save ALL callee-saved registers (ESI, EDI, EBX, EBP) because the
/// Rust cdecl impl may clobber them, and WeaponRelease (our caller) depends
/// on them being preserved.
#[unsafe(naked)]
unsafe extern "C" fn trampoline_fire_weapon() {
    core::arch::naked_asm!(
        // Save all callee-saved + EDX
        "push ebx",
        "push esi",
        "push edi",
        "push ebp",
        "push edx",
        // Push cdecl args: (weapon_ctx=EAX, local_struct=ECX, worm)
        // Stack: 5 pushes (20) + ret (4) = 24 to stack param
        "push [esp+24]",      // worm
        "push ecx",           // local_struct
        "push eax",           // weapon_ctx
        "call {impl_fn}",
        "add esp, 12",
        // Restore everything
        "pop edx",
        "pop ebp",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret 0x4",
        impl_fn = sym fire_weapon_impl,
    );
}

/// Rust implementation of FireWeapon dispatch.
///
/// `entry`: EAX = active WeaponEntry pointer (from CTaskWorm+0x36C).
/// `local_struct`: ECX = stack-local buffer from WeaponRelease.
/// `worm`: stack param = CTaskWorm pointer (ESI in original).
///
/// Completion flag at worm+0x3C (CGameTask.subclass_data[12]).
/// Params pointer at entry+0x3C (WeaponEntry.fire_complete) — different object, same offset.
unsafe extern "cdecl" fn fire_weapon_impl(
    entry: *const WeaponEntry, local_struct: *const u8, worm: *mut CTaskWorm,
) {
    use openwa_core::rebase::rb;

    let weapon_type = (*entry).fire_type;
    let subtype_34 = (*entry).fire_subtype_34;
    let subtype_38 = (*entry).fire_subtype_38;
    let fire_params = &raw const (*entry).fire_params;
    // Log weapon fire
    let weapon_id = (*worm).selected_weapon;
    let weapon_name = Weapon::try_from(weapon_id)
        .map(|wp| format!("{:?}", wp))
        .unwrap_or_else(|id| format!("Unknown({})", id));
    let _ = log_line(&format!(
        "[Weapon] FireWeapon: {} (id={}) type={} sub34={} sub38={}",
        weapon_name, weapon_id, weapon_type, subtype_34, subtype_38
    ));

    (*worm).set_fire_complete(0);

    match weapon_type {
        1 => match subtype_38 {
            1 => call_fire_placed_explosive(worm, fire_params, local_struct, rb(0x51EC80)), // PlacedExplosive
            2 => call_fire_stdcall3(worm, fire_params, local_struct, rb(0x51DFB0)),      // Projectile
            3 => call_fire_thiscall2(worm, fire_params, local_struct, rb(0x51E0F0)),     // CreateWeaponProjectile
            4 => call_fire_thiscall2(worm, fire_params, local_struct, rb(0x51ED90)),     // CreateArrow (Shotgun/Longbow)
            _ => {}
        },
        2 => match subtype_38 {
            1 => call_fire_stdcall3(worm, fire_params, local_struct, rb(0x51E1C0)),      // RopeType1
            2 => call_fire_thiscall2(worm, fire_params, local_struct, rb(0x51E0F0)),     // CreateWeaponProjectile
            3 => call_fire_stdcall3(worm, fire_params, local_struct, rb(0x51E240)),      // RopeType3
            _ => {}
        },
        3 => {
            // StrikeFire takes a pointer to the subtype_34 field (reinterpreted as fire params)
            let subtype_34_ptr = &raw const (*entry).fire_subtype_34 as *const WeaponFireParams;
            call_fire_stdcall3(worm, subtype_34_ptr, local_struct, rb(0x51E2C0));
        }
        4 => {
            fire_weapon_special(subtype_34, entry, worm, local_struct);
        }
        _ => {}
    }

    (*worm).set_fire_complete(1);
}

// ── Sub-function bridges ────────────────────────────────────
// All bridges save/restore ESI+EDI, set ESI=EDI=worm, then call.
// This preserves LLVM's callee-saved registers while providing
// the usercall context that sub-functions expect.

/// Bridge: PlacedExplosive — usercall(ECX=local_struct, EDX=worm, [ESP+4]=fire_params), RET 0x4.
/// Args: (worm, fire_params, local_struct, addr).
#[unsafe(naked)]
unsafe extern "C" fn call_fire_placed_explosive(
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _local_struct: *const u8, _addr: u32,
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
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _local: *const u8, _addr: u32,
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
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _local: *const u8, _addr: u32,
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
    _worm: *mut CTaskWorm, _fire_params: *const WeaponFireParams, _local: *const u8, _addr: u32,
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
    subtype: i32, entry: *const WeaponEntry, worm: *mut CTaskWorm, local_struct: *const u8,
) {
    use openwa_core::rebase::rb;

    // Pointer to fire_subtype_38 field, reinterpreted as fire params pointer for Girder
    let params_38_ptr = &raw const (*entry).fire_subtype_38 as *const WeaponFireParams;

    match subtype {
        // Blowtorch
        1 => (*worm).set_state(0x6C),
        // Pneumatic Drill (pure Rust)
        2 => fire_drill(worm, local_struct),
        // Girder
        3 => call_fire_stdcall3(worm, params_38_ptr, local_struct, rb(0x51E350)),
        // Baseball Bat
        4 => (*worm).set_state(0x6D),
        // Fire Punch
        5 => (*worm).set_state(0x75),
        // Dragon Ball
        6 => (*worm).set_state(0x70),
        // Kamikaze
        8 => (*worm).set_state(0x6E),
        // Prod (pure Rust)
        9 => fire_prod(worm, local_struct),
        // Air Strike (EAX=worm)
        10 => call_fire_usercall(worm as *const (), worm, rb(0x51E710)),
        // Scales of Justice
        11 => (*worm).set_state(0x71),
        // Napalm Strike
        13 => fire_send_team_message(worm, 0x2B),
        // Mail/Mine/Mole (pure Rust)
        14 => fire_mail_mine_mole(worm),
        // Teleport: check worm state, then execute or cancel
        16 => {
            let worm_state = (*worm).state();
            if can_teleport(worm_state) {
                call_fire_usercall(worm_state as *const (), worm, rb(0x51EB00));
            } else {
                (*worm).set_state(0x74);
            }
        }
        // Freeze (EAX=worm)
        17 => call_fire_usercall(worm as *const (), worm, rb(0x51E920)),
        // Suicide Bomber
        18 => (*worm).set_state(0x72),
        // Skip Go (pure Rust)
        19 => fire_skip_go(worm, entry),
        // Surrender (pure Rust)
        20 => fire_surrender(worm),
        // Select Worm (pure Rust)
        21 => fire_select_worm(worm),
        // Jet Pack (EAX=entry)
        22 => call_fire_usercall(entry as *const (), worm, rb(0x51EC30)),
        // Magic Bullet
        23 => (*worm).set_state(0x78),
        // Low Gravity (EAX=entry)
        24 => call_fire_usercall(entry as *const (), worm, rb(0x51EA60)),
        _ => {}
    }
}

/// Teleport validity check — pure Rust port of 0x516930.
/// Returns true if the worm's current state allows teleportation.
/// Valid states: 0x78 (idle-aim), 0x7B-0x7D (pre-fire range).
fn can_teleport(state: u32) -> bool {
    state == 0x78 || (0x7B..=0x7D).contains(&state)
}

// ── Pure Rust fire handlers (no bridge needed) ──────────────

/// Look up the CTaskTeam entity for a worm via SharedData.
///
/// Returns null if not found.
unsafe fn lookup_team_task(worm: *const CTaskWorm) -> *mut openwa_core::task::CTaskTeam {
    use openwa_core::task::{SharedDataTable, Task};

    let table = SharedDataTable::from_task((*worm).as_task_ptr());
    // CTaskTeam is registered with key (0, 0x14) in SharedData
    table.lookup(0, 0x14) as *mut openwa_core::task::CTaskTeam
}

/// Send a message to the worm's CTaskTeam entity via SharedData lookup.
///
/// Pattern shared by Napalm Strike (msg 0x2B), Surrender (msg 0x29), etc.
/// Looks up CTaskTeam, then calls HandleMessage (vtable slot 2).
///
/// The 0x40C-byte local buffer is passed as data pointer; team_index is written
/// at buf+0x00 to identify which team fired.
unsafe fn fire_send_team_message(worm: *mut CTaskWorm, msg_type: u32) {
    let team = lookup_team_task(worm);
    if team.is_null() {
        return;
    }

    let mut buf = [0u8; 0x40C];
    let team_index = (*worm).team_index;
    buf[0..4].copy_from_slice(&team_index.to_ne_bytes());

    (*team).handle_message(
        (*worm).as_task_ptr_mut(),
        msg_type,
        4,
        buf.as_ptr(),
    );
}

/// Select Worm (subtype 21) — pure Rust replacement for 0x51EBE0.
///
/// Sends message 0x5D to CTaskTeam with buf = [8, team_index, ...].
unsafe fn fire_select_worm(worm: *mut CTaskWorm) {
    let team = lookup_team_task(worm);
    if team.is_null() {
        return;
    }

    let mut buf = [0u8; 0x40C];
    buf[0..4].copy_from_slice(&8u32.to_ne_bytes());
    buf[4..8].copy_from_slice(&(*worm).team_index.to_ne_bytes());

    (*team).handle_message(
        (*worm).as_task_ptr_mut(),
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
    let ddgame = (*worm).ddgame();
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

/// Surrender (subtype 20) — pure Rust replacement for 0x51E600.
///
/// Sends message 0x29 to CTaskTeam with buf = [team_index], then increments
/// WormEntry.turn_action_counter_Maybe by 14 (0x0E).
unsafe fn fire_surrender(worm: *mut CTaskWorm) {
    use openwa_core::engine::ddgame::TeamArenaRef;

    fire_send_team_message(worm, 0x29);

    let ddgame = (*worm).ddgame();
    let arena = TeamArenaRef::from_ptr(&raw mut (*ddgame).team_arena);
    let team_index = (*worm).team_index as usize;
    let worm_index = (*worm).worm_index as usize;
    let entry = arena.team_worm_mut(team_index, worm_index);
    entry.turn_action_counter_Maybe += 14;
}

/// Mail/Mine/Mole (subtype 14) — pure Rust replacement for 0x51E670.
///
/// Conditionally calls worm->vtable[0xE](0x65) based on game version and worm state,
/// then sends message 0x28 to CTaskTeam, then increments
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
    let ddgame = (*worm).ddgame();
    let version = (*ddgame).version_flag_4;
    let worm_state = (*worm).state();

    let should_call_vtable = version < 2
        || (version >= 5
            && (worm_state == 0x7D || (worm_state == 0x78 && version < 8)));

    if should_call_vtable {
        (*worm).set_state(0x65);
    }

    fire_send_team_message(worm, 0x28);

    let arena = TeamArenaRef::from_ptr(&raw mut (*ddgame).team_arena);
    let team_index = (*worm).team_index as usize;
    let worm_index = (*worm).worm_index as usize;
    let entry = arena.team_worm_mut(team_index, worm_index);
    entry.turn_action_counter_Maybe += 7;
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

/// Pneumatic Drill (subtype 2) — pure Rust port of 0x51E3E0.
///
/// Calls SpecialImpact with facing-offset position and scaled direction.
/// The original is usercall(ECX=local_struct, ESI=worm) — the old bridge
/// did not set ECX, so this port also fixes a latent bug.
unsafe fn fire_drill(worm: *mut CTaskWorm, local_struct: *const u8) {
    let ddgame = (*worm).ddgame();
    let version = (*ddgame).version_flag_4;
    let entry = &*(*worm).active_weapon_entry;
    let shot_count = entry.fire_params.shot_count;
    let weapon_type = entry.fire_subtype_38;
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

    let ddgame = (*worm).ddgame();
    let version = (*ddgame).version_flag_4;
    let entry = &*(*worm).active_weapon_entry;
    let shot_count = entry.fire_params.shot_count;
    let spread = entry.fire_params.spread;
    let weapon_type = entry.fire_subtype_38;
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

/// Bridge: usercall(EAX=eax_val, ESI=worm, EDI=worm), plain RET.
/// Args: (eax_val, worm, addr). Saves/restores ESI+EDI. Uses EBX to hold addr.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_usercall(_eax: *const (), _worm: *mut CTaskWorm, _addr: u32) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov eax, [esp+16]",  // eax_val (3 saves=12 + ret=4 = 16)
        "mov esi, [esp+20]",  // worm
        "mov edi, [esp+20]",
        "mov ebx, [esp+24]",  // addr
        "call ebx",
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
    use openwa_core::task::{SharedDataTable, Task};
    use openwa_core::wa_alloc::wa_malloc;

    let ddgame = &mut *(*worm).ddgame();

    // Pool capacity check: pool_count + 7 must be <= 700
    if ddgame.object_pool_count + 7 > 700 {
        ddgame.show_pool_overflow_warning();
        return;
    }

    // Look up parent CTaskTurnGame via SharedData (key_esi=0, key_edi=0x19)
    let table = SharedDataTable::from_task((*worm).as_task_ptr());
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

    let ddgame = &mut *(*worm).ddgame();

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
    use openwa_core::task::{SharedDataTable, Task};
    use openwa_core::wa_alloc::wa_malloc;

    let ddgame = &mut *(*worm).ddgame();

    // Pool capacity check: pool_count + 2 must be <= 700
    if ddgame.object_pool_count + 2 > 700 {
        ddgame.show_pool_overflow_warning();
        return;
    }

    // Look up parent CTaskTurnGame via SharedData (key 0, 0x19)
    let table = SharedDataTable::from_task((*worm).as_task_ptr());
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
        let trampoline = hook::install("FireWeapon", va::FIRE_WEAPON, trampoline_fire_weapon as *const ())?;
        ORIG_FIRE_WEAPON.store(trampoline as u32, Ordering::Relaxed);

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
