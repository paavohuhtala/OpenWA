/// GameInfo — large game configuration/session struct.
///
/// Created by `GameInfo__InitSession` (0x4608E0), populated by
/// `GameInfo__LoadOptions` (0x460AC0) which reads registry values and
/// copies global data into known offsets.
///
/// PARTIAL: Only fields discovered through GameInfo__LoadOptions are mapped.
/// The actual struct is likely larger. Conservative size 0xF500.
#[repr(C)]
pub struct GameInfo {
    /// 0x0000-0x044B: Unknown
    pub _unknown_0000: [u8; 0x44C],

    // --- Speech configuration ---

    /// 0x044C: Number of teams with speech banks loaded (byte).
    /// Used by DSSound_LoadAllSpeechBanks to iterate teams.
    pub speech_team_count: u8,

    /// 0x044D-0xDAE7: Unknown
    pub _unknown_044d: [u8; 0xDAE8 - 0x44D],

    // --- Cluster 1: data paths ---

    /// 0xDAE8: Config DWORD (copied from global 0x88E390)
    pub _config_dword_dae8: u32,
    /// 0xDAEC: Land data path ("data\land.dat", 14 bytes incl. null)
    pub land_dat_path: [u8; 14],

    /// 0xDAFA-0xF39F: Unknown
    pub _unknown_dafa: [u8; 0x18A6],

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
    /// 0xF3B0: Home lock (registry: HomeLock, default 0)
    pub home_lock: u8,
    /// 0xF3B1: Unknown
    pub _unknown_f3b1: [u8; 3],
    /// 0xF3B4: Config DWORDs from globals (7 consecutive u32s).
    /// LoadOptions writes 5 DWORDs from G_CONFIG_DWORDS_F3B4 at indices 0..5,
    /// then 3 DWORDs from G_CONFIG_DWORDS_F3C4 at indices 4..7 (overlapping).
    pub _config_block_f3b4: [u32; 7],
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

    /// 0xF4C5-0xF4FF: Unknown remainder
    pub _unknown_f4c5: [u8; 0x3B],
}

const _: () = assert!(core::mem::size_of::<GameInfo>() == 0xF500);

struct HexU32s<'a>(&'a [u32]);

impl core::fmt::Debug for HexU32s<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[")?;
        for (i, v) in self.0.iter().enumerate() {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "0x{v:08X}")?;
        }
        write!(f, "]")
    }
}

impl core::fmt::Debug for GameInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Extract land_dat_path as a string (null-terminated)
        let land_str = self.land_dat_path.iter()
            .position(|&b| b == 0)
            .map(|end| core::str::from_utf8(&self.land_dat_path[..end]).unwrap_or("<invalid utf8>"))
            .unwrap_or(core::str::from_utf8(&self.land_dat_path).unwrap_or("<invalid utf8>"));

        // Extract speech_path as a string (null-terminated)
        let speech_str = self.speech_path.iter()
            .position(|&b| b == 0)
            .map(|end| core::str::from_utf8(&self.speech_path[..end]).unwrap_or("<invalid utf8>"))
            .unwrap_or(core::str::from_utf8(&self.speech_path).unwrap_or("<invalid utf8>"));

        f.debug_struct("GameInfo")
            // Cluster 1: data paths
            .field("_config_dword_dae8", &format_args!("0x{:08X}", self._config_dword_dae8))
            .field("land_dat_path", &land_str)
            // Cluster 2: game options
            .field("_config_byte_f3a0", &self._config_byte_f3a0)
            .field("detail_level", &self.detail_level)
            .field("energy_bar", &self.energy_bar)
            .field("info_transparency", &self.info_transparency)
            .field("info_spy", &self.info_spy)
            .field("chat_pinned", &self.chat_pinned)
            .field("chat_lines", &self.chat_lines)
            .field("pinned_chat_lines", &format_args!("0x{:08X}", self.pinned_chat_lines))
            .field("home_lock", &self.home_lock)
            .field("_config_block_f3b4", &HexU32s(&self._config_block_f3b4))
            .field("_config_dword_f3d4", &format_args!("0x{:08X}", self._config_dword_f3d4))
            .field("_config_dword_f3d8", &format_args!("0x{:08X}", self._config_dword_f3d8))
            .field("capture_transparent_pngs", &self.capture_transparent_pngs)
            .field("camera_unlock_mouse_speed", &self.camera_unlock_mouse_speed)
            .field("_config_dword_f3e4", &format_args!("0x{:08X}", self._config_dword_f3e4))
            .field("background_debris_parallax", &format_args!("0x{:08X}", self.background_debris_parallax))
            .field("topmost_explosion_onomatopoeia", &self.topmost_explosion_onomatopoeia)
            .field("_zeroed_f3f0", &self._zeroed_f3f0)
            .field("_conditional_config_f3f4", &HexU32s(&self._conditional_config_f3f4))
            .field("speech_path", &speech_str)
            .finish()
    }
}
