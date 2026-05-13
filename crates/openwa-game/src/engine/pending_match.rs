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
    /// .font_palette_idx`. Should be unique per team unless you want
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

        // `+0xD0BC` is the alliance-group count = number of distinct
        // turn-order/color groups. `Task_TurnGame__advance_ally_0`
        // (0x0055CAB0) uses it as the modulus when cycling the active
        // alliance pointer; a zero value triggers
        // STATUS_INTEGER_DIVIDE_BY_ZERO. `ProcessTeamColors` writes
        // this via a `CPlayers__GetTotalTeamsWithColour` count; on the
        // CustomLauncher path we count distinct `color_idx` values
        // across pending teams.
        let mut distinct_colors: u8 = 0;
        let mut seen: u64 = 0;
        for team in pending.teams.iter().take(MAX_TEAM_RECORDS) {
            let bit = 1u64 << (team.color_idx as u64 & 63);
            if seen & bit == 0 {
                seen |= bit;
                distinct_colors += 1;
            }
        }
        // Defensive: never write zero (would just re-crash). With at
        // least one pending team this branch never trips.
        if distinct_colors == 0 {
            distinct_colors = 1;
        }
        *((gi as *mut u8).add(0xD0BC)) = distinct_colors;

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
            rec.font_palette_idx = team.color_idx;
            rec.eliminated_flag = 0;
            rec.turn_order_idx = team.turn_order;
            rec.wins_count = 0;

            let name_bytes = team.name.as_bytes();
            let copy_len = name_bytes.len().min(rec.name.len() - 1);
            rec.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
            rec.name[copy_len..].fill(0);

            // Worm setup: per-worm records at team_record + 0xA74 (stride
            // 0x28) and worm count byte at team_record + 0xBB4. Layout per
            // `GameWorld__InitTeamsFromSetup` (0x005220B0): first u16 of
            // each worm slot = starting HP; offsets +3..+0x13 = worm name
            // (16-byte ASCII).
            let team_rec_base = rec as *mut _ as *mut u8;
            let worm_count = team.worm_names.len().min(MAX_WORMS_PER_TEAM);
            *team_rec_base.add(0xBB4) = worm_count as u8;
            for (worm_idx, worm_name) in team.worm_names.iter().take(MAX_WORMS_PER_TEAM).enumerate()
            {
                let worm_ptr = team_rec_base.add(0xA74 + worm_idx * 0x28);
                core::ptr::write_bytes(worm_ptr, 0, 0x28);
                *(worm_ptr as *mut u16) = team.worm_hp;
                let nb = worm_name.as_bytes();
                let nlen = nb.len().min(15);
                core::ptr::copy_nonoverlapping(nb.as_ptr(), worm_ptr.add(3), nlen);
            }
        }

        // Zero any team slots beyond `team_count` so stale snapshot data
        // can't leak in via a previous Frontend launch.
        for i in (team_count as usize)..MAX_TEAM_RECORDS {
            let rec = &mut (*gi).team_records[i];
            rec.owner_player_slot = 0;
            rec.font_palette_idx = 0;
            rec.eliminated_flag = 0;
            rec.turn_order_idx = 0;
            rec.wins_count = 0;
            rec.name.fill(0);
        }

        // Copy scheme payload into G_SCHEME_DATA (402 bytes, V3 layout).
        // V1/V2 payloads are shorter — zero the tail so leftover bytes
        // from a previous launch don't bleed through.
        let dst = rb(va::G_SCHEME_DATA) as *mut u8;
        core::ptr::write_bytes(dst, 0, SCHEME_BUFFER_SIZE);
        let src = &pending.scheme.payload;
        let copy_len = src.len().min(SCHEME_BUFFER_SIZE);
        core::ptr::copy_nonoverlapping(src.as_ptr(), dst, copy_len);
    }
}
