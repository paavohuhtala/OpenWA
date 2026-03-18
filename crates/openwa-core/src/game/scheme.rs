/// Scheme file (.wsc) parser for Worms Armageddon.
///
/// .wsc files store game settings (turn time, wind, per-weapon ammo/delay, etc.).
/// Located in `User\Schemes\*.wsc`.
///
/// Binary format:
///   Bytes 0-3: Magic "SCHM" (0x5343484D)
///   Byte 4:    Version byte
///   Bytes 5+:  Payload (size depends on version)
///
/// Version → payload size:
///   0x01 → 0xD8 bytes (216): 36 options + 45 weapons × 4
///   0x02 → 0x124 bytes (292): V1 + 19 super weapons × 4
///   0x03 → 0x192 bytes (402): V2 + 110 bytes extended options
///
/// Source: Ghidra decompilation of Scheme__ReadFile (0x4D3890),
///         Scheme__SaveFile (0x4D44F0), worms2d.info/Game_scheme_file

/// Magic header bytes for .wsc files.
pub const SCHEME_MAGIC: [u8; 4] = *b"SCHM";

/// Size of the file header (magic + version byte).
pub const SCHEME_HEADER_SIZE: usize = 5;

/// Payload size for version 1 schemes.
pub const SCHEME_PAYLOAD_V1: usize = 0xD8;

/// Payload size for version 2 schemes.
pub const SCHEME_PAYLOAD_V2: usize = 0x124;

/// Payload size for version 3 schemes.
pub const SCHEME_PAYLOAD_V3: usize = 0x192;

/// Scheme file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemeVersion {
    /// Version 1: 0xD8 byte payload (total file: 221 bytes)
    V1,
    /// Version 2: 0x124 byte payload (total file: 297 bytes)
    V2,
    /// Version 3: 0x192 byte payload (total file: 407 bytes)
    /// V2 + 110 bytes extended options.
    V3,
}

impl SchemeVersion {
    /// Raw version byte as stored in the file.
    pub fn to_byte(self) -> u8 {
        match self {
            SchemeVersion::V1 => 1,
            SchemeVersion::V2 => 2,
            SchemeVersion::V3 => 3,
        }
    }

    /// Parse version from the raw byte. Returns `None` for unknown versions.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(SchemeVersion::V1),
            2 => Some(SchemeVersion::V2),
            3 => Some(SchemeVersion::V3),
            _ => None,
        }
    }

    /// Expected payload size for this version.
    pub fn payload_size(self) -> usize {
        match self {
            SchemeVersion::V1 => SCHEME_PAYLOAD_V1,
            SchemeVersion::V2 => SCHEME_PAYLOAD_V2,
            SchemeVersion::V3 => SCHEME_PAYLOAD_V3,
        }
    }
}

/// Errors from parsing a .wsc scheme file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemeError {
    /// File is too short to contain the header.
    TooShort { len: usize },
    /// Magic bytes don't match "SCHM".
    BadMagic([u8; 4]),
    /// Unknown version byte (not 1, 2, or 3).
    UnknownVersion(u8),
    /// Payload size doesn't match what the version byte expects.
    PayloadMismatch { expected: usize, got: usize },
}

impl core::fmt::Display for SchemeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SchemeError::TooShort { len } => {
                write!(
                    f,
                    "file too short ({len} bytes, need at least {SCHEME_HEADER_SIZE})"
                )
            }
            SchemeError::BadMagic(m) => {
                write!(
                    f,
                    "bad magic: {:02X} {:02X} {:02X} {:02X} (expected SCHM)",
                    m[0], m[1], m[2], m[3]
                )
            }
            SchemeError::UnknownVersion(v) => {
                write!(f, "unknown version byte: 0x{v:02X}")
            }
            SchemeError::PayloadMismatch { expected, got } => {
                write!(f, "payload size mismatch: expected {expected}, got {got}")
            }
        }
    }
}

/// A parsed .wsc scheme file.
///
/// The payload bytes are initially opaque. Typed field accessors will be
/// added incrementally as we map individual byte offsets through RE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeFile {
    pub version: SchemeVersion,
    /// Raw payload bytes (game options + per-weapon settings).
    /// Length matches `version.payload_size()`.
    pub payload: Vec<u8>,
}

impl SchemeFile {
    /// Parse a scheme file from raw bytes (entire file contents).
    pub fn from_bytes(data: &[u8]) -> Result<Self, SchemeError> {
        if data.len() < SCHEME_HEADER_SIZE {
            return Err(SchemeError::TooShort { len: data.len() });
        }

        let magic: [u8; 4] = data[0..4].try_into().unwrap();
        if magic != SCHEME_MAGIC {
            return Err(SchemeError::BadMagic(magic));
        }

        let version =
            SchemeVersion::from_byte(data[4]).ok_or(SchemeError::UnknownVersion(data[4]))?;
        let expected_payload = version.payload_size();
        let actual_payload = data.len() - SCHEME_HEADER_SIZE;

        // V3 schemes are saved with variable length: only extended options bytes
        // that differ from defaults are included. The original game pre-fills the
        // extended region with SCHEME_V3_DEFAULTS then reads the file on top, so
        // short V3 files get defaults for the missing tail. We replicate this by
        // accepting any size between V2 (0x124) and V3 (0x192) and padding.
        if version == SchemeVersion::V3 {
            if actual_payload < SCHEME_PAYLOAD_V2 || actual_payload > SCHEME_PAYLOAD_V3 {
                return Err(SchemeError::PayloadMismatch {
                    expected: expected_payload,
                    got: actual_payload,
                });
            }
            let file_payload = &data[SCHEME_HEADER_SIZE..];
            let mut payload = vec![0u8; SCHEME_PAYLOAD_V3];
            // Copy file data (V2 portion + whatever extended bytes are present)
            payload[..actual_payload].copy_from_slice(file_payload);
            // Fill remaining extended options with defaults
            if actual_payload < SCHEME_PAYLOAD_V3 {
                let defaults_start = actual_payload.saturating_sub(SCHEME_PAYLOAD_V2);
                payload[actual_payload..SCHEME_PAYLOAD_V3]
                    .copy_from_slice(&EXTENDED_OPTIONS_DEFAULTS[defaults_start..]);
            }
            return Ok(SchemeFile { version, payload });
        }

        if actual_payload != expected_payload {
            return Err(SchemeError::PayloadMismatch {
                expected: expected_payload,
                got: actual_payload,
            });
        }

        Ok(SchemeFile {
            version,
            payload: data[SCHEME_HEADER_SIZE..].to_vec(),
        })
    }

    /// Serialize back to .wsc format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(SCHEME_HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&SCHEME_MAGIC);
        buf.push(self.version.to_byte());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Total file size (header + payload).
    pub fn file_size(&self) -> usize {
        SCHEME_HEADER_SIZE + self.payload.len()
    }
}

impl SchemeFile {
    /// Load a scheme file from disk.
    pub fn from_file(path: &std::path::Path) -> Result<Self, SchemeFileError> {
        let data = std::fs::read(path).map_err(SchemeFileError::Io)?;
        Self::from_bytes(&data).map_err(SchemeFileError::Parse)
    }

    /// Save a scheme file to disk.
    pub fn to_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        std::fs::write(path, self.to_bytes())
    }
}

// === Payload layout constants ===

/// Byte offset within the payload where game options start.
pub const OPTIONS_OFFSET: usize = 0;
/// Size of the game options section in the payload.
pub const OPTIONS_SIZE: usize = 36;
/// Byte offset within the payload where V1 weapon settings start.
pub const WEAPONS_V1_OFFSET: usize = OPTIONS_SIZE; // 36
/// Number of weapons in V1 schemes.
pub const WEAPONS_V1_COUNT: usize = 45;
/// Number of super weapons added in V2.
pub const WEAPONS_V2_COUNT: usize = 19;
/// Total weapons in V2+ schemes.
pub const WEAPONS_TOTAL_COUNT: usize = WEAPONS_V1_COUNT + WEAPONS_V2_COUNT; // 64
/// Bytes per weapon entry.
pub const WEAPON_ENTRY_SIZE: usize = 4;
/// Byte offset where V2 super weapons start.
pub const WEAPONS_V2_OFFSET: usize = WEAPONS_V1_OFFSET + WEAPONS_V1_COUNT * WEAPON_ENTRY_SIZE; // 216 = 0xD8
/// Byte offset where V3 extended options start.
pub const EXTENDED_OPTIONS_OFFSET: usize = WEAPONS_V2_OFFSET + WEAPONS_V2_COUNT * WEAPON_ENTRY_SIZE; // 292 = 0x124
/// Size of the V3 extended options section.
pub const EXTENDED_OPTIONS_SIZE: usize = 110; // 0x6E

/// Per-weapon settings (4 bytes each in the .wsc file).
///
/// Weapon order in the file differs from the runtime `Weapon` enum —
/// the .wsc uses the "scheme order" defined by the original game UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponSettings {
    /// Ammo count. 0 = none, 1-10 = count, 10/0x80+ = infinite.
    pub ammo: u8,
    /// Power level. 0-20 (max varies by weapon).
    pub power: u8,
    /// Turn delay before weapon becomes available. 0 = immediate.
    pub delay: u8,
    /// Crate probability. 0-100 percentage.
    pub crate_probability: u8,
}

impl WeaponSettings {
    pub fn from_bytes(b: &[u8]) -> Self {
        Self {
            ammo: b[0],
            power: b[1],
            delay: b[2],
            crate_probability: b[3],
        }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        [self.ammo, self.power, self.delay, self.crate_probability]
    }
}

/// Game options (first 36 bytes of payload).
///
/// These are the core game settings displayed in the scheme editor.
/// All offsets are relative to the start of the payload (file offset 0x05).
///
/// Source: worms2d.info/Game_scheme_file, Ghidra analysis of Scheme__ReadFile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemeOptions {
    /// Seconds between turns (hot-seat delay). Payload +0x00, file 0x05.
    pub hot_seat_delay: u8,
    /// Seconds to retreat after weapon use (grounded). Payload +0x01, file 0x06.
    pub retreat_time: u8,
    /// Seconds to retreat after weapon use (on rope). Payload +0x02, file 0x07.
    pub rope_retreat_time: u8,
    /// Display total round time on screen. Payload +0x03, file 0x08.
    pub display_total_round_time: bool,
    /// Enable automatic replays. Payload +0x04, file 0x09.
    pub automatic_replays: bool,
    /// Fall damage amount at critical velocity. Payload +0x05, file 0x0A.
    pub fall_damage: u8,
    /// Artillery mode (worms can't move). Payload +0x06, file 0x0B.
    pub artillery_mode: bool,
    /// Bounty mode marker. 0x00=unset, 0x5F/0x89=editor magic. Payload +0x07, file 0x0C.
    pub bounty_mode: u8,
    /// Stockpiling mode: 0=Off, 1=On, 2=Anti. Payload +0x08, file 0x0D.
    pub stockpiling: u8,
    /// Worm selection: 0=Sequential, 1=Manual, 2=Random. Payload +0x09, file 0x0E.
    pub worm_select: u8,
    /// Sudden death event: 0=RoundEnds, 1=NuclearStrike, 2=HP→1, 3=Nothing. Payload +0x0A, file 0x0F.
    pub sudden_death_event: u8,
    /// Water rise rate during sudden death. Payload +0x0B, file 0x10.
    pub water_rise_rate: u8,
    /// Weapon crate probability (-100 to 100, signed). Payload +0x0C, file 0x11.
    pub weapon_crate_probability: i8,
    /// Donor cards enabled. Payload +0x0D, file 0x12.
    pub donor_cards: bool,
    /// Health crate probability (-100 to 100, signed). Payload +0x0E, file 0x13.
    pub health_crate_probability: i8,
    /// Energy gained from health crates. Payload +0x0F, file 0x14.
    pub health_crate_energy: u8,
    /// Utility crate probability (-100 to 100, signed). Payload +0x10, file 0x15.
    pub utility_crate_probability: i8,
    /// Hazardous object type bitmask. Payload +0x11, file 0x16.
    pub hazardous_object_types: u8,
    /// Mine fuse delay in seconds. 0x80+ = random. Payload +0x12, file 0x17.
    pub mine_delay: i8,
    /// Dud mines (some mines don't explode). Payload +0x13, file 0x18.
    pub dud_mines: bool,
    /// Manual worm placement at start. Payload +0x14, file 0x19.
    pub manual_worm_placement: bool,
    /// Initial worm energy. 0=instant death. Payload +0x15, file 0x1A.
    pub worm_energy: u8,
    /// Turn time in seconds. Encoding varies by range. Payload +0x16, file 0x1B.
    pub turn_time: u8,
    /// Round time in minutes. 0=immediate SD. Payload +0x17, file 0x1C.
    pub round_time: u8,
    /// Number of round wins required to win match. Payload +0x18, file 0x1D.
    pub number_of_wins: u8,
    /// Blood color: false=pink, true=red. Payload +0x19, file 0x1E.
    pub blood: bool,
    /// Aqua Sheep mode. Payload +0x1A, file 0x1F.
    pub aqua_sheep: bool,
    /// Sheep Heaven mode. Payload +0x1B, file 0x20.
    pub sheep_heaven: bool,
    /// God Worms (infinite health). Payload +0x1C, file 0x21.
    pub god_worms: bool,
    /// Indestructible terrain. Payload +0x1D, file 0x22.
    pub indestructible_land: bool,
    /// Upgraded grenades. Payload +0x1E, file 0x23.
    pub upgraded_grenade: bool,
    /// Upgraded shotgun. Payload +0x1F, file 0x24.
    pub upgraded_shotgun: bool,
    /// Upgraded cluster bombs. Payload +0x20, file 0x25.
    pub upgraded_clusters: bool,
    /// Upgraded longbow. Payload +0x21, file 0x26.
    pub upgraded_longbow: bool,
    /// Team weapons enabled. Payload +0x22, file 0x27.
    pub team_weapons: bool,
    /// Super weapons enabled. Payload +0x23, file 0x28.
    pub super_weapons: bool,
}

impl SchemeOptions {
    /// Read options from the first 36 bytes of a payload slice.
    pub fn from_bytes(b: &[u8]) -> Self {
        Self {
            hot_seat_delay: b[0],
            retreat_time: b[1],
            rope_retreat_time: b[2],
            display_total_round_time: b[3] != 0,
            automatic_replays: b[4] != 0,
            fall_damage: b[5],
            artillery_mode: b[6] != 0,
            bounty_mode: b[7],
            stockpiling: b[8],
            worm_select: b[9],
            sudden_death_event: b[10],
            water_rise_rate: b[11],
            weapon_crate_probability: b[12] as i8,
            donor_cards: b[13] != 0,
            health_crate_probability: b[14] as i8,
            health_crate_energy: b[15],
            utility_crate_probability: b[16] as i8,
            hazardous_object_types: b[17],
            mine_delay: b[18] as i8,
            dud_mines: b[19] != 0,
            manual_worm_placement: b[20] != 0,
            worm_energy: b[21],
            turn_time: b[22],
            round_time: b[23],
            number_of_wins: b[24],
            blood: b[25] != 0,
            aqua_sheep: b[26] != 0,
            sheep_heaven: b[27] != 0,
            god_worms: b[28] != 0,
            indestructible_land: b[29] != 0,
            upgraded_grenade: b[30] != 0,
            upgraded_shotgun: b[31] != 0,
            upgraded_clusters: b[32] != 0,
            upgraded_longbow: b[33] != 0,
            team_weapons: b[34] != 0,
            super_weapons: b[35] != 0,
        }
    }

    /// Serialize options back to 36 bytes.
    pub fn to_bytes(&self) -> [u8; OPTIONS_SIZE] {
        [
            self.hot_seat_delay,
            self.retreat_time,
            self.rope_retreat_time,
            self.display_total_round_time as u8,
            self.automatic_replays as u8,
            self.fall_damage,
            self.artillery_mode as u8,
            self.bounty_mode,
            self.stockpiling,
            self.worm_select,
            self.sudden_death_event,
            self.water_rise_rate,
            self.weapon_crate_probability as u8,
            self.donor_cards as u8,
            self.health_crate_probability as u8,
            self.health_crate_energy,
            self.utility_crate_probability as u8,
            self.hazardous_object_types,
            self.mine_delay as u8,
            self.dud_mines as u8,
            self.manual_worm_placement as u8,
            self.worm_energy,
            self.turn_time,
            self.round_time,
            self.number_of_wins,
            self.blood as u8,
            self.aqua_sheep as u8,
            self.sheep_heaven as u8,
            self.god_worms as u8,
            self.indestructible_land as u8,
            self.upgraded_grenade as u8,
            self.upgraded_shotgun as u8,
            self.upgraded_clusters as u8,
            self.upgraded_longbow as u8,
            self.team_weapons as u8,
            self.super_weapons as u8,
        ]
    }
}

/// Weapon order in .wsc scheme files (indices 0-63).
///
/// This order differs from the runtime `Weapon` enum. The scheme file uses
/// the UI panel order established by the original game.
///
/// Source: worms2d.info/Game_scheme_file
pub const SCHEME_WEAPON_ORDER: [&str; 64] = [
    // V1 weapons (0-44)
    "Bazooka",           // 0
    "Homing Missile",    // 1
    "Mortar",            // 2
    "Grenade",           // 3
    "Cluster Bomb",      // 4
    "Skunk",             // 5
    "Petrol Bomb",       // 6
    "Banana Bomb",       // 7
    "Handgun",           // 8
    "Shotgun",           // 9
    "Uzi",               // 10
    "Minigun",           // 11
    "Longbow",           // 12
    "Airstrike",         // 13
    "Napalm Strike",     // 14
    "Mine",              // 15
    "Fire Punch",        // 16
    "Dragon Ball",       // 17
    "Kamikaze",          // 18
    "Prod",              // 19
    "Battle Axe",        // 20
    "Blowtorch",         // 21
    "Pneumatic Drill",   // 22
    "Girder",            // 23
    "Ninja Rope",        // 24
    "Parachute",         // 25
    "Bungee",            // 26
    "Teleport",          // 27
    "Dynamite",          // 28
    "Sheep",             // 29
    "Baseball Bat",      // 30
    "Flame Thrower",     // 31
    "Homing Pigeon",     // 32
    "Mad Cow",           // 33
    "Holy Hand Grenade", // 34
    "Old Woman",         // 35
    "Sheep Launcher",    // 36
    "Super Sheep",       // 37
    "Mole Bomb",         // 38
    "Jet Pack",          // 39
    "Low Gravity",       // 40
    "Laser Sight",       // 41
    "Fast Walk",         // 42
    "Invisibility",      // 43
    "Damage x2",         // 44
    // V2 super weapons (45-63)
    "Freeze",               // 45
    "Super Banana Bomb",    // 46
    "Mine Strike",          // 47
    "Girder Starter Pack",  // 48
    "Earthquake",           // 49
    "Scales of Justice",    // 50
    "Ming Vase",            // 51
    "Mike's Carpet Bomb",   // 52
    "Patsy's Magic Bullet", // 53
    "Indian Nuclear Test",  // 54
    "Select Worm",          // 55
    "Salvation Army",       // 56
    "Mole Squadron",        // 57
    "MB Bomb",              // 58
    "Concrete Donkey",      // 59
    "Suicide Bomber",       // 60
    "Sheep Strike",         // 61
    "Mail Strike",          // 62
    "Armageddon",           // 63
];

/// V3 extended options (110 bytes, payload offset 0x124).
///
/// Present in V3 schemes; for V1/V2, the game fills this region from
/// ROM defaults at 0x649AB8.
///
/// Many fields use fixed-point 16.16 format (`Fixed`), tri-state (0/1/0x80),
/// or small enums. See field docs for valid ranges.
///
/// Source: worms2d.info/Game_scheme_file, Ghidra FUN_004d5110 validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtendedOptions {
    /// Data version (currently 0). Offset +0x00.
    pub data_version: u32,
    /// Constant wind enabled. Offset +0x04.
    pub constant_wind: bool,
    /// Wind strength (signed). Offset +0x05.
    pub wind: i16,
    /// Wind bias. Offset +0x07.
    pub wind_bias: u8,
    /// Gravity (fixed 16.16). Offset +0x08. Default 0xF5C2.
    pub gravity: i32,
    /// Terrain friction (fixed 16.16). Offset +0x0C. Default 0xF5C2.
    pub terrain_friction: i32,
    /// Rope knocking. 0xFF = use default. Offset +0x10.
    pub rope_knocking: u8,
    /// Blood level. 0xFF = use default. Offset +0x11.
    pub blood_level: u8,
    /// Unrestrict rope. Offset +0x12.
    pub unrestrict_rope: bool,
    /// Auto-place worms by ally. Offset +0x13.
    pub auto_place_worms_by_ally: bool,
    /// No-crate probability. 0xFF = ignore. Offset +0x14.
    pub no_crate_probability: u8,
    /// Maximum crate count. Offset +0x15.
    pub max_crate_count: u16,
    /// Sudden death disables worm select. Offset +0x17.
    pub sd_disables_worm_select: bool,
    /// Sudden death worm damage per turn. Offset +0x18.
    pub sd_worm_damage_per_turn: u8,
    /// Phased worms (allied). 0-3. Offset +0x19.
    pub phased_worms_allied: u8,
    /// Phased worms (enemy). 0-3. Offset +0x1A.
    pub phased_worms_enemy: u8,
    /// Circular aim. Offset +0x1B.
    pub circular_aim: bool,
    /// Anti-lock aim. Offset +0x1C.
    pub anti_lock_aim: bool,
    /// Anti-lock power. Offset +0x1D.
    pub anti_lock_power: bool,
    /// Worm selection doesn't end hot seat. Offset +0x1E.
    pub worm_select_no_end_hot_seat: bool,
    /// Worm selection is never cancelled. Offset +0x1F.
    pub worm_select_never_cancelled: bool,
    /// Batty rope. Offset +0x20.
    pub batty_rope: bool,
    /// Rope-roll drops. 0-2. Offset +0x21.
    pub rope_roll_drops: u8,
    /// X-impact loss of control. 0 or 0xFF. Offset +0x22.
    pub x_impact_loss_of_control: u8,
    /// Keep control after bumping head. Offset +0x23.
    pub keep_control_bump_head: bool,
    /// Keep control after skimming. 0-2. Offset +0x24.
    pub keep_control_skimming: u8,
    /// Fall damage triggered by explosions. Offset +0x25.
    pub explosion_fall_damage: bool,
    /// Explosions push all objects. Tri-state: 0/1/0x80. Offset +0x26.
    pub explosions_push_all: u8,
    /// Undetermined crates. Tri-state. Offset +0x27.
    pub undetermined_crates: u8,
    /// Undetermined fuses. Tri-state. Offset +0x28.
    pub undetermined_fuses: u8,
    /// Pause timer while firing. Offset +0x29.
    pub pause_timer_while_firing: bool,
    /// Loss of control doesn't end turn. Offset +0x2A.
    pub loss_of_control_no_end_turn: bool,
    /// Weapon use doesn't end turn. Offset +0x2B.
    pub weapon_use_no_end_turn: bool,
    /// Above option doesn't block any weapons. Offset +0x2C.
    pub weapon_use_no_block: bool,
    /// Pneumatic drill imparts velocity. Tri-state. Offset +0x2D.
    pub drill_imparts_velocity: u8,
    /// Girder radius assist. Offset +0x2E.
    pub girder_radius_assist: bool,
    /// Petrol turn decay (fixed 16.16 fractional part). Offset +0x2F.
    pub petrol_turn_decay: u16,
    /// Petrol touch decay. Offset +0x31.
    pub petrol_touch_decay: u8,
    /// Maximum flamelet count. Offset +0x32.
    pub max_flamelet_count: u16,
    /// Maximum projectile speed (fixed 16.16). Offset +0x34.
    pub max_projectile_speed: i32,
    /// Maximum rope speed (fixed 16.16). Offset +0x38.
    pub max_rope_speed: i32,
    /// Maximum jet pack speed (fixed 16.16). Offset +0x3C.
    pub max_jet_pack_speed: i32,
    /// Game engine speed (fixed 16.16). Offset +0x40.
    pub game_engine_speed: i32,
    /// Indian rope glitch. Tri-state. Offset +0x44.
    pub indian_rope_glitch: u8,
    /// Herd-doubling glitch. Tri-state. Offset +0x45.
    pub herd_doubling_glitch: u8,
    /// Jet pack bungee glitch. Offset +0x46.
    pub jet_pack_bungee_glitch: bool,
    /// Angle cheat glitch. Offset +0x47.
    pub angle_cheat_glitch: bool,
    /// Glide glitch. Offset +0x48.
    pub glide_glitch: bool,
    /// Skipwalking. 0/1/0xFF. Offset +0x49.
    pub skipwalking: i8,
    /// Block roofing. 0-2. Offset +0x4A.
    pub block_roofing: u8,
    /// Floating weapon glitch. Offset +0x4B.
    pub floating_weapon_glitch: bool,
    /// RubberWorm bounciness (fixed 16.16). Offset +0x4C.
    pub rw_bounciness: i32,
    /// RubberWorm air viscosity (fixed 16.16). Offset +0x50.
    pub rw_air_viscosity: i32,
    /// RW air viscosity applies to worms. Offset +0x54.
    pub rw_air_viscosity_worms: bool,
    /// RubberWorm wind influence (fixed 16.16). Offset +0x55.
    pub rw_wind_influence: u32,
    /// RW wind influence applies to worms. Offset +0x59.
    pub rw_wind_influence_worms: bool,
    /// RW gravity type. 0-3. Offset +0x5A.
    pub rw_gravity_type: u8,
    /// RW gravity strength (fixed 16.16). Offset +0x5B.
    pub rw_gravity_strength: i32,
    /// RW crate rate. Offset +0x5F.
    pub rw_crate_rate: u8,
    /// RW crate shower. Offset +0x60.
    pub rw_crate_shower: bool,
    /// RW anti-sink. Offset +0x61.
    pub rw_anti_sink: bool,
    /// RW remember weapons. Offset +0x62.
    pub rw_remember_weapons: bool,
    /// RW extended fuses/herds. Offset +0x63.
    pub rw_extended_fuses: bool,
    /// RW anti-lock aim. Offset +0x64.
    pub rw_anti_lock_aim: bool,
    /// Terrain overlap phasing glitch. Tri-state. Offset +0x65.
    pub terrain_overlap_glitch: u8,
    /// Fractional round timer. Offset +0x66.
    pub fractional_round_timer: bool,
    /// Automatic end-of-turn retreat. Offset +0x67.
    pub auto_retreat: bool,
    /// Health crates cure poison. 0/1/2/0xFF. Offset +0x68.
    pub health_crates_cure_poison: i8,
    /// RW Kaos mod. 0-5. Offset +0x69.
    pub rw_kaos_mod: u8,
    /// Sheep Heaven's Gate bitmask. Offset +0x6A.
    pub sheep_heavens_gate: u8,
    /// Conserve instant utilities. Offset +0x6B.
    pub conserve_instant_utilities: bool,
    /// Expedite instant utilities. Offset +0x6C.
    pub expedite_instant_utilities: bool,
    /// Double time stack limit. Offset +0x6D.
    pub double_time_stack_limit: u8,
}

/// Helper: read a little-endian u16 from a byte slice.
fn read_u16_le(b: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([b[offset], b[offset + 1]])
}

/// Helper: read a little-endian i32 from a byte slice.
fn read_i32_le(b: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]])
}

/// Helper: read a little-endian u32 from a byte slice.
fn read_u32_le(b: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]])
}

impl ExtendedOptions {
    /// Read extended options from a 110-byte slice (payload offset 0x124).
    pub fn from_bytes(b: &[u8]) -> Self {
        Self {
            data_version: read_u32_le(b, 0x00),
            constant_wind: b[0x04] != 0,
            wind: read_u16_le(b, 0x05) as i16,
            wind_bias: b[0x07],
            gravity: read_i32_le(b, 0x08),
            terrain_friction: read_i32_le(b, 0x0C),
            rope_knocking: b[0x10],
            blood_level: b[0x11],
            unrestrict_rope: b[0x12] != 0,
            auto_place_worms_by_ally: b[0x13] != 0,
            no_crate_probability: b[0x14],
            max_crate_count: read_u16_le(b, 0x15),
            sd_disables_worm_select: b[0x17] != 0,
            sd_worm_damage_per_turn: b[0x18],
            phased_worms_allied: b[0x19],
            phased_worms_enemy: b[0x1A],
            circular_aim: b[0x1B] != 0,
            anti_lock_aim: b[0x1C] != 0,
            anti_lock_power: b[0x1D] != 0,
            worm_select_no_end_hot_seat: b[0x1E] != 0,
            worm_select_never_cancelled: b[0x1F] != 0,
            batty_rope: b[0x20] != 0,
            rope_roll_drops: b[0x21],
            x_impact_loss_of_control: b[0x22],
            keep_control_bump_head: b[0x23] != 0,
            keep_control_skimming: b[0x24],
            explosion_fall_damage: b[0x25] != 0,
            explosions_push_all: b[0x26],
            undetermined_crates: b[0x27],
            undetermined_fuses: b[0x28],
            pause_timer_while_firing: b[0x29] != 0,
            loss_of_control_no_end_turn: b[0x2A] != 0,
            weapon_use_no_end_turn: b[0x2B] != 0,
            weapon_use_no_block: b[0x2C] != 0,
            drill_imparts_velocity: b[0x2D],
            girder_radius_assist: b[0x2E] != 0,
            petrol_turn_decay: read_u16_le(b, 0x2F),
            petrol_touch_decay: b[0x31],
            max_flamelet_count: read_u16_le(b, 0x32),
            max_projectile_speed: read_i32_le(b, 0x34),
            max_rope_speed: read_i32_le(b, 0x38),
            max_jet_pack_speed: read_i32_le(b, 0x3C),
            game_engine_speed: read_i32_le(b, 0x40),
            indian_rope_glitch: b[0x44],
            herd_doubling_glitch: b[0x45],
            jet_pack_bungee_glitch: b[0x46] != 0,
            angle_cheat_glitch: b[0x47] != 0,
            glide_glitch: b[0x48] != 0,
            skipwalking: b[0x49] as i8,
            block_roofing: b[0x4A],
            floating_weapon_glitch: b[0x4B] != 0,
            rw_bounciness: read_i32_le(b, 0x4C),
            rw_air_viscosity: read_i32_le(b, 0x50),
            rw_air_viscosity_worms: b[0x54] != 0,
            rw_wind_influence: read_u32_le(b, 0x55),
            rw_wind_influence_worms: b[0x59] != 0,
            rw_gravity_type: b[0x5A],
            rw_gravity_strength: read_i32_le(b, 0x5B),
            rw_crate_rate: b[0x5F],
            rw_crate_shower: b[0x60] != 0,
            rw_anti_sink: b[0x61] != 0,
            rw_remember_weapons: b[0x62] != 0,
            rw_extended_fuses: b[0x63] != 0,
            rw_anti_lock_aim: b[0x64] != 0,
            terrain_overlap_glitch: b[0x65],
            fractional_round_timer: b[0x66] != 0,
            auto_retreat: b[0x67] != 0,
            health_crates_cure_poison: b[0x68] as i8,
            rw_kaos_mod: b[0x69],
            sheep_heavens_gate: b[0x6A],
            conserve_instant_utilities: b[0x6B] != 0,
            expedite_instant_utilities: b[0x6C] != 0,
            double_time_stack_limit: b[0x6D],
        }
    }

    /// Serialize extended options to 110 bytes.
    pub fn to_bytes(&self) -> [u8; EXTENDED_OPTIONS_SIZE] {
        let mut b = [0u8; EXTENDED_OPTIONS_SIZE];
        b[0x00..0x04].copy_from_slice(&self.data_version.to_le_bytes());
        b[0x04] = self.constant_wind as u8;
        b[0x05..0x07].copy_from_slice(&self.wind.to_le_bytes());
        b[0x07] = self.wind_bias;
        b[0x08..0x0C].copy_from_slice(&self.gravity.to_le_bytes());
        b[0x0C..0x10].copy_from_slice(&self.terrain_friction.to_le_bytes());
        b[0x10] = self.rope_knocking;
        b[0x11] = self.blood_level;
        b[0x12] = self.unrestrict_rope as u8;
        b[0x13] = self.auto_place_worms_by_ally as u8;
        b[0x14] = self.no_crate_probability;
        b[0x15..0x17].copy_from_slice(&self.max_crate_count.to_le_bytes());
        b[0x17] = self.sd_disables_worm_select as u8;
        b[0x18] = self.sd_worm_damage_per_turn;
        b[0x19] = self.phased_worms_allied;
        b[0x1A] = self.phased_worms_enemy;
        b[0x1B] = self.circular_aim as u8;
        b[0x1C] = self.anti_lock_aim as u8;
        b[0x1D] = self.anti_lock_power as u8;
        b[0x1E] = self.worm_select_no_end_hot_seat as u8;
        b[0x1F] = self.worm_select_never_cancelled as u8;
        b[0x20] = self.batty_rope as u8;
        b[0x21] = self.rope_roll_drops;
        b[0x22] = self.x_impact_loss_of_control;
        b[0x23] = self.keep_control_bump_head as u8;
        b[0x24] = self.keep_control_skimming;
        b[0x25] = self.explosion_fall_damage as u8;
        b[0x26] = self.explosions_push_all;
        b[0x27] = self.undetermined_crates;
        b[0x28] = self.undetermined_fuses;
        b[0x29] = self.pause_timer_while_firing as u8;
        b[0x2A] = self.loss_of_control_no_end_turn as u8;
        b[0x2B] = self.weapon_use_no_end_turn as u8;
        b[0x2C] = self.weapon_use_no_block as u8;
        b[0x2D] = self.drill_imparts_velocity;
        b[0x2E] = self.girder_radius_assist as u8;
        b[0x2F..0x31].copy_from_slice(&self.petrol_turn_decay.to_le_bytes());
        b[0x31] = self.petrol_touch_decay;
        b[0x32..0x34].copy_from_slice(&self.max_flamelet_count.to_le_bytes());
        b[0x34..0x38].copy_from_slice(&self.max_projectile_speed.to_le_bytes());
        b[0x38..0x3C].copy_from_slice(&self.max_rope_speed.to_le_bytes());
        b[0x3C..0x40].copy_from_slice(&self.max_jet_pack_speed.to_le_bytes());
        b[0x40..0x44].copy_from_slice(&self.game_engine_speed.to_le_bytes());
        b[0x44] = self.indian_rope_glitch;
        b[0x45] = self.herd_doubling_glitch;
        b[0x46] = self.jet_pack_bungee_glitch as u8;
        b[0x47] = self.angle_cheat_glitch as u8;
        b[0x48] = self.glide_glitch as u8;
        b[0x49] = self.skipwalking as u8;
        b[0x4A] = self.block_roofing;
        b[0x4B] = self.floating_weapon_glitch as u8;
        b[0x4C..0x50].copy_from_slice(&self.rw_bounciness.to_le_bytes());
        b[0x50..0x54].copy_from_slice(&self.rw_air_viscosity.to_le_bytes());
        b[0x54] = self.rw_air_viscosity_worms as u8;
        b[0x55..0x59].copy_from_slice(&self.rw_wind_influence.to_le_bytes());
        b[0x59] = self.rw_wind_influence_worms as u8;
        b[0x5A] = self.rw_gravity_type;
        b[0x5B..0x5F].copy_from_slice(&self.rw_gravity_strength.to_le_bytes());
        b[0x5F] = self.rw_crate_rate;
        b[0x60] = self.rw_crate_shower as u8;
        b[0x61] = self.rw_anti_sink as u8;
        b[0x62] = self.rw_remember_weapons as u8;
        b[0x63] = self.rw_extended_fuses as u8;
        b[0x64] = self.rw_anti_lock_aim as u8;
        b[0x65] = self.terrain_overlap_glitch;
        b[0x66] = self.fractional_round_timer as u8;
        b[0x67] = self.auto_retreat as u8;
        b[0x68] = self.health_crates_cure_poison as u8;
        b[0x69] = self.rw_kaos_mod;
        b[0x6A] = self.sheep_heavens_gate;
        b[0x6B] = self.conserve_instant_utilities as u8;
        b[0x6C] = self.expedite_instant_utilities as u8;
        b[0x6D] = self.double_time_stack_limit;
        b
    }

    /// Validate raw extended options bytes (110 bytes) against WA's field constraints.
    ///
    /// Returns `true` if all fields are within valid ranges, exactly matching
    /// the logic of `Scheme__ValidateExtendedOptions` (0x4D5110).
    ///
    /// Note: operates on raw bytes, not parsed struct fields, because WA validates
    /// at the byte level (e.g., bool fields must be exactly 0x00 or 0x01).
    pub fn validate_bytes(b: &[u8]) -> bool {
        fn is_bool(v: u8) -> bool {
            v == 0 || v == 1
        }
        fn is_tristate(v: u8) -> bool {
            v == 0 || v == 1 || v == 0x80
        }

        read_u32_le(b, 0x00) == 0                                        // data_version
        && (read_i32_le(b, 0x08) as u32).wrapping_sub(1) < 0xC8_0000     // gravity [1, 0xC80000]
        && (read_i32_le(b, 0x0C) as u32) < 0x2_8CCD                     // terrain_friction
        && is_bool(b[0x12])                                              // unrestrict_rope
        && is_bool(b[0x13])                                              // auto_place_worms_by_ally
        && is_bool(b[0x17])                                              // sd_disables_worm_select
        && b[0x19] < 4                                                   // phased_worms_allied
        && b[0x1A] < 4                                                   // phased_worms_enemy
        && is_bool(b[0x1B])                                              // circular_aim
        && is_bool(b[0x1C])                                              // anti_lock_aim
        && is_bool(b[0x1D])                                              // anti_lock_power
        && is_bool(b[0x1E])                                              // worm_select_no_end_hot_seat
        && is_bool(b[0x1F])                                              // worm_select_never_cancelled
        && is_bool(b[0x20])                                              // batty_rope
        && b[0x21] < 3                                                   // rope_roll_drops
        && (b[0x22] == 0 || b[0x22] == 0xFF)                             // x_impact_loss_of_control
        && is_bool(b[0x23])                                              // keep_control_bump_head
        && b[0x24] < 3                                                   // keep_control_skimming
        && is_bool(b[0x25])                                              // explosion_fall_damage
        && is_tristate(b[0x26])                                          // explosions_push_all
        && is_tristate(b[0x27])                                          // undetermined_crates
        && is_tristate(b[0x28])                                          // undetermined_fuses
        && is_bool(b[0x29])                                              // pause_timer_while_firing
        && is_bool(b[0x2A])                                              // loss_of_control_no_end_turn
        && is_bool(b[0x2B])                                              // weapon_use_no_end_turn
        && is_bool(b[0x2C])                                              // weapon_use_no_block
        && is_tristate(b[0x2D])                                          // drill_imparts_velocity
        && is_bool(b[0x2E])                                              // girder_radius_assist
        && b[0x31] != 0                                                  // petrol_touch_decay nonzero
        && read_u16_le(b, 0x32) != 0                                    // max_flamelet_count nonzero
        && (read_i32_le(b, 0x34) as u32) < 0x8000_0000                  // max_projectile_speed > 0
        && (read_i32_le(b, 0x38) as u32) < 0x8000_0000                  // max_rope_speed > 0
        && (read_i32_le(b, 0x3C) as u32) < 0x8000_0000                  // max_jet_pack_speed > 0
        && (read_i32_le(b, 0x40) as u32).wrapping_sub(0x1000) < 0x7F_F001 // game_engine_speed
        && is_tristate(b[0x44])                                          // indian_rope_glitch
        && is_tristate(b[0x45])                                          // herd_doubling_glitch
        && is_bool(b[0x46])                                              // jet_pack_bungee_glitch
        && is_bool(b[0x47])                                              // angle_cheat_glitch
        && is_bool(b[0x48])                                              // glide_glitch
        && (b[0x49] as i8 as i32 + 1) as u32 <= 2                       // skipwalking {-1,0,1}
        && b[0x4A] < 3                                                   // block_roofing
        && is_bool(b[0x4B])                                              // floating_weapon_glitch
        && (read_i32_le(b, 0x4C) as u32) < 0x1_0001                     // rw_bounciness
        && (read_i32_le(b, 0x50) as u32) < 0x4001                       // rw_air_viscosity
        && is_bool(b[0x54])                                              // rw_air_viscosity_worms
        && read_u32_le(b, 0x55) < 0x1_0001                              // rw_wind_influence
        && is_bool(b[0x59])                                              // rw_wind_influence_worms
        && b[0x5A] < 4                                                   // rw_gravity_type
        && (read_i32_le(b, 0x5B) as u32).wrapping_add(0x4000_0000) < 0x8000_0001 // rw_gravity_strength
        && is_bool(b[0x60])                                              // rw_crate_shower
        && is_bool(b[0x61])                                              // rw_anti_sink
        && is_bool(b[0x62])                                              // rw_remember_weapons
        && is_bool(b[0x63])                                              // rw_extended_fuses
        && is_bool(b[0x64])                                              // rw_anti_lock_aim
        && is_tristate(b[0x65])                                          // terrain_overlap_glitch
        && is_bool(b[0x66])                                              // fractional_round_timer
        && is_bool(b[0x67])                                              // auto_retreat
        && (b[0x68] as i8 as i32 + 1) as u32 <= 3                       // health_crates_cure_poison
        && b[0x69] < 6                                                   // rw_kaos_mod
        && b[0x6A].wrapping_sub(1) < 7                                   // sheep_heavens_gate [1,7]
        && is_bool(b[0x6B])                                              // conserve_instant_utilities
        && is_bool(b[0x6C]) // expedite_instant_utilities
    }
}

/// V3 extended options defaults from WA.exe ROM at 0x649AB8 (110 bytes).
///
/// Applied to V1/V2 schemes at struct+0x138 (payload+0x124).
/// Dumped directly from the binary — tri-state fields use 0x80 = "engine default".
pub const EXTENDED_OPTIONS_DEFAULTS: [u8; EXTENDED_OPTIONS_SIZE] = [
    // Byte-exact copy of WA.exe ROM at 0x649AB8 (110 bytes).
    // Dumped at runtime to avoid transcription errors.
    0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x0F, // +0x00
    0x70, 0x3D, 0x00, 0x00, 0xC2, 0xF5, 0x00, 0x00, // +0x08
    0xFF, 0xFF, 0x00, 0x00, 0xFF, 0x05, 0x00, 0x01, // +0x10
    0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // +0x18
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x80, // +0x20
    0x80, 0x01, 0x00, 0x00, 0x00, 0x80, 0x00, 0x32, // +0x28
    0x33, 0x1E, 0xC8, 0x00, 0x00, 0x00, 0x20, 0x00, // +0x30
    0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x05, 0x00, // +0x38
    0x00, 0x00, 0x01, 0x00, 0x80, 0x80, 0x01, 0x01, // +0x40
    0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // +0x48
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // +0x50
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, // +0x58
    0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, // +0x60
    0x01, 0x00, 0x07, 0x00, 0x00, 0x01, // +0x68
];

// === Typed accessors on SchemeFile ===

impl SchemeFile {
    /// Get the game options from the payload.
    pub fn options(&self) -> SchemeOptions {
        SchemeOptions::from_bytes(&self.payload[OPTIONS_OFFSET..])
    }

    /// Get weapon settings for a weapon by scheme index (0-63).
    ///
    /// Indices 0-44 are V1 weapons, 45-63 are V2 super weapons.
    /// Returns `None` if the index is out of range for this scheme version.
    pub fn weapon(&self, scheme_index: usize) -> Option<WeaponSettings> {
        if scheme_index >= WEAPONS_TOTAL_COUNT {
            return None;
        }
        // V1 schemes only have weapons 0-44
        if scheme_index >= WEAPONS_V1_COUNT && self.version == SchemeVersion::V1 {
            return None;
        }
        let offset = WEAPONS_V1_OFFSET + scheme_index * WEAPON_ENTRY_SIZE;
        Some(WeaponSettings::from_bytes(&self.payload[offset..]))
    }

    /// Get extended options (V3 only). Returns `None` for V1/V2 schemes.
    pub fn extended_options(&self) -> Option<ExtendedOptions> {
        if self.payload.len() > EXTENDED_OPTIONS_OFFSET {
            Some(ExtendedOptions::from_bytes(
                &self.payload[EXTENDED_OPTIONS_OFFSET..],
            ))
        } else {
            None
        }
    }

    /// Get extended options, falling back to ROM defaults for V1/V2 schemes.
    pub fn extended_options_or_defaults(&self) -> ExtendedOptions {
        self.extended_options()
            .unwrap_or_else(|| ExtendedOptions::from_bytes(&EXTENDED_OPTIONS_DEFAULTS))
    }

    /// Number of weapons available in this scheme version.
    pub fn weapon_count(&self) -> usize {
        match self.version {
            SchemeVersion::V1 => WEAPONS_V1_COUNT,
            _ => WEAPONS_TOTAL_COUNT,
        }
    }
}

/// Error type for file-based scheme operations.
#[derive(Debug)]
pub enum SchemeFileError {
    Io(std::io::Error),
    Parse(SchemeError),
}

impl core::fmt::Display for SchemeFileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SchemeFileError::Io(e) => write!(f, "I/O error: {e}"),
            SchemeFileError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_roundtrip() {
        for v in [SchemeVersion::V1, SchemeVersion::V2, SchemeVersion::V3] {
            assert_eq!(SchemeVersion::from_byte(v.to_byte()), Some(v));
        }
        // Unknown versions return None
        assert_eq!(SchemeVersion::from_byte(0), None);
        assert_eq!(SchemeVersion::from_byte(4), None);
    }

    #[test]
    fn payload_sizes() {
        assert_eq!(SchemeVersion::V1.payload_size(), 0xD8);
        assert_eq!(SchemeVersion::V2.payload_size(), 0x124);
        assert_eq!(SchemeVersion::V3.payload_size(), 0x192);
    }

    #[test]
    fn parse_v1_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x01);
        data.extend_from_slice(&[0xAA; SCHEME_PAYLOAD_V1]);
        assert_eq!(data.len(), 221);

        let scheme = SchemeFile::from_bytes(&data).unwrap();
        assert_eq!(scheme.version, SchemeVersion::V1);
        assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V1);
        assert!(scheme.payload.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn parse_v2_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x02);
        data.extend_from_slice(&[0xBB; SCHEME_PAYLOAD_V2]);
        assert_eq!(data.len(), 297);

        let scheme = SchemeFile::from_bytes(&data).unwrap();
        assert_eq!(scheme.version, SchemeVersion::V2);
        assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V2);
    }

    #[test]
    fn roundtrip_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x01);
        data.extend_from_slice(&[0x42; SCHEME_PAYLOAD_V1]);

        let scheme = SchemeFile::from_bytes(&data).unwrap();
        assert_eq!(scheme.to_bytes(), data);
    }

    #[test]
    fn error_too_short() {
        assert_eq!(
            SchemeFile::from_bytes(b"SCH"),
            Err(SchemeError::TooShort { len: 3 })
        );
    }

    #[test]
    fn error_bad_magic() {
        let mut data = vec![b'N', b'O', b'P', b'E', 0x01];
        data.extend_from_slice(&[0; SCHEME_PAYLOAD_V1]);
        assert!(matches!(
            SchemeFile::from_bytes(&data),
            Err(SchemeError::BadMagic([b'N', b'O', b'P', b'E']))
        ));
    }

    #[test]
    fn error_payload_mismatch() {
        // v1 header but only 10 bytes of payload
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x01);
        data.extend_from_slice(&[0; 10]);

        assert!(matches!(
            SchemeFile::from_bytes(&data),
            Err(SchemeError::PayloadMismatch {
                expected: 0xD8,
                got: 10
            })
        ));
    }

    #[test]
    fn v3_variable_length_padded_with_defaults() {
        // V3 scheme with only V2-length payload (no extended options in file)
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x03);
        data.extend_from_slice(&[0xAA; SCHEME_PAYLOAD_V2]); // V2 portion filled with 0xAA
                                                            // No extended options bytes — should be padded with defaults

        let scheme = SchemeFile::from_bytes(&data).expect("should accept short V3");
        assert_eq!(scheme.version, SchemeVersion::V3);
        assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V3);
        // V2 portion should be file data
        assert_eq!(scheme.payload[0], 0xAA);
        assert_eq!(scheme.payload[SCHEME_PAYLOAD_V2 - 1], 0xAA);
        // Extended portion should be defaults
        assert_eq!(
            &scheme.payload[SCHEME_PAYLOAD_V2..],
            &EXTENDED_OPTIONS_DEFAULTS[..]
        );
    }

    #[test]
    fn v3_full_length_accepted() {
        // V3 scheme with full 0x192 payload
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x03);
        data.extend_from_slice(&[0; SCHEME_PAYLOAD_V3]);

        let scheme = SchemeFile::from_bytes(&data).expect("should accept full V3");
        assert_eq!(scheme.version, SchemeVersion::V3);
        assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V3);
    }

    #[test]
    fn v3_too_short_rejected() {
        // V3 with less than V2-length payload should be rejected
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x03);
        data.extend_from_slice(&[0; 0x100]); // less than V2 (0x124)

        assert!(SchemeFile::from_bytes(&data).is_err());
    }
}
