//! Rust-side replacement for the lobby-globals that WA's MFC
//! `LobbyDialog` populates before a Start handler calls
//! [`crate::engine::init_session`].
//!
//! ## Background
//!
//! WA's normal Start flow (e.g. `FrontendLocalMP__OnStartMatch` at
//! `0x004A1260`) leaves a pile of MFC + BSS globals (player records at
//! `DAT_008779e4`, team records at `DAT_00877ffc`, `G_SCHEME_DATA`,
//! etc.) in a populated state by the time InitSession runs. The four
//! bridged InitSession helpers — `Replay__ProcessTeamColors`,
//! `CGameInfo__CreateWAGameReplay`, `CGameInfo__ConvertScheme`, and
//! `Replay__ValidateTeamSetup` — read from those globals.
//!
//! On the `CustomLauncher` launch path (set via
//! [`crate::engine::launch_source::LaunchSource::CustomLauncher`]) the
//! MFC frontend never ran, so those globals are zero or stale. Until
//! every helper is fully ported, [`crate::engine::init_session`] skips
//! all four on the CustomLauncher path — but a clean launch still
//! needs the *outputs* those helpers would have written into
//! `GameInfo` (and into `G_SCHEME_DATA`).
//!
//! [`PendingCustomMatch`] is the Rust-side substitute. The launcher
//! fills it in, parks it via [`set`], then schedules an InitSession
//! call. The InitSession orchestrator picks it up (via [`take`]) and
//! writes the equivalent bytes directly.
//!
//! ## Scope (incremental)
//!
//! This first cut deliberately covers only the highest-signal writes:
//! team count, per-team identity (name + color + turn order), and the
//! scheme byte buffer. The dump-diff workflow described in
//! `project_local_mp_start_handler` will reveal which additional
//! fields the game actually reads downstream; each missing region gets
//! added here as the diff demands.

use std::ffi::CString;
use std::sync::Mutex;

use openwa_core::scheme::SchemeFile;
use openwa_core::wgt::WgtTeam;

/// Maximum number of teams that fit in [`crate::engine::GameInfo::team_records`].
pub const MAX_TEAMS: usize = crate::engine::game_info::MAX_TEAM_RECORDS;

/// Maximum worms per team, matching WA's hardcoded per-team capacity
/// (the `GameInfoTeamRecord.worm_data` array has 8 slots).
pub const MAX_WORMS_PER_TEAM: usize = 8;

/// Per-team identity carried by [`PendingCustomMatch`].
#[derive(Clone, Debug)]
pub struct PendingTeam {
    /// Team display name. Truncated to 15 bytes (16 with the trailing NUL)
    /// when written to `GameInfo`.
    pub name: String,
    /// Color / alliance group index. Written to `team_records[i]
    /// .team_color_idx`. Should be unique per team unless you want
    /// allied teams (which share an alliance group).
    pub color_idx: u8,
    /// Turn-order group. Written to `team_records[i].turn_order_idx`.
    /// Teams with the same group ID share a turn slot.
    pub turn_order: u8,
    /// Starting HP for every worm on this team. WA frontend default = 100.
    pub worm_hp: u16,
    /// Worm display names. `len()` becomes the per-team worm count (read
    /// from `team_record + 0xBB4` by `GameWorld__InitTeamsFromSetup` at
    /// `0x005220B0`). Capped at [`MAX_WORMS_PER_TEAM`].
    pub worm_names: Vec<String>,
    /// Grave-sprite index, written to lobby `G_TEAM_DATA[i] + 0x123` so
    /// `Replay__ProcessTeamColors` propagates it into the active team
    /// state. `0x00..=0x05` selects one of the six animated graves;
    /// `0x06..=0x7F` reaches WA's "non-grave sprite as a grave" extension
    /// list; `0x80..=0xFE` is reserved for custom-bitmap entries (the
    /// bitmap data itself is NOT yet wired up — see `custom_grave`).
    pub grave_id: u8,
    /// Custom-grave bitmap (24×32 8bpp + 256-colour palette), present
    /// only when [`grave_id`] >= [`openwa_core::wgt::CUSTOM_GRAVE_THRESHOLD`].
    ///
    /// **Currently not propagated to WA** — the field exists so a
    /// [`PendingTeam::from_wgt`] consumer can round-trip a parsed WGT
    /// entry without losing data. Wiring this through the per-team
    /// custom-grave slot needs another RE pass.
    pub custom_grave: Option<openwa_core::wgt::CustomGrave>,
    /// Soundbank directory name (e.g. `"English"`, `"Finnish"`,
    /// `"Thespian"`). Written into the lobby team record at +0x14, which
    /// [`Replay__ProcessTeamColors`] copies into the per-team speech
    /// config block at `GameInfo + 0xF4C6 + N*0xC2 + 0x81`. WA then
    /// loads each speech line from `<install>\user\speech\<name>\*.wav`.
    /// Empty string defaults to `"English"` in [`populate_lobby_globals`].
    pub soundbank_name: String,
    /// Fanfare name (e.g. `"Finland"`). Not written to game memory yet;
    /// stored for the same reason as [`soundbank_name`].
    pub fanfare_name: String,
    /// Team's Special Weapon, indexing
    /// [`openwa_core::weapon::SPECIAL_WEAPONS`] (the 8-entry cycle list
    /// FlameThrower, MoleBomb, OldWoman, HomingPigeon, SheepLauncher,
    /// MadCow, HolyGrenade, SuperSheep). When the scheme has
    /// `team_weapons` enabled, each team starts with 1–2 shots of this.
    pub special_weapon: u8,
    /// Team flag bitmap (20×17 8bpp + 256-colour palette). When
    /// present, [`populate_lobby_globals`] copies palette and bitmap
    /// into the lobby team record's flag slots
    /// (lobby `+0x127` / `+0x527`). `None` leaves both blocks zero
    /// (engine's blank-flag fallback).
    pub flag: Option<openwa_core::wgt::TeamFlag>,
}

impl PendingTeam {
    /// Build a team with the default 8 worms named `W1..W8` and HP 100.
    pub fn new(name: impl Into<String>, color_idx: u8) -> Self {
        let worm_names = (1..=MAX_WORMS_PER_TEAM as u32)
            .map(|i| format!("W{i}"))
            .collect();
        Self {
            name: name.into(),
            color_idx,
            turn_order: color_idx,
            worm_hp: 100,
            worm_names,
            grave_id: 0,
            custom_grave: None,
            soundbank_name: String::new(),
            fanfare_name: String::new(),
            special_weapon: 0,
            flag: None,
        }
    }

    /// Build a [`PendingTeam`] from a parsed `.WGT` team entry. The
    /// `color_idx` controls the team's visible colour + alliance group
    /// independent of any colour preference baked into the WGT record
    /// (which the format does not actually store at top level).
    ///
    /// All 8 worm-name slots are preserved positionally — empty slots
    /// are substituted with `WormN` rather than dropped, because WA's
    /// `team_record + 0xBB4` worm-count + per-worm name array indexes by
    /// slot position, not by "Nth non-empty entry".
    pub fn from_wgt(team: &WgtTeam, color_idx: u8) -> Self {
        let worm_names: Vec<String> = team
            .worm_names_iter()
            .enumerate()
            .map(|(i, n)| {
                if n.is_empty() {
                    format!("Worm{}", i + 1)
                } else {
                    n
                }
            })
            .collect();
        Self {
            name: team.name_str(),
            color_idx,
            turn_order: color_idx,
            worm_hp: 100,
            worm_names,
            grave_id: team.grave_id,
            custom_grave: team.custom_grave.clone(),
            soundbank_name: team.soundbank_str(),
            fanfare_name: team.fanfare_str(),
            special_weapon: team.special_weapon,
            flag: Some(team.flag.clone()),
        }
    }
}

/// Rust-side replacement for the lobby globals that feed
/// [`crate::engine::init_session`]. See module docs.
#[derive(Clone, Debug)]
pub struct PendingCustomMatch {
    /// `game_version` value written into `GameInfo + 0xD778`. WA's
    /// `FrontendLocalMP__OnStartMatch` hardcodes this to `500` for
    /// current-version offline matches; that's the recommended value.
    pub game_version: i32,
    /// `type_label` argument forwarded to [`crate::engine::init_session`].
    /// Currently unused on the CustomLauncher path because
    /// `CGameInfo__CreateWAGameReplay` is the only reader and it stays
    /// bridged-off there — kept for future use and parity with the
    /// `Frontend` path's call shape.
    pub type_label: Option<CString>,
    /// Active teams. `len()` becomes `GameInfo.num_teams` and
    /// `team_record_count`. Up to [`MAX_TEAMS`] entries.
    pub teams: Vec<PendingTeam>,
    /// Parsed scheme. The 402-byte V3 payload is copied verbatim into
    /// `G_SCHEME_DATA`. Shorter-version schemes are zero-padded.
    pub scheme: SchemeFile,
}

static SLOT: Mutex<Option<PendingCustomMatch>> = Mutex::new(None);

/// Park a [`PendingCustomMatch`] for the next CustomLauncher-mode
/// InitSession to pick up. Overwrites any previous value.
pub fn set(m: PendingCustomMatch) {
    if let Ok(mut g) = SLOT.lock() {
        *g = Some(m);
    }
}

/// Pop the parked [`PendingCustomMatch`] (single-shot consumption).
pub fn take() -> Option<PendingCustomMatch> {
    SLOT.lock().ok().and_then(|mut g| g.take())
}

/// Inspect the parked [`PendingCustomMatch`] without consuming it.
pub fn peek() -> Option<PendingCustomMatch> {
    SLOT.lock().ok().and_then(|g| g.clone())
}

/// True if a [`PendingCustomMatch`] is currently parked.
pub fn is_set() -> bool {
    SLOT.lock().map(|g| g.is_some()).unwrap_or(false)
}

// ─── Apply: write a PendingCustomMatch's bytes into GameInfo + globals ─────

use crate::address::va;
use crate::engine::GameInfo;
use crate::engine::game_info::MAX_TEAM_RECORDS;
use crate::rebase::rb;

/// Per-lobby-player record stride (`G_PLAYER_ARRAY`, 13 entries).
const LOBBY_PLAYER_STRIDE: usize = 0x78;
/// Maximum number of lobby-player slots between `G_PLAYER_ARRAY` and
/// `G_TEAM_DATA` ((0x877FFC - 0x8779E4) / 0x78 = 13).
const MAX_LOBBY_PLAYERS: usize = 13;
/// Per-lobby-team record stride (`G_TEAM_DATA`, 6 entries).
/// Matches `core::mem::size_of::<crate::engine::replay::ReplayTeamEntry>()`.
const LOBBY_TEAM_STRIDE: usize = 0xD7B;

/// V3 scheme payload size (matches `openwa_core::scheme::SCHEME_PAYLOAD_V3`).
/// `G_SCHEME_DATA` is a 402-byte buffer.
const SCHEME_BUFFER_SIZE: usize = openwa_core::scheme::SCHEME_PAYLOAD_V3;

/// Write the [`PendingCustomMatch`] state into `GameInfo` (via `gi`) and
/// `G_SCHEME_DATA`. Mirrors the *outputs* of WA's bridged InitSession
/// helpers (`ProcessTeamColors`, `ConvertScheme`) so the CustomLauncher
/// path can launch without those bridges running.
///
/// First-iteration scope: team identity + scheme bytes only. The
/// dump-diff workflow (see `project_local_mp_start_handler`) drives
/// which additional fields get added here.
///
/// # Safety
///
/// `gi` must point to a writeable `GameInfo` (typically `G_GAME_INFO`).
/// Called from [`crate::engine::init_session`] which already holds main-thread
/// affinity.
pub unsafe fn apply(gi: *mut GameInfo, pending: &PendingCustomMatch) {
    unsafe {
        let _ = openwa_core::log::log_line(&format!(
            "[pending_match] applying: game_version={} teams={} scheme_version={:?}",
            pending.game_version,
            pending.teams.len(),
            pending.scheme.version,
        ));

        let team_count = pending.teams.len().min(MAX_TEAM_RECORDS) as u8;

        (*gi).game_version = pending.game_version;
        (*gi).num_teams = team_count;
        (*gi).team_record_count = team_count;

        // `ProcessTeamColors` writes `alliance_group_count` via a
        // `CPlayers__GetTotalTeamsWithColour` count; on the
        // CustomLauncher path we count distinct `color_idx` values
        // across pending teams. A zero value crashes
        // `Task_TurnGame__advance_ally_0` (divide-by-zero on the
        // modulus), so default to 1 if no teams supplied a colour.
        let mut distinct_colors: u8 = 0;
        let mut seen: u64 = 0;
        for team in pending.teams.iter().take(MAX_TEAM_RECORDS) {
            let bit = 1u64 << (team.color_idx as u64 & 63);
            if seen & bit == 0 {
                seen |= bit;
                distinct_colors += 1;
            }
        }
        if distinct_colors == 0 {
            distinct_colors = 1;
        }
        (*gi).alliance_group_count = distinct_colors;

        // Note: `game_speed_config` (+0xD988) gets clobbered by
        // `ConvertScheme` (writes V3-extended-options byte at
        // `G_SCHEME_DATA + 0x164`, zero for V2 schemes). The late-fixup
        // in `init_session` restores it to Fixed 1.0 *after* ConvertScheme
        // runs, so writing it here would be undone.

        for (i, team) in pending.teams.iter().take(MAX_TEAM_RECORDS).enumerate() {
            let rec = &mut (*gi).team_records[i];
            // Single local player owns every team. See doc comment on
            // `GameInfoTeamRecord::owner_player_slot` for why non-zero
            // values here disable input dispatch.
            rec.owner_player_slot = 0;
            rec.team_color_idx = team.color_idx;
            rec.eliminated_flag = 0;
            rec.turn_order_idx = team.turn_order;
            rec.wins_count = 0;

            // WA stores name fields as Windows-1252. The UI hands us
            // UTF-8 strings (egui text widgets) and the WGT parser
            // round-trips through UTF-8 too, so we re-encode here.
            openwa_core::cp1252::encode_into_fixed(&mut rec.name, &team.name);

            // Worm setup: per-worm records at team_record + 0xA74 (stride
            // 0x28) and worm count byte at team_record + 0xBB4. Layout per
            // `GameWorld__InitTeamsFromSetup` (0x005220B0): first u16 of
            // each worm slot = starting HP; offsets +3..+0x13 = worm name
            // (16-byte CP1252).
            let team_rec_base = rec as *mut _ as *mut u8;
            let worm_count = team.worm_names.len().min(MAX_WORMS_PER_TEAM);
            *team_rec_base.add(0xBB4) = worm_count as u8;
            for (worm_idx, worm_name) in team.worm_names.iter().take(MAX_WORMS_PER_TEAM).enumerate()
            {
                let worm_ptr = team_rec_base.add(0xA74 + worm_idx * 0x28);
                core::ptr::write_bytes(worm_ptr, 0, 0x28);
                *(worm_ptr as *mut u16) = team.worm_hp;
                // 16-byte name field at +3 with the 16th byte reserved
                // for the NUL terminator (15 bytes of body).
                let dst = core::slice::from_raw_parts_mut(worm_ptr.add(3), 16);
                openwa_core::cp1252::encode_into_fixed(dst, worm_name);
            }
        }

        // Zero any team slots beyond `team_count` so stale snapshot data
        // can't leak in via a previous Frontend launch.
        for i in (team_count as usize)..MAX_TEAM_RECORDS {
            let rec = &mut (*gi).team_records[i];
            rec.owner_player_slot = 0;
            rec.team_color_idx = 0;
            rec.eliminated_flag = 0;
            rec.turn_order_idx = 0;
            rec.wins_count = 0;
            rec.name.fill(0);
        }

        // Copy scheme payload into G_SCHEME_DATA (402 bytes, V3 layout)
        // and fill the V3 extended-options tail with
        // [`openwa_core::scheme::EXTENDED_OPTIONS_DEFAULTS`] for any
        // bytes the file didn't supply. Without the defaults,
        // `CGameInfo__SetAmmo`'s read of `G_SCHEME_DATA + 0x12C` (the
        // Y-gravity dword) lands on a zero byte, propagates to
        // `GameInfo + 0xD9A8`, propagates again to
        // `game_state_stream + 0x22C`, and worms float on the
        // CustomLauncher path. (The Frontend lobby pre-fills the same
        // defaults during scheme load; openwa-core's parse only pads V3
        // schemes with a short payload, not V1/V2.)
        let dst = rb(va::G_SCHEME_DATA) as *mut u8;
        core::ptr::write_bytes(dst, 0, SCHEME_BUFFER_SIZE);
        let src = &pending.scheme.payload;
        let copy_len = src.len().min(SCHEME_BUFFER_SIZE);
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst, copy_len);

        if copy_len < SCHEME_BUFFER_SIZE {
            let defaults = &openwa_core::scheme::EXTENDED_OPTIONS_DEFAULTS;
            let tail_off = openwa_core::scheme::EXTENDED_OPTIONS_OFFSET;
            // For schemes that supplied some extended bytes (truncated
            // V3), keep what they wrote and only fill the gap; for
            // V1/V2 with no extended region, fill the whole tail.
            let pad_start = copy_len.max(tail_off);
            let pad_end = SCHEME_BUFFER_SIZE;
            let defaults_off = pad_start - tail_off;
            core::ptr::copy_nonoverlapping(
                defaults.as_ptr().add(defaults_off),
                dst.add(pad_start),
                pad_end - pad_start,
            );
        }
    }
}

/// Populate the MFC-lobby globals `Replay__ProcessTeamColors` consumes,
/// so the helper can run unchanged on the CustomLauncher path.
///
/// Layout:
/// - **`G_HOST_PLAYER`** (4 bytes at `0x008779E0`): host index, set to 0.
/// - **`G_PLAYER_ARRAY`** (13 × `0x78` records at `0x008779E4`): the lobby
///   player roster. PTC iterates by checking byte `+0x74` (active flag);
///   reads bytes `+0x11..+0x44` as the per-team config block copied
///   into `GameInfoTeamRecord` (offset +0x55 of the team_input_config
///   entry). We synthesise a single active local player at index 0.
/// - **`G_PLAYER_COUNT`** (byte at `0x0087D0DE`): set to 1.
/// - **`G_TEAM_DATA`** (6 × `0xD7B` records at `0x00877FFC`): the lobby
///   team roster. Uses the existing
///   [`crate::engine::replay::ReplayTeamEntry`] layout (same buffer
///   replay-loading writes; its `flag` at +0x124 is the active gate).
///   Per-team identity (alliance, name, worm names, worm count, color)
///   gets populated; the 340-byte weapon-kit at `+0x527` stays zero for
///   now (TODO once the encoding is reverse-engineered or pulled from a
///   real saved-team blob).
/// - **`G_TEAM_COUNT`** (byte at `0x0087D0E0`): set to `pending.teams.len()`.
///
/// # Safety
///
/// Mutates fixed globals. Caller must guarantee the WA process is
/// suspended/single-threaded for the duration (init_session does).
pub unsafe fn populate_lobby_globals(pending: &PendingCustomMatch) {
    use crate::engine::replay::ReplayTeamEntry;

    unsafe {
        let team_count = pending.teams.len().min(MAX_TEAM_RECORDS) as u8;
        let _ = openwa_core::log::log_line(&format!(
            "[pending_match] populating lobby globals: 1 player, {team_count} teams",
        ));

        // ── Player array ────────────────────────────────────────────────────
        let players = rb(va::G_PLAYER_ARRAY) as *mut u8;
        core::ptr::write_bytes(players, 0, LOBBY_PLAYER_STRIDE * MAX_LOBBY_PLAYERS);

        // Player 0: active, short name "P1", display name "Player".
        let p0 = players;
        let short_name = b"P1";
        core::ptr::copy_nonoverlapping(short_name.as_ptr(), p0, short_name.len());
        let display = b"Player";
        core::ptr::copy_nonoverlapping(display.as_ptr(), p0.add(0x11), display.len());
        // +0x74 = active flag (per PTC: `local_a0[0x1d] != '\\0'` gate).
        *p0.add(0x74) = 1;

        *(rb(va::G_HOST_PLAYER) as *mut u32) = 0;
        *(rb(va::G_PLAYER_COUNT) as *mut u8) = 1;

        // ── Team array ──────────────────────────────────────────────────────
        let teams = rb(va::G_TEAM_DATA) as *mut ReplayTeamEntry;
        core::ptr::write_bytes(teams as *mut u8, 0, LOBBY_TEAM_STRIDE * MAX_TEAM_RECORDS);

        for (i, team) in pending.teams.iter().take(MAX_TEAM_RECORDS).enumerate() {
            let entry = teams.add(i);

            // PTC reads `*pcVar6` (= +0x000, signed) as the owning player
            // index; -1 = anonymous CPU team. ReplayTeamEntry calls this
            // `team_type` but for our local-player offline match it's the
            // owner player slot.
            (*entry).team_type = 0;
            (*entry).alliance = team.color_idx;
            // +0x002 → game team_record.wins_count (PTC copies as-is).
            (*entry).unknown_02 = 0;

            // `config_abbrev` (+0x003..+0x013, 17 bytes) is what PTC copies
            // into `GameInfoTeamRecord.name` (16 bytes + tail). Use the
            // team's display name here, encoded as CP1252.
            let entry_u8 = entry as *mut u8;
            let abbrev_dst = core::slice::from_raw_parts_mut(entry_u8.add(0x03), 0x11);
            openwa_core::cp1252::encode_into_fixed(abbrev_dst, &team.name);

            // Empty soundbank_name defaults to "English" so the stock
            // bank loads. See `ReplayTeamEntry::speech_bank_dir` for the
            // propagation path through ProcessTeamColors into the GameInfo
            // per-team speech config slot.
            let bank = if team.soundbank_name.is_empty() {
                "English"
            } else {
                team.soundbank_name.as_str()
            };
            openwa_core::cp1252::encode_into_fixed(&mut (*entry).speech_bank_dir, bank);

            // Worm count + per-worm names. PTC's inner loop hardcodes 8
            // iterations starting at `pcVar6 + 0x9b` with stride 0x11.
            // `worm_count` at +0x098 is the validated 1..=8 count.
            let worm_count = team.worm_names.len().min(MAX_WORMS_PER_TEAM);
            (*entry).worm_count = worm_count as u8;
            (*entry).worm_count_raw = worm_count as u8;
            (*entry).color = team.color_idx;

            let worm_names_base = (entry as *mut u8).add(0x9B);
            for (worm_idx, worm_name) in team.worm_names.iter().take(MAX_WORMS_PER_TEAM).enumerate()
            {
                // Each worm-name slot is 0x11 bytes (16 body + NUL).
                let slot =
                    core::slice::from_raw_parts_mut(worm_names_base.add(worm_idx * 0x11), 0x11);
                openwa_core::cp1252::encode_into_fixed(slot, worm_name);
            }

            // Grave-sprite id. PTC propagates this into the active team
            // state. We can write the byte even for grave_id >= 0x80
            // (custom-bitmap range) — the actual bitmap upload is a
            // separate, currently-unported pipeline; until that's wired
            // up, custom-grave teams will render with the configured
            // index but reuse the default sprite for that slot.
            (*entry).grave = team.grave_id;

            // Special Weapon index (+0x125). Verified by headful gameplay
            // test 2026-05-15: the turn manager grants the indexed
            // SPECIAL_WEAPONS entry once Team Weapons mode unlocks
            // partway through the match.
            (*entry).special_weapon = team.special_weapon;

            // Team flag (palette + 20×17 bitmap). PTC at +0x127/+0x527
            // → active team_record's HUD flag slot. Empty WGT flag
            // (parser default) leaves both blocks zeroed so the HUD
            // shows the engine's blank-flag fallback.
            if let Some(wgt_flag) = team.flag.as_ref() {
                let pal_len = wgt_flag.palette.len().min(0x400);
                core::ptr::copy_nonoverlapping(
                    wgt_flag.palette.as_ptr(),
                    (*entry).flag_palette.as_mut_ptr(),
                    pal_len,
                );
                let bmp_len = wgt_flag.bitmap.len().min(0x154);
                core::ptr::copy_nonoverlapping(
                    wgt_flag.bitmap.as_ptr(),
                    (*entry).flag_bitmap.as_mut_ptr(),
                    bmp_len,
                );
            }

            // Active flag last (PTC's gate).
            (*entry).flag = 1;
        }

        *(rb(va::G_TEAM_COUNT) as *mut u8) = team_count;
    }
}
