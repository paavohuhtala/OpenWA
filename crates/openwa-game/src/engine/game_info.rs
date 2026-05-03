/// Per-team scalar configuration record, stored as a 6-element array
/// inside [`GameInfo`] starting at offset `0x450` (stride `0xBB8`,
/// 0-based).
///
/// Holds **static** team setup (color, name, win count, eliminated flag,
/// speech bank id). The **mutable** runtime worm state — current HPs,
/// worm count, etc. — lives in
/// [`GameWorld::team_arena`](crate::engine::GameWorld::team_arena),
/// indexed 1-based.
///
/// PARTIAL: only the first ~7 bytes of each 3000-byte record are mapped.
/// The remainder probably holds turn-time overrides, weapon ammo
/// overrides, hat / grave bitmap indices, AI level, and per-team profile
/// data.
#[repr(C)]
pub struct GameInfoTeamRecord {
    /// 0x000: Speech bank index (`-1` = no bank). Read by
    /// `GameRuntime__WriteLogTeamLabel` as an index into the speech-bank
    /// table at `gameinfo + bank_id * 0x50 + 4`. Also re-read by
    /// `WorldEntity__InitAllianceData` as the team's alliance group.
    pub speech_bank_id: i8,
    /// 0x001: Font palette index — selects the team's scoreboard text
    /// color (slot 9..16 in WA's font table). Equivalently, the alliance
    /// id used by `WorldEntity__InitAllianceData`.
    pub font_palette_idx: u8,
    /// 0x002: Eliminated flag. `0` = include in leaderboard / scoring;
    /// non-zero = team eliminated for scoring purposes.
    pub eliminated_flag: u8,
    /// 0x003: Turn-order index (used by queue-sequence matching in
    /// `FUN_0055C920`). Confidence: medium.
    pub turn_order_idx: u8,
    pub _unknown_004: u8,
    /// 0x005: Round wins counter (u8). Sort key for the ESC-menu
    /// leaderboard.
    pub wins_count: u8,
    /// 0x006: Team name, null-terminated ASCII. WA's "%d" sprintf path
    /// for the leaderboard treats this as up to 14 bytes + NUL.
    pub name: [u8; 0x10],
    pub _rest: [u8; 0xBB8 - 0x16],
}

const _: () = assert!(core::mem::size_of::<GameInfoTeamRecord>() == 0xBB8);

/// Per-team **input** configuration record stored as a 6-element array in
/// [`GameInfo`] at offset 0x004 (stride 0x50, 0-based).
///
/// Layout is currently opaque — `NetInputCtrl::Init` just memcpys it into
/// `NetInputCtrl::team_records[i].config_data`. The downstream consumer
/// (input dispatch / replay capture / AI driver) is what interprets the
/// fields.
///
/// Distinct from [`GameInfoTeamRecord`] (the 0xBB8-byte gameplay/scoring
/// record at offset 0x450).
#[repr(C)]
pub struct TeamInputConfig {
    pub _opaque: [u8; 0x50],
}

const _: () = assert!(core::mem::size_of::<TeamInputConfig>() == 0x50);

/// Number of per-team record slots — both [`GameInfoTeamRecord`] (at +0x450)
/// and [`TeamInputConfig`] (at +0x004). WA classic supports up to 6 active teams.
pub const MAX_TEAM_RECORDS: usize = 6;

/// GameInfo — large game configuration/session struct.
///
/// Created by `GameInfo__InitSession` (0x4608E0), populated by
/// `GameInfo__LoadOptions` (0x460AC0) which reads registry values and
/// copies global data into known offsets.
///
/// PARTIAL: Only fields discovered through GameInfo__LoadOptions and
/// GameEngine__InitHardware are mapped. Size extended to 0xF91C to
/// cover all known accesses.
#[repr(C)]
pub struct GameInfo {
    /// 0x0000: Number of teams (byte, read as first byte of struct).
    pub num_teams: u8,
    /// 0x0001-0x0003: Padding before [`team_input_configs`].
    pub _unknown_0001: [u8; 3],

    /// 0x0004-0x01E3: Per-team input config records (stride 0x50, max 6).
    /// `NetInputCtrl::Init` (0x0058C0D0) memcpys `num_teams` of these verbatim
    /// into `NetInputCtrl::team_records[i].config_data`. Layout of each record
    /// is opaque to `NetInputCtrl::Init`; the downstream consumer (input
    /// dispatch / replay capture) interprets them.
    pub team_input_configs: [TeamInputConfig; MAX_TEAM_RECORDS],

    /// 0x01E4-0x044B: Unknown
    pub _unknown_01e4: [u8; 0x44C - 0x1E4],

    /// 0x044C: Count of populated `team_records` entries (≤
    /// [`MAX_TEAM_RECORDS`]). Used as the iteration bound by every team
    /// loop — the speech-bank loader, alliance/scoring init, the ESC
    /// menu leaderboard, and the headless log writer all read this byte.
    /// (Previously misnamed `speech_team_count`; speech-bank loading is
    /// just one of several consumers.)
    pub team_record_count: u8,

    /// 0x044D-0x044F: Unknown
    pub _unknown_044d: [u8; 3],
    /// 0x0450-0x4A9F: Per-team scalar configuration records (stride 0xBB8).
    pub team_records: [GameInfoTeamRecord; MAX_TEAM_RECORDS],
    /// 0x4AA0-0xD773: Unknown
    pub _unknown_4aa0: [u8; 0xD774 - 0x4AA0],
    /// 0xD774: Initial RNG seed from scheme options.
    pub rng_seed: u32,

    /// 0xD778: Game version/mode. Compared against various thresholds:
    /// -2 = network game, -3..0 = different game modes, 8+ = new versions.
    /// Used by GameWorld constructor for conditional initialization.
    pub game_version: i32,

    /// 0xD77C-0xD787: Unknown
    pub _unknown_d77c: [u8; 0xD788 - 0xD77C],
    /// 0xD788: Scoring parameter A (multiplied by 50 for initial score).
    pub scoring_param_a: u16,
    /// 0xD78A: Scoring parameter B (multiplied by 50 for initial score).
    pub scoring_param_b: u16,
    /// 0xD78C-0xD923: Per-weapon scheme settings — 0x198 bytes of packed
    /// per-weapon overlay data consumed by [`overlay_scheme_weapon_settings`]
    /// (Rust port of WA 0x0053AD80). Each byte (or u16 / signed byte ×
    /// 1000ms / byte ÷ 10000 cap) sources one field on a `WeaponEntry` or
    /// its `WeaponFireParams`. Layout is irregular — the per-weapon shape
    /// varies (utility weapons get short blocks, weapon 24/25 share bytes,
    /// weapon 62 is rewritten twice) so consumers index by the offsets the
    /// overlay function hard-codes rather than treating this as a struct.
    pub weapon_scheme_bytes: [u8; 0xD924 - 0xD78C],
    /// 0xD924: Starting team color index (u8). Copied to GameWorld.team_color at init.
    pub team_color_source: u8,
    /// 0xD925: Unknown
    pub _unknown_d925: u8,
    /// 0xD926: Per-scheme facing-fade byte. Mirrored into
    /// `WormEntity._field_15c` by every damage-path branch (cases 0x1C/0x76,
    /// 0x4B and a few state-reset arms of case 0x24); reader TBD.
    pub _scheme_d926: u8,
    /// 0xD927-0xD931: Unknown
    pub _unknown_d927: [u8; 0xD932 - 0xD927],
    /// 0xD932: DoubleTurnTime availability threshold (u16).
    /// If game_version > 0xD1 and this > 0x7FFF, DoubleTurnTime is disabled.
    pub double_turn_time_threshold: u16,
    /// 0xD934-0xD937: Unknown
    pub _unknown_d934: [u8; 0xD938 - 0xD934],
    /// 0xD938: Random crate drop percentage — land mines (u8, 0-100).
    pub drop_pct_land: u8,
    /// 0xD939: Random crate drop percentage — mines (u8, 0-100).
    pub drop_pct_mine: u8,
    /// 0xD93A: Random crate drop percentage — oil barrels (u8, 0-100).
    pub drop_pct_barrel: u8,
    /// 0xD93B: Unknown
    pub _unknown_d93b: u8,
    /// 0xD93C: Super weapon allowed flag. If 0, super weapons are
    /// disabled (except when game_version < 0x2A).
    pub super_weapon_allowed: u8,
    /// 0xD93D-0xD940: Unknown
    pub _unknown_d93d: [u8; 4],
    /// 0xD941: Sudden-death disable flag. Initialized from the round-time
    /// scheme byte (`(scheme[..] * 5) / 100`); when zero, the ESC menu
    /// shows the "Force Sudden Death" item (gated together with
    /// [`scheme_sd_secondary_lockout`](Self::scheme_sd_secondary_lockout))
    /// and `WorldRoot+0x55` is set to 1 if the round time is also zero.
    pub scheme_no_sd: u8,
    /// 0xD942-0xD943: Unknown
    pub _unknown_d942: [u8; 2],

    /// 0xD944: Network config byte 1 (copied to network object+0x28).
    pub net_config_1: u8,
    /// 0xD945: Unknown
    pub _unknown_d945: u8,
    /// 0xD946: Network config byte 2 (copied to network object+0x29).
    pub net_config_2: u8,

    /// 0xD947: "Draw allowed" flag (`scheme[..] * 0x14`). When zero, the
    /// ESC menu shows the "Draw This Round" item.
    pub scheme_no_draw: u8,
    /// 0xD948: Secondary sudden-death eligibility lockout. Default-init
    /// to `1`; later overridden by lobby/scheme post-processing. Paired
    /// with [`scheme_no_sd`](Self::scheme_no_sd) — the ESC menu shows
    /// "Force Sudden Death" only when **both** are zero.
    pub scheme_sd_secondary_lockout: u8,
    /// 0xD949: "Leaderboard hidden" flag. Default-init to `1`; later
    /// overridden by lobby/scheme post-processing. When non-zero the ESC
    /// menu suppresses the "First Team to N Wins" header and the
    /// per-team scoreboard rows.
    pub scheme_no_leaderboard: u8,
    /// 0xD94A: When zero, damage paths (cases 0x1C/0x76, 0x3B, 0x3E, 0x4B)
    /// add the post-clamp applied damage to `damage_taken_this_turn`; when
    /// nonzero they add the pre-clamp raw damage instead. Likely the
    /// "kaboom counter" / "true damage" scheme toggle.
    pub _scheme_d94a: u8,
    /// 0xD94B: Landscape scheme flag (nonzero enables terrain features via Landscape vtable slot 6).
    pub landscape_scheme_flag: u8,
    /// 0xD94C: Donkey (weapon 0x36) disable flag.
    pub donkey_disabled: u8,
    /// 0xD94D-0xD94E: Unknown
    pub _unknown_d94d: [u8; 2],
    /// 0xD94F: The `N` literal in the ESC menu's "First Team to N Wins"
    /// header — the wins-to-victory threshold.
    pub scheme_first_to_n_wins: u8,
    /// 0xD950-0xD954: Unknown
    pub _unknown_d950: [u8; 5],
    /// 0xD955: Terrain drop config byte A. Copied to GameWorld.terrain_pct_a.
    pub terrain_cfg_a: u8,
    /// 0xD956: When set, the AquaSheep slot is treated as SuperSheep instead.
    pub aquasheep_is_supersheep: u8,
    /// 0xD957: Terrain drop config byte C. Copied to GameWorld.terrain_pct_c.
    pub terrain_cfg_c: u8,
    /// 0xD958: Terrain drop config byte B. Copied to GameWorld.terrain_pct_b.
    pub terrain_cfg_b: u8,
    /// 0xD959: Version-gated weapon restriction. If nonzero and game_version > 0x29,
    /// returns -2 for unavailable weapons.
    pub weapon_version_gate: u8,
    /// 0xD95A: Unknown
    pub _unknown_d95a: u8,
    /// 0xD95B: Gates the writeback of `_scheme_d926` into the worm's
    /// `_field_15c`. When zero, damage paths skip the facing-fade copy on
    /// the entry guard (the per-damage-kind switch can still write it).
    pub _scheme_d95b: u8,
    /// 0xD95C: Same-alliance damage threshold (u8). When the sender and the
    /// receiver share `weapon_alliance`, damage paths (msgs 0x1C/0x76 ApplyDamage,
    /// 0x4B SpecialImpact, 0x51 PoisonWorm) read this byte: values `> 2` block
    /// the damage entirely. Effectively the friendly-fire toggle.
    pub friendly_fire_threshold: u8,
    /// 0xD95D: Cross-alliance damage threshold (u8). Same comparison shape as
    /// `friendly_fire_threshold`, applied when the sender and receiver have
    /// different `weapon_alliance` values. `> 2` blocks the damage.
    pub enemy_fire_threshold: u8,
    /// 0xD95E-0xD967: Unknown
    pub _unknown_d95e: [u8; 0xD968 - 0xD95E],
    /// 0xD968: Extended team count (u16). Used for buffer allocation sizing.
    pub num_teams_alloc: u16,
    /// 0xD96A: Extended terrain drop percentage (u8, remainder after land/mine/barrel).
    pub ext_terrain_pct: u8,
    /// 0xD96B: Extended terrain drop type (u8). 0 = auto-fill remainder.
    pub ext_terrain_type: u8,
    /// 0xD96C-0xD96D: Unknown
    pub _unknown_d96c: [u8; 2],
    /// 0xD96E: Read by the `Jump` (msg 0x24) handler in the `WeaponAimed`
    /// (0x78) state branch. When zero, the worm transitions to `PostFire`
    /// (0x7E) if it is still moving; when non-zero, the moving-worm
    /// transition is skipped and the worm always returns to `Idle` (0x65).
    /// Likely a "skip post-fire animation" scheme toggle. Writers TBD.
    pub _scheme_d96e: u8,
    /// 0xD96F-0xD987: Unknown
    pub _unknown_d96f: [u8; 0xD988 - 0xD96F],
    /// 0xD988: Game speed config (Fixed16.16). Read as i32 for headful game_speed_target.
    /// Note: high byte (0xD98B) overlaps with terrain_flag — use `terrain_flag` accessor.
    pub game_speed_config: i32,
    /// 0xD98C-0xD98F: Unknown
    pub _unknown_d98c: [u8; 0xD990 - 0xD98C],
    /// 0xD990: Team slot allocation count (u32). Used to size GameWorld+0x514 array.
    pub team_slot_count: u32,
    /// 0xD994: Object slot allocation count (u32). Used to size GameWorld+0x518 array
    /// and buffer object allocation.
    pub object_slot_count: u32,
    /// 0xD998-0xD9A1: Unknown
    pub _unknown_d998: [u8; 0xD9A2 - 0xD998],
    /// 0xD9A2: Network weapon exception flag. When net_config_2 != 0,
    /// weapons 10/0x37/0x38 are only disabled if this is also 0.
    pub net_weapon_exception: u8,
    /// 0xD9A3-0xD9B0: Unknown
    pub _unknown_d9a3: [u8; 0xD9B1 - 0xD9A3],
    /// 0xD9B1: Scheme sub-version byte (signed). Read by SelectFuse /
    /// SelectHerd handlers as a "scheme allows extended fuse/herd range"
    /// gate (compared against `0x1A`/`0x1F` thresholds).
    pub _scheme_d9b1: i8,
    /// 0xD9B2-0xD9CE: Unknown
    pub _unknown_d9b2: [u8; 0xD9CF - 0xD9B2],
    /// 0xD9CF: "Force all weapons aimed" scheme flag (u8). When non-zero,
    /// the tail of [`overlay_scheme_weapon_settings`] sets
    /// `requires_aiming = 1` on every weapon 1..71 — confirmed sole reader
    /// (only immediate `0xD9CF` in WA.exe lives at 0x0053C0FF, inside that
    /// final loop).
    pub force_all_weapons_aim: u8,
    /// 0xD9D0: Extended fuse / herd range scheme flag. When non-zero,
    /// SelectFuse widens the accepted fuse range from `[1..=5]` to `[1..=9]`
    /// (and `[0..=9]` if `_scheme_d9b1 > 0x1A`); SelectHerd similarly widens
    /// the herd index cap.
    pub _scheme_d9d0: u8,
    /// 0xD9D1-0xD9DB: Unknown
    pub _unknown_d9d1: [u8; 0xD9DC - 0xD9D1],
    /// 0xD9DC: Index of the starting team for this round.
    pub starting_team_index: i8,
    /// 0xD9DD: Game mode flag. Negative = training/replay mode. Also used as starting-team index for activity flags in normal mode.
    pub game_mode_flag: i8,
    /// 0xD9DE-0xD9DF: Unknown
    pub _unknown_d9de: [u8; 2],

    /// 0xD9E0: Streaming audio config data (path config passed to streaming audio ctor).
    /// Address of this field is passed as a pointer parameter.
    pub streaming_audio_config: [u8; 0xDAA4 - 0xD9E0],

    /// 0xDAA4: Speech/streaming audio enabled flag.
    /// If nonzero, streaming audio subsystem is created in InitHardware.
    pub speech_enabled: u8,

    /// 0xDAA5: Music master volume percentage (u8, 0..100). Read by
    /// `ApplyVolumeSettings` and used as a multiplier when scaling the
    /// live sound-volume Fixed value down for `Music::SetVolume` —
    /// `music_set = (music_volume_percent * sound_volume_fixed) / 100`.
    /// Sourced from the global config / options screen.
    pub music_volume_percent: u8,
    /// 0xDAA6-0xDAA7: Unknown
    pub _unknown_daa6: [u8; 2],
    /// 0xDAA8: Initial sound volume percentage (i32, 0..100). Used only
    /// at `init_game_state` to seed [`GameRuntime::sound_volume`] via the
    /// percent→Fixed conversion `(val << 16) / 100`. Sourced from the
    /// global config / options screen.
    pub sound_volume_percent: i32,

    /// 0xDAAC: Landscape data path (passed to Landscape constructor).
    /// Points to a path string used for loading level terrain data.
    pub landscape_data_path: [u8; 0xDAE8 - 0xDAAC],

    // --- Cluster 1: data paths ---
    /// 0xDAE8: Config DWORD (copied from global 0x88E390)
    pub _config_dword_dae8: u32,
    /// 0xDAEC: Land data path ("data\land.dat", 14 bytes incl. null)
    pub land_dat_path: [u8; 14],

    // --- Replay configuration (populated by ReplayLoader) ---
    /// 0xDAFA-0xDB07: Unknown
    pub _unknown_dafa: [u8; 0xDB08 - 0xDAFA],
    /// 0xDB08: Invisibility (weapon 0x42) mode flag (u32). Controls team-count
    /// vs network_ecx check for availability.
    pub invisibility_mode: u32,
    /// 0xDB0C: Replay/network config flag (u8). Checked by InitGameState.
    pub replay_config_flag: u8,
    /// 0xDB0D-0xDB1B: Unknown
    pub _unknown_db0d: [u8; 0xDB1C - 0xDB0D],
    /// 0xDB1C: Replay map sub-type (first DWORD of first payload).
    /// >= 1: map stored in playback.thm. Negative: inline map data.
    pub replay_map_type: i32,
    /// 0xDB20: Replay payload field 2 (sub-type < 1 path).
    pub replay_payload_2: i32,
    /// 0xDB24: Replay payload extra data (variable-length, up to 0x24 bytes).
    pub replay_payload_extra: [u8; 0x24],
    /// 0xDB48: Replay active flag (set to 1 by ReplayLoader).
    pub replay_active: u8,
    /// 0xDB49-0xDB4F: Unknown
    pub _unknown_db49: [u8; 0xDB50 - 0xDB49],
    /// 0xDB50: Replay file format version.
    pub replay_format_version: u32,
    /// 0xDB54: Replay file format version (duplicate/backup).
    pub replay_format_version_2: u32,
    /// 0xDB58: Replay field (set to 0xFFFFFFFF during loading).
    pub replay_field_db58: u32,
    /// 0xDB5C-0xDB5F: Unknown
    pub _unknown_db5c: [u8; 4],
    /// 0xDB60: Replay filename buffer (C string, null-terminated).
    pub replay_filename: [u8; 0x400],
    /// 0xDF60-0xEF37: Unknown
    pub _unknown_df60: [u8; 0xEF38 - 0xDF60],
    /// 0xEF38: Headless log FILE* stream (CRT FILE pointer). Nonzero means
    /// headless logging is enabled — DispatchFrame writes `HH:MM:SS.CC`
    /// per-frame timestamps here, and StepFrame's end-of-game branch writes
    /// the end-of-round stats block. Null = no log.
    pub headless_log_stream: *mut core::ffi::c_void,
    /// 0xEF3C: Replay tick rate (ticks per replay frame). Zero in non-replay mode;
    /// DispatchFrame uses this as the primary replay-vs-live branch.
    pub replay_ticks: i32,
    /// 0xEF40-0xEF5F: Unknown
    pub _unknown_ef40: [u8; 0xEF60 - 0xEF40],
    /// 0xEF60: Cleared to 0 during replay loading.
    pub replay_field_ef60: u32,
    /// 0xEF64-0xF33F: Unknown
    pub _unknown_ef64: [u8; 0xF340 - 0xEF64],
    /// 0xF340: State flag. InitGameState: `(this != 0) - 1` → wrapper+0xEC.
    pub _field_f340: u32,

    /// 0xF344: Sound start frame threshold (i32). Sound is suppressed when
    /// GameWorld.frame_counter < this value. Checked by IsSoundSuppressed and
    /// DispatchGlobalSound.
    pub sound_start_frame: i32,

    /// 0xF348: Sound mute flag (byte). Nonzero = all sound suppressed.
    /// Checked by IsSoundSuppressed, DispatchGlobalSound, PlaySoundPooled_Direct.
    pub sound_mute: u8,

    /// 0xF349-0xF34B: Unknown
    pub _unknown_f349: [u8; 0xF34C - 0xF349],
    /// 0xF34C: State field (i32). Set to -1 by InitGameState.
    pub _field_f34c: i32,
    /// 0xF350: Replay end frame (i32). In replay mode, DispatchFrame triggers
    /// game-over once `GameWorld.frame_counter` passes this value.
    pub replay_end_frame: i32,
    /// 0xF354-0xF360: Unknown
    pub _unknown_f354: [u8; 0xF361 - 0xF354],
    /// 0xF361: Render phase config byte. Copied to GameWorld.render_phase at init.
    pub render_phase_cfg: u8,
    /// 0xF362: Unknown byte copied to GameWorld+0x7788 during turn state init.
    pub _field_f362: u8,
    /// 0xF363: Config byte. Copied to GameWorld._field_7644 at init.
    pub _field_f363: u8,
    /// 0xF364: Config byte. Copied to GameWorld._field_7648 at init.
    pub _field_f364: u8,
    /// 0xF365: HUD team-bar extended-mode config (u8 bool). Source for
    /// [`crate::engine::runtime::GameRuntime::hud_team_bar_extended`] —
    /// bool-ified into the runtime field at game-state init. When set,
    /// the in-game HUD uses the tall team-bar layout and suppresses
    /// the "unseen chat messages" indicator.
    pub _field_f365: u8,
    /// 0xF366-0xF367: Unknown
    pub _unknown_f366: [u8; 0xF368 - 0xF366],
    /// 0xF368: Worm selection count config (i32). 0 = use default.
    pub worm_select_cfg_a: i32,
    /// 0xF36C: Worm selection count alt config (i32). -1 = use default (7).
    pub worm_select_cfg_b: i32,
    /// 0xF370: Display palette flag (u8). Bool-ified to palette+0x10 if headful.
    pub _field_f370: u8,
    /// 0xF371-0xF373: Unknown
    pub _unknown_f371: [u8; 0xF374 - 0xF371],

    /// 0xF374: Display flags passed to DisplayGfx::Init.
    pub display_flags: u32,

    /// 0xF378-0xF383: Unknown
    pub _unknown_f378: [u8; 0xF384 - 0xF378],
    /// 0xF384: Input-detection flags consumed by `Keyboard::CheckAction` case
    /// 0x42 (tilde / backtick equivalence). Bit 0 CLEAR enables the
    /// scancode-based probe (`MapVirtualKeyA(0x29, MAPVK_VSC_TO_VK)`); bit 1
    /// CLEAR enables the layout-based probe (`VkKeyScanA('`')`). Polarity is
    /// "feature disabled when bit set" — WA's only consumer.
    pub _field_f384: u32,
    /// 0xF388-0xF38B: Unknown
    pub _unknown_f388: [u8; 0xF38C - 0xF388],
    /// 0xF38C: Sound distance attenuation factor (i32). When nonzero, enables 3D
    /// positional audio via Distance3D_Attenuation. Zero = all sounds at full volume.
    pub sound_attenuation: i32,
    /// 0xF390-0xF397: Unknown
    pub _unknown_f390: [u8; 0xF398 - 0xF390],
    /// 0xF398: "Snap animations" / non-advancing-frame latch (i32). When
    /// non-zero, three different easers in `DispatchFrame` collapse to
    /// their target values instead of stepping one tick:
    /// - The timer ratio smoother snaps `turn_timer_max` to
    ///   `turn_timer_current` (also gates `sound_started`).
    /// - Slot A animation (`_field_3fc`) snaps to its target.
    /// - Both ESC-menu animations (`esc_menu_anim` / `confirm_anim`) snap
    ///   to their targets.
    ///
    /// Likely set during pause/seek/resume frames where simulation time
    /// isn't advancing smoothly — collapsing in-progress eases avoids
    /// visible animation glitches and time-aligned audio firing on a
    /// frame that doesn't represent real elapsed time. The original
    /// "sound suppression" interpretation describes one downstream effect
    /// (sound is gated by the same flag) but not the field's purpose.
    pub _field_f398: i32,
    /// 0xF39C: Read as a `u32` and copied verbatim into
    /// `GameSession.display_param_1` by `GameSession::Run`. Purpose unknown.
    pub _field_f39c: u32,

    // --- Cluster 2: game options (populated by LoadOptions) ---
    /// 0xF3A0: Unknown config byte (from global 0x7C0D38)
    pub _config_byte_f3a0: u8,
    /// 0xF3A1: Detail level (registry: DetailLevel, default 5)
    pub detail_level: u8,
    /// 0xF3A2: Energy bar display (registry: EnergyBar, default 1)
    pub energy_bar: u8,
    /// 0xF3A3: Info transparency (registry: InfoTransparency, default 0)
    pub info_transparency: u8,
    /// 0xF3A4: Info spy enabled (registry: InfoSpy, default 1, bool coerced)
    pub info_spy: u8,
    /// 0xF3A5: Chat pinned (registry: ChatPinned, default 0)
    pub chat_pinned: u8,
    /// 0xF3A6: Unknown
    pub _unknown_f3a6: [u8; 2],
    /// 0xF3A8: Chat line count (registry: ChatLines, default 0)
    pub chat_lines: u32,
    /// 0xF3AC: Pinned chat lines (registry: PinnedChatLines, default 0xFFFFFFFF)
    pub pinned_chat_lines: u32,
    /// 0xF3B0: Home lock (registry: HomeLock, default 0).
    ///
    /// Authoritatively a `u8` — `LoadOptions` is the only writer and writes
    /// the low byte only. `InitHardware` and `DispatchFrame` happen to read
    /// this as a `word` in the disassembly, but the high byte (0xF3B1) is
    /// zero-initialised and never modified, so the wider read is bit-for-bit
    /// equivalent to a byte read. Consumers should use this `u8` field.
    ///
    /// Roles: `(home_lock != 0)` sets `GameSession.home_lock_active` during
    /// `InitHardware`, and `DispatchFrame` compares it against
    /// `GameWorld._field_77d4 / 50` to trigger `game_state = 4` (headless exit)
    /// after the matching number of turn seconds.
    pub home_lock: u8,
    /// 0xF3B1: Unknown
    pub _unknown_f3b1: [u8; 3],
    /// 0xF3B4: Display width — first DWORD of the config block.
    /// Written from G_CONFIG_DWORDS_F3B4, updated by DisplayGfx::Init retry loop.
    pub display_width: u32,
    /// 0xF3B8: Display height — second DWORD of the config block.
    /// Written from G_CONFIG_DWORDS_F3B4, updated by DisplayGfx::Init retry loop.
    pub display_height: u32,
    /// 0xF3BC: Remaining config DWORDs (indices 2..7 of the original block).
    /// LoadOptions writes indices 0..5 from G_CONFIG_DWORDS_F3B4,
    /// then indices 4..7 from G_CONFIG_DWORDS_F3C4 (overlapping at [2]).
    pub _config_dwords_f3bc: [u32; 5],
    /// 0xF3D0: Unknown (not written by LoadOptions)
    pub _unknown_f3d0: [u8; 4],
    /// 0xF3D4: Config DWORD (from global 0x88E3B0[0])
    pub _config_dword_f3d4: u32,
    /// 0xF3D8: Config DWORD (from global 0x88E3B0[1])
    pub _config_dword_f3d8: u32,
    /// 0xF3DC: Capture transparent PNGs flag (registry, default 0)
    pub capture_transparent_pngs: u32,
    /// 0xF3E0: Camera unlock mouse speed (registry, clamped to 0xB504 then squared)
    pub camera_unlock_mouse_speed: u32,
    /// 0xF3E4: Config DWORD (from global 0x88E44C)
    pub _config_dword_f3e4: u32,
    /// 0xF3E8: Background debris parallax (registry, fixed-point 16.16)
    pub background_debris_parallax: u32,
    /// 0xF3EC: Topmost explosion onomatopoeia flag (registry, default 0)
    pub topmost_explosion_onomatopoeia: u32,
    /// 0xF3F0: Zeroed at init
    pub _zeroed_f3f0: u16,
    /// 0xF3F2: Unknown
    pub _unknown_f3f2: [u8; 2],
    /// 0xF3F4: Conditional config block (4 DWORDs from global 0x88E3B8, only if guard==0)
    pub _conditional_config_f3f4: [u32; 4],
    /// 0xF404: Speech directory path (null-terminated, up to 129 bytes)
    pub speech_path: [u8; 0x81],
    /// 0xF485: Config data block (64 bytes copied from global 0x88DFF3)
    pub _config_block_f485: [u8; 64],

    /// 0xF4C5-0xF4FF: Unknown
    pub _unknown_f4c5: [u8; 0xF500 - 0xF4C5],

    // --- Extended region (beyond original 0xF500 conservative estimate) ---
    /// 0xF500-0xF913: Unknown
    pub _unknown_f500: [u8; 0xF914 - 0xF500],

    /// 0xF914: Headless/stats mode flag.
    /// If nonzero, InitHardware creates a GameStats stub instead of display hardware.
    pub headless_mode: u32,

    /// 0xF918: Input state field — Keyboard+0x4 stores a pointer TO this address.
    /// The game reads/writes through Keyboard's pointer, so this must stay in place.
    pub input_state_f918: u32,
}

const _: () = assert!(core::mem::size_of::<GameInfo>() == 0xF91C);
const _: () = assert!(core::mem::offset_of!(GameInfo, team_records) == 0x450);
const _: () = assert!(core::mem::offset_of!(GameInfo, scheme_no_sd) == 0xD941);
const _: () = assert!(core::mem::offset_of!(GameInfo, scheme_no_draw) == 0xD947);
const _: () = assert!(core::mem::offset_of!(GameInfo, scheme_sd_secondary_lockout) == 0xD948);
const _: () = assert!(core::mem::offset_of!(GameInfo, scheme_no_leaderboard) == 0xD949);
const _: () = assert!(core::mem::offset_of!(GameInfo, scheme_first_to_n_wins) == 0xD94F);

impl GameInfo {
    /// Access terrain_flag at offset 0xD98B (high byte of game_speed_config).
    /// Set from map object during replay loading: 0 = cavern terrain.
    pub fn terrain_flag(&self) -> u8 {
        (self.game_speed_config >> 24) as u8
    }
    /// Set terrain_flag (high byte of game_speed_config at 0xD98B).
    pub fn set_terrain_flag(&mut self, val: u8) {
        self.game_speed_config = (self.game_speed_config & 0x00FFFFFF) | ((val as i32) << 24);
    }
}

struct HexU32s<'a>(&'a [u32]);

impl core::fmt::Debug for HexU32s<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[")?;
        for (i, v) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "0x{v:08X}")?;
        }
        write!(f, "]")
    }
}

impl core::fmt::Debug for GameInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Extract land_dat_path as a string (null-terminated)
        let land_str = self
            .land_dat_path
            .iter()
            .position(|&b| b == 0)
            .map(|end| core::str::from_utf8(&self.land_dat_path[..end]).unwrap_or("<invalid utf8>"))
            .unwrap_or(core::str::from_utf8(&self.land_dat_path).unwrap_or("<invalid utf8>"));

        // Extract speech_path as a string (null-terminated)
        let speech_str = self
            .speech_path
            .iter()
            .position(|&b| b == 0)
            .map(|end| core::str::from_utf8(&self.speech_path[..end]).unwrap_or("<invalid utf8>"))
            .unwrap_or(core::str::from_utf8(&self.speech_path).unwrap_or("<invalid utf8>"));

        f.debug_struct("GameInfo")
            // Cluster 1: data paths
            .field(
                "_config_dword_dae8",
                &format_args!("0x{:08X}", self._config_dword_dae8),
            )
            .field("land_dat_path", &land_str)
            // Cluster 2: game options
            .field("_config_byte_f3a0", &self._config_byte_f3a0)
            .field("detail_level", &self.detail_level)
            .field("energy_bar", &self.energy_bar)
            .field("info_transparency", &self.info_transparency)
            .field("info_spy", &self.info_spy)
            .field("chat_pinned", &self.chat_pinned)
            .field("chat_lines", &self.chat_lines)
            .field(
                "pinned_chat_lines",
                &format_args!("0x{:08X}", self.pinned_chat_lines),
            )
            .field("home_lock", &self.home_lock)
            .field(
                "display_flags",
                &format_args!("0x{:08X}", self.display_flags),
            )
            .field("display_width", &self.display_width)
            .field("display_height", &self.display_height)
            .field("_config_dwords_f3bc", &HexU32s(&self._config_dwords_f3bc))
            .field(
                "_config_dword_f3d4",
                &format_args!("0x{:08X}", self._config_dword_f3d4),
            )
            .field(
                "_config_dword_f3d8",
                &format_args!("0x{:08X}", self._config_dword_f3d8),
            )
            .field("capture_transparent_pngs", &self.capture_transparent_pngs)
            .field("camera_unlock_mouse_speed", &self.camera_unlock_mouse_speed)
            .field(
                "_config_dword_f3e4",
                &format_args!("0x{:08X}", self._config_dword_f3e4),
            )
            .field(
                "background_debris_parallax",
                &format_args!("0x{:08X}", self.background_debris_parallax),
            )
            .field(
                "topmost_explosion_onomatopoeia",
                &self.topmost_explosion_onomatopoeia,
            )
            .field("_zeroed_f3f0", &self._zeroed_f3f0)
            .field(
                "_conditional_config_f3f4",
                &HexU32s(&self._conditional_config_f3f4),
            )
            .field("speech_path", &speech_str)
            .field("speech_enabled", &self.speech_enabled)
            .field("headless_mode", &self.headless_mode)
            .field(
                "input_state_f918",
                &format_args!("0x{:08X}", self.input_state_f918),
            )
            .finish()
    }
}
