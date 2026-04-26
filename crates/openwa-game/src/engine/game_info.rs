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
    /// 0x0001-0x044B: Unknown
    pub _unknown_0001: [u8; 0x44C - 1],

    // --- Speech configuration ---
    /// 0x044C: Number of teams with speech banks loaded (byte).
    /// Used by DSSound_LoadAllSpeechBanks to iterate teams.
    pub speech_team_count: u8,

    /// 0x044D-0xD773: Unknown
    pub _unknown_044d: [u8; 0xD774 - 0x44D],
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
    /// 0xD78C-0xD923: Unknown
    pub _unknown_d78c: [u8; 0xD924 - 0xD78C],
    /// 0xD924: Starting team color index (u8). Copied to GameWorld.team_color at init.
    pub team_color_source: u8,
    /// 0xD925-0xD931: Unknown
    pub _unknown_d925: [u8; 0xD932 - 0xD925],
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
    /// 0xD93D-0xD943: Unknown
    pub _unknown_d93d: [u8; 0xD944 - 0xD93D],

    /// 0xD944: Network config byte 1 (copied to network object+0x28).
    pub net_config_1: u8,
    /// 0xD945: Unknown
    pub _unknown_d945: u8,
    /// 0xD946: Network config byte 2 (copied to network object+0x29).
    pub net_config_2: u8,

    /// 0xD947-0xD94A: Unknown
    pub _unknown_d947: [u8; 4],
    /// 0xD94B: Landscape scheme flag (nonzero enables terrain features via Landscape vtable slot 6).
    pub landscape_scheme_flag: u8,
    /// 0xD94C: Donkey (weapon 0x36) disable flag.
    pub donkey_disabled: u8,
    /// 0xD94D-0xD954: Unknown
    pub _unknown_d94d: [u8; 0xD955 - 0xD94D],
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
    /// 0xD95A-0xD967: Unknown
    pub _unknown_d95a: [u8; 0xD968 - 0xD95A],
    /// 0xD968: Extended team count (u16). Used for buffer allocation sizing.
    pub num_teams_alloc: u16,
    /// 0xD96A: Extended terrain drop percentage (u8, remainder after land/mine/barrel).
    pub ext_terrain_pct: u8,
    /// 0xD96B: Extended terrain drop type (u8). 0 = auto-fill remainder.
    pub ext_terrain_type: u8,
    /// 0xD96C-0xD987: Unknown
    pub _unknown_d96c: [u8; 0xD988 - 0xD96C],
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
    /// 0xD9A3-0xD9DB: Unknown
    pub _unknown_d9a3: [u8; 0xD9DC - 0xD9A3],
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

    /// 0xDAA5-0xDAA7: Unknown
    pub _unknown_daa5: [u8; 0xDAA8 - 0xDAA5],
    /// 0xDAA8: Turn percentage raw value (i32). Converted to fixed-point: `(val << 16) / 100`.
    pub turn_percentage_raw: i32,

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
    /// 0xF365: Terrain config flag (u8). Bool-ified to wrapper+0x414.
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

    /// 0xF378-0xF38B: Unknown
    pub _unknown_f378: [u8; 0xF38C - 0xF378],
    /// 0xF38C: Sound distance attenuation factor (i32). When nonzero, enables 3D
    /// positional audio via Distance3D_Attenuation. Zero = all sounds at full volume.
    pub sound_attenuation: i32,
    /// 0xF390-0xF397: Unknown
    pub _unknown_f390: [u8; 0xF398 - 0xF390],
    /// 0xF398: Sound suppression flag (i32). Checked together with
    /// `sound_start_frame` and `sound_mute` by DispatchFrame to decide whether
    /// timing ratio updates advance normally or snap to target.
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
    /// Roles: `(home_lock != 0)` sets `GameSession.fullscreen_flag` during
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

    /// 0xF918: Input state field — DDKeyboard+0x4 stores a pointer TO this address.
    /// The game reads/writes through DDKeyboard's pointer, so this must stay in place.
    pub input_state_f918: u32,
}

const _: () = assert!(core::mem::size_of::<GameInfo>() == 0xF91C);

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
