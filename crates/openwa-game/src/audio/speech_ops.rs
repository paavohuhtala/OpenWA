//! Speech / fanfare / FE SFX operations.
//!
//! Pure Rust reimplementations of WA.exe speech and fanfare functions.
//! Called from hook trampolines in openwa-dll.
//!
//! Original WA functions:
//! - PlayFeSfx (0x4D7960)
//! - PlayFanfare_Default (0x4D7500)
//! - PlayFanfare (0x4D7630)
//! - PlayFanfare_CurrentTeam (0x4D78E0)
//! - FindCurrentTeamIndex (0x46A7D0)
//! - GetTeamConfigName (0x4A62A0)
//! - GameRuntime__LoadSpeechWAV (0x571530)
//! - DSSound_LoadSpeechBank (0x571660)
//! - DSSound_LoadAllSpeechBanks (0x571A70)

use std::ffi::{CStr, c_char};

use heapless::CString;

use crate::address::va::{self, game_info_offsets};
use crate::audio::wav_player::{self, WavPlayer};
use crate::audio::{SpeechLineTableEntry, SpeechSlotTable};
use crate::engine::GameRuntime;
use crate::engine::runtime::SPEECH_NAME_ENTRY_SIZE;
use crate::rebase::rb;

/// Windows MAX_PATH (260 bytes including nul terminator).
const MAX_PATH: usize = 260;

// ============================================================
// Team config name lookup (port of GetTeamConfigName 0x4A62A0)
// ============================================================

/// Team config names indexed 0-48, matching WA.exe's jump table at 0x4A62A0.
/// Used for fanfare WAV file paths (e.g. "user\Fanfare\Finland.wav").
/// Note: "Leichtenstein" is WA's original misspelling — must match for file lookup.
const TEAM_CONFIG_NAMES: [&str; 49] = [
    "UK",            // 0
    "Argentina",     // 1
    "Australia",     // 2
    "Austria",       // 3
    "Belgium",       // 4
    "Brazil",        // 5
    "Canada",        // 6
    "Croatia",       // 7
    "Simple",        // 8
    "Cyprus",        // 9
    "Simple",        // 10
    "Denmark",       // 11
    "Finland",       // 12
    "France",        // 13
    "Simple",        // 14
    "Germany",       // 15
    "Greece",        // 16
    "Simple",        // 17
    "Hungary",       // 18
    "Iceland",       // 19
    "India",         // 20
    "Simple",        // 21
    "Psycho Laugh",  // 22
    "Psycho Laugh",  // 23
    "Ireland",       // 24
    "Israel",        // 25
    "Italy",         // 26
    "Japan",         // 27
    "Leichtenstein", // 28
    "Luxembourg",    // 29
    "Simple",        // 30
    "Malta",         // 31
    "Mexico",        // 32
    "Morocco",       // 33
    "Netherlands",   // 34
    "New Zealand",   // 35
    "Norway",        // 36
    "Poland",        // 37
    "Portugal",      // 38
    "Simple",        // 39
    "Romania",       // 40
    "Russia",        // 41
    "Singapore",     // 42
    "South Africa",  // 43
    "Spain",         // 44
    "Sweden",        // 45
    "Switzerland",   // 46
    "Turkey",        // 47
    "USA",           // 48
];

/// Look up a team config name by team_type (1-49). Returns "Simple" for out-of-range.
pub fn team_config_name(team_type: u32) -> &'static str {
    if (1..=49).contains(&team_type) {
        TEAM_CONFIG_NAMES[(team_type - 1) as usize]
    } else {
        "Simple"
    }
}

// ============================================================
// WavPlayer access helper
// ============================================================

/// Get a pointer to the WavPlayer struct at a global address.
#[inline]
unsafe fn wav_player_at(ghidra_addr: u32) -> *mut WavPlayer {
    rb(ghidra_addr) as *mut WavPlayer
}

// ============================================================
// PlayFeSfx — port of 0x4D7960
// ============================================================

/// Build "fesfx\<name>.wav" path and play on the FESFX WavPlayer.
pub unsafe fn play_fe_sfx(sfx_name: &str) {
    unsafe {
        let mut path = CString::<MAX_PATH>::new();
        let _ = path.extend_from_bytes(b"fesfx\\");
        let _ = path.extend_from_bytes(sfx_name.as_bytes());
        let _ = path.extend_from_bytes(b".wav");

        let player = wav_player_at(va::FESFX_WAV_PLAYER);
        wav_player::wav_player_stop(player);
        wav_player::wav_player_load_and_play(player, path.as_ptr(), 0);
        wav_player::wav_player_play(player, 0);
    }
}

// ============================================================
// PlayFanfare_Default — port of 0x4D7500
// ============================================================

/// Look up team config name, build fanfare path, play on FANFARE WavPlayer.
pub unsafe fn play_fanfare_default(team_type: u32) {
    unsafe {
        let name = team_config_name(team_type);

        let wa_path = CStr::from_ptr(rb(va::WA_DATA_PATH) as *const i8)
            .to_str()
            .unwrap_or(".");

        let mut path = CString::<MAX_PATH>::new();
        let _ = path.extend_from_bytes(wa_path.as_bytes());
        let _ = path.extend_from_bytes(b"\\user\\Fanfare\\");
        let _ = path.extend_from_bytes(name.as_bytes());
        let _ = path.extend_from_bytes(b".wav");

        let player = wav_player_at(va::FANFARE_WAV_PLAYER);
        wav_player::wav_player_stop(player);
        wav_player::wav_player_load_and_play(player, path.as_ptr(), 0);
        wav_player::wav_player_play(player, 0);
    }
}

// ============================================================
// PlayFanfare — port of 0x4D7630
// ============================================================

/// Build the fanfare WAV path and play it on the FANFARE player.
/// Falls back to `play_fanfare_default` if loading fails.
pub unsafe fn play_fanfare(team_type: u32, has_custom_path: bool) {
    unsafe {
        let name = team_config_name(team_type);

        let mut path = CString::<MAX_PATH>::new();
        if has_custom_path {
            let custom_path = CStr::from_ptr(rb(0x0088E282) as *const i8)
                .to_str()
                .unwrap_or(".");
            let _ = path.extend_from_bytes(custom_path.as_bytes());
            let _ = path.extend_from_bytes(b"\\user\\Fanfare\\");
            let _ = path.extend_from_bytes(name.as_bytes());
            let _ = path.extend_from_bytes(b".wav");
        } else {
            let _ = path.extend_from_bytes(b"user\\Fanfare\\");
            let _ = path.extend_from_bytes(name.as_bytes());
            let _ = path.extend_from_bytes(b".wav");
        }

        let player = wav_player_at(va::FANFARE_WAV_PLAYER);
        wav_player::wav_player_stop(player);
        let loaded = wav_player::wav_player_load_and_play(player, path.as_ptr(), 0);
        if loaded {
            wav_player::wav_player_play(player, 0);
        } else {
            play_fanfare_default(team_type);
        }
    }
}

// ============================================================
// PlayFanfare_CurrentTeam — port of 0x4D78E0
// ============================================================

/// Find current team index by comparing team names.
///
/// Port of FindCurrentTeamIndex (0x46A7D0). Iterates up to 6 team entries
/// at stride 0xD7B, comparing name at offset +0x627 against the search name.
unsafe fn find_current_team_index(name_ptr: u32) -> u32 {
    unsafe {
        let name = name_ptr as *const u8;
        let base = rb(0x00877FFC) as *const u8;

        for i in 0u32..6 {
            let entry = base.add(i as usize * 0xD7B);
            if *entry.add(0x121) == 0 {
                continue;
            }
            let team_name = entry.add(0x627);
            if c_str_eq(team_name, name) {
                return i;
            }
        }
        -1i32 as u32
    }
}

/// Compare two null-terminated C strings for equality.
unsafe fn c_str_eq(a: *const u8, b: *const u8) -> bool {
    unsafe {
        let mut i = 0;
        loop {
            let ca = *a.add(i);
            let cb = *b.add(i);
            if ca != cb {
                return false;
            }
            if ca == 0 {
                return true;
            }
            i += 1;
        }
    }
}

/// Read the default team type, find current team, play the fanfare.
///
/// Returns 1 on success, 0 if team not found.
pub unsafe fn play_fanfare_current_team(eax_index: u32) -> u32 {
    unsafe {
        let default_team_type = *(rb(0x0088DFAC) as *const u32);

        let team_index = find_current_team_index(eax_index);
        if team_index == -1i32 as u32 {
            return 0;
        }

        let custom_types_active = *(rb(0x0087D0DE) as *const u8);
        let team_type = if custom_types_active != 0 {
            let team_data = rb(0x00877FFC) as *const u8;
            let team_type_byte = *team_data.add(team_index as usize * 0xD7B) as i8;
            let team_types_array = rb(0x00877A54) as *const u32;
            *team_types_array.add(team_type_byte as usize * 0x1E)
        } else {
            default_team_type
        };

        let team_data_2 = rb(0x00878093) as *const u8;
        let has_custom_path = *team_data_2.add(team_index as usize * 0xD7B) != 0;

        play_fanfare(team_type, has_custom_path);

        1
    }
}

// ============================================================
// GameRuntime__LoadSpeechWAV — port of 0x571530
// ============================================================

/// Search speech name table for existing WAV, reuse slot if found.
/// Otherwise load new WAV via DSSound vtable. Returns 1 on success, 0 on failure.
pub unsafe fn load_speech_wav(
    runtime: *mut GameRuntime,
    team_index: u32,
    line_id: u32,
    wav_path: *const c_char,
    full_path: *const c_char,
) -> u32 {
    unsafe {
        let runtime = &mut *runtime;
        let count = runtime.speech_name_count as usize;
        let search_name = core::ffi::CStr::from_ptr(wav_path).to_bytes();

        let mut found_idx: Option<usize> = None;
        for i in 0..count {
            let entry = &runtime.speech_name_table[i];
            let entry_len = entry.iter().position(|&b| b == 0).unwrap_or(entry.len());
            if entry[..entry_len] == *search_name {
                found_idx = Some(i);
                break;
            }
        }

        let world = &mut *runtime.world;
        let slot_table = &mut world.speech_slot_table;

        if let Some(idx) = found_idx {
            slot_table.set(
                team_index as usize,
                line_id,
                idx as u32 + SpeechSlotTable::BUFFER_OFFSET,
            );
            return 1;
        }

        let slot_idx = count as u32 + SpeechSlotTable::BUFFER_OFFSET;
        let result =
            crate::audio::dssound::load_wav(runtime.sound, slot_idx as i32, full_path as *const u8);

        if result != 0 {
            slot_table.set(team_index as usize, line_id, slot_idx);

            let dest = &mut runtime.speech_name_table[count];
            let copy_len = search_name.len().min(SPEECH_NAME_ENTRY_SIZE - 1);
            dest[..copy_len].copy_from_slice(&search_name[..copy_len]);
            dest[copy_len] = 0;

            runtime.speech_name_count = (count + 1) as u32;
            return 1;
        }

        0
    }
}

// ============================================================
// Helpers
// ============================================================

/// Extract bytes from a null-terminated C string pointer.
unsafe fn cstr_bytes<'a>(ptr: *const u8) -> &'a [u8] {
    unsafe {
        if ptr.is_null() {
            return &[];
        }
        CStr::from_ptr(ptr as *const i8).to_bytes()
    }
}

// ============================================================
// DSSound_LoadSpeechBank — port of 0x571660
// ============================================================

/// Iterate speech line table, build WAV paths, load each into DSSound.
/// Falls back to default speech dir on failure.
pub unsafe fn load_speech_bank(
    ddgw: *const GameRuntime,
    team_index: u32,
    speech_base_path: *const u8,
    speech_dir: *const u8,
) {
    unsafe {
        let table_base = rb(va::SPEECH_LINE_TABLE) as *const SpeechLineTableEntry;
        let mut i: usize = 0;

        loop {
            let entry = &*table_base.add(i);

            if entry.name_ptr.is_null() {
                return;
            }

            let name_bytes = cstr_bytes(entry.name_ptr);

            let mut wav_path = CString::<MAX_PATH>::new();
            let _ = wav_path.extend_from_bytes(cstr_bytes(speech_dir));
            let _ = wav_path.extend_from_bytes(b"\\");
            let _ = wav_path.extend_from_bytes(name_bytes);
            let _ = wav_path.extend_from_bytes(b".wav");

            let mut full_path = CString::<MAX_PATH>::new();
            let _ = full_path.extend_from_bytes(cstr_bytes(speech_base_path));
            let _ = full_path.extend_from_bytes(b"\\");
            let _ = full_path.extend_from_bytes(wav_path.as_bytes());

            let result = load_speech_wav(
                ddgw as *mut GameRuntime,
                team_index,
                entry.id,
                wav_path.as_ptr(),
                full_path.as_ptr(),
            );

            if result == 0 {
                let next_entry = &*table_base.add(i + 1);
                if next_entry.name_ptr.is_null() || next_entry.id != entry.id {
                    let game_info = (*(*ddgw).world).game_info as *const u8;
                    let default_dir = game_info.add(game_info_offsets::DEFAULT_SPEECH_DIR as usize);
                    let default_base =
                        game_info.add(game_info_offsets::DEFAULT_SPEECH_BASE_PATH as usize);

                    let mut wav_path2 = CString::<MAX_PATH>::new();
                    let _ = wav_path2.extend_from_bytes(cstr_bytes(default_dir));
                    let _ = wav_path2.extend_from_bytes(b"\\");
                    let _ = wav_path2.extend_from_bytes(name_bytes);
                    let _ = wav_path2.extend_from_bytes(b".wav");

                    let mut full_path2 = CString::<MAX_PATH>::new();
                    let _ = full_path2.extend_from_bytes(cstr_bytes(default_base));
                    let _ = full_path2.extend_from_bytes(b"\\");
                    let _ = full_path2.extend_from_bytes(wav_path2.as_bytes());

                    load_speech_wav(
                        ddgw as *mut GameRuntime,
                        team_index,
                        entry.id,
                        wav_path2.as_ptr(),
                        full_path2.as_ptr(),
                    );
                }
            } else {
                let current_id = entry.id;
                while {
                    let next = &*table_base.add(i + 1);
                    !next.name_ptr.is_null() && next.id == current_id
                } {
                    i += 1;
                }
            }

            i += 1;
        }
    }
}

// ============================================================
// DSSound_LoadAllSpeechBanks — port of 0x571A70
// ============================================================

/// Clear speech slot table, then load speech bank for each team.
pub unsafe fn load_all_speech_banks(ddgw: *const GameRuntime) {
    unsafe {
        let world = &mut *(*ddgw).world;

        world.speech_slot_table.clear();

        let game_info = &*world.game_info;
        let team_count = game_info.team_record_count as u32;

        let gi = world.game_info as *const u8;
        for i in 0..team_count {
            let team_offset = (i * game_info_offsets::SPEECH_TEAM_STRIDE) as usize;
            let base_path = gi.add(game_info_offsets::SPEECH_BASE_PATH as usize + team_offset);
            let dir = gi.add(game_info_offsets::SPEECH_DIR as usize + team_offset);

            load_speech_bank(ddgw, i, base_path, dir);
        }
    }
}
