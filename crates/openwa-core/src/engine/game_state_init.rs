//! Pure Rust implementations of DDGame__InitGameState sub-functions,
//! and the top-level InitGameState itself.
//!
//! Each sub-function is hooked individually so it works regardless of whether
//! InitGameState itself is Rust or the original WA code.

use crate::audio::dssound::DSSound;
use crate::bitgrid::DisplayBitGrid;
use crate::engine::ddgame::DDGame;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_info::GameInfo;
use crate::fixed::Fixed;
use crate::render::display::gfx::DisplayGfx;
use crate::render::landscape::PCLandscape;
use crate::wa_alloc::{wa_malloc_struct_zeroed, wa_malloc_zeroed};

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
    use crate::wa_alloc::wa_malloc_zeroed;
    let alloc_size = ((capacity + 3) & !3) + 0x20;
    let data = wa_malloc_zeroed(alloc_size);

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
#[inline]
unsafe fn game_info_from_wrapper(wrapper: *mut DDGameWrapper) -> *mut GameInfo {
    (*(*wrapper).ddgame).game_info
}

/// Helper: read the DDGame pointer from a DDGameWrapper pointer.
#[inline]
unsafe fn ddgame_from_wrapper(wrapper: *mut DDGameWrapper) -> *mut DDGame {
    (*wrapper).ddgame
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
pub unsafe fn init_team_scoring(wrapper: *mut DDGameWrapper) {
    let game_info = game_info_from_wrapper(wrapper);

    let scoring_param_a = (*game_info).scoring_param_a as u32;
    let scoring_param_b = (*game_info).scoring_param_b as u32;
    let value_a = scoring_param_a * 50;
    let value_b = scoring_param_b * 50;

    // Initialize 7 parallel arrays (13 elements each).
    for i in 0..MAX_TEAMS {
        (*wrapper).team_score_array_0[i] = 0;
        (*wrapper).team_starting_marker[i] = 0;
        (*wrapper).team_scoring_a[i] = value_a;
        (*wrapper).team_score_array_3[i] = 0;
        (*wrapper).team_scoring_b[i] = value_b;
        (*wrapper).team_scoring_c[i] = value_b;
        (*wrapper).team_score_array_6[i] = 1;
    }

    // Mark the starting team
    let starting_team = (*game_info).starting_team_index as usize;
    (*wrapper).team_starting_marker[starting_team] = 1;

    let mode_flag = (*game_info).game_mode_flag;
    let num_teams = (*game_info).num_teams as i32;

    if mode_flag < 0 {
        // Training/replay mode: set all flags to -2 (0xFFFFFFFE)
        for i in 0..num_teams as usize {
            (*wrapper).team_activity_flags[i] = 0xFFFF_FFFE;
        }

        // For each active team slot in GameInfo, clear the flag to 0
        let game_info2 = game_info_from_wrapper(wrapper);
        let alliance_team_count = (*game_info2).speech_team_count as i32;
        if alliance_team_count > 0 {
            let mut offset: usize = 0;
            for _ in 0..alliance_team_count {
                let alliance_group =
                    *((game_info2 as *const u8).add(0x450 + offset) as *const i8) as i32;
                if alliance_group >= 0 {
                    (*wrapper).team_activity_flags[alliance_group as usize] = 0;
                }
                offset += TEAM_ENTRY_STRIDE;
            }
        }
    } else {
        // Normal mode: set all flags to -1 (0xFFFFFFFF)
        for i in 0..num_teams as usize {
            (*wrapper).team_activity_flags[i] = 0xFFFF_FFFF;
        }

        // Set starting team's flag to 1
        let starting_flag_team = (*game_info_from_wrapper(wrapper)).game_mode_flag as i32;
        (*wrapper).team_activity_flags[starting_flag_team as usize] = 1;
    }

    // Zero CTask sub-object fields for each team's task pointer
    let num_teams2 = (*game_info_from_wrapper(wrapper)).num_teams as i32;
    if num_teams2 > 0 {
        for i in 0..num_teams2 as usize {
            let task = (*wrapper).team_task_ptrs[i];
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
pub unsafe fn init_alliance_data(wrapper: *mut DDGameWrapper) {
    // Zero 13 alliance bitmasks
    (*wrapper)._alliance_bitmasks = [0u32; 13];

    // Build alliance bitmasks: for each team, set bit (alliance_id & 0x1F)
    // in bitmask[alliance_group]
    let game_info = game_info_from_wrapper(wrapper);
    let num_teams = (*game_info).num_teams as i32;

    let alliance_team_count = (*game_info).speech_team_count as i32;
    if num_teams != 0 && alliance_team_count > 0 {
        let mut offset: usize = 0;
        for _ in 0..alliance_team_count {
            let gi = game_info_from_wrapper(wrapper);
            let alliance_group = *((gi as *const u8).add(0x450 + offset) as *const i8) as i32;
            let alliance_id = *((gi as *const u8).add(0x451 + offset));

            if alliance_group >= 0 {
                (*wrapper)._alliance_bitmasks[alliance_group as usize] |=
                    1u32 << (alliance_id & 0x1F);
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
    let mut seen: [u8; 6] = [0; 6];
    let mut aux_count: usize = 0;

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
                // No alliance: flag = value from replay_flag_a (i8 sign-extended)
                let override_val = (*wrapper).replay_flag_a as i8 as i32;
                *scoring_flag = override_val as u32;
            } else {
                // Check if this team is allied with the starting team
                let gi = game_info_from_wrapper(wrapper);
                let num_teams_byte = (*gi).num_teams;
                let override_byte = (*wrapper).replay_flag_a;

                if num_teams_byte == 0 || override_byte != 0 {
                    *scoring_flag = 1;
                } else {
                    let game_version = (*gi).game_version;
                    let starting_team = (*gi).starting_team_index as i32;

                    let team_bitmask: u32 = if game_version < 0x83 {
                        (*wrapper)._alliance_bitmasks[alliance_group as usize]
                    } else {
                        1u32 << (alliance_id & 0x1F)
                    };

                    let starting_bitmask = (*wrapper)._alliance_bitmasks[starting_team as usize];

                    if (starting_bitmask & team_bitmask) != 0 {
                        *scoring_flag = 1;
                    }
                }

                // Build auxiliary array for non-starting-team alliances
                let gi2 = game_info_from_wrapper(wrapper);
                let starting_team2 = (*gi2).starting_team_index as i32;

                if alliance_group != starting_team2 && seen[alliance_id as usize] == 0 {
                    let starting_bitmask2 = (*wrapper)._alliance_bitmasks[starting_team2 as usize];
                    if (starting_bitmask2 & (1u32 << (alliance_id & 0x1F))) != 0 {
                        (*wrapper).slot_to_team[aux_count] = (alliance_id + 0x10) as i32;
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
unsafe fn wa_init_feature_flags(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) = core::mem::transmute(
        crate::rebase::rb(crate::address::va::DDGAME_INIT_FEATURE_FLAGS) as usize,
    );
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
pub unsafe fn init_turn_state(wrapper: *mut DDGameWrapper) {
    let ddgame = ddgame_from_wrapper(wrapper);
    let game_info = (*ddgame).game_info;

    (*wrapper)._field_458 = 0xFFFF_FFFF;
    (*wrapper)._field_450 = 0;
    (*wrapper)._field_454 = 0;

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
    (*ddgame)._field_778c = Fixed::ONE;
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

    // Landscape vtable slot 1: set control flag (donkey_disabled)
    let ddgame4 = ddgame_from_wrapper(wrapper);
    let landscape = (*ddgame4).landscape;
    if !landscape.is_null() {
        let param = (*(*ddgame4).game_info).donkey_disabled as u32;
        PCLandscape::set_control_flag_raw(landscape, param);
    }
}

/// Pure Rust implementation of CGameTask__InitLandscapeFlags (0x528480).
///
/// Convention: usercall(EAX=wrapper), plain RET.
///
/// Checks game_info.landscape_scheme_flag and dispatches landscape vtable slot 6 with
/// appropriate parameters, then updates DDGame.level_width_raw.
pub unsafe fn init_landscape_flags(wrapper: *mut DDGameWrapper) {
    let ddgame = ddgame_from_wrapper(wrapper);
    let game_info = (*ddgame).game_info;

    let scheme_flag = (*game_info).landscape_scheme_flag;

    // Read the 4 DDGame fields used as params
    let field_7318 = (*ddgame).gfx_color_table[3];
    let field_730c = (*ddgame).gfx_color_table[0];
    let field_734c = (*ddgame)._field_734c;
    let field_7340 = (*ddgame)._field_7340;

    let landscape = (*ddgame).landscape;

    if scheme_flag != 0 {
        PCLandscape::init_borders_raw(
            landscape, 1, 1, 1, 1, field_7318, field_730c, field_734c, field_7340,
        );
        (*ddgame).level_width_raw = 1;
    } else if (*ddgame).level_width_raw != 0 {
        PCLandscape::init_borders_raw(
            landscape, 0, 0, 1, 0, field_7318, field_730c, field_734c, field_7340,
        );
    }
}

// =============================================================================
// Top-level DDGame__InitGameState (0x526500) — Rust port
// =============================================================================

/// Pure Rust implementation of DDGame__InitGameState (0x526500).
///
/// Convention: stdcall(this=DDGameWrapper*), RET 0x4.
///
/// Called once per game session from the DDGameWrapper constructor to initialize
/// all game state: sub-objects, display layers, team configuration, weapon tables,
/// turn logic, and the initial state serialization/checksum.
pub unsafe fn init_game_state(wrapper: *mut DDGameWrapper) {
    use crate::address::va;
    use crate::rebase::rb;
    use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

    let ddgame = (*wrapper).ddgame;

    // ===== Copy replay mode flags from GameInfo =====
    let game_info = (*ddgame).game_info;
    (*wrapper).replay_flag_a = (*game_info).invisibility_mode as u8;
    (*wrapper).replay_flag_b = ((*game_info).invisibility_mode >> 8) as u8;

    // ===== SpriteGfxTable__Init =====
    {
        let game_version = (*game_info).game_version;
        let count: u32 = if game_version >= 0x3C { 0x400 } else { 0x100 };
        let table_ptr = core::ptr::addr_of_mut!((*ddgame)._unknown_600) as *mut u8;
        sprite_gfx_table_init(table_ptr, count);
    }

    // ===== Allocate HudPanel (0x940 bytes) =====
    {
        let mem = wa_malloc_zeroed(0x940);
        let result = if mem.is_null() {
            core::ptr::null_mut()
        } else {
            let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                core::mem::transmute(rb(va::HUD_PANEL_CONSTRUCTOR) as usize);
            ctor(mem)
        };
        (*ddgame).hud_panel = result;
    }

    // ===== Allocate weapon table buffer (0x80D0 bytes) =====
    {
        let mem = wa_malloc_zeroed(0x80D0);
        (*ddgame).weapon_table = mem as *mut crate::game::weapon::WeaponTable;
    }

    // ===== Allocate unknown 0x2C object =====
    {
        let mem = wa_malloc(0x2C);
        if !mem.is_null() {
            *(mem.add(0x08) as *mut u32) = 0;
            *(mem.add(0x0C) as *mut u32) = 0;
            *(mem.add(0x10) as *mut u32) = 0;
            *(mem.add(0x1C) as *mut u32) = 0;
            *(mem.add(0x20) as *mut u32) = 0;
            *(mem.add(0x24) as *mut u32) = 0;
        }
        (*ddgame)._unknown_51c = mem;
    }

    // ===== Allocate RenderQueue (0x12028 bytes) =====
    {
        let mem = wa_malloc_zeroed(0x12028) as *mut u32;
        if !mem.is_null() {
            *mem.add(0x4001) = 0; // mem[0x10004] = 0
            *mem = 0x10000; // mem[0] = 0x10000
        }
        (*ddgame).render_queue = mem as *mut crate::render::queue::RenderQueue;
    }

    // ===== Allocate GameStateStream (0x264 bytes) =====
    {
        let mem = wa_malloc_zeroed(0x264) as *mut u32;
        if !mem.is_null() {
            *mem = rb(0x664194); // GameStateStream vtable
            game_state_stream_init(mem.add(1));
            *mem.add(1) = 0;
            *mem.add(0x90) = ddgame as u32; // DDGame ptr backref
            *mem.add(0x8B) = 0;
            *mem.add(0x8C) = 0;
        }
        (*ddgame).game_state_stream = mem as *mut u8;
    }

    // ===== Allocate unknown 0x4A74 object =====
    {
        let mem = wa_malloc_zeroed(0x4A74);
        if !mem.is_null() {
            *(mem.add(0x4A50) as *mut u32) = ddgame as u32;
        }
        (*ddgame)._unknown_52c = mem;
    }

    // Get display dimensions
    DisplayGfx::get_dimensions_raw(
        (*ddgame).display,
        core::ptr::addr_of_mut!((*ddgame).level_width_sound) as *mut u32,
        core::ptr::addr_of_mut!((*ddgame).screen_height_pixels) as *mut u32,
    );
    (*ddgame)._field_77b0 = 0;

    // ===== Allocate BufferObject (main_buffer at wrapper+0x0C) =====
    (*wrapper).main_buffer = allocate_buffer_object(ddgame, game_info);

    // ===== Allocate RingBuffer A (wrapper+0x3C, capacity 0x2000) =====
    (*wrapper).ring_buffer_a = allocate_ring_buffer_raw(0x3C, 0x2000);

    // ===== Allocate RingBuffer B (wrapper+0x28, capacity 0x2000) =====
    (*wrapper).ring_buffer_b = allocate_ring_buffer_raw(0x3C, 0x2000);

    // ===== Allocate render buffer A (wrapper+0x40, capacity 0x10000) =====
    (*wrapper).render_buffer_a = allocate_ring_buffer_raw(0x48, 0x10000);

    // ===== Allocate render buffer B (wrapper+0x14, capacity 0x10000) =====
    (*wrapper).render_buffer_b = allocate_ring_buffer_raw(0x48, 0x10000);

    // ===== wrapper+0x44: network ring buffer (conditional) =====
    (*wrapper).network_ring_buffer = core::ptr::null_mut();
    if (*ddgame).network_ecx != 0 {
        (*wrapper).network_ring_buffer = allocate_ring_buffer_init();
    }

    // ===== Allocate state buffer (wrapper+0x48) =====
    (*wrapper).state_buffer = allocate_buffer_object(ddgame, game_info);

    // ===== Allocate statistics object (0xB94 bytes, wrapper+0x4C) =====
    {
        let mem = wa_malloc(0xB94);
        if !mem.is_null() {
            *(mem.add(0xB54) as *mut u32) = 0;
            *(mem.add(0xB58) as *mut u32) = 0;
            *(mem.add(0xB64) as *mut u32) = 0;
            *(mem.add(0xB68) as *mut u32) = 0;
            *(mem.add(0xB74) as *mut u32) = 0;
            *(mem.add(0xB78) as *mut u32) = 0;
            *(mem.add(0xB84) as *mut u32) = 0;
            *(mem.add(0xB88) as *mut u32) = 0;
        }
        (*wrapper).statistics = mem;
    }

    // ===== Allocate ring buffer C (wrapper+0x50, capacity 0x1000) =====
    (*wrapper).ring_buffer_c = allocate_ring_buffer_raw(0x3C, 0x1000);

    // ===== Zero out scalar fields =====
    (*wrapper)._field_494 = 0;
    (*wrapper)._field_498 = 0;
    (*wrapper)._field_493 = 0;

    // Zero team task pointers (13 entries)
    (*wrapper).team_task_ptrs = [core::ptr::null_mut(); 13];

    // ===== Allocate 3 PaletteContexts (0x72C bytes each) =====
    (*wrapper).palette_ctx_a = allocate_palette_context();
    (*wrapper).palette_ctx_b = allocate_palette_context();
    (*wrapper).palette_ctx_c = allocate_palette_context();

    // ===== Select vector normalize function based on game version =====
    {
        let game_version = (*game_info).game_version;
        (*ddgame).vector_normalize_fn = if game_version < 0x99 {
            rb(va::VECTOR_NORMALIZE_SIMPLE)
        } else {
            rb(va::VECTOR_NORMALIZE_OVERFLOW)
        };
    }

    // ===== Initialize sentinel fields =====
    (*wrapper)._field_468 = -1;
    (*wrapper)._field_46c = -1;
    (*wrapper)._field_470 = -1;
    (*wrapper).game_state = 0;
    (*wrapper).game_end_speed = 0;
    (*wrapper)._field_264 = 0;
    (*wrapper).sync_checksum_a = 0;
    (*wrapper).checksum_valid = 0;
    (*wrapper)._field_278 = 0;
    (*wrapper)._field_27c = 0;
    (*wrapper)._field_3fc = 0;
    (*wrapper)._field_3f4 = -1;
    (*wrapper)._field_3f8 = -1;

    // ===== Resolution-dependent team render indices =====
    {
        let screen_height = (*ddgame).screen_height_pixels;
        if screen_height < 0x2D0 {
            // < 720px: smaller layout
            (*wrapper).max_team_render_index = 0xC;
            (*wrapper).team_render_indices = [8, 7, 5, 2, 4, 3, 1, 6];
        } else {
            // >= 720px: larger layout
            (*wrapper).max_team_render_index = 0x10;
            (*wrapper).team_render_indices = [0x10, 0xF, 0xD, 10, 0xC, 0xB, 9, 0xE];
        }

        (*wrapper).team_count_config = if screen_height < 600 { 7 } else { 10 };
    }

    // ===== Game mode and sentinel arrays =====
    (*wrapper).game_mode_flag = 1;

    // Fill team slot mapping with -1 sentinels
    (*wrapper).team_to_slot_a = [-1i32; 8];
    (*wrapper).slot_to_team = [-1i32; 16];
    (*wrapper)._field_3ec = -1;

    // ===== Team-to-slot mapping (conditional on network/replay mode) =====
    if (*ddgame).network_ecx != 0 || (*wrapper).replay_flag_a != 0 {
        let team_count = (*game_info).num_teams as i32;
        let exclude_team = (*game_info).starting_team_index as i32;
        let mut slot_idx = 0usize;
        // slot_to_team[0..12] overlaps the reverse mapping area (0x3BC = slot_to_team[4])
        let pivar7_base = core::ptr::addr_of_mut!((*wrapper).slot_to_team[4]) as *mut i32;
        for team_id in 0..team_count {
            if team_id != exclude_team {
                *pivar7_base.add(team_id as usize) = slot_idx as i32;
                (*wrapper).team_to_slot_a[slot_idx] = team_id;
                slot_idx += 1;
            } else {
                *pivar7_base.add(team_id as usize) = -1;
            }
        }
    }

    // ===== Game logic initialization =====
    (*wrapper).game_end_phase = 0;
    (*wrapper).init_flag = 1;

    // Already-ported sub-functions
    init_team_scoring(wrapper);
    init_alliance_data(wrapper);

    // ===== Game speed/timing fields =====
    (*wrapper).health_precision = 500;
    (*wrapper).timing_jitter_state = 0;
    (*wrapper)._field_464 = 0;
    (*wrapper)._field_460 = 0;
    (*wrapper)._field_478 = 0;
    (*wrapper)._field_418 = 0;

    // Turn percentage: (game_info.turn_percentage << 16) / 100
    let turn_pct_raw = (*game_info).turn_percentage_raw;
    (*wrapper).turn_percentage = (turn_pct_raw << 16) / 100;

    // ===== Display setup (headful only) =====
    let is_headful = (*ddgame).sound_available != 0;

    if is_headful {
        init_game_state_display(wrapper, ddgame, game_info);
    }

    // ===== Worm selection count and terrain config =====
    {
        let mut val = (*game_info).worm_select_cfg_a;
        let min_teams = (*wrapper).min_active_teams;
        let team_count_cfg = (*wrapper).team_count_config;

        if val == 0 {
            val = team_count_cfg;
        } else if val < min_teams {
            val = min_teams;
        } else if val > 0x20 {
            val = 0x20;
        }
        (*wrapper).worm_select_count = val;

        val = (*game_info).worm_select_cfg_b;
        if val == -1 {
            val = 7;
        } else if val < min_teams {
            val = min_teams;
        } else if val > 0x20 {
            val = 0x20;
        }
        (*wrapper).worm_select_count_alt = val;

        (*wrapper)._field_414 = ((*game_info)._field_f365 != 0) as u32;

        if is_headful {
            // DDGame+0x000 = keyboard ptr; keyboard+0x10 = config field
            let keyboard = (*ddgame).keyboard as *mut u8;
            *(keyboard.add(0x10) as *mut u32) = ((*game_info)._field_f370 != 0) as u32;
        }
    }

    // Ensure worm_select_count >= worm_select_count_alt
    if (*wrapper).worm_select_count < (*wrapper).worm_select_count_alt {
        (*wrapper).worm_select_count = (*wrapper).worm_select_count_alt;
    }

    // ===== Zero state fields =====
    (*wrapper)._field_434 = 0;
    (*wrapper)._field_438 = -1;
    (*wrapper)._field_43c = -1;
    (*wrapper)._field_440 = 0;
    (*wrapper)._field_444 = 0;
    (*wrapper)._field_424 = 0;
    (*wrapper)._field_428 = 0;
    (*wrapper)._field_448 = 0;
    (*wrapper)._field_44c = 0;
    (*wrapper)._field_42c = 0;
    (*wrapper)._field_430 = 0;
    (*wrapper)._field_40c = 0;
    (*wrapper)._field_410 = 0;

    // ===== Landscape flags =====
    init_landscape_flags(wrapper);

    // ===== Level bounds and camera initialization =====
    init_game_state_level_bounds(ddgame, game_info);

    // ===== Display objects for HUD (headful only) =====
    if is_headful {
        init_game_state_hud_objects(ddgame, game_info);
    }

    // ===== Team name validation =====
    init_game_state_team_names(ddgame, game_info);

    // ===== Weapon and team initialization =====
    (*ddgame).hud_status_code = 0;
    (*ddgame).hud_status_text = core::ptr::null();

    // InitWeaponTable
    {
        let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
            core::mem::transmute(rb(va::INIT_WEAPON_TABLE) as usize);
        f(wrapper);
    }

    // DDGame__InitTeamsFromSetup
    {
        let team_arena = core::ptr::addr_of_mut!((*ddgame).team_arena) as u32;
        let gi_ptr = (*ddgame).game_info as u32;
        let f: unsafe extern "stdcall" fn(u32, u32) =
            core::mem::transmute(rb(va::INIT_TEAMS_FROM_SETUP) as usize);
        f(team_arena, gi_ptr);
    }

    // ===== Version-dependent stack/render config =====
    init_game_state_version_config(ddgame, game_info);

    // ===== Water level and wind =====
    init_game_state_water_level(ddgame, game_info);

    // ===== Random bag initialization =====
    init_game_state_random_bags(ddgame, game_info);

    // ===== Turn state =====
    init_turn_state(wrapper);

    // ===== Weapon availability loop =====
    init_game_state_weapon_avail(ddgame, game_info);

    // ===== Team/object tracking arrays =====
    init_game_state_tracking_arrays(ddgame, game_info);

    // ===== Statistics counters =====
    {
        let base = core::ptr::addr_of_mut!((*ddgame)._unknown_9890) as *mut u32;
        for i in 0..10usize {
            *base.add(i) = 0;
        }
    }

    // ===== TeamManager constructor =====
    {
        let mem = wa_malloc_zeroed(0x6C);
        let result = if mem.is_null() {
            core::ptr::null_mut()
        } else {
            let ctor: unsafe extern "stdcall" fn(*mut u8, u32) -> *mut u8 =
                core::mem::transmute(rb(va::TEAM_MANAGER_CONSTRUCTOR) as usize);
            ctor(mem, ddgame as u32)
        };
        (*ddgame).turn_order_widget = result as *mut _;
    }

    // ===== Network sync callback =====
    let sound = (*ddgame).sound;
    if !sound.is_null() {
        DSSound::update_channels_raw(sound);
    }

    // ===== CTaskTurnGame constructor =====
    {
        let game_version = (*game_info).game_version;
        let gi_ptr = (*ddgame).game_info as u32;
        if game_version == -2 {
            // Online game: ECX = *net_bridge (dereferenced!) for the constructor
            let mem = wa_malloc_zeroed(0x324);
            if mem.is_null() {
                (*wrapper).task_turn_game = core::ptr::null_mut();
            } else {
                let net_bridge = (*wrapper).net_bridge;
                let ecx_val = *(net_bridge as *const u32); // deref net_bridge to get ECX
                call_ctor_with_ecx(mem, gi_ptr, ecx_val, rb(va::CTASK_TURNGAME_CTOR));
                *(mem.add(0x300) as *mut u32) = net_bridge as u32;
                // Override vtables for online mode
                *(mem as *mut u32) = rb(0x669C28);
                *(mem.add(0x30) as *mut u32) = rb(0x669C44);
                (*wrapper).task_turn_game = mem;
            }
        } else {
            // Normal game: ECX must = DDGame for the constructor
            let mem = wa_malloc_zeroed(0x320);
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                call_ctor_with_ecx(mem, gi_ptr, ddgame as u32, rb(va::CTASK_TURNGAME_CTOR))
            };
            (*wrapper).task_turn_game = result;
        }
    }

    // ===== Zero frame timing state (0x98-0xD4) =====
    (*wrapper).timing_ref_lo = 0;
    (*wrapper).timing_ref_hi = 0;
    (*wrapper).last_frame_time_lo = 0;
    (*wrapper).last_frame_time_hi = 0;
    (*wrapper).frame_accum_a_lo = 0;
    (*wrapper).frame_accum_a_hi = 0;
    (*wrapper).frame_accum_b_lo = 0;
    (*wrapper).frame_accum_b_hi = 0;
    (*wrapper).frame_accum_c_lo = 0;
    (*wrapper).frame_accum_c_hi = 0;
    (*wrapper).initial_ref_lo = 0;
    (*wrapper).initial_ref_hi = 0;
    (*wrapper).pause_detect_lo = 0;
    (*wrapper).pause_detect_hi = 0;

    // ===== Replay/network mode flag =====
    {
        let replay_a = (*wrapper).replay_flag_a;
        let result = if replay_a == 0
            || ((*game_info).replay_config_flag != 0 && (*wrapper)._field_49c > 0xC)
        {
            0u32
        } else {
            0xFFFF_FFFFu32
        };
        (*ddgame).field_7ef0 = result as i32;
    }

    (*wrapper)._field_4b0 = 0xFFFF_FFFF;
    (*wrapper)._field_4ac = 0;
    (*wrapper)._field_0e0 = 0;

    // 0xEC: (game_info.f340 != 0) - 1 (i.e., 0 if nonzero, 0xFFFFFFFF if zero)
    (*wrapper).frame_delay_counter = (((*game_info)._field_f340 != 0) as i32).wrapping_sub(1);

    (*game_info)._field_f34c = -1;
    (*wrapper).game_state = 1;

    // ===== Serialize initial game state =====
    {
        let serialize: unsafe extern "stdcall" fn(*mut DDGameWrapper, *mut u8) =
            core::mem::transmute(rb(va::SERIALIZE_GAME_STATE) as usize);
        serialize(wrapper, (*wrapper).state_buffer);
    }

    // ===== Copy game state to statistics buffer =====
    {
        let src = core::ptr::addr_of!((*ddgame)._unknown_8168) as *const u32;
        let dst = (*wrapper).statistics as *mut u32;
        core::ptr::copy_nonoverlapping(src, dst, 0x2E5);
    }

    // ===== Compute initial checksum =====
    {
        let buf_ptr = *((*wrapper).state_buffer as *const *const u8);
        let buf_len = *((*wrapper).state_buffer.add(0x0C) as *const u32); // field[3]
        let mut hash = 0u32;
        for i in 0..buf_len as usize {
            hash = hash.rotate_left(3).wrapping_add(*buf_ptr.add(i) as u32);
        }

        // Get frame counter via landscape vtable
        let landscape = (*ddgame).landscape;
        let frame = PCLandscape::get_frame_checksum_raw(landscape);

        (*wrapper).initial_checksum = frame.wrapping_add(hash);
    }

    // ===== Weapon panel (headful only) =====
    if is_headful {
        let mem = wa_malloc_zeroed(0x208);
        let result = if mem.is_null() {
            core::ptr::null_mut()
        } else {
            // usercall: ESI=this(mem), stack=(DDGame), RET 0x4
            call_usercall_esi_stack1(mem, ddgame as u32, rb(va::INIT_WEAPON_PANEL))
        };
        (*ddgame).weapon_panel = result;
    }
}

// =============================================================================
// Helper functions for init_game_state
// =============================================================================

/// Allocate a BufferObject (0x48 bytes) with size computed from game setup.
/// Used for main_buffer and state_buffer.
///
/// Pure Rust port of BufferObject__Constructor (0x545FD0).
/// The original function is usercall: stdcall(this, size, ddgame) with an implicit
/// EDI register carrying the second buffer's capacity.
///
/// ## Dual-buffer layout (0x48 = 18 × u32):
/// - [0]: buf1 data ptr    [5]: buf2 data ptr
/// - [1]: buf1 capacity    [6]: buf2 capacity
/// - [2]: 0                [7]: 0
/// - [3]: 0                [8]: 0
/// - [4]: ddgame ptr       [9]: ddgame ptr
///
/// ## Second buffer capacity (originally passed via EDI):
/// Base = 0x2DC. If game_info[0xD9B4] != 0 AND (game_info[0xD9B1] as i8) is
/// outside range [2, 0x22], add 0x190 (total 0x46C).
unsafe fn allocate_buffer_object(ddgame: *mut DDGame, game_info: *const GameInfo) -> *mut u8 {
    use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

    let mem = wa_malloc_zeroed(0x48) as *mut u32;
    if mem.is_null() {
        return core::ptr::null_mut();
    }

    // Primary buffer: size from game setup
    let num_teams = (*game_info).num_teams_alloc as u32;
    let num_objects = (*game_info).object_slot_count;
    let buf1_capacity = num_teams * 0x450 + 0x4F178 + num_objects * 0x70;

    let buf1 = wa_malloc(((buf1_capacity + 3) & !3) + 0x20);
    core::ptr::write_bytes(buf1, 0, buf1_capacity as usize);

    *mem.add(0) = buf1 as u32;
    *mem.add(1) = buf1_capacity;
    *mem.add(2) = 0;
    *mem.add(3) = 0;
    *mem.add(4) = ddgame as u32;

    // Secondary buffer: capacity from game_info version fields (originally EDI)
    let gi_raw = game_info as *const u8;
    let field_d9b4 = *gi_raw.add(0xD9B4);
    let field_d9b1 = *gi_raw.add(0xD9B1) as i8;
    let extra = if field_d9b4 != 0 && ((field_d9b1 as i32 - 2) as u32) >= 0x21 {
        0x190u32
    } else {
        0
    };
    let buf2_capacity = extra + 0x2DC;

    let buf2 = wa_malloc(((buf2_capacity + 3) & !3) + 0x20);
    core::ptr::write_bytes(buf2, 0, buf2_capacity as usize);

    *mem.add(5) = buf2 as u32;
    *mem.add(6) = buf2_capacity;
    *mem.add(7) = 0;
    *mem.add(8) = 0;
    *mem.add(9) = ddgame as u32;

    mem as *mut u8
}

/// Allocate a raw ring-buffer-like object with manual field initialization.
/// Used for objects at 0x3C (cap 0x2000), 0x28 (cap 0x2000), etc.
/// `alloc_size` = total allocation size (0x3C or 0x48).
/// `capacity` = buffer capacity (0x1000, 0x2000, or 0x10000).
unsafe fn allocate_ring_buffer_raw(alloc_size: u32, capacity: u32) -> *mut u8 {
    use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

    let mem = wa_malloc_zeroed(alloc_size) as *mut u32;
    if mem.is_null() {
        return core::ptr::null_mut();
    }
    // Initialize: [1] = capacity, [0] = buffer ptr, [2..6] = 0
    *mem.add(1) = capacity;
    let buf = wa_malloc(capacity + 0x20);
    core::ptr::write_bytes(buf, 0, capacity as usize);
    *mem = buf as u32;
    *mem.add(6) = 0;
    *mem.add(5) = 0;
    *mem.add(4) = 0;
    *mem.add(3) = 0;
    *mem.add(2) = 0;

    mem as *mut u8
}

/// Allocate a RingBuffer using the already-ported ring_buffer_init function.
/// Used for the conditional network ring buffer.
unsafe fn allocate_ring_buffer_init() -> *mut u8 {
    use crate::wa_alloc::wa_malloc_zeroed;

    let mem = wa_malloc_zeroed(0x3C);
    if mem.is_null() {
        return core::ptr::null_mut();
    }
    ring_buffer_init(mem, 0x2000);
    mem
}

/// Allocate and initialize a PaletteContext (0x72C bytes).
/// Sets dirty_range_min=1, dirty_range_max=0xFF, then calls PaletteContext__Init.
unsafe fn allocate_palette_context() -> *mut crate::render::palette::PaletteContext {
    use crate::render::palette::{palette_context_init, PaletteContext};

    let ctx = wa_malloc_struct_zeroed::<PaletteContext>();
    if ctx.is_null() {
        return core::ptr::null_mut();
    }
    (*ctx).dirty_range_min = 1;
    (*ctx).dirty_range_max = 0xFF;
    palette_context_init(ctx);
    (*ctx).dirty = 0;
    ctx
}

/// Bridge: stdcall constructor with ECX = implicit register param.
/// cdecl(this, stack_param, ecx_val, target_addr) -> result
/// Sets ECX, then calls target as stdcall(this, stack_param) -> result.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_ctor_with_ecx(
    _this: *mut u8,
    _stack_param: u32,
    _ecx_val: u32,
    _target: u32,
) -> *mut u8 {
    core::arch::naked_asm!(
        // Stack: [ret_addr] [this] [stack_param] [ecx_val] [target]
        "movl 12(%esp), %ecx", // ECX = ecx_val
        "movl 16(%esp), %eax", // EAX = target address
        // Push stdcall args: stack_param, then this
        "pushl 8(%esp)", // push stack_param (now at esp+12 due to push)
        "pushl 8(%esp)", // push this (now at esp+12 due to two pushes)
        "calll *%eax",   // call target (stdcall cleans 8 bytes)
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(ESI=ptr, stack=param), plain RET 0x4.
/// cdecl(ptr, stack_param, target_addr) -> result
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_usercall_esi_stack1(
    _ptr: *mut u8,
    _stack_param: u32,
    _target: u32,
) -> *mut u8 {
    core::arch::naked_asm!(
        // [ret@0] [ptr@4] [stack_param@8] [target@12]
        "pushl %esi",          // save ESI
        "movl 8(%esp), %esi",  // ESI = ptr (shifted by push)
        "movl 16(%esp), %eax", // EAX = target
        "pushl 12(%esp)",      // push stack_param (shifted by 2 pushes)
        "calll *%eax",         // stdcall cleans 4 bytes
        "popl %esi",           // restore ESI
        "retl",
        options(att_syntax),
    );
}

/// Display setup phase of InitGameState (headful only).
///
/// Creates DisplayGfx layers, camera objects, and the main display surface.
unsafe fn init_game_state_display(
    wrapper: *mut DDGameWrapper,
    ddgame: *mut DDGame,
    game_info: *const GameInfo,
) {
    use crate::address::va;
    use crate::rebase::rb;
    use crate::wa_alloc::wa_malloc_zeroed;

    let max_team_render = (*wrapper).max_team_render_index;

    let mut display_width: u32 = 0;
    let mut display_height: u32 = 0;
    DisplayGfx::get_dimensions_raw((*ddgame).display, &mut display_width, &mut display_height);

    // Screen height for HUD: 0x12C (300) if wide layout, 0x8C (140) if narrow
    let screen_height: i32 = if max_team_render != 0xC {
        0xA0 + 0x8C // 300
    } else {
        0x8C // 140
    };
    (*wrapper).screen_height_hud = screen_height;

    // Count active teams from slot_to_team array
    let team_count = (*game_info).num_teams as u32;
    let count_active = |wrapper: *mut DDGameWrapper, base: u32| -> u32 {
        let mut n = base;
        if (*wrapper).slot_to_team[0] >= 0 {
            let mut idx = 1;
            loop {
                n += 1;
                if (*wrapper).slot_to_team[idx] < 0 {
                    break;
                }
                idx += 1;
            }
        }
        n
    };
    let active_count = count_active(wrapper, team_count);

    // min_teams: (int)(active_count - 1) < 2 ? 1 : active_count - 1
    let min_teams: i32 = if (active_count.wrapping_sub(1) as i32) < 2 {
        1
    } else {
        count_active(wrapper, team_count) as i32 - 1
    };
    (*wrapper).min_active_teams = min_teams;

    // screen_offset = display_width - screen_height
    (*wrapper).screen_offset = display_width as i32 - screen_height;
    let screen_offset = display_width as i32 - screen_height;

    // ===== Layer 1 (wrapper+0x18): BitGrid(8, max_team*64+2, screen_offset-7) =====
    {
        let height = max_team_render * 64 + 2;
        let width = screen_offset - 7;
        (*wrapper).display_gfx_a = create_display_gfx_layer_sized(height as u32, width as u32);
    }

    // ===== Layer 2 (wrapper+0x1C): BitGrid(8, max_team*33+6, display_width) =====
    {
        let height = max_team_render * 33 + 6;
        (*wrapper).display_gfx_b = create_display_gfx_layer_sized(height as u32, display_width);
    }

    // ===== Layer 3 (wrapper+0x20): BitGrid(8, (min_teams+1)*max_team+1, screen_height) =====
    {
        let height = (min_teams + 1) * max_team_render + 1;
        (*wrapper).display_gfx_c =
            create_display_gfx_layer_sized(height as u32, screen_height as u32);
    }

    // ===== ConstructFull for main display (wrapper+0x24) =====
    // usercall: ECX=gfx_color_table[7], EDX=max_team_render_index
    // stdcall stack: (this, display, team_idx, screen_offset-3, render_param)
    {
        let mem = wa_malloc_zeroed(0x468);
        let result = if mem.is_null() {
            core::ptr::null_mut()
        } else {
            {
                let p_display = (*ddgame).display as u32;
                let p_team = (*wrapper).team_render_indices[0] as u32;
                let p_offset = screen_offset - 3;
                let p_render = (*ddgame).gfx_color_table[8]; // [8] = 0x732C
                let p_ecx = (*ddgame).gfx_color_table[7]; // [7] = 0x7328
                let p_edx = max_team_render as u32;
                let target = rb(va::DISPLAYGFX_CONSTRUCT_FULL);
                let f: unsafe extern "fastcall" fn(
                    u32,
                    u32,
                    *mut u8,
                    u32,
                    u32,
                    i32,
                    u32,
                ) -> *mut u8 = core::mem::transmute(target as usize);
                f(p_ecx, p_edx, mem, p_display, p_team, p_offset, p_render)
            }
        };
        (*wrapper).display_gfx_main = result;
    }

    // Turn timer
    {
        let timer_val = max_team_render << 5;
        (*wrapper).turn_timer_max = timer_val;
        (*wrapper).turn_timer_current = timer_val;
        (*wrapper)._field_404 = 0;
    }

    // Dirty rect / clipping calls on layers A, B, C
    fill_display_layer((*wrapper).display_gfx_a as *mut DisplayBitGrid, ddgame);
    fill_display_layer((*wrapper).display_gfx_b as *mut DisplayBitGrid, ddgame);
    fill_display_layer((*wrapper).display_gfx_c as *mut DisplayBitGrid, ddgame);

    // ===== Layer 4 (wrapper+0x2C): BitGrid(8, 0x100, 0x154) — constant =====
    (*wrapper).display_gfx_d = create_display_gfx_layer_sized(0x100, 0x154);

    // ===== Layer 5 (wrapper+0x34): BitGrid(8, 0x30, 0xC0) — constant =====
    (*wrapper).display_gfx_e = create_display_gfx_layer_sized(0x30, 0xC0);

    // Create 2 camera objects (wrapper+0x30, +0x38)
    (*wrapper).camera_a = create_camera_object(wrapper, ddgame, 0x2C);
    (*wrapper).camera_b = create_camera_object(wrapper, ddgame, 0x34);
}

/// Create a DisplayGfx layer (0x4C bytes): malloc + memset + BitGrid__Init + vtable.
/// Each layer has specific height/width for its BitGrid.
unsafe fn create_display_gfx_layer_sized(height: u32, width: u32) -> *mut u8 {
    use crate::bitgrid::{BitGrid, BIT_GRID_DISPLAY_VTABLE};
    use crate::rebase::rb;
    use crate::wa_alloc::wa_malloc_zeroed;

    let mem = wa_malloc_zeroed(0x4C) as *mut u32;
    if mem.is_null() {
        return core::ptr::null_mut();
    }
    // BitGrid::init at the start of the buffer (this IS a BitGrid, not a wrapper)
    BitGrid::init(mem as *mut BitGrid, 8, width, height);
    // Override base vtable (0x6640EC) with DisplayBitGrid vtable (0x664144)
    *mem = rb(BIT_GRID_DISPLAY_VTABLE);
    mem as *mut u8
}

/// Fill a DisplayBitGrid layer with its background color, respecting clip bounds.
unsafe fn fill_display_layer(gfx: *mut DisplayBitGrid, ddgame: *mut DDGame) {
    if gfx.is_null() {
        return;
    }
    let width = (*gfx).width as i32;
    let height = (*gfx).height as i32;
    if width <= 0 || height <= 0 {
        return;
    }
    let clip_right = (*gfx).clip_right as i32;
    let clip_bottom = (*gfx).clip_bottom as i32;
    if clip_right <= 0 || clip_bottom <= 0 {
        return;
    }
    let clip_left = (*gfx).clip_left as i32;
    let clip_top = (*gfx).clip_top as i32;
    if clip_left >= width || clip_top >= height {
        return;
    }

    let x1 = clip_left.max(0);
    let y1 = clip_top.max(0);
    let x2 = clip_right.min(width);
    let y2 = clip_bottom.min(height);

    let color = (*ddgame).gfx_color_table[7] as u8;
    DisplayBitGrid::fill_rect_raw(gfx, x1, y1, x2, y2, color);
}

/// Create a camera/display object (0x3D4 bytes).
unsafe fn create_camera_object(
    wrapper: *mut DDGameWrapper,
    ddgame: *mut DDGame,
    display_gfx_offset: usize,
) -> *mut u8 {
    use crate::wa_alloc::wa_malloc_zeroed;

    let mem = wa_malloc_zeroed(0x3D4) as *mut i32;
    if mem.is_null() {
        return core::ptr::null_mut();
    }

    let w = wrapper as *const u8;
    let display_gfx = *(w.add(display_gfx_offset) as *const *const u8);

    *mem.add(1) = (*ddgame).display as i32;
    *mem.add(3) = (*ddgame).gfx_color_table[0] as i32;
    *mem.add(2) = (*ddgame).gfx_color_table[7] as i32;
    *mem = display_gfx as i32;

    let w_val = *(display_gfx.add(0x14) as *const i32);
    let h_val = *(display_gfx.add(0x18) as *const i32);

    *mem.add(9) = w_val;
    *mem.add(4) = w_val / 2;
    *mem.add(10) = h_val;
    *mem.add(5) = h_val / 2;

    mem as *mut u8
}

/// Initialize level bounds and camera center positions.
unsafe fn init_game_state_level_bounds(ddgame: *mut DDGame, game_info: *const GameInfo) {
    (*ddgame)._field_7794 = 0;
    (*ddgame)._field_7798 = 0;

    let is_cavern = (*ddgame).level_width_raw as i32;
    if is_cavern == 0 {
        // Open-air level
        (*ddgame).level_bound_min_x = Fixed(0xF802_0000u32 as i32);
        (*ddgame).level_bound_min_y = Fixed(0xF802_0000u32 as i32);
        let level_width = (*ddgame).level_width as i32;
        (*ddgame).level_bound_max_x = Fixed((level_width + 0x7FE) * 0x10000);
    } else {
        // Cavern level
        let game_version = (*game_info).game_version;
        let bound = if game_version >= -1 { 0x20000i32 } else { 0i32 };
        (*ddgame).level_bound_min_x = Fixed(bound);
        (*ddgame).level_bound_min_y = Fixed(0x20000);
        let level_width = (*ddgame).level_width;
        (*ddgame).level_bound_max_x =
            Fixed((level_width.wrapping_mul(0x10000)).wrapping_sub(bound as u32) as i32);
    }

    // Camera center initialization (4 viewports)
    // Each iteration writes to viewport_coords[i+1] (entries 1..5).
    let level_w = (*ddgame).level_width as i32;
    let level_h = (*ddgame).level_height as i32;
    let cx = Fixed((level_w << 16) / 2);
    let cy = Fixed((level_h << 16) / 2);
    for i in 0..4 {
        let entry = &mut (*ddgame).viewport_coords[i + 1];
        entry.center_x = cx;
        entry.center_y = cy;
        entry.center_x_target = cx;
        entry.center_y_target = cy;
    }

    // Map dimension fields
    (*ddgame).map_boundary_width = 0x30D4;
    (*ddgame).map_boundary_height = (*ddgame).level_height;

    let game_version = (*game_info).game_version;
    if game_version > 0x32 {
        let level_w = (*ddgame).level_width as i32;
        (*ddgame).map_boundary_width = (level_w + 0x2954) as u32;
        (*ddgame).map_boundary_height = 0x2B8;
    }
}

/// Create HUD display objects (headful only, conditional on is_headful).
unsafe fn init_game_state_hud_objects(ddgame: *mut DDGame, _game_info: *const GameInfo) {
    use crate::address::va;
    use crate::rebase::rb;
    use crate::wa_alloc::wa_malloc_zeroed;

    // DisplayObject for HUD background
    // usercall: ECX=gfx_color_table[6], EDX=gfx_color_table[7], stdcall stack=(this, DDGame+0x33C)
    {
        let mem = wa_malloc_zeroed(0x58) as *mut u32;
        let result = if mem.is_null() {
            core::ptr::null_mut()
        } else {
            let ctor: unsafe extern "fastcall" fn(u32, u32, *mut u32, u32) -> *mut u8 =
                core::mem::transmute(rb(va::DISPLAY_OBJECT_CONSTRUCTOR) as usize);
            ctor(
                (*ddgame).gfx_color_table[6], // ECX
                (*ddgame).gfx_color_table[7], // EDX
                mem,
                core::ptr::addr_of!((*ddgame).sprite_cache[128]) as u32,
            )
        };
        (*ddgame)._unknown_540 = result;
    }

    // DisplayGfx textbox for HUD text
    // thiscall: ECX=DDGame.display, stack=(this, 0x13, 2)
    {
        let mem = wa_malloc_zeroed(0x158) as *mut u32;
        let result = if mem.is_null() {
            core::ptr::null_mut()
        } else {
            let ctor: unsafe extern "thiscall" fn(*mut DisplayGfx, *mut u32, u32, u32) -> *mut u8 =
                core::mem::transmute(rb(va::CONSTRUCT_TEXTBOX) as usize);
            ctor((*ddgame).display, mem, 0x13, 2)
        };
        (*ddgame)._unknown_544 = result;
    }
}

/// Validate team names — check for CPU team markers.
unsafe fn init_game_state_team_names(ddgame: *mut DDGame, game_info: *mut GameInfo) {
    // Set initial team color from game_info
    (*ddgame).team_color = (*game_info).team_color_source as u32;

    // Check team names for CPU marker (high bit set)
    let team_count = (*game_info).speech_team_count;
    let gi_raw = game_info as *const u8;
    if team_count != 0 {
        for i in 0..team_count as usize {
            let name_byte = *(gi_raw.add(0x450 + i * 3000));
            if name_byte >= 0x80 {
                // CPU team detected — set team color to -1
                (*ddgame).team_color = 0xFFFF_FFFF;
                break;
            }
        }
    }
}

/// Version-dependent configuration (stack size, rendering constants).
unsafe fn init_game_state_version_config(ddgame: *mut DDGame, game_info: *const GameInfo) {
    let game_version = (*game_info).game_version;

    // Stack height: game_version < -1 → 0x700, >= -1 → 0x800
    (*ddgame).stack_height = if game_version < -1 { 0x700 } else { 0x800 };

    // Copy RNG seed
    let rng_seed = (*game_info).rng_seed;
    (*ddgame).game_rng = rng_seed;
    (*ddgame).team_health_ratio[0] = rng_seed as i32;

    // Zero various fields
    (*ddgame).frame_counter = 0;
    (*ddgame)._field_5d4 = 0;
    (*ddgame)._field_5d8 = 0;
    (*ddgame)._field_5dc = 0;
    (*ddgame)._field_7e4c = 0;
    (*ddgame)._field_5d0 = 0;

    // Zero counter arrays (6 entries each × 3 blocks starting at turn_time_limit)
    {
        let base = core::ptr::addr_of_mut!((*ddgame).turn_time_limit) as *mut u32;
        for i in 0..18usize {
            *base.add(i) = 0;
        }
    }

    // Zero more fields
    (*ddgame)._field_45e0 = 0;
    (*ddgame)._field_45e4 = 0;
    (*ddgame)._field_45e8 = 0;
}

/// Water level and boundary calculations.
unsafe fn init_game_state_water_level(ddgame: *mut DDGame, game_info: *mut GameInfo) {
    // Water level: (100 - level_height_raw) * level_height / 100
    // level_height_raw appears to encode a water height percentage here
    let height_raw = (*ddgame).level_height_raw as i32;
    let level_height = (*ddgame).level_height as i32;
    let water_level = ((100 - height_raw) * level_height) / 100;
    (*ddgame).water_level = water_level;

    // Clamp to 0 for newer versions
    let game_version = (*game_info).game_version;
    if water_level < 0 && game_version > 0x179 {
        (*ddgame).water_level = 0;
    }

    // Max Y boundary
    let water_val = (*ddgame).water_level;
    let min_y = (*ddgame).level_bound_min_y.0;
    let max_y_candidate = (water_val + 0xA0) * 0x10000;
    let min_y_plus = min_y + 0x300_0000;
    let max_y = if max_y_candidate > min_y_plus {
        max_y_candidate
    } else {
        min_y_plus
    };
    (*ddgame).level_bound_max_y = Fixed(max_y);

    // Clamp for newer versions
    if game_version > 0x11E && (max_y >> 16) + 0x28 > 0x7FFF {
        (*ddgame).level_bound_max_y = Fixed(0x7FD7_0000u32 as i32);
    }

    // Derived water fields
    (*ddgame).water_kill_y = (*ddgame).level_bound_max_y.to_int() + 0x28;
    (*ddgame)._field_5f8 = 0;
    (*ddgame).water_level_initial = (*ddgame).water_level;
    (*ddgame)._field_5ec = 0;

    // Terrain type percentages
    (*ddgame).terrain_pct_a = (*game_info).terrain_cfg_a as u32;
    (*ddgame).terrain_pct_b = (*game_info).terrain_cfg_b as u32;
    (*ddgame).terrain_pct_c = (*game_info).terrain_cfg_c as u32;

    // Game state flags
    (*ddgame)._field_5f0 = 1;
    (*ddgame)._field_5f4 = 100;
    (*ddgame)._field_5fc = 0;
}

/// Random bag initialization (land, mine, barrel, weapon).
unsafe fn init_game_state_random_bags(ddgame: *mut DDGame, game_info: *mut GameInfo) {
    let ddgame_raw = ddgame as *mut u8;
    // Random bag at DDGame+0x360C (5 zeroes + 1 one)
    {
        let write_idx_ptr = ddgame_raw.add(0x379C) as *mut i32;
        let mut write_idx = *write_idx_ptr;
        if write_idx + 5 < 0x65 {
            for _ in 0..5 {
                *(ddgame_raw.add(0x360C) as *mut u32).add(write_idx as usize) = 0;
                write_idx += 1;
            }
            *write_idx_ptr = write_idx;
        }
        if write_idx + 1 < 0x65 {
            *(ddgame_raw.add(0x360C) as *mut u32).add(write_idx as usize) = 1;
            *write_idx_ptr = write_idx + 1;
        }
    }

    // Terrain type percentages validation and clamping
    {
        let pct_land = (*game_info).drop_pct_land as i32;
        let pct_mine = (*game_info).drop_pct_mine as i32;
        let pct_barrel = (*game_info).drop_pct_barrel as i32;

        let mut remaining = 100 - pct_land;
        if remaining < 0 {
            // Land > 100%, clamp all
            (*game_info).drop_pct_land = 100;
            (*game_info).drop_pct_mine = 0;
            (*game_info).drop_pct_barrel = 0;
            remaining = 0;
        } else {
            remaining -= pct_mine;
            if remaining < 0 {
                (*game_info).drop_pct_mine = (pct_mine + remaining) as u8;
                (*game_info).drop_pct_barrel = 0;
                remaining = 0;
            } else {
                remaining -= pct_barrel;
                if remaining < 0 {
                    (*game_info).drop_pct_barrel = (pct_barrel + remaining) as u8;
                }
            }
        }

        // Extended terrain type from game_info
        let ext_type = (*game_info).ext_terrain_type;
        let ext_pct = (*game_info).ext_terrain_pct as i32;
        if ext_type == 0 {
            (*game_info).ext_terrain_pct = remaining as u8;
        } else if remaining < ext_pct {
            (*game_info).ext_terrain_pct = 0;
        }
    }

    // Fill terrain type random bags at DDGame+0x3F84
    {
        let write_idx_ptr = ddgame_raw.add(0x4114) as *mut i32;

        // Land entries (type 1)
        let count = (*game_info).drop_pct_land as u32;
        let mut idx = *write_idx_ptr;
        if (idx as u32 + count) < 0x65 {
            for _ in 0..count {
                *(ddgame_raw.add(0x3F84) as *mut u32).add(idx as usize) = 1;
                idx += 1;
            }
            *write_idx_ptr = idx;
        }

        // Mine entries (type 2)
        let count = (*game_info).drop_pct_mine as u32;
        idx = *write_idx_ptr;
        if (idx as u32 + count) < 0x65 {
            for _ in 0..count {
                *(ddgame_raw.add(0x3F84) as *mut u32).add(idx as usize) = 2;
                idx += 1;
            }
            *write_idx_ptr = idx;
        }

        // Barrel entries (type 4)
        let count = (*game_info).drop_pct_barrel as u32;
        idx = *write_idx_ptr;
        if (idx as u32 + count) < 0x65 {
            for _ in 0..count {
                *(ddgame_raw.add(0x3F84) as *mut u32).add(idx as usize) = 4;
                idx += 1;
            }
            *write_idx_ptr = idx;
        }

        // Extended entries (type 0)
        let count = (*game_info).ext_terrain_pct as u32;
        idx = *write_idx_ptr;
        if (idx as u32 + count) < 0x65 {
            for _ in 0..count {
                *(ddgame_raw.add(0x3F84) as *mut u32).add(idx as usize) = 0;
                idx += 1;
            }
            *write_idx_ptr = idx;
        }
    }

    // Random bag at DDGame+0x3934 (entries 0, 1)
    {
        let write_idx_ptr = ddgame_raw.add(0x3AC4) as *mut i32;
        let mut idx = *write_idx_ptr;
        if idx + 1 < 0x65 {
            *(ddgame_raw.add(0x3934) as *mut u32).add(idx as usize) = 0;
            idx += 1;
            *write_idx_ptr = idx;
        }
        if idx + 1 < 0x65 {
            *(ddgame_raw.add(0x3934) as *mut u32).add(idx as usize) = 1;
            *write_idx_ptr = idx + 1;
        }
    }

    // Random bag at DDGame+0x42AC (entries 0..3)
    {
        let write_idx_ptr = ddgame_raw.add(0x443C) as *mut i32;
        for val in 0..4i32 {
            let idx = *write_idx_ptr;
            if idx + 1 < 0x65 {
                *(ddgame_raw.add(0x42AC) as *mut i32).add(idx as usize) = val;
                *write_idx_ptr = idx + 1;
            }
        }
    }
}

/// Weapon availability loop using already-ported check_weapon_avail.
unsafe fn init_game_state_weapon_avail(ddgame: *mut DDGame, game_info: *const GameInfo) {
    let ddgame_raw = ddgame as *mut u8;
    let game_version = (*game_info).game_version;

    for weapon_id in 1..0x47i32 {
        let avail = check_weapon_avail(ddgame, weapon_id as u32);
        let is_avail = if game_version < 0x4D {
            avail != 0
        } else {
            avail > 0
        };

        if is_avail {
            let write_idx_ptr = ddgame_raw.add(0x3DEC) as *mut i32;
            let idx = *write_idx_ptr;
            if idx + 1 < 0x65 {
                *(ddgame_raw.add(0x3C5C) as *mut i32).add(idx as usize) = weapon_id;
                *write_idx_ptr = idx + 1;
            }
        }
    }
}

/// Allocate team/object tracking arrays and initialize the 0x51C object.
unsafe fn init_game_state_tracking_arrays(ddgame: *mut DDGame, game_info: *const GameInfo) {
    use crate::wa_alloc::wa_malloc;

    // Game speed
    (*ddgame).game_speed = Fixed::ONE;

    let is_headful = (*ddgame).sound_available != 0;
    if !is_headful {
        (*ddgame).game_speed_target = Fixed(0x1000_0000);
    } else {
        (*ddgame).game_speed_target = Fixed((*game_info).game_speed_config);
    }

    // Network speed callback
    let sound = (*ddgame).sound;
    if !sound.is_null() {
        DSSound::set_frequency_scale_raw(
            sound,
            (*ddgame).game_speed.0 as u32,
            (*ddgame).game_speed_target.0,
        );
    }

    // Misc fields
    (*ddgame).sound_queue_count = 0;
    (*ddgame).render_phase = (*game_info).render_phase_cfg as i32;
    (*ddgame)._field_7640 = 0;
    (*ddgame)._field_7644 = (*game_info)._field_f363 as u32;
    (*ddgame)._field_7648 = (*game_info)._field_f364 as u32;

    // Render state
    (*ddgame)._field_7390 = 0;
    (*ddgame).render_scale = Fixed::ONE;
    (*ddgame)._field_7398 = 0;

    // Allocate team tracking arrays (DDGame+0x514, DDGame+0x518)
    {
        let count_a = (*game_info).team_slot_count;
        let size = count_a.wrapping_mul(4);
        let arr = wa_malloc(size);
        (*ddgame)._unknown_514 = arr;
        for i in 0..count_a as usize {
            *(arr as *mut u32).add(i) = 0;
        }
    }
    {
        let count_b = (*game_info).object_slot_count;
        let size = count_b.wrapping_mul(4);
        let arr = wa_malloc(size);
        (*ddgame)._unknown_518 = arr;
        for i in 0..count_b as usize {
            *(arr as *mut u32).add(i) = 0;
        }
    }

    // Initialize the 0x51C object (vector-like structure)
    {
        let obj = (*ddgame)._unknown_51c;
        if !obj.is_null() {
            *(obj as *mut u32) = 0; // first field = 0
                                    // The vector operations (FUN_005370c0, etc.) are complex.
                                    // Bridge to original via setting fields directly.
            let obj32 = obj as *mut u32;
            // Reset the internal vector state
            *(obj32.add(5)) = 0; // +0x14

            // Reset the second vector (at +0x1C..+0x24)
            let start = *(obj32.add(7)) as *mut u32; // +0x1C
            let end = *(obj32.add(8)) as *mut u32; // +0x20
            if start != end {
                // Move elements: dest = start + (capacity - end offset)
                // This is a std::vector erase-to-end operation.
                // For init, just set write pointer = start
                *(obj32.add(8)) = *(obj32.add(7)); // end = start
            }

            *(obj32.add(10)) = 0xFFFF_FFFF; // +0x28 = -1
        }
    }
}

/// Pure Rust port of GameStateStream__Init (0x4FB490).
///
/// Convention: stdcall(sub_object_ptr), plain RET.
///
/// Initializes the sub-object within GameStateStream:
/// - +0x14: capacity (0x100)
/// - +0x18/+0x1C: zeroed
/// - +0x20: main buffer (0x420 bytes, first 0x400 zeroed)
/// - +0x24..+0x224: 32 sub-buffer elements (each 0x10 bytes)
///
/// Each sub-buffer element (FUN_004fdc20):
/// - 0: capacity (0x100)
/// - 1/2: zeroed
/// - 3: buffer (0x420 bytes, first 0x400 zeroed)
unsafe fn game_state_stream_init(sub_obj: *mut u32) {
    *sub_obj.add(5) = 0x100; // +0x14: capacity
    *sub_obj.add(6) = 0; // +0x18
    *sub_obj.add(7) = 0; // +0x1C

    // +0x20: main buffer
    let buf = wa_malloc_zeroed(0x420);
    *sub_obj.add(8) = buf as u32;

    // +0x24: 32 sub-buffer elements, each 0x10 bytes (4 u32s)
    for i in 0..32usize {
        let elem = sub_obj.add(9 + i * 4); // +0x24 + i*0x10
        *elem = 0x100; // capacity
        *elem.add(1) = 0;
        *elem.add(2) = 0;
        let elem_buf = wa_malloc_zeroed(0x420);
        *elem.add(3) = elem_buf as u32;
    }
}
