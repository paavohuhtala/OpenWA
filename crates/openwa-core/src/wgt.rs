//! Team file (.WGT) parser for Worms Armageddon.
//!
//! .WGT files store a roster of teams selectable from the frontend's team
//! picker. Located in `User\Teams\*.WGT`. The retail install ships
//! `User\Teams\WG.WGT` populated with sample teams.
//!
//! Binary layout (Worms Armageddon variant) — see worms2d.info/Team_file
//! for the canonical reference. Per-team records vary in length because
//! the custom-grave block is present only when `grave_id >= 0x80`.
//!
//! Limitations: this parser deliberately decodes only the fields OpenWA
//! currently consumes (team identity, worm names, soundbank/fanfare
//! names, grave, flag). The various unknown trailers + per-mission stats
//! are preserved as opaque byte slices so a future round-trip writer can
//! re-emit them verbatim.

/// Magic header bytes for .WGT files.
pub const WGT_MAGIC: [u8; 4] = *b"WGT\0";

/// Size of the file header (magic + version + team count + cheat flags + 1 unknown).
pub const WGT_HEADER_SIZE: usize = 11;

/// Length of fixed team-name field, in bytes (null-terminated ASCII).
pub const TEAM_NAME_LEN: usize = 17;
/// Length of fixed worm-name field, in bytes (null-terminated ASCII).
pub const WORM_NAME_LEN: usize = 17;
/// Number of worm-name slots per team.
pub const WORMS_PER_TEAM: usize = 8;
/// Length of soundbank-name / fanfare-name / grave-filename / flag-filename
/// fields, in bytes (null-terminated ASCII).
pub const FILENAME_LEN: usize = 32;
/// Length of an embedded 256-colour BGR0 palette (256 × 4 bytes).
pub const PALETTE_LEN: usize = 256 * 4;
/// Dimensions of the custom-grave bitmap (24 wide × 32 tall, 8bpp).
pub const GRAVE_BMP_LEN: usize = 24 * 32;
/// Dimensions of the team-flag bitmap (20 wide × 17 tall, 8bpp).
pub const FLAG_BMP_LEN: usize = 20 * 17;
/// Per-team stats trailer: 10 stat dwords + 33×2 mission dwords.
pub const STATS_LEN: usize = 4 * 10 + 4 * 33 * 2;
/// Per-team trailer after the flag bitmap (dm_rank + training data + unknowns).
pub const POST_FLAG_LEN: usize = 1 + 24 + 40 + 6 + 10 + 28 + 1;
/// Total per-team record size when no custom grave is embedded.
pub const TEAM_RECORD_LEN_NO_GRAVE: usize = TEAM_NAME_LEN
    + WORM_NAME_LEN * WORMS_PER_TEAM
    + 1                             // control
    + FILENAME_LEN                  // soundbank name
    + 1                             // soundbank location
    + FILENAME_LEN                  // fanfare name
    + 1                             // use custom fanfare
    + 1                             // grave id
    + 1                             // team weapon
    + STATS_LEN
    + FILENAME_LEN                  // flag filename
    + PALETTE_LEN                   // flag palette
    + FLAG_BMP_LEN
    + POST_FLAG_LEN;
/// Per-team record size when a custom grave block is embedded.
pub const TEAM_RECORD_LEN_WITH_GRAVE: usize =
    TEAM_RECORD_LEN_NO_GRAVE + FILENAME_LEN + PALETTE_LEN + GRAVE_BMP_LEN;

/// Grave-id sentinel: values ≥ this carry an inline custom-grave block.
pub const CUSTOM_GRAVE_THRESHOLD: u8 = 0x80;

/// Errors from parsing a .WGT team file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WgtError {
    /// File is too short to contain the header.
    TooShortHeader { len: usize },
    /// Magic bytes don't match "WGT\0".
    BadMagic([u8; 4]),
    /// Ran out of bytes partway through team `index`.
    Truncated {
        team_index: usize,
        need: usize,
        have: usize,
    },
}

impl core::fmt::Display for WgtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WgtError::TooShortHeader { len } => write!(
                f,
                "file too short ({len} bytes, need at least {WGT_HEADER_SIZE})"
            ),
            WgtError::BadMagic(m) => write!(
                f,
                "bad magic: {:02X} {:02X} {:02X} {:02X} (expected WGT\\0)",
                m[0], m[1], m[2], m[3]
            ),
            WgtError::Truncated {
                team_index,
                need,
                have,
            } => write!(
                f,
                "truncated at team {team_index}: need {need} more bytes, have {have}"
            ),
        }
    }
}

/// Header-level cheat flags + roster metadata (offset 0x04..0x0A).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WgtHeader {
    /// Unknown header byte at offset 0x04 — possibly a version field. The
    /// retail WG.WGT writes `0x04` here.
    pub unknown_04: u8,
    /// Utility upgrade cheats bitmask (offset 0x06):
    /// 1=Laser Sight, 2=Fast Walk, 4=Invisibility, 8=Low Gravity, 16=Jetpack.
    pub utility_cheats: u8,
    /// Weapon upgrade cheats bitmask (offset 0x07):
    /// 1=Grenade, 2=Shotgun, 4=Banana Bomb, 8=Longbow, 16=Aqua Sheep.
    pub weapon_cheats: u8,
    /// Game cheats bitmask (offset 0x08):
    /// 1=God Worms, 2=Blood, 4=Sheep Heaven.
    pub game_cheats: u8,
    /// Combined indestructible-terrain / Full-Wormage scheme flag (offset 0x09).
    pub indestructible_full_wormage: u8,
    /// Unknown byte at offset 0x0A.
    pub unknown_0a: u8,
}

/// Custom grave block (24×32 bitmap with embedded palette).
///
/// Present in a team record only when `grave_id >= 0x80`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomGrave {
    /// Filename the grave was imported from (informational, 32-byte
    /// null-terminated ASCII).
    pub filename: [u8; FILENAME_LEN],
    /// 256-colour palette, BGR0 byte order (1024 bytes).
    pub palette: Vec<u8>,
    /// 24×32 8bpp bitmap data (row-major, top-to-bottom).
    pub bitmap: Vec<u8>,
}

/// Per-team flag bitmap (20×17 8bpp + embedded palette).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamFlag {
    /// Filename the flag was imported from (32-byte null-terminated ASCII).
    pub filename: [u8; FILENAME_LEN],
    /// 256-colour palette, BGR0 byte order (1024 bytes).
    pub palette: Vec<u8>,
    /// 20×17 8bpp bitmap data (row-major, top-to-bottom).
    pub bitmap: Vec<u8>,
}

/// One team's record within a `WgtFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgtTeam {
    /// Team display name (17-byte null-terminated ASCII).
    pub name: [u8; TEAM_NAME_LEN],
    /// 8 worm names, each 17-byte null-terminated ASCII.
    pub worm_names: [[u8; WORM_NAME_LEN]; WORMS_PER_TEAM],
    /// Team controller: 0 = local player, 1..=5 = CPU level 1..5.
    pub control: u8,
    /// Soundbank name (32-byte null-terminated ASCII). Maps to
    /// `User\Speech\<name>\` for custom banks, or the built-in tables
    /// for stock entries.
    pub soundbank_name: [u8; FILENAME_LEN],
    /// Soundbank location indicator (0 or 1; exact semantics unconfirmed).
    pub soundbank_location: u8,
    /// Fanfare name (32-byte null-terminated ASCII), used when
    /// `use_custom_fanfare` is set.
    pub fanfare_name: [u8; FILENAME_LEN],
    /// 0 = use the player's country fanfare, 1 = use `fanfare_name`.
    pub use_custom_fanfare: u8,
    /// Grave sprite index (0x00..=0xFE) or 0xFF for the embedded custom
    /// bitmap stored in `custom_grave`. Values ≥ 0x80 trigger the
    /// `custom_grave` block in the file even though the visible behaviour
    /// (sprite vs. bitmap) is gated on 0xFF.
    pub grave_id: u8,
    /// Inline custom-grave bitmap, present iff `grave_id >= 0x80`.
    pub custom_grave: Option<CustomGrave>,
    /// Default team weapon byte. Indexes into `SCHEME_WEAPON_ORDER`.
    pub team_weapon: u8,
    /// Cumulative team stats (10 dwords + 33×2 mission dwords). Stored
    /// opaque — we do not currently surface fields. `STATS_LEN` bytes.
    pub stats: Vec<u8>,
    /// Team flag bitmap (always present).
    pub flag: TeamFlag,
    /// Deathmatch rank.
    pub dm_rank: i8,
    /// Training mission times in seconds (6 dwords).
    pub training_times: [u32; 6],
    /// Unknown trailing region #1 (40 bytes).
    pub unknown_40: Vec<u8>,
    /// Training mission medals (6 bytes).
    pub training_medals: [u8; 6],
    /// Unknown trailing region #2 (10 bytes).
    pub unknown_10: Vec<u8>,
    /// Unknown trailing region #3 (7 dwords, seemingly random).
    pub unknown_7dwords: [u32; 7],
    /// Final unknown byte.
    pub unknown_tail: u8,
}

/// Parsed `.WGT` file: header + team roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgtFile {
    pub header: WgtHeader,
    pub teams: Vec<WgtTeam>,
}

impl WgtFile {
    /// Parse a .WGT file from raw bytes (entire file contents).
    pub fn from_bytes(data: &[u8]) -> Result<Self, WgtError> {
        if data.len() < WGT_HEADER_SIZE {
            return Err(WgtError::TooShortHeader { len: data.len() });
        }
        let mut c = Cursor::new(data);
        let magic = c.read_array::<4>(0)?;
        if magic != WGT_MAGIC {
            return Err(WgtError::BadMagic(magic));
        }
        let unknown_04 = c.read_u8(0)?;
        let team_count = c.read_u8(0)? as usize;
        let utility_cheats = c.read_u8(0)?;
        let weapon_cheats = c.read_u8(0)?;
        let game_cheats = c.read_u8(0)?;
        let indestructible_full_wormage = c.read_u8(0)?;
        let unknown_0a = c.read_u8(0)?;

        let header = WgtHeader {
            unknown_04,
            utility_cheats,
            weapon_cheats,
            game_cheats,
            indestructible_full_wormage,
            unknown_0a,
        };

        let mut teams = Vec::with_capacity(team_count);
        for idx in 0..team_count {
            teams.push(WgtTeam::read(&mut c, idx)?);
        }

        Ok(WgtFile { header, teams })
    }

    /// Load a .WGT file from disk.
    pub fn from_file(path: &std::path::Path) -> Result<Self, WgtFileError> {
        let data = std::fs::read(path).map_err(WgtFileError::Io)?;
        Self::from_bytes(&data).map_err(WgtFileError::Parse)
    }
}

impl WgtTeam {
    fn read(c: &mut Cursor<'_>, idx: usize) -> Result<Self, WgtError> {
        let name = c.read_array::<TEAM_NAME_LEN>(idx)?;
        let mut worm_names = [[0u8; WORM_NAME_LEN]; WORMS_PER_TEAM];
        for slot in worm_names.iter_mut() {
            *slot = c.read_array::<WORM_NAME_LEN>(idx)?;
        }
        let control = c.read_u8(idx)?;
        let soundbank_name = c.read_array::<FILENAME_LEN>(idx)?;
        let soundbank_location = c.read_u8(idx)?;
        let fanfare_name = c.read_array::<FILENAME_LEN>(idx)?;
        let use_custom_fanfare = c.read_u8(idx)?;
        let grave_id = c.read_u8(idx)?;

        let custom_grave = if grave_id >= CUSTOM_GRAVE_THRESHOLD {
            let filename = c.read_array::<FILENAME_LEN>(idx)?;
            let palette = c.read_vec(PALETTE_LEN, idx)?;
            let bitmap = c.read_vec(GRAVE_BMP_LEN, idx)?;
            Some(CustomGrave {
                filename,
                palette,
                bitmap,
            })
        } else {
            None
        };

        let team_weapon = c.read_u8(idx)?;
        let stats = c.read_vec(STATS_LEN, idx)?;

        let flag_filename = c.read_array::<FILENAME_LEN>(idx)?;
        let flag_palette = c.read_vec(PALETTE_LEN, idx)?;
        let flag_bitmap = c.read_vec(FLAG_BMP_LEN, idx)?;
        let flag = TeamFlag {
            filename: flag_filename,
            palette: flag_palette,
            bitmap: flag_bitmap,
        };

        let dm_rank = c.read_u8(idx)? as i8;
        let mut training_times = [0u32; 6];
        for slot in training_times.iter_mut() {
            *slot = c.read_u32_le(idx)?;
        }
        let unknown_40 = c.read_vec(40, idx)?;
        let training_medals = c.read_array::<6>(idx)?;
        let unknown_10 = c.read_vec(10, idx)?;
        let mut unknown_7dwords = [0u32; 7];
        for slot in unknown_7dwords.iter_mut() {
            *slot = c.read_u32_le(idx)?;
        }
        let unknown_tail = c.read_u8(idx)?;

        Ok(WgtTeam {
            name,
            worm_names,
            control,
            soundbank_name,
            soundbank_location,
            fanfare_name,
            use_custom_fanfare,
            grave_id,
            custom_grave,
            team_weapon,
            stats,
            flag,
            dm_rank,
            training_times,
            unknown_40,
            training_medals,
            unknown_10,
            unknown_7dwords,
            unknown_tail,
        })
    }

    /// Team name decoded to UTF-8 from the on-disk Windows-1252 bytes,
    /// truncated at the first NUL.
    pub fn name_str(&self) -> String {
        crate::cp1252::decode_cstr(&self.name)
    }

    /// Soundbank name (CP1252 → UTF-8, NUL-truncated).
    pub fn soundbank_str(&self) -> String {
        crate::cp1252::decode_cstr(&self.soundbank_name)
    }

    /// Fanfare name (CP1252 → UTF-8, NUL-truncated).
    pub fn fanfare_str(&self) -> String {
        crate::cp1252::decode_cstr(&self.fanfare_name)
    }

    /// Flag filename (CP1252 → UTF-8, NUL-truncated).
    pub fn flag_filename_str(&self) -> String {
        crate::cp1252::decode_cstr(&self.flag.filename)
    }

    /// Iterator over all 8 worm-name slots, each decoded CP1252 → UTF-8
    /// and NUL-truncated. Slots whose first byte is NUL come through as
    /// empty strings — callers that care about absent slots should
    /// inspect emptiness explicitly rather than filtering, since worm
    /// indices are positional in WA's data structures.
    pub fn worm_names_iter(&self) -> impl Iterator<Item = String> + '_ {
        self.worm_names
            .iter()
            .map(|n| crate::cp1252::decode_cstr(n))
    }
}

// ─── Cursor helper ─────────────────────────────────────────────────────────

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn need(&self, n: usize, team_index: usize) -> Result<(), WgtError> {
        if self.pos + n > self.data.len() {
            if self.pos < WGT_HEADER_SIZE {
                return Err(WgtError::TooShortHeader {
                    len: self.data.len(),
                });
            }
            return Err(WgtError::Truncated {
                team_index,
                need: n,
                have: self.data.len().saturating_sub(self.pos),
            });
        }
        Ok(())
    }

    fn read_u8(&mut self, team_index: usize) -> Result<u8, WgtError> {
        self.need(1, team_index)?;
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u32_le(&mut self, team_index: usize) -> Result<u32, WgtError> {
        self.need(4, team_index)?;
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn read_array<const N: usize>(&mut self, team_index: usize) -> Result<[u8; N], WgtError> {
        self.need(N, team_index)?;
        let mut out = [0u8; N];
        out.copy_from_slice(&self.data[self.pos..self.pos + N]);
        self.pos += N;
        Ok(out)
    }

    fn read_vec(&mut self, n: usize, team_index: usize) -> Result<Vec<u8>, WgtError> {
        self.need(n, team_index)?;
        let v = self.data[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(v)
    }
}

// ─── File-based error wrapper ──────────────────────────────────────────────

/// Error type for file-based WGT operations.
#[derive(Debug)]
pub enum WgtFileError {
    Io(std::io::Error),
    Parse(WgtError),
}

impl core::fmt::Display for WgtFileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WgtFileError::Io(e) => write!(f, "I/O error: {e}"),
            WgtFileError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_header(team_count: u8) -> [u8; WGT_HEADER_SIZE] {
        let mut h = [0u8; WGT_HEADER_SIZE];
        h[0..4].copy_from_slice(&WGT_MAGIC);
        h[4] = 0x04;
        h[5] = team_count;
        h
    }

    fn synth_team_record_no_grave() -> Vec<u8> {
        let mut buf = vec![0u8; TEAM_RECORD_LEN_NO_GRAVE];
        // Stamp a recognisable team name + first worm name.
        buf[0..5].copy_from_slice(b"Test\0");
        buf[TEAM_NAME_LEN..TEAM_NAME_LEN + 5].copy_from_slice(b"Worm\0");
        // Control = CPU 2
        buf[TEAM_NAME_LEN + WORM_NAME_LEN * WORMS_PER_TEAM] = 2;
        buf
    }

    #[test]
    fn header_sizes() {
        assert_eq!(WGT_HEADER_SIZE, 11);
        assert_eq!(TEAM_RECORD_LEN_NO_GRAVE, 2032);
        assert_eq!(TEAM_RECORD_LEN_WITH_GRAVE, 2032 + 1824);
    }

    #[test]
    fn parse_zero_team_file() {
        let data = synth_header(0);
        let wgt = WgtFile::from_bytes(&data).unwrap();
        assert_eq!(wgt.teams.len(), 0);
        assert_eq!(wgt.header.unknown_04, 0x04);
    }

    #[test]
    fn parse_single_team_no_grave() {
        let mut data = synth_header(1).to_vec();
        data.extend_from_slice(&synth_team_record_no_grave());
        let wgt = WgtFile::from_bytes(&data).unwrap();
        assert_eq!(wgt.teams.len(), 1);
        let t = &wgt.teams[0];
        assert_eq!(t.name_str(), "Test");
        assert_eq!(t.worm_names_iter().next().unwrap(), "Worm");
        assert_eq!(t.control, 2);
        assert!(t.custom_grave.is_none());
    }

    #[test]
    fn parse_team_with_custom_grave() {
        let mut data = synth_header(1).to_vec();
        let mut team = vec![0u8; TEAM_RECORD_LEN_WITH_GRAVE];
        team[0..3].copy_from_slice(b"CG\0");
        // grave_id = 0xFF triggers the custom grave block. Offset within
        // the team record: name + 8 worms + control + soundbank +
        // soundbank_loc + fanfare + use_custom_fanfare = 219.
        let grave_id_off = TEAM_NAME_LEN
            + WORM_NAME_LEN * WORMS_PER_TEAM
            + 1
            + FILENAME_LEN
            + 1
            + FILENAME_LEN
            + 1;
        team[grave_id_off] = 0xFF;
        // Stamp a palette + bitmap signature inside the custom-grave block.
        let custom_grave_off = grave_id_off + 1;
        team[custom_grave_off..custom_grave_off + 3].copy_from_slice(b"gv\0");
        data.extend_from_slice(&team);
        let wgt = WgtFile::from_bytes(&data).unwrap();
        let t = &wgt.teams[0];
        assert_eq!(t.name_str(), "CG");
        assert_eq!(t.grave_id, 0xFF);
        let cg = t.custom_grave.as_ref().expect("custom grave present");
        assert_eq!(&cg.filename[..3], b"gv\0");
        assert_eq!(cg.palette.len(), PALETTE_LEN);
        assert_eq!(cg.bitmap.len(), GRAVE_BMP_LEN);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut data = synth_header(0);
        data[0] = b'X';
        assert!(matches!(
            WgtFile::from_bytes(&data),
            Err(WgtError::BadMagic(_))
        ));
    }

    #[test]
    fn truncated_at_team_reports_index() {
        let mut data = synth_header(2).to_vec();
        // Only one full team record follows.
        data.extend_from_slice(&synth_team_record_no_grave());
        let err = WgtFile::from_bytes(&data).unwrap_err();
        assert!(matches!(err, WgtError::Truncated { team_index: 1, .. }));
    }

    #[test]
    fn short_header_rejected() {
        let data = [0u8; 4];
        assert!(matches!(
            WgtFile::from_bytes(&data),
            Err(WgtError::TooShortHeader { len: 4 })
        ));
    }
}
