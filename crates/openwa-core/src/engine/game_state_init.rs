//! Pure Rust implementations of DDGame__InitGameState sub-functions.
//!
//! Each function is hooked individually so it works regardless of whether
//! InitGameState itself is Rust or the original WA code.

use crate::engine::ddgame::DDGame;
use crate::engine::game_info::GameInfo;
use crate::wa_alloc::wa_malloc;

/// Pure Rust implementation of SpriteGfxTable__Init (0x541620).
///
/// Convention: fastcall(ECX=base, EDX=count), plain RET.
///
/// Initializes two parallel arrays:
/// - `base[0..count]`: identity permutation (index[i] = i)
/// - `base+0x2000[0..count]`: all 0xFFFFFFFF (unused markers)
/// Plus 3 trailer fields at +0x3000/+0x3004/+0x3008.
pub unsafe fn sprite_gfx_table_init(base: *mut u8, count: u32) {
    for i in 0..count {
        // Index table: base + i*4 = i
        *((base as *mut u32).add(i as usize)) = i;
        // Lookup table: base + 0x2000 + i*4 = 0xFFFFFFFF
        *((base.add(0x2000) as *mut u32).add(i as usize)) = 0xFFFF_FFFF;
    }
    *(base.add(0x3000) as *mut u32) = count;
    *(base.add(0x3004) as *mut u32) = 0;
    *(base.add(0x3008) as *mut u32) = count;
}

/// Pure Rust implementation of RingBuffer__Init (0x541060).
///
/// Convention: usercall(EAX=capacity, ESI=struct_ptr), plain RET.
///
/// Allocates a zero-filled buffer of `capacity` bytes (aligned + 0x20 header),
/// then initializes the ring buffer struct (7 DWORDs):
/// - `[0]`: data pointer
/// - `[1]`: capacity
/// - `[2]`-`[6]`: zeroed (head, tail, count, etc.)
pub unsafe fn ring_buffer_init(struct_ptr: *mut u8, capacity: u32) {
    let alloc_size = ((capacity + 3) & !3) + 0x20;
    let data = wa_malloc(alloc_size);
    if !data.is_null() {
        core::ptr::write_bytes(data, 0, capacity as usize);
    }

    let s = struct_ptr as *mut u32;
    *s.add(0) = data as u32; // data pointer
    *s.add(1) = capacity; // capacity
    *s.add(2) = 0; // field 2
    *s.add(3) = 0; // field 3
    *s.add(4) = 0; // field 4
    *s.add(5) = 0; // field 5
    *s.add(6) = 0; // field 6
}

/// Maximum number of teams (13 slots: 6 real + sentinels/padding).
const MAX_TEAMS: usize = 13;

/// Team entry stride in GameInfo (3000 bytes per team slot).
const TEAM_ENTRY_STRIDE: usize = 3000;

/// Helper: read the GameInfo pointer from a DDGameWrapper pointer.
///
/// Chain: wrapper+0x488 → DDGame, DDGame+0x24 → GameInfo.
#[inline]
unsafe fn game_info_from_wrapper(wrapper: *mut u8) -> *mut GameInfo {
    let ddgame = *(wrapper.add(0x488) as *const *mut DDGame);
    (*ddgame).game_info
}

/// Helper: read the DDGame pointer from a DDGameWrapper pointer.
#[inline]
unsafe fn ddgame_from_wrapper(wrapper: *mut u8) -> *mut DDGame {
    *(wrapper.add(0x488) as *const *mut DDGame)
}

/// Pure Rust implementation of CGameTask_InitTeamScoring_Maybe (0x528510).
///
/// Convention: fastcall(ECX=wrapper), plain RET.
///
/// Initializes 7 parallel u32 arrays (13 elements each) in CGameTask, then
/// sets the starting-team flag, initializes team task pointer flags based on
/// game mode (training vs normal), and zeros CTask sub-object fields.
///
/// # CGameTask array layout (all 13 × u32):
/// - +0x0F4..+0x128: array_0  — zeroed
/// - +0x128..+0x15C: array_1  — zeroed
/// - +0x15C..+0x190: array_2  — scoring_param_a * 50
/// - +0x190..+0x1C4: array_3  — zeroed
/// - +0x1C4..+0x1F8: array_4  — scoring_param_b * 50
/// - +0x1F8..+0x22C: array_5  — scoring_param_b * 50
/// - +0x2BC..+0x2F0: array_6  — set to 1
///
/// # Other fields:
/// - +0x128[starting_team]: set to 1 (starting team marker in array_1)
/// - +0x22C[0..team_count]: team activity flags (-1 normal, -2 training)
/// - +0x054[0..team_count]: CTask pointers — sub-fields zeroed if non-null
pub unsafe fn init_team_scoring(wrapper: *mut u8) {
    let game_info = game_info_from_wrapper(wrapper);

    let scoring_param_a = (*game_info).scoring_param_a as u32;
    let scoring_param_b = (*game_info).scoring_param_b as u32;
    let value_a = scoring_param_a * 50; // uVar1 * 0x32
    let value_b = scoring_param_b * 50; // iVar4

    // Initialize 7 parallel arrays (13 elements each).
    // puVar3 starts at wrapper+0x128, indexed relative to that.
    let base = wrapper.add(0x128) as *mut u32;
    for i in 0..MAX_TEAMS {
        let p = base.add(i);
        *p.sub(0xd) = 0; // +0x0F4[i]: array_0 — zeroed
        *p = 0; // +0x128[i]: array_1 — zeroed
        *p.add(0xd) = value_a; // +0x15C[i]: array_2 — scoring_param_a * 50
        *p.add(0x27) = value_b; // +0x1C4[i]: array_4 — scoring_param_b * 50
        *p.add(0x34) = value_b; // +0x1F8[i]: array_5 — scoring_param_b * 50
        *p.add(0x1a) = 0; // +0x190[i]: array_3 — zeroed
        *p.add(99) = 1; // +0x2BC[i]: array_6 — set to 1
    }

    // Mark the starting team in array_1: wrapper+0x128[starting_team] = 1
    let starting_team = (*game_info).starting_team_index as i32;
    *(base.add(starting_team as usize)) = 1;

    // GameInfo+0xD9DD: mode flag (i8, negative = training/replay mode)
    let mode_flag = (*game_info).game_mode_flag;

    // GameInfo+0x0000: first byte = total team count (num_teams, u8)
    let num_teams = (*game_info).num_teams as i32;

    // Team activity flags at wrapper+0x22C (one u32 per team)
    let flags_base = wrapper.add(0x22C) as *mut u32;

    if mode_flag < 0 {
        // Training/replay mode: set all flags to -2 (0xFFFFFFFE)
        if num_teams > 0 {
            for i in 0..num_teams as usize {
                *flags_base.add(i) = 0xFFFF_FFFE;
            }
        }

        // For each active team slot in GameInfo, clear the flag to 0
        // GameInfo+0x44C: team count for alliance data (u8)
        let game_info2 = game_info_from_wrapper(wrapper);
        let alliance_team_count = (*game_info2).speech_team_count as i32;
        if alliance_team_count > 0 {
            let mut offset: usize = 0;
            for _ in 0..alliance_team_count {
                // GameInfo+0x450 + team_index*3000: alliance group (i8)
                let alliance_group =
                    *((game_info2 as *const u8).add(0x450 + offset) as *const i8) as i32;
                if alliance_group >= 0 {
                    *flags_base.add(alliance_group as usize) = 0;
                }
                // Re-read game_info in case pointer chain changed (matches original)
                let gi = game_info_from_wrapper(wrapper);
                let _ = gi; // original re-reads but we already have the value
                offset += TEAM_ENTRY_STRIDE;
            }
        }
    } else {
        // Normal mode: set all flags to -1 (0xFFFFFFFF)
        if num_teams > 0 {
            for i in 0..num_teams as usize {
                *flags_base.add(i) = 0xFFFF_FFFF;
            }
        }

        // Set starting team's flag to 1
        // GameInfo+0xD9DD: starting team index for flags (i8, reuse game_mode_flag which is the same field)
        let starting_flag_team = (*game_info_from_wrapper(wrapper)).game_mode_flag as i32;
        *flags_base.add(starting_flag_team as usize) = 1;
    }

    // Zero CTask sub-object fields for each team's task pointer
    // wrapper+0x54[i] = CTask pointer, clear offsets +0x08..+0x18 (5 DWORDs)
    let num_teams2 = (*game_info_from_wrapper(wrapper)).num_teams as i32;
    if num_teams2 > 0 {
        let task_ptrs = wrapper.add(0x54) as *mut *mut u8;
        for i in 0..num_teams2 as usize {
            let task = *task_ptrs.add(i);
            if !task.is_null() {
                *(task.add(0x18) as *mut u32) = 0;
                *(task.add(0x14) as *mut u32) = 0;
                *(task.add(0x10) as *mut u32) = 0;
                *(task.add(0x0C) as *mut u32) = 0;
                *(task.add(0x08) as *mut u32) = 0;
            }
        }
    }
}

/// Pure Rust implementation of CGameTask_InitAllianceData_Maybe (0x5262D0).
///
/// Convention: usercall(EAX=wrapper), plain RET.
///
/// Initializes alliance bitmask table, per-team scoring flags in DDGame,
/// and an auxiliary alliance array.
///
/// # Layout:
/// - wrapper+0x350..+0x384: 13 × u32 alliance bitmasks (one per alliance group)
/// - DDGame.team_scoring_flags: 6 × u32 per-team scoring flags
/// - wrapper+0x3AC..: auxiliary array of non-starting-team alliance IDs (+0x10)
///
/// # GameInfo fields used:
/// - num_teams
/// - speech_team_count (alliance_team_count)
/// - +0x450 + i*3000: alliance_group (i8) for team i
/// - +0x451 + i*3000: alliance_id (u8) for team i
/// - game_version
/// - starting_team_index
///
/// - wrapper+0x490: first byte used as scoring override value
pub unsafe fn init_alliance_data(wrapper: *mut u8) {
    // Zero 13 alliance bitmasks at wrapper+0x350..+0x384
    for i in 0..MAX_TEAMS {
        *(wrapper.add(0x350) as *mut u32).add(i) = 0;
    }

    // Build alliance bitmasks: for each team, set bit (alliance_id & 0x1F)
    // in bitmask[alliance_group]
    let game_info = game_info_from_wrapper(wrapper);
    let num_teams = (*game_info).num_teams as i32;

    // Global DAT_007a087c+4 = 0 (static variable, offset 0x7A0880)
    // We skip this — it's a global we don't own. The original sets it but
    // it appears unused in the init path.

    let alliance_team_count = (*game_info).speech_team_count as i32;
    if num_teams != 0 && alliance_team_count > 0 {
        let mut offset: usize = 0;
        for _ in 0..alliance_team_count {
            let gi = game_info_from_wrapper(wrapper);
            let alliance_group = *((gi as *const u8).add(0x450 + offset) as *const i8) as i32;
            let alliance_id = *((gi as *const u8).add(0x451 + offset));

            if alliance_group >= 0 {
                let bitmask = (wrapper.add(0x350) as *mut u32).add(alliance_group as usize);
                *bitmask |= 1u32 << (alliance_id & 0x1F);
            }

            offset += TEAM_ENTRY_STRIDE;
        }
    }

    // Zero 6 per-team scoring flags (DDGame.team_scoring_flags)
    let ddgame = ddgame_from_wrapper(wrapper);
    (*ddgame).team_scoring_flags = [0u32; 6];

    // Build scoring flags and auxiliary alliance array
    let game_info = game_info_from_wrapper(wrapper);
    let alliance_team_count = (*game_info).speech_team_count as i32;

    // Local tracking: which alliance IDs we've already added to auxiliary array
    let mut seen: [u8; 6] = [0; 6]; // local_8[0..6], indexed by alliance_id
    let mut aux_count: usize = 0; // index into wrapper+0x3AC auxiliary array

    if alliance_team_count > 0 {
        let mut team_offset: usize = 0;

        for team_idx in 0..alliance_team_count {
            let gi = game_info_from_wrapper(wrapper);
            let alliance_group = *((gi as *const u8).add(0x450 + team_offset) as *const i8) as i32;
            let alliance_id = *((gi as *const u8).add(0x451 + team_offset)) as u32;

            let ddgame = ddgame_from_wrapper(wrapper);
            let scoring_flag =
                core::ptr::addr_of_mut!((*ddgame).team_scoring_flags[team_idx as usize]);

            if alliance_group < 0 {
                // No alliance: flag = value from wrapper+0x490 (i8 sign-extended)
                let override_val = *(wrapper.add(0x490) as *const i8) as i32;
                *scoring_flag = override_val as u32;
            } else {
                // Check if this team is allied with the starting team
                let gi = game_info_from_wrapper(wrapper);
                let num_teams_byte = (*gi).num_teams;
                let override_byte = *(wrapper.add(0x490) as *const u8);

                if num_teams_byte == 0 || override_byte != 0 {
                    // No teams or override set: flag = 1
                    *scoring_flag = 1;
                } else {
                    let game_version = (*gi).game_version;
                    let starting_team = (*gi).starting_team_index as i32;

                    let team_bitmask: u32 = if game_version < 0x83 {
                        // Old version: use full alliance bitmask
                        *((wrapper.add(0x350) as *const u32).add(alliance_group as usize))
                    } else {
                        // New version: just the single team's bit
                        1u32 << (alliance_id & 0x1F)
                    };

                    let starting_bitmask =
                        *((wrapper.add(0x350) as *const u32).add(starting_team as usize));

                    if (starting_bitmask & team_bitmask) != 0 {
                        // Same alliance as starting team: flag = 1
                        *scoring_flag = 1;
                    }
                    // else: flag stays 0 (already zeroed above)
                }

                // Build auxiliary array for non-starting-team alliances
                let gi2 = game_info_from_wrapper(wrapper);
                let starting_team2 = (*gi2).starting_team_index as i32;

                if alliance_group != starting_team2 && seen[alliance_id as usize] == 0 {
                    let starting_bitmask2 =
                        *((wrapper.add(0x350) as *const u32).add(starting_team2 as usize));
                    if (starting_bitmask2 & (1u32 << (alliance_id & 0x1F))) != 0 {
                        // Store alliance_id + 0x10 into auxiliary array
                        let aux_ptr = (wrapper.add(0x3AC) as *mut u32).add(aux_count);
                        *aux_ptr = alliance_id + 0x10;
                        aux_count += 1;
                        seen[alliance_id as usize] = 1;
                    }
                }
            }

            team_offset += TEAM_ENTRY_STRIDE;
        }
    }
}

/// Pure Rust implementation of DDGame__IsSuperWeapon (0x565960).
///
/// Convention: usercall(EAX=weapon_index) + 1 stack param (param_1: u8), plain RET.
/// Returns 1 for super weapons, param_1 for 0x3B, 0 otherwise.
pub unsafe fn is_super_weapon(weapon_index: u32, param_1: u8) -> u8 {
    match weapon_index {
        10 | 0x13 | 0x1D | 0x1E | 0x1F | 0x24 | 0x29 | 0x2A | 0x2D | 0x2E | 0x31 | 0x32 | 0x33
        | 0x36 | 0x37 | 0x38 | 0x3C | 0x3D => 1,
        0x3B => param_1,
        _ => 0,
    }
}

/// Pure Rust implementation of DDGame__CheckWeaponAvail (0x53FFC0).
///
/// Convention: fastcall(ECX=ddgame) + unaff_ESI=weapon_index, plain RET. Returns i32.
///
/// Checks whether a weapon (1..0x46) is available given current game state.
/// `ddgame` is the DDGame pointer directly (not wrapper).
pub unsafe fn check_weapon_avail(ddgame: *mut DDGame, weapon_index: u32) -> i32 {
    let gi = (*ddgame).game_info;
    let game_version = (*gi).game_version;
    let num_teams = (*gi).num_teams;

    use crate::game::weapon::Weapon;

    // Step 1: Special per-weapon disabling rules
    match weapon_index {
        w if w == Weapon::Earthquake as u32
            || w == Weapon::NuclearTest as u32
            || w == Weapon::Armageddon as u32 =>
        {
            if (*gi).net_config_2 != 0 && (*gi).net_weapon_exception == 0 {
                return 0;
            }
        }
        w if w == Weapon::Donkey as u32 => {
            if (*gi).donkey_disabled != 0 {
                return 0;
            }
        }
        w if w == Weapon::Invisibility as u32 => {
            if (*gi).invisibility_mode == 0 {
                if (*ddgame).network_ecx == 0 {
                    return 0;
                }
            } else if (num_teams as u32) < 2 {
                return 0;
            }
        }
        w if w == Weapon::DoubleTurnTime as u32 => {
            if game_version > 0xD1 && (*gi).double_turn_time_threshold > 0x7FFF {
                return 0;
            }
        }
        _ => {}
    }

    // Step 2: Branch on weapon defined flag (nonzero = weapon exists in table).
    let weapon_table = (*ddgame).weapon_table;
    let defined = (*weapon_table).entries[weapon_index as usize].defined;

    if (*ddgame).level_width_raw == 0 || defined != 0 {
        // Main path: check super weapon flag
        let super_result = is_super_weapon(weapon_index, (*ddgame).version_flag_3);
        if super_result != 0 && (*gi).super_weapon_allowed == 0 {
            // (game_version < 0x2A) - 1: if < 0x2A → 0, else → -1
            return (game_version < 0x2A) as i32 - 1;
        }

        if (*ddgame).supersheep_restricted == 0 {
            return 1;
        }

        // AquaSheep (25) or SuperSheep (24) depending on weapon_index_offset
        let restricted_id = Weapon::AquaSheep as u32 - ((*gi).aquasheep_is_supersheep != 0) as u32;
        if weapon_index != restricted_id {
            return 1;
        }

        return 0;
    }

    // Else branch: level_width_raw != 0 AND weapon_entry == 0
    if game_version > 0x29 && (*gi).weapon_version_gate != 0 {
        return -2;
    }

    0
}

/// Bridge to DDGame__InitFeatureFlags (0x524700): stdcall(wrapper), RET 0x4.
unsafe fn wa_init_feature_flags(wrapper: *mut u8) {
    let f: unsafe extern "stdcall" fn(*mut u8) = core::mem::transmute(crate::rebase::rb(
        crate::address::va::DDGAME_INIT_FEATURE_FLAGS,
    ) as usize);
    f(wrapper);
}

/// Pure Rust implementation of CGameTask__InitTurnState (0x528690).
///
/// Convention: usercall(EAX=wrapper), plain RET.
///
/// Initializes turn-related state fields in both the DDGameWrapper and DDGame
/// structs. Zeroes camera state, timing fields, per-team flags, and calls
/// DDGame__InitFeatureFlags. Also dispatches a vtable call on the landscape
/// object.
pub unsafe fn init_turn_state(wrapper: *mut u8) {
    let ddgame = ddgame_from_wrapper(wrapper);
    let game_info = (*ddgame).game_info;

    // wrapper+0x458 = -1, wrapper+0x450 = 0, wrapper+0x454 = 0
    *(wrapper.add(0x458) as *mut u32) = 0xFFFF_FFFF;
    *(wrapper.add(0x450) as *mut u32) = 0;
    *(wrapper.add(0x454) as *mut u32) = 0;

    // DDGame+0x72E0/E4 = -1, DDGame+0x72E8 = 0
    (*ddgame)._unknown_72e0 = 0xFFFF_FFFF;
    (*ddgame).render_slot_count = 0xFFFF_FFFF;
    (*ddgame)._unknown_72e8 = 0;

    // Zero render entry table first u32 at DDGame+0x73B0, stride 0x14, while offset < 0x118
    {
        let base = core::ptr::addr_of_mut!((*ddgame).render_entries) as *mut u8;
        let mut off = 0u32;
        while off < 0x118 {
            *(base.add(off as usize) as *mut u32) = 0;
            off += 0x14;
        }
    }

    // More DDGame field zeroing
    (*ddgame).render_state_flag = 0;
    (*ddgame)._field_72f4 = 0;
    (*ddgame)._field_72f8 = 0;
    (*ddgame)._field_72fc = 0;
    (*ddgame)._field_7300 = 0;
    (*ddgame)._field_7304 = 0;

    // DDGame.rng_state_1/2 = game_info.rng_seed (RNG seed from scheme)
    let rng_seed = (*game_info).rng_seed;
    (*ddgame).rng_state_1 = rng_seed;
    (*ddgame).rng_state_2 = rng_seed; // duplicate

    (*ddgame)._field_7378 = 0;
    (*ddgame)._field_7374 = 0;
    (*ddgame)._field_737c = 0;
    (*ddgame)._field_77dc = 0;
    (*ddgame)._field_77e0 = 0;
    (*ddgame)._field_7784 = 0;

    // DDGame._field_7788 = game_info._field_f362 (byte → u32)
    (*ddgame)._field_7788 = (*game_info)._field_f362 as u32;
    (*ddgame)._field_778c = 0x10000; // Fixed-point 1.0
    (*ddgame)._field_7790 = 0;

    // Camera center: (level_width << 16) / 2, (level_height << 16) / 2
    let level_width = (*ddgame).level_width as i32;
    let level_height = (*ddgame).level_height as i32;
    let cx = (level_width << 16) / 2;
    let cy = (level_height << 16) / 2;
    (*ddgame).viewport_width = cx;
    (*ddgame).viewport_width_2 = cx; // duplicate
    (*ddgame).viewport_height = cy;
    (*ddgame).viewport_height_2 = cy; // duplicate

    (*ddgame)._field_7d84 = 0;
    (*ddgame)._field_7e4c = 0;
    (*ddgame)._field_77d4 = 0;
    (*ddgame)._field_77d8 = 0;

    // Per-team loop
    let num_teams = (*game_info).num_teams as i32;
    if num_teams > 0 {
        for i in 0..num_teams as usize {
            (*ddgame)._field_7d88[i] = 0;
            (*ddgame)._field_7dbc[i] = 1;
            (*ddgame)._field_7dc9[i] = 1;
            (*ddgame)._field_7dd6[i] = 0;
            (*ddgame)._field_7de3[i] = 0;
            (*ddgame)._field_7df0[i] = 0;
        }
    }

    (*ddgame)._field_7e03 = 0;
    (*ddgame)._field_7e04 = 0;

    // Call DDGame__InitFeatureFlags (600-line feature flag init, bridged)
    wa_init_feature_flags(wrapper);

    // Post-feature-flag field writes
    (*ddgame)._field_7e41 = 0;
    (*ddgame)._fields_7e50 = [0u32; 8];
    (*ddgame)._fields_7e88 = [0u32; 5];
    (*ddgame).field_7ea0 = 0;
    (*ddgame).field_7ea4 = 0;

    (*ddgame)._field_8148 = 1;

    let ddgame2 = ddgame_from_wrapper(wrapper);
    (*ddgame2)._field_8158 = 0;
    (*ddgame2)._field_815c = 0;
    let ddgame3 = ddgame_from_wrapper(wrapper);
    (*ddgame3)._field_8160 = 0;
    (*ddgame3)._field_8164 = 0;

    // Vtable dispatch: DDGame.landscape → vtable slot 1,
    // param = game_info.donkey_disabled (byte at +0xD94C)
    let ddgame4 = ddgame_from_wrapper(wrapper);
    let landscape = (*ddgame4).landscape as *mut u8;
    if !landscape.is_null() {
        let vtable = *(landscape as *const *const usize);
        let slot1: unsafe extern "thiscall" fn(*mut u8, u32) = core::mem::transmute(*vtable.add(1));
        let param = (*(*ddgame4).game_info).donkey_disabled as u32;
        slot1(landscape, param);
    }
}

/// Pure Rust implementation of CGameTask__InitLandscapeFlags (0x528480).
///
/// Convention: usercall(EAX=wrapper), plain RET.
///
/// Checks game_info.landscape_scheme_flag and dispatches landscape vtable slot 6 with
/// appropriate parameters, then updates DDGame.level_width_raw.
pub unsafe fn init_landscape_flags(wrapper: *mut u8) {
    let ddgame = ddgame_from_wrapper(wrapper);
    let game_info = (*ddgame).game_info;

    let scheme_flag = (*game_info).landscape_scheme_flag;

    // Read the 4 DDGame fields used as params
    let field_7318 = (*ddgame).gfx_color_table[3];
    let field_730c = (*ddgame).gfx_color_table[0];
    let field_734c = (*ddgame)._field_734c;
    let field_7340 = (*ddgame)._field_7340;

    // DDGame.landscape object
    let landscape = (*ddgame).landscape as *mut u8;
    let vtable = *(landscape as *const *const usize);
    // Vtable slot 6 = offset +0x18
    let slot6: unsafe extern "thiscall" fn(*mut u8, u32, u32, u32, u32, u32, u32, u32, u32) =
        core::mem::transmute(*vtable.add(6));

    if scheme_flag != 0 {
        // Call with (1, 1, 1, 1, field_7318, field_730c, field_734c, field_7340)
        slot6(
            landscape, 1, 1, 1, 1, field_7318, field_730c, field_734c, field_7340,
        );
        (*ddgame).level_width_raw = 1;
    } else if (*ddgame).level_width_raw != 0 {
        // Call with (0, 0, 1, 0, field_7318, field_730c, field_734c, field_7340)
        slot6(
            landscape, 0, 0, 1, 0, field_7318, field_730c, field_734c, field_7340,
        );
    }
}
