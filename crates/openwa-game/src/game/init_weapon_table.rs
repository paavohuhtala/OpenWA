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

/// Weapons that are unconditionally forced to `availability = 1` after
/// the baseline pass — the WA case-switch labels with the +10 offset
/// pre-applied. Mostly super weapons + Earthquake.
const FORCE_AVAILABLE_WEAPONS: &[usize] = &[
    10, 19, 29, 30, 31, 36, 41, 42, 45, 46, 49, 50, 51, 54, 55, 56, 60, 61,
];

/// Weapon 59 is force-enabled only when [`GameWorld::version_flag_3`]
/// is non-zero (mirrors the `case 0x31:` branch in WA).
const FORCE_AVAILABLE_VERSION_GATED: usize = 59;

/// Source weapon entry whose `fire_params` block (excluding
/// `entry_metadata`) is mirrored into [`GameWorld::weapon_table_backup`].
/// Specifically: `weapon_table.entries[23].fire_params[..0x5E]`, i.e.
/// dwords 0..94, which is everything WA copies to MissileEntity.
const BACKUP_SOURCE_ENTRY: usize = 23;

/// Per-team scheme-ammo dword offsets cleared when `CheckWeaponAvail`
/// returns 0. Layout: 6 teams × 2 slots, with each team's pair at
/// `(team * 0x11E - 0x8E, team * 0x11E)` relative to the per-weapon
/// base (`game_info + 0xD150 + (weapon - 1) * 2`).
const ZERO_AMMO_OFFSETS: &[isize] = &[
    -0x8E, 0, 0x90, 0x11E, 0x1AE, 0x23C, 0x2CC, 0x35A, 0x3EA, 0x478, 0x508, 0x596,
];

/// Per-team scheme-ammo offsets clamped to `-1` when `CheckWeaponAvail`
/// returns -2 (cap-only mode), conditional on `game_version < 0x196` OR
/// existing value < 0. One slot per team, stride 0x11E.
const CAP_AMMO_OFFSETS: &[isize] = &[0, 0x11E, 0x23C, 0x35A, 0x478, 0x596];

// ─── Game-info accessors for the scheme-ammo region ────────────────────────
//
// The byte offsets below all live inside `GameInfo._unknown_4aa0` (a 0x82D4-byte
// unmapped span). Splitting that into typed sub-fields would require a much
// larger RE pass; for now we localize the byte arithmetic to these helpers so
// the porting code stays readable.

/// Team count for the per-team ammo-carry array (byte at GameInfo+0xD0BC).
unsafe fn ammo_carry_team_count(game_info: *const GameInfo) -> u8 {
    unsafe { *(game_info as *const u8).add(0xD0BC) }
}

/// Per-team ammo-carry pair `(src, dst)` at GameInfo+0xD0F0/+0xD0F2,
/// stride 0x11E.
unsafe fn ammo_carry_slot_pair(game_info: *mut GameInfo, team: usize) -> (*mut u16, *mut u16) {
    unsafe {
        let base = (game_info as *mut u8).add(0xD0F0 + team * 0x11E);
        (base as *mut u16, base.add(2) as *mut u16)
    }
}

/// Pointer into the per-weapon scheme-ammo array at
/// `GameInfo + 0xD150 + (weapon - 1) * 2 + offset`. `weapon` is 1..71
/// (weapon 0 has no scheme ammo).
unsafe fn scheme_weapon_ammo_slot(
    game_info: *mut GameInfo,
    weapon: u32,
    offset: isize,
) -> *mut u16 {
    unsafe {
        let base = (game_info as *mut u8).offset(0xD150 + ((weapon - 1) as isize) * 2);
        base.offset(offset) as *mut u16
    }
}

// ─── Port ──────────────────────────────────────────────────────────────────

/// Pure Rust port of WA's `InitWeaponTable` (0x0053CAB0).
/// `__stdcall(runtime)`, `RET 0x4`.
pub unsafe fn init_weapon_table(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let game_info = (*world).game_info;
        let weapon_table = (*world).weapon_table;

        // Zero entire table.
        core::ptr::write_bytes(weapon_table, 0, 1);

        // Init each entry: panel_state=-1, availability=-1, enabled=1.
        for entry in (*weapon_table).entries.iter_mut() {
            entry.panel_state = -1;
            entry.availability = -1;
            entry.enabled = 1;
        }

        // Force weapons 0, 57, 58 to availability=0 (unavailable).
        (*weapon_table).entries[0].availability = 0;
        (*weapon_table).entries[57].availability = 0;
        (*weapon_table).entries[58].availability = 0;

        // Power cap: per-version gating.
        let game_version = (*game_info).game_version;
        let cap: u32 = if game_version < 0x32 {
            (*world).level_width
        } else {
            0x7FFF
        };
        bridge_init_weapon_defaults_baseline(weapon_table, game_info, cap);
        bridge_init_weapon_defaults_extended(weapon_table, game_info);

        // First backup (very-old schemes): copy entry-23 fire_params region
        // to world.weapon_table_backup *before* the scheme overlay. Mirrors
        // WA's two mutually exclusive backup blocks.
        if game_version < -1 {
            backup_weapon_table_region(weapon_table, world);
        }

        bridge_overlay_scheme_weapon_settings(world);
        bridge_init_weapon_name_strings(world);
        bridge_assign_panel_slots(weapon_table);

        // Per-team ammo carry: D0F0 → D0F2.
        per_team_ammo_carry(game_info, game_version);

        // Two-flag forced shot_count override.
        if (*game_info)._unknown_d945 != 0 && (*game_info).net_config_2 != 0 {
            (*weapon_table).entries[11].shot_count = 1;
            (*weapon_table).entries[15].shot_count = 1;
        }

        // Second backup (normal schemes, game_version >= -1).
        if game_version >= -1 {
            backup_weapon_table_region(weapon_table, world);
        }

        // Post-backup overrides: dwords 25 and 3 of the backup region.
        (*world).weapon_table_backup[25] = 0;
        (*world).weapon_table_backup[3] = 0x7E;

        // Force-availability=1 sweep over the super-weapon list.
        for &id in FORCE_AVAILABLE_WEAPONS {
            (*weapon_table).entries[id].availability = 1;
        }
        if (*world).version_flag_3 != 0 {
            (*weapon_table).entries[FORCE_AVAILABLE_VERSION_GATED].availability = 1;
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
                for &off in ZERO_AMMO_OFFSETS {
                    *scheme_weapon_ammo_slot(game_info, w, off) = 0;
                }
            } else if ret < -1 {
                // ret == -2: clamp 6 slots to -1 (only when game version
                // < 0x196 OR existing value already < 0).
                for &off in CAP_AMMO_OFFSETS {
                    let p = scheme_weapon_ammo_slot(game_info, w, off) as *mut i16;
                    if game_version < 0x196 || *p < 0 {
                        *p = -1;
                    }
                }
            }
            // ret == -1: availability cleared above; ammo left intact.
        }
    }
}

/// Copy 0x5E dwords from `weapon_table.entries[23].fire_params` (the
/// weapon-23 fire-params header — everything except `entry_metadata`)
/// into [`GameWorld::weapon_table_backup`].
unsafe fn backup_weapon_table_region(weapon_table: *mut WeaponTable, world: *mut GameWorld) {
    unsafe {
        let src =
            &(*weapon_table).entries[BACKUP_SOURCE_ENTRY].fire_params as *const _ as *const u32;
        let dst = (*world).weapon_table_backup.as_mut_ptr();
        core::ptr::copy_nonoverlapping(src, dst, (*world).weapon_table_backup.len());
    }
}

/// Per-team ammo carry: zero `D0F2` (or copy/add `D0F0 → D0F2`, then
/// zero `D0F0`) for each active team.
unsafe fn per_team_ammo_carry(game_info: *mut GameInfo, game_version: i32) {
    unsafe {
        let team_count = ammo_carry_team_count(game_info);
        for t in 0..team_count as usize {
            let (src, dst) = ammo_carry_slot_pair(game_info, t);
            if (*game_info).aquasheep_is_supersheep == 0 {
                *dst = 0;
            } else {
                *dst = if game_version < 10 {
                    *src
                } else {
                    (*dst).wrapping_add(*src)
                };
                *src = 0;
            }
        }
    }
}
