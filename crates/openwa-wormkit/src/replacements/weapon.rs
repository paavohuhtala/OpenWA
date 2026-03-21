//! Weapon hooks.
//!
//! Replaces WA.exe functions that manage weapon ammo in the TeamArenaState area (DDGame + 0x4628):
//! - GetAmmo (0x5225E0): query ammo count with delay/phase checks
//! - AddAmmo (0x522640): add ammo to a weapon slot
//! - SubtractAmmo (0x522680): decrement ammo count
//! - CountAliveWorms (0x5225A0): check if >1 worm alive on team
//! - FireWeapon (0x51EE60): passthrough with weapon type logging

use core::sync::atomic::{AtomicU32, Ordering};

use openwa_core::address::va;
use openwa_core::engine::ddgame::{self, TeamArenaRef};
use openwa_core::game::weapon::{WeaponEntry, WeaponFireParams};
use openwa_core::game::Weapon;
use openwa_core::log::log_line;
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
    if state.game_phase >= ddgame::GAME_PHASE_NORMAL_MIN && weapon_id == Weapon::SelectWorm as u32 {
        if count_alive_worms_impl(team_index, arena) == 0 {
            return 0;
        }
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
    entry: *const WeaponEntry, local_struct: u32, worm: *mut CTaskWorm,
) {
    use openwa_core::rebase::rb;

    let weapon_type = (*entry).fire_type;
    let subtype_34 = (*entry).fire_subtype_34;
    let subtype_38 = (*entry).fire_subtype_38;
    let fire_params = &raw const (*entry).fire_params;
    let w = worm as u32;

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
            1 => call_fire_stdcall1(w, fire_params, rb(0x51EC80)),                    // PlacedExplosive
            2 => call_fire_stdcall3(w, fire_params, local_struct, rb(0x51DFB0)),      // Projectile
            3 => call_fire_thiscall2(w, fire_params, local_struct, rb(0x51E0F0)),     // CreateWeaponProjectile
            4 => call_fire_stdcall2(w, fire_params, local_struct, rb(0x51ED90)),      // Shotgun
            _ => {}
        },
        2 => match subtype_38 {
            1 => call_fire_stdcall3(w, fire_params, local_struct, rb(0x51E1C0)),      // RopeType1
            2 => call_fire_thiscall2(w, fire_params, local_struct, rb(0x51E0F0)),     // CreateWeaponProjectile
            3 => call_fire_stdcall3(w, fire_params, local_struct, rb(0x51E240)),      // RopeType3
            _ => {}
        },
        3 => {
            // GrenadeMortar receives &fire_subtype_34 as its params pointer
            let subtype_34_ptr = &raw const (*entry).fire_subtype_34 as *const WeaponFireParams;
            call_fire_stdcall3(w, subtype_34_ptr, local_struct, rb(0x51E2C0));
        }
        4 => {
            let subtype_38_ptr = &raw const (*entry).fire_subtype_38 as u32;
            fire_weapon_special(subtype_34, subtype_38_ptr, worm, local_struct, entry);
        }
        _ => {}
    }

    (*worm).set_fire_complete(1);
}

// ── Sub-function bridges ────────────────────────────────────
// All preserve ESI=worm for usercall sub-functions.

// ── Sub-function bridges ────────────────────────────────────
// All bridges set ESI=worm AND EDI=worm before calling, since
// sub-functions are usercall and read both registers implicitly.
// Stack offsets are carefully calculated for each bridge.

// ── Sub-function bridges ────────────────────────────────────
// All bridges save/restore ESI+EDI, set ESI=EDI=worm, then call.
// This preserves LLVM's callee-saved registers while providing
// the usercall context that sub-functions expect.

/// Bridge: PlacedExplosive — stdcall(fire_params), RET 0x4.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_stdcall1(
    _worm: u32, _fire_params: *const WeaponFireParams, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov esi, [esp+16]",  // worm (3 saves=12 + ret=4 = 16)
        "mov edi, [esp+16]",
        "mov ebx, [esp+24]",  // addr
        "push [esp+20]",      // params
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
    _worm: u32, _fire_params: *const WeaponFireParams, _local: u32, _addr: u32,
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
    _worm: u32, _fire_params: *const WeaponFireParams, _local: u32, _addr: u32,
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

/// Bridge: Shotgun — stdcall(fire_params, local_struct), RET 0x8.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_stdcall2(
    _worm: u32, _fire_params: *const WeaponFireParams, _local: u32, _addr: u32,
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
/// Type 4 (special) weapon dispatch.
///
/// EAX at entry = weapon entry pointer (unchanged from type-4 switch).
/// Some handlers explicitly set EAX=worm or EAX=*(worm+0x44).
/// Handlers without explicit MOV EAX inherit the entry pointer.
unsafe fn fire_weapon_special(
    subtype: i32, params_38: u32, worm: *mut CTaskWorm, local_struct: u32,
    entry: *const WeaponEntry,
) {
    use openwa_core::rebase::rb;
    let w = worm as u32;
    let e = entry as u32;

    match subtype {
        1 => fire_worm_vtable_0xe(w, 0x6C),                                             // Blowtorch
        2 => call_fire_usercall(e, w, rb(0x51E3E0)),                                     // Drill (EAX=entry)
        3 => call_fire_stdcall3(w, params_38 as *const WeaponFireParams, local_struct, rb(0x51E350)), // Girder
        4 => fire_worm_vtable_0xe(w, 0x6D),                                             // Baseball Bat
        5 => fire_worm_vtable_0xe(w, 0x75),                                             // Fire Punch
        6 => fire_worm_vtable_0xe(w, 0x70),                                             // Dragon Ball
        8 => fire_worm_vtable_0xe(w, 0x6E),                                             // Kamikaze
        9 => call_fire_usercall_stdcall1(e, w, local_struct, rb(0x51E480)),              // Prod (EAX=entry)
        10 => call_fire_usercall(w, w, rb(0x51E710)),                                    // Air Strike (EAX=worm)
        11 => fire_worm_vtable_0xe(w, 0x71),                                            // Scales of Justice
        13 => call_fire_usercall(e, w, rb(0x51E5C0)),                                    // Napalm (EAX=entry)
        14 => call_fire_usercall(w, w, rb(0x51E670)),                                    // Mail/Mine/Mole (EAX=worm)
        16 => {
            // Teleport: MOV EAX,[ESI+0x44] (worm state) before check
            let worm_state = (*worm).state();
            let result = call_fire_usercall_ret(worm_state, w, rb(0x516930));
            if result != 0 {
                call_fire_usercall(result as u32, w, rb(0x51EB00));
            } else {
                fire_worm_vtable_0xe(w, 0x74);
            }
        }
        17 => call_fire_usercall(w, w, rb(0x51E920)),                                    // Freeze (EAX=worm)
        18 => fire_worm_vtable_0xe(w, 0x72),                                            // Suicide Bomber
        19 => fire_skip_go(worm, entry),                                                 // Skip Go (pure Rust)
        20 => call_fire_usercall(w, w, rb(0x51E600)),                                    // Surrender (EAX=worm)
        21 => call_fire_usercall(e, w, rb(0x51EBE0)),                                    // Select Worm (EAX=entry)
        22 => call_fire_usercall(e, w, rb(0x51EC30)),                                    // Jet Pack (EAX=entry)
        23 => fire_worm_vtable_0xe(w, 0x78),                                            // Magic Bullet
        24 => call_fire_usercall(e, w, rb(0x51EA60)),                                    // Low Grav (EAX=entry)
        _ => {}
    }
}

// ── Pure Rust fire handlers (no bridge needed) ──────────────

/// Skip Go (subtype 19) — pure Rust replacement for 0x51E8C0.
///
/// Toggles a bit in the team's `TeamHeader.turn_action_flags` (+0x7C).
/// Bit position comes from weapon entry's fire_params.
/// In game_version > 0x1C: toggles (set/clear). Otherwise: always sets.
unsafe fn fire_skip_go(worm: *const CTaskWorm, entry: *const WeaponEntry) {
    use openwa_core::engine::ddgame::TeamArenaRef;
    use openwa_core::engine::DDGame;

    let ddgame = (*worm).base.base.ddgame as *mut DDGame;
    let game_version = (*(*ddgame).game_info).game_version;
    let team_index = (*worm).team_index as usize;

    let bit_index = (*entry).fire_params._data[0] & 0x1F;
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

// ── Naked asm bridges ───────────────────────────────────────

/// Bridge: usercall(EAX=eax_val, ESI=worm, EDI=worm), plain RET.
/// Args: (eax_val, worm, addr). Saves/restores ESI+EDI. Uses EBX to hold addr.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_usercall(_eax: u32, _worm: u32, _addr: u32) {
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

/// Bridge: usercall(EAX=eax_val, ESI=worm, EDI=worm), plain RET, returns EAX.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_usercall_ret(_eax: u32, _worm: u32, _addr: u32) -> i32 {
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

/// Bridge: usercall(EAX=eax_val, ESI=worm, EDI=worm) + stdcall(1 param), RET 0x4.
#[unsafe(naked)]
unsafe extern "C" fn call_fire_usercall_stdcall1(
    _eax: u32, _worm: u32, _param: u32, _addr: u32,
) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov eax, [esp+16]",  // eax_val
        "mov esi, [esp+20]",  // worm
        "mov edi, [esp+20]",
        "mov ebx, [esp+28]",  // addr (3 saves=12 + ret=4 + 3 args=12 = 28)
        "push [esp+24]",      // param
        "call ebx",
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

/// Call worm->vtable[0xE](msg_id) — thiscall(ECX=worm, stack=msg_id), RET 0x4.
/// Original: `(**(code **)(*worm + 0x38))(msg_id)`.
/// Uses naked bridge to avoid LLVM stack tracking issues with push/RET mismatch.
#[unsafe(naked)]
unsafe extern "C" fn fire_worm_vtable_0xe(_worm: u32, _msg_id: u32) {
    core::arch::naked_asm!(
        "push ebx",
        "push esi",
        "push edi",
        "mov ecx, [esp+16]",  // worm (3 saves=12 + ret=4 = 16)
        "mov esi, [esp+16]",  // ESI = worm
        "mov edi, [esp+16]",  // EDI = worm
        "mov ebx, [ecx]",     // vtable
        "mov ebx, [ebx+0x38]",// vtable[0xE]
        "push [esp+20]",      // msg_id
        "call ebx",           // thiscall: ECX=worm, RET 0x4 cleans msg_id
        "pop edi",
        "pop esi",
        "pop ebx",
        "ret",
    );
}

// ============================================================
// Hook installation
// ============================================================

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install("AddAmmo", va::ADD_AMMO, trampoline_add_ammo as *const ())?;

        let _ = hook::install("GetAmmo", va::GET_AMMO, trampoline_get_ammo as *const ())?;

        let _ = hook::install(
            "SubtractAmmo",
            va::SUBTRACT_AMMO,
            trampoline_subtract_ammo as *const (),
        )?;

        let _ = hook::install(
            "CountAliveWorms",
            va::COUNT_ALIVE_WORMS,
            trampoline_count_alive_worms as *const (),
        )?;

        let trampoline = hook::install(
            "FireWeapon",
            va::FIRE_WEAPON,
            trampoline_fire_weapon as *const (),
        )?;
        ORIG_FIRE_WEAPON.store(trampoline as u32, Ordering::Relaxed);
    }

    Ok(())
}
