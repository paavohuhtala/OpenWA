//! Rust port of WA's `InitWeaponTable` (0x0053CAB0).
//!
//! Populates the per-game [`WeaponTable`] at [`GameWorld::weapon_table`]
//! from the active scheme. Replaces the static fixture in
//! [`crate::game::weapon_data`] as the *runtime* source of weapon data —
//! the fixture remains as a vanilla baseline for tests and tooling.
//!
//! The outer skeleton (memset, init/availability loops, scheme-version
//! gating, post-helper fixups, final `CheckWeaponAvail` sweep) is ported
//! to Rust here. The five large helpers it calls remain bridged to WA
//! and can be ported piecemeal:
//!
//! - 0x00537320 — baseline weapon defaults (~1100 inst, unrolled writes)
//! - 0x00539100 — extended weapon defaults (~1100 inst)
//! - 0x0053AD80 — overlay scheme weapon-settings bytes onto entries
//! - 0x0053C130 — `InitWeaponNameStrings` (alloc + fill name1/name2)
//! - 0x00537130 — assign each entry to its weapon-panel row/column

use core::ffi::c_void;

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::engine::runtime::GameRuntime;
use crate::engine::world::GameWorld;
use crate::game::weapon::{WeaponTable, check_weapon_avail};
use crate::rebase::rb;
use openwa_core::weapon::WeaponId;

// ─── Bridged WA addresses ──────────────────────────────────────────────────

static mut INIT_WEAPON_DEFAULTS_BASELINE_ADDR: u32 = 0;
static mut INIT_WEAPON_DEFAULTS_EXTENDED_ADDR: u32 = 0;
static mut OVERLAY_SCHEME_WEAPON_SETTINGS_ADDR: u32 = 0;
static mut INIT_WEAPON_NAME_STRINGS_ADDR: u32 = 0;
static mut ASSIGN_WEAPON_PANEL_SLOTS_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        INIT_WEAPON_DEFAULTS_BASELINE_ADDR = rb(va::INIT_WEAPON_DEFAULTS_BASELINE);
        INIT_WEAPON_DEFAULTS_EXTENDED_ADDR = rb(va::INIT_WEAPON_DEFAULTS_EXTENDED);
        OVERLAY_SCHEME_WEAPON_SETTINGS_ADDR = rb(va::OVERLAY_SCHEME_WEAPON_SETTINGS);
        INIT_WEAPON_NAME_STRINGS_ADDR = rb(va::INIT_WEAPON_NAME_STRINGS);
        ASSIGN_WEAPON_PANEL_SLOTS_ADDR = rb(va::ASSIGN_WEAPON_PANEL_SLOTS);
    }
}

// ─── Helper bridges (still WA-side) ────────────────────────────────────────

/// `__usercall(EAX = weapon_table, [stack] = game_info, [stack] = cap)`,
/// `RET 0x8`. Massive unrolled assignment: writes baseline defaults
/// (power, fuse, ammo, blast radius, etc.) into all 71 entries.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_init_weapon_defaults_baseline(
    _table: *mut WeaponTable,
    _game_info: *mut GameInfo,
    _cap: u32,
) {
    // Stdcall pushes args right-to-left, so on entry:
    //   [esp+0]=ret, [esp+4]=table, [esp+8]=gi, [esp+12]=cap
    // Trampoline: pop the table slot (already in EAX), tail-jump to WA.
    // WA's RET 0x8 cleans the remaining gi+cap.
    core::arch::naked_asm!(
        "mov eax, [esp+4]",
        "pop ecx",
        "pop edx",
        "push ecx",
        "jmp dword ptr [{addr}]",
        addr = sym INIT_WEAPON_DEFAULTS_BASELINE_ADDR,
    );
}

/// `__usercall(EAX = weapon_table, [stack] = game_info)`, `RET 0x4`.
/// Companion to baseline defaults; covers the second half of the table.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_init_weapon_defaults_extended(
    _table: *mut WeaponTable,
    _game_info: *mut GameInfo,
) {
    core::arch::naked_asm!(
        "mov eax, [esp+4]",
        "pop ecx",
        "pop edx",
        "push ecx",
        "jmp dword ptr [{addr}]",
        addr = sym INIT_WEAPON_DEFAULTS_EXTENDED_ADDR,
    );
}

/// `__usercall(EDI = weapon_table, [stack] = world)`, `RET 0x4`. Reads
/// the per-weapon scheme settings at `game_info + 0xD78C..D923` and
/// overlays them onto each [`WeaponEntry`](super::weapon::WeaponEntry)
/// — including `retreat_time` (3000ms for most weapons), which it
/// computes as `scheme_byte * 1000` and writes through `EDI + 0x1EC`.
/// EDI must be preloaded with the weapon-table base; without it the
/// helper writes through whatever EDI happens to hold, silently
/// corrupting unrelated memory.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_overlay_scheme_weapon_settings(_world: *mut GameWorld) {
    core::arch::naked_asm!(
        "push edi",
        "mov ecx, [esp+8]",
        "mov edi, [ecx+0x510]",
        "push ecx",
        "call dword ptr [{addr}]",
        "pop edi",
        "ret 4",
        addr = sym OVERLAY_SCHEME_WEAPON_SETTINGS_ADDR,
    );
}

/// `__usercall(ESI = weapon_table, EDI = localized_template)`, plain RET.
/// Allocates two 0x1C-byte string buffers per entry (name1 at +0x00,
/// name2 at +0x04) and fills them from the localization tables. The
/// EDI input mirrors WA's call site, which loads it from
/// [`GameWorld::localized_template`] (`world + 0x18`).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_init_weapon_name_strings(_world: *mut GameWorld) {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov ecx, [esp+12]",
        "mov esi, [ecx+0x510]",
        "mov edi, [ecx+0x18]",
        "call dword ptr [{addr}]",
        "pop edi",
        "pop esi",
        "ret 4",
        addr = sym INIT_WEAPON_NAME_STRINGS_ADDR,
    );
}

/// `__usercall(EAX = weapon_table)`, plain RET. Writes each entry's
/// `panel_state` field (+0x08) to its weapon-panel row (1..0xC).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_assign_panel_slots(_table: *mut WeaponTable) {
    core::arch::naked_asm!(
        "mov eax, [esp+4]",
        "call dword ptr [{addr}]",
        "ret 4",
        addr = sym ASSIGN_WEAPON_PANEL_SLOTS_ADDR,
    );
}

// ─── Constants ─────────────────────────────────────────────────────────────

/// Total weapon-table size in bytes: 71 × 0x1D0.
const WEAPON_TABLE_BYTES: usize = 71 * 0x1D0;

/// Backup region copied between [`WeaponTable`] (+0x29EC) and
/// [`GameWorld`] (+0x74C8): 0x5E dwords = 0x178 bytes.
const BACKUP_DWORDS: usize = 0x5E;

/// Weapons that are unconditionally forced to `availability = 1` after
/// the baseline pass — the WA case-switch labels with the +10 offset
/// pre-applied. Mostly super weapons + Earthquake.
const FORCE_AVAILABLE_WEAPONS: &[u32] = &[
    10, 19, 29, 30, 31, 36, 41, 42, 45, 46, 49, 50, 51, 54, 55, 56, 60, 61,
];

/// Weapon 59 is force-enabled only when [`GameWorld::version_flag_3`]
/// is non-zero (mirrors the `case 0x31:` branch in WA).
const FORCE_AVAILABLE_VERSION_GATED: u32 = 59;

/// Per-team scheme-ammo offsets cleared when `CheckWeaponAvail`
/// returns 0 (weapon unavailable for non-cap reasons). Each offset is
/// added to `game_info + 0xD150 + (weapon - 1) * 2`.
const ZERO_AMMO_OFFSETS: &[isize] = &[
    -0x8E, 0, 0x90, 0x11E, 0x1AE, 0x23C, 0x2CC, 0x35A, 0x3EA, 0x478, 0x508, 0x596,
];

/// Per-team scheme-ammo offsets clamped to `0xFFFF` when
/// `CheckWeaponAvail` returns -2 (cap-only mode), conditional on
/// `game_version < 0x196` OR existing value < 0.
const CAP_AMMO_OFFSETS: &[isize] = &[0, 0x11E, 0x23C, 0x35A, 0x478, 0x596];

// ─── Port ──────────────────────────────────────────────────────────────────

/// Pure Rust port of WA's `InitWeaponTable` (0x0053CAB0).
/// `__stdcall(runtime)`, `RET 0x4`.
pub unsafe fn init_weapon_table(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let game_info = (*world).game_info;
        let weapon_table = (*world).weapon_table;
        let table_bytes = weapon_table as *mut u8;
        let world_bytes = world as *mut u8;
        let gi_bytes = game_info as *mut u8;

        // Zero entire table.
        core::ptr::write_bytes(table_bytes, 0, WEAPON_TABLE_BYTES);

        // Init each entry: panel_state=-1, availability=-1, enabled=1.
        for entry in (*weapon_table).entries.iter_mut() {
            entry.panel_state = -1;
            entry.availability = -1;
            entry.enabled = 1;
        }

        // Special baseline writes.
        (*weapon_table).entries[0].availability = 0;
        *(table_bytes.add(0x6774) as *mut u32) = 0;
        *(table_bytes.add(0x6944) as *mut u32) = 0;

        // Power cap: per-version gating.
        let game_version = (*game_info).game_version;
        let cap: u32 = if game_version < 0x32 {
            (*world).level_width
        } else {
            0x7FFF
        };
        bridge_init_weapon_defaults_baseline(weapon_table, game_info, cap);
        bridge_init_weapon_defaults_extended(weapon_table, game_info);

        // First backup (very-old schemes): copy weapon_table+0x29EC region
        // to world+0x74C8 *before* the scheme overlay. Mirrors WA's two
        // mutually exclusive backup blocks.
        if game_version < -1 {
            backup_weapon_table_region(table_bytes, world_bytes);
        }

        bridge_overlay_scheme_weapon_settings(world);
        bridge_init_weapon_name_strings(world);
        bridge_assign_panel_slots(weapon_table);

        // Per-team ammo carry: D0F0 → D0F2.
        per_team_ammo_carry(game_info, gi_bytes, game_version);

        // Two-flag forced enable (writes are at raw table offsets, not
        // structured WeaponEntry fields).
        if *gi_bytes.add(0xD945) != 0 && *gi_bytes.add(0xD946) != 0 {
            *(table_bytes.add(0x1404) as *mut u32) = 1;
            *(table_bytes.add(0x1B44) as *mut u32) = 1;
        }

        // Second backup (normal schemes, game_version >= -1).
        if game_version >= -1 {
            backup_weapon_table_region(table_bytes, world_bytes);
        }

        *(world_bytes.add(0x752C) as *mut u32) = 0;
        *(world_bytes.add(0x74D4) as *mut u32) = 0x7E;

        // Force-availability=1 sweep over the super-weapon list.
        for &id in FORCE_AVAILABLE_WEAPONS {
            (*weapon_table).entries[id as usize].availability = 1;
        }
        if (*world).version_flag_3 != 0 {
            (*weapon_table).entries[FORCE_AVAILABLE_VERSION_GATED as usize].availability = 1;
        }

        // Final per-weapon `CheckWeaponAvail` sweep over weapons 1..71.
        // Note: weapon 0 is intentionally skipped.
        for w in 1u32..71 {
            let weapon = WeaponId(w);
            let ret = check_weapon_avail(world, weapon);
            if ret > 0 {
                continue;
            }
            (*weapon_table).entries[w as usize].availability = 0;

            if ret == 0 {
                // Zero 12 per-team ammo slots in the scheme array.
                let base = gi_bytes.offset(0xD150 + ((w - 1) as isize) * 2);
                for &off in ZERO_AMMO_OFFSETS {
                    *(base.offset(off) as *mut u16) = 0;
                }
            } else if ret < -1 {
                // ret == -2: clamp 6 slots to 0xFFFF (only when game
                // version < 0x196 OR existing value < 0).
                let base = gi_bytes.offset(0xD150 + ((w - 1) as isize) * 2);
                for &off in CAP_AMMO_OFFSETS {
                    let p = base.offset(off) as *mut i16;
                    if game_version < 0x196 || *p < 0 {
                        *p = -1;
                    }
                }
            }
            // ret == -1: availability cleared above; ammo left intact.
        }
    }
}

/// Copy [`WeaponTable`] (+0x29EC, 0x178 bytes) into [`GameWorld`] (+0x74C8).
unsafe fn backup_weapon_table_region(table_bytes: *mut u8, world_bytes: *mut u8) {
    unsafe {
        core::ptr::copy_nonoverlapping(
            table_bytes.add(0x29EC) as *const c_void,
            world_bytes.add(0x74C8) as *mut c_void,
            BACKUP_DWORDS * 4,
        );
    }
}

/// Per-team ammo carry: zero `D0F2` (or copy/add `D0F0 → D0F2`, then
/// zero `D0F0`) for each active team. Stride is 0x11E bytes.
unsafe fn per_team_ammo_carry(game_info: *mut GameInfo, gi_bytes: *mut u8, game_version: i32) {
    unsafe {
        let team_count = *gi_bytes.add(0xD0BC);
        if (*game_info).aquasheep_is_supersheep == 0 {
            for t in 0..team_count {
                let dst = gi_bytes.add(0xD0F2 + (t as usize) * 0x11E) as *mut u16;
                *dst = 0;
            }
        } else {
            for t in 0..team_count {
                let src = gi_bytes.add(0xD0F0 + (t as usize) * 0x11E) as *mut u16;
                let dst = gi_bytes.add(0xD0F2 + (t as usize) * 0x11E) as *mut u16;
                if game_version < 10 {
                    *dst = *src;
                } else {
                    *dst = (*dst).wrapping_add(*src);
                }
                *src = 0;
            }
        }
    }
}
