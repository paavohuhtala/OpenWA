//! Rust port of WA's `InitWeaponTable` (0x0053CAB0) — runtime source of
//! weapon data populated into `GameWorld::weapon_table` at world init.

use crate::engine::game_info::GameInfo;
use crate::engine::runtime::GameRuntime;
use crate::engine::world::GameWorld;
use crate::game::init_weapon_defaults::populate_weapon_table_defaults;
use crate::game::init_weapon_names::init_weapon_name_strings;
use crate::game::weapon::{WeaponTable, check_weapon_avail};
use crate::wa::string_resource::{res, wa_load_string};
use openwa_core::weapon::{WeaponId, is_super_weapon};

/// Source entry whose `fire_params` (less `entry_metadata`) is mirrored
/// into [`GameWorld::weapon_table_backup`].
const BACKUP_SOURCE_ENTRY: usize = 23;

/// Per-team scheme-ammo dword offsets cleared when `CheckWeaponAvail`
/// returns 0 — 6 teams × 2 slots, paired at
/// `(team * 0x11E - 0x8E, team * 0x11E)` relative to the per-weapon base
/// at `game_info + 0xD150 + (weapon - 1) * 2`.
const ZERO_AMMO_OFFSETS: &[isize] = &[
    -0x8E, 0, 0x90, 0x11E, 0x1AE, 0x23C, 0x2CC, 0x35A, 0x3EA, 0x478, 0x508, 0x596,
];

/// Per-team scheme-ammo offsets clamped to `-1` when `CheckWeaponAvail`
/// returns -2; one slot per team, stride 0x11E.
const CAP_AMMO_OFFSETS: &[isize] = &[0, 0x11E, 0x23C, 0x35A, 0x478, 0x596];

// Scheme-ammo region byte accessors — all inside `GameInfo._unknown_4aa0`
// (0x82D4-byte unmapped span). Splitting that into typed sub-fields would
// take a much larger RE pass; localize the arithmetic here for now.

/// Team count at GameInfo+0xD0BC.
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

/// Per-weapon scheme-ammo slot at
/// `GameInfo + 0xD150 + (weapon - 1) * 2 + offset`. `weapon` is 1..71.
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

/// `__stdcall(runtime)`, `RET 0x4`. Port of WA 0x0053CAB0.
pub unsafe fn init_weapon_table(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let game_info = (*world).game_info;
        let weapon_table = (*world).weapon_table;

        core::ptr::write_bytes(weapon_table, 0, 1);
        for entry in (*weapon_table).entries.iter_mut() {
            entry.panel_state = -1;
            entry.availability = -1;
            entry.enabled = 1;
        }
        (*weapon_table).entries[0].availability = 0;
        (*weapon_table).entries[57].availability = 0;
        (*weapon_table).entries[58].availability = 0;

        let game_version = (*game_info).game_version;
        let cap: u32 = if game_version < 0x32 {
            (*world).level_width
        } else {
            0x7FFF
        };
        populate_weapon_table_defaults(weapon_table, game_info, cap);

        // Two mutually-exclusive backup blocks: very-old schemes back up
        // *before* the scheme overlay, normal schemes back up *after*.
        if game_version < -1 {
            backup_weapon_table_region(weapon_table, world);
        }

        overlay_scheme_weapon_settings(world);
        init_weapon_name_strings(world);
        assign_panel_slots(&mut *weapon_table);
        per_team_ammo_carry(game_info, game_version);

        if (*game_info)._unknown_d945 != 0 && (*game_info).net_config_2 != 0 {
            (*weapon_table).entries[11].shot_count = 1;
            (*weapon_table).entries[15].shot_count = 1;
        }

        if game_version >= -1 {
            backup_weapon_table_region(weapon_table, world);
        }
        (*world).weapon_table_backup[25] = 0;
        (*world).weapon_table_backup[3] = 0x7E;

        // Tentative force-on for super weapons — they have no per-weapon
        // scheme byte in the overlay above, so the `check_weapon_avail`
        // sweep below would otherwise see them at the init value (-1) and
        // leave them ungated.
        let select_worm_super = (*world).version_flag_3 != 0;
        for w in WeaponId::iter_known() {
            if is_super_weapon(w, select_worm_super) {
                (*weapon_table).entries[w.0 as usize].availability = 1;
            }
        }

        for w in WeaponId::iter_known() {
            let ret = check_weapon_avail(world, w);
            if ret > 0 {
                continue;
            }
            (*weapon_table).entries[w.0 as usize].availability = 0;

            if ret == 0 {
                for &off in ZERO_AMMO_OFFSETS {
                    *scheme_weapon_ammo_slot(game_info, w.0, off) = 0;
                }
            } else if ret < -1 {
                for &off in CAP_AMMO_OFFSETS {
                    let p = scheme_weapon_ammo_slot(game_info, w.0, off) as *mut i16;
                    if game_version < 0x196 || *p < 0 {
                        *p = -1;
                    }
                }
            }
            // ret == -1: availability cleared, ammo left intact.
        }
    }
}

/// Copy entry-23 `fire_params` (0x5E dwords, less `entry_metadata`) into
/// [`GameWorld::weapon_table_backup`].
unsafe fn backup_weapon_table_region(weapon_table: *mut WeaponTable, world: *mut GameWorld) {
    unsafe {
        let src =
            &(*weapon_table).entries[BACKUP_SOURCE_ENTRY].fire_params as *const _ as *const u32;
        let dst = (*world).weapon_table_backup.as_mut_ptr();
        core::ptr::copy_nonoverlapping(src, dst, (*world).weapon_table_backup.len());
    }
}

/// `__usercall(EAX = table)`. Port of WA 0x00537130. Panel layout:
/// panel 0 = entry 0 + entries 62..=70 (utility/special); panels 1..=4
/// and 6..=12 hold 5 entries each in sequence; panel 5 is the only
/// oversized one (6 entries, 21..=26).
fn assign_panel_slots(table: &mut WeaponTable) {
    table.entries[0].panel_state = 0;
    for i in 62..=70 {
        table.entries[i].panel_state = 0;
    }
    for (panel, range) in [
        (1, 1..=5),
        (2, 6..=10),
        (3, 11..=15),
        (4, 16..=20),
        (5, 21..=26),
        (6, 27..=31),
        (7, 32..=36),
        (8, 37..=41),
        (9, 42..=46),
        (10, 47..=51),
        (11, 52..=56),
        (12, 57..=61),
    ] {
        for i in range {
            table.entries[i].panel_state = panel;
        }
    }
}

/// Per-team ammo carry: GameInfo+0xD0F0 → +0xD0F2, then zero +0xD0F0.
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

/// `__usercall(EDI = weapon_table, [stack] = world)`, `RET 0x4`. Port of
/// WA 0x0053AD80. Overlays scheme bytes from `game_info.weapon_scheme_bytes`
/// (GameInfo+0xD78C..0xD923) onto the weapon-table entries built by
/// [`populate_weapon_table_defaults`]. Quirks worth knowing: weapons 24/25
/// share scheme bytes 0xCB..0xD2; weapon 62 is rewritten twice; weapon 37
/// packs 3 bytes at +0x4348..+0x434A. Per-weapon block comments below
/// flag these and the gaps in the scheme byte range.
unsafe fn overlay_scheme_weapon_settings(world: *mut GameWorld) {
    unsafe {
        let game_info = (*world).game_info;
        let scheme = (*game_info).weapon_scheme_bytes.as_mut_ptr();
        let table = (*world).weapon_table as *mut u8;

        // Prevent divide-by-zero in the napalm/petrol-bomb-cap fields
        // (`inv10k!` calls below for scheme+0xF2 / +0x138). Old schemes
        // (game_version < 0x34) also surface a HUD warning.
        for &(scheme_off, str_id) in &[
            (0xF2usize, res::LOG_CRASH_BADNAPALM),
            (0x138usize, res::LOG_CRASH_BADPETROL),
        ] {
            let cap = scheme.add(scheme_off);
            if *cap == 0 {
                *cap = 1;
                if (*game_info).game_version < 0x34 {
                    (*world).hud_status_code = 6;
                    (*world).hud_status_text = wa_load_string(str_id);
                }
            }
        }

        // bu/wu: unsigned byte/word → u32 store.
        // bs1k/bu1k: byte (signed/unsigned) × 1000 → i32/u32 store (ms scaling).
        // bu50: byte × 50. bb: byte → byte (3 packed-byte sites).
        // inv10k: `10000 / byte` for cap fields.
        macro_rules! bu {
            ($edi:expr, $sch:expr) => {{
                let v = *scheme.add($sch) as u32;
                *(table.add($edi) as *mut u32) = v;
            }};
        }
        macro_rules! wu {
            ($edi:expr, $sch:expr) => {{
                let v = *(scheme.add($sch) as *const u16) as u32;
                *(table.add($edi) as *mut u32) = v;
            }};
        }
        macro_rules! bs1k {
            ($edi:expr, $sch:expr) => {{
                let v = (*(scheme.add($sch) as *const i8)) as i32 * 1000;
                *(table.add($edi) as *mut i32) = v;
            }};
        }
        macro_rules! bu1k {
            ($edi:expr, $sch:expr) => {{
                let v = (*scheme.add($sch) as u32) * 1000;
                *(table.add($edi) as *mut u32) = v;
            }};
        }
        macro_rules! bu50 {
            ($edi:expr, $sch:expr) => {{
                let v = (*scheme.add($sch) as u32) * 50;
                *(table.add($edi) as *mut u32) = v;
            }};
        }
        macro_rules! bb {
            ($edi:expr, $sch:expr) => {{
                *table.add($edi) = *scheme.add($sch);
            }};
        }
        macro_rules! inv10k {
            ($edi:expr, $sch:expr) => {{
                let b = *scheme.add($sch) as i32;
                *(table.add($edi) as *mut i32) = 10000 / b;
            }};
        }

        // ─── weapon 1 ───────────────────────────────────────────────
        bu!(0x1F4, 0x00);
        bu!(0x1F8, 0x01);
        bu!(0x1DC, 0x02);
        bs1k!(0x1EC, 0x03);
        bu!(0x224, 0x04);
        bu!(0x220, 0x05);
        bu!(0x21C, 0x06);
        bu!(0x24C, 0x07);
        // ─── weapon 2 ───────────────────────────────────────────────
        bu!(0x3C4, 0x08);
        bu!(0x3C8, 0x09);
        bu!(0x3AC, 0x0A);
        bs1k!(0x3BC, 0x0B);
        bu!(0x3F4, 0x0C);
        bu!(0x3F0, 0x0D);
        bu!(0x3EC, 0x0E);
        wu!(0x468, 0x10);
        wu!(0x46C, 0x12);
        // ─── weapon 3 ───────────────────────────────────────────────
        bu!(0x594, 0x14);
        bu!(0x598, 0x15);
        bu!(0x57C, 0x16);
        bs1k!(0x58C, 0x17);
        bu!(0x5C4, 0x18);
        bu!(0x5C0, 0x19);
        bu!(0x5BC, 0x1A);
        bu!(0x5EC, 0x1B);
        // ─── weapon 4 ───────────────────────────────────────────────
        bu!(0x668, 0x1C);
        wu!(0x66C, 0x1E);
        bu!(0x678, 0x20);
        bu!(0x688, 0x21);
        // ─── weapon 5 ───────────────────────────────────────────────
        bu!(0x764, 0x22);
        bu!(0x768, 0x23);
        bu!(0x74C, 0x24);
        bs1k!(0x75C, 0x25);
        bu!(0x794, 0x26);
        bu!(0x790, 0x27);
        bu!(0x78C, 0x28);
        wu!(0x808, 0x2A);
        wu!(0x80C, 0x2C);
        // ─── weapon 6 ───────────────────────────────────────────────
        bu!(0xB04, 0x2E);
        bu!(0xB08, 0x2F);
        bu!(0xAEC, 0x30);
        bs1k!(0xAFC, 0x31);
        bu!(0xB34, 0x32);
        bu!(0xB30, 0x33);
        bu!(0xB2C, 0x34);
        bu1k!(0xB6C, 0x35);
        bu!(0xB5C, 0x36);
        // ─── weapon 7 ───────────────────────────────────────────────
        bu!(0xCD4, 0x38);
        bu!(0xCD8, 0x39);
        bu!(0xCBC, 0x3A);
        bs1k!(0xCCC, 0x3B);
        bu!(0xD04, 0x3C);
        bu!(0xD00, 0x3D);
        bu!(0xCFC, 0x3E);
        bu1k!(0xD3C, 0x3F);
        bu!(0xD2C, 0x40);
        // ─── weapon 8 ───────────────────────────────────────────────
        bu!(0xDA8, 0x42);
        wu!(0xDAC, 0x44);
        bu!(0xDB8, 0x46);
        bu!(0xDC8, 0x47);
        // ─── weapon 9 ───────────────────────────────────────────────
        bu!(0xEA4, 0x48);
        bu!(0xEA8, 0x49);
        bu!(0xE8C, 0x4A);
        bs1k!(0xE9C, 0x4B);
        bu!(0xED4, 0x4C);
        bu!(0xED0, 0x4D);
        bu!(0xECC, 0x4E);
        bu1k!(0xF0C, 0x4F);
        bu!(0xEFC, 0x50);
        // ─── weapon 10 ──────────────────────────────────────────────
        bu!(0xF78, 0x52);
        wu!(0xF7C, 0x54);
        bu!(0xF88, 0x56);
        bu!(0xF98, 0x57);
        // (gap: scheme bytes 0x58..0x69 unread)
        // ─── weapon 11 ──────────────────────────────────────────────
        bu!(0x1414, 0x6A);
        bu!(0x1418, 0x6B);
        bu!(0x13FC, 0x6C);
        bs1k!(0x140C, 0x6D);
        bu!(0x144C, 0x6E);
        bu!(0x1448, 0x6F);
        bu!(0x1444, 0x70);
        bu!(0x142C, 0x71);
        bu!(0x1434, 0x72);
        // ─── weapon 12 ──────────────────────────────────────────────
        bu!(0x15E4, 0x73);
        bu!(0x15E8, 0x74);
        bu!(0x15CC, 0x75);
        bs1k!(0x15DC, 0x76);
        bu!(0x161C, 0x77);
        bu!(0x1618, 0x78);
        bu!(0x1614, 0x79);
        bu!(0x15FC, 0x7A);
        bu!(0x1604, 0x7B);
        // ─── weapon 13 ──────────────────────────────────────────────
        bu!(0x17B4, 0x7C);
        bu!(0x17B8, 0x7D);
        bu!(0x179C, 0x7E);
        bs1k!(0x17AC, 0x7F);
        bu!(0x17EC, 0x80);
        bu!(0x17E8, 0x81);
        bu!(0x17E4, 0x82);
        bu!(0x17CC, 0x83);
        bu!(0x17D4, 0x84);
        // ─── weapon 14 ──────────────────────────────────────────────
        bu!(0x1984, 0x85);
        bu!(0x1988, 0x86);
        bu!(0x196C, 0x87);
        bs1k!(0x197C, 0x88);
        bu!(0x19BC, 0x89);
        bu!(0x19B8, 0x8A);
        bu!(0x19B4, 0x8B);
        bu!(0x199C, 0x8C);
        bu!(0x19A4, 0x8D);
        // ─── weapon 16 ──────────────────────────────────────────────
        bu!(0x1D24, 0x8E);
        bu!(0x1D28, 0x8F);
        bu!(0x1D0C, 0x90);
        bs1k!(0x1D1C, 0x91);
        bu!(0x1D40, 0x92);
        bu!(0x1D3C, 0x93);
        bu!(0x1D38, 0x94);
        bu!(0x1D44, 0x95);
        // ─── weapon 17 ──────────────────────────────────────────────
        bu!(0x1EF4, 0x96);
        bu!(0x1EF8, 0x97);
        bu!(0x1EDC, 0x98);
        bs1k!(0x1EEC, 0x99);
        bu!(0x1F1C, 0x9A);
        bu!(0x1F18, 0x9B);
        bu!(0x1F14, 0x9C);
        wu!(0x1F20, 0x9E);
        // ─── weapon 18 ──────────────────────────────────────────────
        bu!(0x20C4, 0xA0);
        bu!(0x20C8, 0xA1);
        bu!(0x20AC, 0xA2);
        bs1k!(0x20BC, 0xA3);
        bu!(0x20E8, 0xA4);
        bu!(0x20EC, 0xA5);
        bu!(0x20E4, 0xA6);
        wu!(0x20D8, 0xA8);
        bu!(0x20DC, 0xAA);
        // ─── weapon 20 ──────────────────────────────────────────────
        bu!(0x2464, 0xAC);
        bu!(0x2468, 0xAD);
        bu!(0x244C, 0xAE);
        bs1k!(0x245C, 0xAF);
        bu!(0x247C, 0xB0);
        bu!(0x2480, 0xB1);
        // scheme[0xB2] → +0x2478, also clears +0x2460 when zero.
        {
            let v = *scheme.add(0xB2) as u32;
            *(table.add(0x2478) as *mut u32) = v;
            if v == 0 {
                *(table.add(0x2460) as *mut u32) = 0;
            }
        }
        // ─── weapon 21 ──────────────────────────────────────────────
        bu!(0x2634, 0xB3);
        bu!(0x2638, 0xB4);
        bu!(0x261C, 0xB5);
        bs1k!(0x262C, 0xB6);
        bu!(0x2664, 0xB7);
        bu!(0x2660, 0xB8);
        bu!(0x265C, 0xB9);
        bu1k!(0x269C, 0xBA);
        // ─── weapon 22 ──────────────────────────────────────────────
        bu!(0x2804, 0xBB);
        bu!(0x2808, 0xBC);
        bu!(0x27EC, 0xBD);
        bs1k!(0x27FC, 0xBE);
        bs1k!(0x2820, 0xC2); // out-of-order — WA reads scheme[0xC2] before [0xBF..0xC1]
        bu!(0x2834, 0xBF);
        bu!(0x2830, 0xC0);
        bu!(0x282C, 0xC1);
        // ─── weapon 23 ──────────────────────────────────────────────
        bu!(0x29D4, 0xC3);
        bu!(0x29D8, 0xC4);
        bu!(0x29BC, 0xC5);
        bs1k!(0x29CC, 0xC6);
        bu!(0x2A04, 0xC7);
        bu!(0x2A00, 0xC8);
        bu!(0x29FC, 0xC9);
        bu1k!(0x2A3C, 0xCA);
        // ─── weapon 24 ──────────────────────────────────────────────
        bu!(0x2BA4, 0xCB);
        bu!(0x2BA8, 0xCC);
        bu!(0x2B8C, 0xCD);
        bs1k!(0x2B9C, 0xCE);
        bu!(0x2BD4, 0xCF);
        bu!(0x2BD0, 0xD0);
        bu!(0x2BCC, 0xD1);
        bu1k!(0x2C0C, 0xD2);
        // ─── weapon 25 (re-overlays scheme bytes 0xCB..0xD2 from weapon 24) ─
        bu!(0x2D74, 0xCB);
        bu!(0x2D78, 0xCC);
        bu!(0x2D5C, 0xCD);
        bs1k!(0x2D6C, 0xCE);
        bu!(0x2DA4, 0xCF);
        bu!(0x2DA0, 0xD0);
        bu!(0x2D9C, 0xD1);
        bu1k!(0x2DDC, 0xD2);
        // ─── weapon 27 ──────────────────────────────────────────────
        bu!(0x3114, 0xD3);
        bu!(0x3118, 0xD4);
        bu!(0x30FC, 0xD5);
        bs1k!(0x310C, 0xD6);
        bu!(0x3158, 0xD7);
        bu!(0x3154, 0xD8);
        bu!(0x3150, 0xD9);
        bu!(0x312C, 0xDA);
        // (gap: scheme bytes 0xDB..0xE7 unread)
        // ─── weapon 28 (napalm cap → 10000/byte at +0x33D0) ─────────
        bu!(0x32E4, 0xE8);
        bu!(0x32E8, 0xE9);
        bu!(0x32CC, 0xEA);
        bs1k!(0x32DC, 0xEB);
        bu!(0x3328, 0xEC);
        bu!(0x3324, 0xED);
        bu!(0x3320, 0xEE);
        bu!(0x32FC, 0xEF);
        bu!(0x33CC, 0xF0);
        bu50!(0x33D4, 0xF1);
        inv10k!(0x33D0, 0xF2);
        // (gap: scheme byte 0xF3 unread)
        // ─── weapon 32 ──────────────────────────────────────────────
        bu!(0x3A24, 0xF4);
        bu!(0x3A28, 0xF5);
        bu!(0x3A0C, 0xF6);
        bs1k!(0x3A1C, 0xF7);
        bu!(0x3A3C, 0xF8);
        bu!(0x3A40, 0xF9);
        bu!(0x3A38, 0xFA);
        wu!(0x3A44, 0xFC);
        // ─── weapon 33 ──────────────────────────────────────────────
        bu!(0x3BF4, 0xFE);
        bu!(0x3BF8, 0xFF);
        bu!(0x3BDC, 0x100);
        bs1k!(0x3BEC, 0x101);
        bu!(0x3C0C, 0x102);
        bu!(0x3C10, 0x103);
        bu!(0x3C08, 0x104);
        wu!(0x3C14, 0x106);
        // ─── weapon 34 ──────────────────────────────────────────────
        bu!(0x3DC4, 0x108);
        bu!(0x3DC8, 0x109);
        bu!(0x3DAC, 0x10A);
        bs1k!(0x3DBC, 0x10B);
        // ─── weapon 35 ──────────────────────────────────────────────
        bu!(0x3F94, 0x10C);
        bu!(0x3F98, 0x10D);
        bu!(0x3F7C, 0x10E);
        bs1k!(0x3F8C, 0x10F);
        bu!(0x3FAC, 0x110);
        bu!(0x3FA8, 0x111);
        // ─── weapon 37 (3-byte poke at +0x4348..+0x434A) ────────────
        bu!(0x4334, 0x112);
        bu!(0x4338, 0x113);
        bu!(0x431C, 0x114);
        bs1k!(0x432C, 0x115);
        bb!(0x4348, 0x116);
        bb!(0x4349, 0x117);
        bb!(0x434A, 0x118);
        // ─── weapon 38 ──────────────────────────────────────────────
        bu!(0x4504, 0x119);
        bu!(0x4508, 0x11A);
        bu!(0x44EC, 0x11B);
        bs1k!(0x44FC, 0x11C);
        // ─── weapon 39 ──────────────────────────────────────────────
        bu!(0x46D4, 0x11D);
        bu!(0x46D8, 0x11E);
        bu!(0x46BC, 0x11F);
        bs1k!(0x46CC, 0x120);
        bu!(0x46E8, 0x121);
        // ─── weapon 40 ──────────────────────────────────────────────
        bu!(0x48A4, 0x122);
        bu!(0x48A8, 0x123);
        bu!(0x488C, 0x124);
        bs1k!(0x489C, 0x125);
        // ─── weapon 43 ──────────────────────────────────────────────
        bu!(0x4E14, 0x126);
        bu!(0x4E18, 0x127);
        bu!(0x4DFC, 0x128);
        bs1k!(0x4E0C, 0x129);
        bu!(0x4E44, 0x12A);
        bu!(0x4E40, 0x12B);
        bu!(0x4E3C, 0x12C);
        bu1k!(0x4E7C, 0x12D);
        bu!(0x4E6C, 0x12E);
        // ─── weapon 47 (petrol-bomb cap → 10000/byte at +0x562C) ────
        bu!(0x5554, 0x12F);
        bu!(0x5558, 0x130);
        bu!(0x553C, 0x131);
        bs1k!(0x554C, 0x132);
        bu!(0x5584, 0x133);
        bu!(0x5580, 0x134);
        bu!(0x557C, 0x135);
        bu!(0x5628, 0x136);
        bu50!(0x5630, 0x137);
        inv10k!(0x562C, 0x138);
        // ─── weapon 52 ──────────────────────────────────────────────
        bu!(0x5E64, 0x139);
        bu!(0x5E68, 0x13A);
        bu!(0x5E4C, 0x13B);
        bs1k!(0x5E5C, 0x13C);
        bu!(0x5E94, 0x13D);
        bu!(0x5E90, 0x13E);
        bu!(0x5E8C, 0x13F);
        // ─── weapon 53 ──────────────────────────────────────────────
        bu!(0x6034, 0x140);
        bu!(0x6038, 0x141);
        bu!(0x601C, 0x142);
        bs1k!(0x602C, 0x143);
        bu!(0x6064, 0x144);
        bu!(0x6060, 0x145);
        bu!(0x605C, 0x146);
        bs1k!(0x609C, 0x147);
        // ─── weapon 64 ──────────────────────────────────────────────
        bu!(0x7424, 0x148);
        bu!(0x7428, 0x149);
        bu!(0x740C, 0x14A);
        bs1k!(0x741C, 0x14B);
        // ─── weapon 65 ──────────────────────────────────────────────
        bu!(0x75F4, 0x14C);
        bu!(0x75F8, 0x14D);
        bu!(0x75DC, 0x14E);
        bs1k!(0x75EC, 0x14F);
        // ─── weapon 67 ──────────────────────────────────────────────
        bu!(0x7994, 0x150);
        bu!(0x7998, 0x151);
        bu!(0x797C, 0x152);
        bs1k!(0x798C, 0x153);
        // ─── weapon 63 (out-of-order: scheme bytes 0x158..0x15B before 0x154..0x157) ─
        bu!(0x7254, 0x158);
        bu!(0x7258, 0x159);
        bu!(0x723C, 0x15A);
        bs1k!(0x724C, 0x15B);
        // ─── weapon 66 ──────────────────────────────────────────────
        bu!(0x77C4, 0x154);
        bu!(0x77C8, 0x155);
        bu!(0x77AC, 0x156);
        bs1k!(0x77BC, 0x157);
        // ─── weapon 68 ──────────────────────────────────────────────
        bu!(0x7B64, 0x15C);
        bu!(0x7B68, 0x15D);
        bu!(0x7B4C, 0x15E);
        bs1k!(0x7B5C, 0x15F);
        // ─── weapon 62 (first pass) ─────────────────────────────────
        bu!(0x7084, 0x160);
        bu!(0x7088, 0x161);
        bu!(0x706C, 0x162);
        bs1k!(0x707C, 0x163);
        // ─── weapon 69 ──────────────────────────────────────────────
        bu!(0x7D34, 0x165);
        bu!(0x7D38, 0x166);
        bu!(0x7D1C, 0x167);
        bs1k!(0x7D2C, 0x168);
        // ─── weapon 70 ──────────────────────────────────────────────
        bu!(0x7F04, 0x169);
        bu!(0x7F08, 0x16A);
        bu!(0x7EEC, 0x16B);
        bs1k!(0x7EFC, 0x16C);
        // ─── weapon 62 (redundant second pass + byte poke at +0x7098) ─
        bu!(0x7084, 0x160);
        bu!(0x7088, 0x161);
        bu!(0x706C, 0x162);
        bs1k!(0x707C, 0x163);
        bb!(0x7098, 0x164);
        // ─── weapon 48 ──────────────────────────────────────────────
        bu!(0x5724, 0x16D);
        bu!(0x5728, 0x16E);
        bu!(0x570C, 0x16F);
        bs1k!(0x571C, 0x170);
        bu!(0x5818, 0x171);
        bu!(0x5814, 0x172);
        bu!(0x5810, 0x173);
        bu1k!(0x5850, 0x174);
        bu!(0x589C, 0x175);
        // ─── weapon 15 ──────────────────────────────────────────────
        bu!(0x1B54, 0x176);
        bu!(0x1B58, 0x177);
        bu!(0x1B3C, 0x178);
        bs1k!(0x1B4C, 0x179);
        bu!(0x1B6C, 0x17A);
        // ─── weapon 9 (extension at +0x1074..+0x1088) ───────────────
        bu!(0x1074, 0x17B);
        bu!(0x1078, 0x17C);
        bu!(0x105C, 0x17D);
        bs1k!(0x106C, 0x17E);
        bb!(0x1088, 0x17F);
        // ─── weapon 44 ──────────────────────────────────────────────
        bu!(0x4FE4, 0x180);
        bu!(0x4FE8, 0x181);
        bu!(0x4FCC, 0x182);
        bs1k!(0x4FDC, 0x183);
        bu!(0x4FFC, 0x184);
        // ─── weapon 5 (extension at +0x934..+0xA60) ─────────────────
        bu!(0x934, 0x185);
        bu!(0x938, 0x186);
        bu!(0x91C, 0x187);
        bs1k!(0x92C, 0x188);
        bu!(0xA28, 0x189);
        bu!(0xA24, 0x18A);
        bu!(0xA20, 0x18B);
        bu1k!(0xA60, 0x18C);
        // ─── weapon 26 ──────────────────────────────────────────────
        bu!(0x2F44, 0x18D);
        bu!(0x2F48, 0x18E);
        bu!(0x2F2C, 0x18F);
        bs1k!(0x2F3C, 0x190);
        bu!(0x3038, 0x191);
        bu!(0x3034, 0x192);
        bu!(0x3030, 0x193);
        // ─── weapon 59 ──────────────────────────────────────────────
        bu!(0x6B14, 0x194);
        bu!(0x6B18, 0x195);
        bu!(0x6AFC, 0x196);
        bs1k!(0x6B0C, 0x197);

        if (*game_info).force_all_weapons_aim != 0 {
            let weapon_table = (*world).weapon_table;
            for w in WeaponId::iter_known() {
                (*weapon_table).entries[w.0 as usize].requires_aiming = 1;
            }
        }
    }
}
