//! Scheme file (.wsc) parser for Worms Armageddon.
//!
//! .wsc files store game settings (turn time, wind, per-weapon ammo/delay, etc.).
//! Located in `User\Schemes\*.wsc`.
//!
//! Binary format:
//!   Bytes 0-3: Magic "SCHM" (0x5343484D)
//!   Byte 4:    Version byte
//!   Bytes 5+:  Payload (size depends on version)
//!
//! Version → payload size:
//!   0x01 → 0xD8 bytes (216): 36 options + 45 weapons × 4
//!   0x02 → 0x124 bytes (292): V1 + 19 super weapons × 4
//!   0x03 → 0x192 bytes (402): V2 + 110 bytes extended options
//!
//! Source: Ghidra decompilation of Scheme__ReadFile (0x4D3890),
//!         Scheme__SaveFile (0x4D44F0), worms2d.info/Game_scheme_file

use deku::prelude::*;

use crate::fixed::Fixed;

/// Magic bytes that identify .wsc scheme files.
pub const SCHEME_MAGIC: [u8; 4] = *b"SCHM";

/// Size of the file header (magic + version byte).
pub const SCHEME_HEADER_SIZE: usize = SCHEME_MAGIC.len() + SchemeVersion::SIZE_BYTES.unwrap();

/// Byte offset within the payload where game options start.
pub const OPTIONS_OFFSET: usize = 0;

/// Size of the game options section in the payload.
pub const OPTIONS_SIZE: usize = SchemeOptions::SIZE_BYTES.unwrap();

/// Byte offset within the payload where standard weapon settings start.
pub const WEAPONS_V1_OFFSET: usize = OPTIONS_OFFSET + OPTIONS_SIZE;

/// Bytes per weapon entry.
pub const WEAPON_ENTRY_SIZE: usize = WeaponSettings::SIZE_BYTES.unwrap();

/// Number of weapons in V1 schemes.
pub const WEAPONS_V1_COUNT: usize = StandardWeapons::SIZE_BYTES.unwrap() / WEAPON_ENTRY_SIZE;

/// Number of super weapons added in V2.
pub const WEAPONS_V2_COUNT: usize = SuperWeapons::SIZE_BYTES.unwrap() / WEAPON_ENTRY_SIZE;

/// Total weapon entries in canonical V3 payloads.
pub const WEAPONS_TOTAL_COUNT: usize = WEAPONS_V1_COUNT + WEAPONS_V2_COUNT;

/// Size of the standard weapons section in the payload.
pub const STANDARD_WEAPONS_SIZE: usize = StandardWeapons::SIZE_BYTES.unwrap();

/// Byte offset within the payload where standard weapon settings start.
pub const STANDARD_WEAPONS_OFFSET: usize = WEAPONS_V1_OFFSET;

/// Byte offset where V2 super weapons start.
pub const WEAPONS_V2_OFFSET: usize = WEAPONS_V1_OFFSET + STANDARD_WEAPONS_SIZE;

/// Size of the super-weapons section in the payload.
pub const SUPER_WEAPONS_SIZE: usize = SuperWeapons::SIZE_BYTES.unwrap();

/// Byte offset within the payload where super weapon settings start.
pub const SUPER_WEAPONS_OFFSET: usize = WEAPONS_V2_OFFSET;

/// Size of the V3 extended options section.
pub const EXTENDED_OPTIONS_SIZE: usize = ExtendedOptions::SIZE_BYTES.unwrap();

/// Byte offset where V3 extended options start.
pub const EXTENDED_OPTIONS_OFFSET: usize = WEAPONS_V2_OFFSET + SUPER_WEAPONS_SIZE;

/// Payload size for version 1 schemes.
pub const SCHEME_PAYLOAD_V1: usize = WEAPONS_V2_OFFSET;

/// Payload size for version 2 schemes.
pub const SCHEME_PAYLOAD_V2: usize = EXTENDED_OPTIONS_OFFSET;

/// Payload size for version 3 schemes.
pub const SCHEME_PAYLOAD_V3: usize = EXTENDED_OPTIONS_OFFSET + EXTENDED_OPTIONS_SIZE;

/// Scheme file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum SchemeVersion {
    /// Version 1: 0xD8 byte payload (total file: 221 bytes)
    V1 = 1,
    /// Version 2: 0x124 byte payload (total file: 297 bytes)
    V2 = 2,
    /// Version 3: 0x192 byte payload (total file: 407 bytes)
    /// V2 + 110 bytes extended options.
    V3 = 3,
}

/// Weapon ammunition stored in a scheme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ammunition {
    Finite(u8),
    Infinite,
}

impl Ammunition {
    const MAX_FINITE: u8 = 9;
    const INFINITE_RAW: u8 = Self::MAX_FINITE + 1;

    pub fn finite(value: u8) -> Option<Self> {
        if value <= Self::MAX_FINITE {
            Some(Self::Finite(value))
        } else {
            None
        }
    }

    fn from_raw(value: u8) -> Self {
        Self::finite(value).unwrap_or(Self::Infinite)
    }

    fn to_raw(self) -> Result<u8, DekuError> {
        match self {
            Self::Finite(value) if value <= Self::MAX_FINITE => Ok(value),
            Self::Finite(_) => Err(DekuError::Parse("finite ammunition must be 0..=9")),
            Self::Infinite => Ok(Self::INFINITE_RAW),
        }
    }
}

impl Default for Ammunition {
    fn default() -> Self {
        Self::Finite(0)
    }
}

/// Settings stored for each individual weapon.
///
/// Weapon order in the file differs from the runtime weapon-id enum —
/// the .wsc uses the "scheme order" defined by the original game UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
pub struct WeaponSettings {
    /// Ammunition count; raw values above 9 mean unlimited ammunition.
    #[deku(
        bytes = "1",
        reader = "u8::from_reader_with_ctx(deku::reader, ()).map(Ammunition::from_raw)",
        writer = "self.ammo.to_raw()?.to_writer(deku::writer, ())"
    )]
    pub ammo: Ammunition,
    /// Weapon strength. The effect varies by weapon; for Jet Pack, this sets fuel.
    pub power: u8,
    /// Turns before the weapon becomes available; 0x80 through 0xFF blocks it indefinitely.
    pub delay: u8,
    /// Chance of the weapon appearing in a crate.
    pub crate_probability: u8,
}

/// These are the core game settings displayed in the scheme editor.
///
/// Source: worms2d.info/Game_scheme_file, Ghidra analysis of Scheme__ReadFile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
pub struct SchemeOptions {
    /// Extra time added between turns so players can switch seats or plan.
    pub hot_seat_delay: u8,
    /// Time available after using a weapon while grounded.
    pub retreat_time: u8,
    /// Time available after using a weapon while roping.
    pub rope_retreat_time: u8,
    /// Shows total round time along with turn time.
    pub display_total_round_time: bool,
    /// Automatically replays the ending of a significant turn outside online games.
    pub automatic_replays: bool,
    /// Damage dealt when a worm hits the ground at critical velocity.
    pub fall_damage: u8,
    /// Prevents worms from moving by walking or jumping.
    pub artillery_mode: bool,
    /// Unused by the game; scheme editors may use it as an editor marker.
    pub bounty_mode: u8,
    /// Controls what happens to unused weapons between rounds.
    pub stockpiling: StockpilingMode,
    /// Chooses how the active worm is selected each turn.
    pub worm_select: WormSelect,
    /// Event triggered after the remaining round time reaches zero.
    pub sudden_death_event: SuddenDeathEvent,
    /// Rate that water rises after each turn during sudden death.
    pub water_rise_rate: u8,
    /// Relative chance that a crate drop contains weapons.
    pub weapon_crate_probability: i8,
    /// Makes defeated teams drop a collectible donor card.
    pub donor_cards: bool,
    /// Relative chance that a crate drop contains energy.
    pub health_crate_probability: i8,
    /// Energy gained by collecting a health crate.
    pub health_crate_energy: u8,
    /// Relative chance that a crate drop contains a utility.
    pub utility_crate_probability: i8,
    /// Selects which hazards appear on the landscape.
    pub hazardous_object_types: u8,
    /// Time between activating a mine and it exploding.
    pub mine_delay: i8,
    /// Makes some landscape mines trigger as duds.
    pub dud_mines: bool,
    /// Lets players place worms on the landscape at the start of the round.
    pub manual_worm_placement: bool,
    /// Energy each worm begins the round with.
    pub worm_energy: u8,
    /// Time available to make a move.
    pub turn_time: u8,
    /// Time before sudden death is triggered.
    pub round_time: u8,
    /// Round wins required to win the match.
    pub number_of_wins: u8,
    /// Draws red particles instead of pink when worms are damaged.
    pub blood: bool,
    /// Converts Super Sheep into Aqua Sheep that can swim underwater.
    pub aqua_sheep: bool,
    /// Makes exploding sheep jump out of destroyed weapon crates.
    pub sheep_heaven: bool,
    /// Gives all worms infinite health except against drowning.
    pub god_worms: bool,
    /// Prevents the landscape from being destroyed except by rising water.
    pub indestructible_land: bool,
    /// Makes grenades more powerful.
    pub upgraded_grenade: bool,
    /// Makes the shotgun fire two consecutive shots.
    pub upgraded_shotgun: bool,
    /// Makes cluster weapons contain more clusters.
    pub upgraded_clusters: bool,
    /// Makes longbows more powerful.
    pub upgraded_longbow: bool,
    /// Lets teams start with their preselected team weapon.
    pub team_weapons: bool,
    /// Allows super weapons to appear in weapon crate drops.
    pub super_weapons: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum StockpilingMode {
    #[default]
    Off = 0,
    On = 1,
    Anti = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum WormSelect {
    #[default]
    Sequential = 0,
    On = 1,
    Random = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8")]
#[repr(u8)]
pub enum SuddenDeathEvent {
    #[default]
    RoundEnds = 0,
    NuclearStrike = 1,
    HpDrops = 2,
    Nothing = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum TriState {
    False = 0,
    True = 1,
    #[default]
    Default = 0x80,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum PhasedWorms {
    #[default]
    Off = 0,
    Worms = 1,
    WormsWeapons = 2,
    WormsWeaponsDamage = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum RopeRollDrops {
    #[default]
    Disabled = 0,
    AsFromRopeOnly = 1,
    AsFromRopeOrJump = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum KeepControlAfterSkimming {
    #[default]
    LoseControl = 0,
    KeepControl = 1,
    KeepControlAndRope = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum Skipwalking {
    Disabled = 0xFF,
    #[default]
    Possible = 0,
    Facilitated = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum BlockRoofing {
    #[default]
    Allow = 0,
    BlockAbove = 1,
    BlockEverywhere = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum RubberWormGravityType {
    #[default]
    Unmodified = 0,
    Standard = 1,
    BlackHoleConstant = 2,
    BlackHoleLinear = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead, DekuWrite, DekuSize)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
pub struct RubberwormOptions {
    /// Makes worms spring back on collision and sets the strength of that bounce.
    #[deku(assert = "(bounciness.to_raw() as u32) < 0x1_0001")]
    pub bounciness: Fixed,
    /// Makes airborne objects experience friction and sets the strength of that friction.
    #[deku(assert = "(air_viscosity.to_raw() as u32) < 0x4001")]
    pub air_viscosity: Fixed,
    /// Applies air viscosity to worms as well.
    pub air_viscosity_applies_to_worms: bool,
    /// Extends the set of objects influenced by wind and sets the strength of that influence.
    #[deku(assert = "(wind_influence.to_raw() as u32) < 0x1_0001")]
    pub wind_influence: Fixed,
    /// Applies wind influence to worms as well.
    pub wind_influence_applies_to_worms: bool,
    /// Selects the RubberWorm gravity behavior.
    pub gravity_type: RubberWormGravityType,
    /// Sets the strength or distance basis for RubberWorm gravity.
    #[deku(assert = "(gravity_strength.to_raw() as u32).wrapping_add(0x40000000) < 0x80000001")]
    pub gravity_strength: Fixed,
    /// Number of crates that can potentially spawn at the start of a turn.
    pub crate_rate: u8,
    /// Makes crates drop continuously during turns.
    pub crate_shower: bool,
    /// Teleports worms back to their last land position after touching the sea once.
    pub anti_sink: bool,
    /// Prevents weapon selection from resetting to a common weapon after rarer weapons are used.
    pub remember_weapons: bool,
    /// Allows all numeric keys to select weapon fuses and herd sizes.
    pub extended_fuses_herds: bool,
    /// Resets aiming angle to zero degrees after each shot.
    pub anti_lock_aim: bool,
}

impl Default for RubberwormOptions {
    fn default() -> Self {
        Self {
            bounciness: Fixed::from_raw(0),
            air_viscosity: Fixed::from_raw(0),
            air_viscosity_applies_to_worms: false,
            wind_influence: Fixed::from_raw(0),
            wind_influence_applies_to_worms: false,
            gravity_type: RubberWormGravityType::default(),
            gravity_strength: Fixed::from_raw(0x1_0000),
            crate_rate: 0,
            crate_shower: false,
            anti_sink: false,
            remember_weapons: false,
            extended_fuses_herds: false,
            anti_lock_aim: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
#[deku(id_type = "u8", ctx = "_: deku::ctx::Endian")]
#[repr(u8)]
pub enum HealthCratesCurePoison {
    Disabled = 0xFF,
    CollectingWorm = 0,
    #[default]
    CollectingWormTeam = 1,
    CollectingWormTeamsAllied = 2,
}

/// Controls what the Sheep Heaven scheme option enables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead, DekuWrite, DekuSize)]
#[deku(ctx = "_: deku::ctx::Endian")]
#[repr(transparent)]
pub struct SheepHeavensGate(#[deku(assert = "*field_0 != 0 && (*field_0 & !0x07) == 0")] u8);

bitflags::bitflags! {
    impl SheepHeavensGate: u8 {
        const SHEEP_EXPLODE_FROM_ALL_CRATES = 0x01;
        const EXTENDED_SHEEP_FUSE_TIME = 0x02;
        const BOOST_SHEEP_WEAPON_CRATE_PROBABILITY = 0x04;
    }
}

impl Default for SheepHeavensGate {
    fn default() -> Self {
        Self::all()
    }
}

/// Options found only in Version 3 schemes.
///
/// Invalid values are reset to defaults when a scheme is loaded. Many settings
/// here were previously controlled through game logic versions or RubberWorm.
///
/// Source: worms2d.info/Game_scheme_file, Ghidra Scheme__ValidateExtendedOptions validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, DekuRead, DekuWrite, DekuSize)]
#[deku(endian = "endian", ctx = "endian: deku::ctx::Endian")]
pub struct ExtendedOptions {
    /// Format discriminator reserved for future interpretation changes.
    #[deku(assert = "*data_version == 0")]
    pub data_version: u32,
    /// Interprets the wind field as a constant value.
    pub constant_wind: bool,
    /// Wind strength and direction, or random-wind maximum when constant wind is off.
    pub wind: i16,
    /// Biases wind direction based on which side of the map worms occupy.
    pub wind_bias: u8,
    /// Acceleration due to gravity in pixels per frame per frame.
    #[deku(assert = "(gravity.to_raw() as u32).wrapping_sub(1) < 0xC8_0000")]
    pub gravity: Fixed,
    /// Proportion of velocity retained during terrain collisions.
    #[deku(assert = "(terrain_friction.to_raw() as u32) < 0x2_8CCD")]
    pub terrain_friction: Fixed,
    /// Rope knocking force relative to ordinary rope knocking.
    pub rope_knocking: u8,
    /// Amount of blood emitted from worms, without changing blood color.
    pub blood_level: u8,
    /// Allows the rope to fire below the horizontal and reduces rope friction.
    pub unrestrict_rope: bool,
    /// Groups worms by allied color during automatic worm placement.
    pub auto_place_worms_by_ally: bool,
    /// Probability that no crate falls at the start of a turn.
    pub no_crate_probability: u8,
    /// Maximum crates that may exist before the game stops spawning new ones.
    pub max_crate_count: u16,
    /// Disables Worm Select when sudden death starts.
    pub sd_disables_worm_select: bool,
    /// Health lost each turn when sudden death is set to nuclear strike.
    pub sd_worm_damage_per_turn: u8,
    /// Controls how allied worms move through and otherwise affect each other.
    pub phased_worms_allied: PhasedWorms,
    /// Controls how enemy worms move through and otherwise affect each other.
    pub phased_worms_enemy: PhasedWorms,
    /// Lets the crosshair continue past the top or bottom of its limits.
    pub circular_aim: bool,
    /// Resets aim between turns to make repeat shots more challenging.
    pub anti_lock_aim: bool,
    /// Makes power decrease after reaching maximum instead of firing the weapon.
    pub anti_lock_power: bool,
    /// Prevents worm selection at turn start from ending the hot-seat timer.
    pub worm_select_no_end_hot_seat: bool,
    /// Prevents Worm Selection from being cancelled by movement or similar actions.
    pub worm_select_never_cancelled: bool,
    /// Lets worms keep Ninja Rope, Bungee, or Jet Pack when a turn ends.
    pub batty_rope: bool,
    /// Controls which weapons may be dropped during a rope roll.
    pub rope_roll_drops: RopeRollDrops,
    /// Controls whether fast horizontal collisions make worms lose control.
    #[deku(assert = "*x_impact_loss_of_control == 0 || *x_impact_loss_of_control == 0xFF")]
    pub x_impact_loss_of_control: u8,
    /// Retains control after upward terrain collisions during rope or bungee rolls.
    pub keep_control_bump_head: bool,
    /// Controls what happens to worm control after skimming on water.
    pub keep_control_skimming: KeepControlAfterSkimming,
    /// Adds fall damage when worms are thrown by explosions.
    pub explosion_fall_damage: bool,
    /// Makes explosions push all objects.
    pub explosions_push_all: TriState,
    /// Delays crate content selection until pickup or Crate Spy collection.
    pub undetermined_crates: TriState,
    /// Delays random mine fuse selection until the mine is triggered.
    pub undetermined_fuses: TriState,
    /// Pauses the turn timer while a weapon is being fired.
    pub pause_timer_while_firing: bool,
    /// Prevents losing worm control from ending the turn.
    pub loss_of_control_no_end_turn: bool,
    /// Prevents weapon use from ending the turn.
    pub weapon_use_no_end_turn: bool,
    /// Keeps Earthquake, Armageddon, and Indian Nuclear Test available with weapon-use continuation.
    pub weapon_use_no_block: bool,
    /// Makes Pneumatic Drill hits impart horizontal velocity to the target worm.
    pub drill_imparts_velocity: TriState,
    /// Prevents the cursor from moving outside the valid girder placement radius.
    pub girder_radius_assist: bool,
    /// How much Petrol Bomb flames decay per turn.
    pub petrol_turn_decay: u16,
    /// How much Petrol Bomb flames decay when touched by a worm.
    #[deku(assert = "*petrol_touch_decay != 0")]
    pub petrol_touch_decay: u8,
    /// Maximum number of fire objects that can exist at once.
    #[deku(assert = "*max_flamelet_count != 0")]
    pub max_flamelet_count: u16,
    /// Maximum speed for objects following projectile physics.
    #[deku(assert = "(max_projectile_speed.to_raw() as u32) < 0x80000000")]
    pub max_projectile_speed: Fixed,
    /// Maximum speed for a worm attached to Ninja Rope or Bungee.
    #[deku(assert = "(max_rope_speed.to_raw() as u32) < 0x80000000")]
    pub max_rope_speed: Fixed,
    /// Maximum speed for a worm using Jet Pack.
    #[deku(assert = "(max_jet_pack_speed.to_raw() as u32) < 0x80000000")]
    pub max_jet_pack_speed: Fixed,
    /// Speed at which physics and sound effects occur.
    #[deku(assert = "(game_engine_speed.to_raw() as u32).wrapping_sub(0x1000) < 0x7F_F001")]
    pub game_engine_speed: Fixed,
    /// Allows moving worms to fire Ninja Rope vertically downwards.
    pub indian_rope_glitch: TriState,
    /// Allows well-timed jumps to release twice the selected Mad Cow herd count.
    pub herd_doubling_glitch: TriState,
    /// Allows Bungee to trigger from Jet Pack.
    pub jet_pack_bungee_glitch: bool,
    /// Allows moving worms to bypass angle limits on Baseball Bat and Longbow.
    pub angle_cheat_glitch: bool,
    /// Allows certain leftward collisions to continue motion instead of landing.
    pub glide_glitch: bool,
    /// Controls skip-walking behavior.
    pub skipwalking: Skipwalking,
    /// Controls whether roofing is allowed or blocked.
    pub block_roofing: BlockRoofing,
    /// Allows precisely dropped impact weapons to rest on surfaces until their fuse expires.
    pub floating_weapon_glitch: bool,
    /// RubberWorm physics and utility behavior.
    pub rubberworm: RubberwormOptions,
    /// Lets objects overlapping land pass through it until reaching open space.
    pub terrain_overlap_glitch: TriState,
    /// Counts sudden-death round time in fractions of seconds.
    pub fractional_round_timer: bool,
    /// Triggers retreat time when turn time expires.
    pub auto_retreat: bool,
    /// Chooses which worms are cured of poison when a health crate is collected.
    pub health_crates_cure_poison: HealthCratesCurePoison,
    /// Selects a RubberWorm Kaos utility-crate probability preset.
    #[deku(assert = "*rw_kaos_mod < 6")]
    pub rw_kaos_mod: u8,
    /// Controls which Sheep Heaven behaviors are enabled.
    pub sheep_heavens_gate: SheepHeavensGate,
    /// Conserves remaining instant utilities for later automatic use.
    pub conserve_instant_utilities: bool,
    /// Consumes instant utilities as soon as they are collected.
    pub expedite_instant_utilities: bool,
    /// Number of times Double Time can be activated in a single turn.
    pub double_time_stack_limit: u8,
}

impl ExtendedOptions {
    /// V3 extended options defaults from WA.exe ROM at 0x649AB8, serialized by Deku.
    pub fn default_bytes() -> [u8; EXTENDED_OPTIONS_SIZE] {
        Self::default()
            .to_bytes()
            .expect("default extended options should serialize")
            .try_into()
            .expect("extended options defaults should be 110 bytes")
    }

    /// Parse the V3 extended options block using the scheme file byte order.
    pub fn from_bytes(b: &[u8]) -> Result<Self, DekuError> {
        Self::from_bytes_with_endian(b, deku::ctx::Endian::Little)
    }

    fn from_bytes_with_endian(b: &[u8], endian: deku::ctx::Endian) -> Result<Self, DekuError> {
        if b.len() != EXTENDED_OPTIONS_SIZE {
            return Err(DekuError::Parse("extended options should be 110 bytes"));
        }

        let mut cursor = deku::no_std_io::Cursor::new(b);
        let mut reader = deku::reader::Reader::new(&mut cursor);
        Self::from_reader_with_ctx(&mut reader, endian)
    }

    /// Serialize V3 extended options using the scheme file byte order.
    pub fn to_bytes(&self) -> Result<Vec<u8>, DekuError> {
        let mut out_buf = Vec::new();
        let mut cursor = deku::no_std_io::Cursor::new(&mut out_buf);
        let mut writer = deku::writer::Writer::new(&mut cursor);
        DekuWriter::to_writer(self, &mut writer, deku::ctx::Endian::Little)?;
        writer.finalize()?;
        Ok(out_buf)
    }

    /// Validate raw extended options bytes against WA's field constraints.
    ///
    /// Returns `true` if all fields are within valid ranges, exactly matching
    /// the logic of `Scheme__ValidateExtendedOptions` (0x4D5110).
    ///
    /// Deku enforces WA's exact 0x00/0x01 bools, enum domains, and field
    /// range assertions while parsing the layout.
    pub fn validate_bytes(b: &[u8]) -> bool {
        Self::from_bytes(b).is_ok()
    }
}

impl Default for ExtendedOptions {
    fn default() -> Self {
        Self {
            data_version: 0,
            constant_wind: false,
            wind: 100,
            wind_bias: 0x0F,
            gravity: Fixed::from_raw(0x3D70),
            terrain_friction: Fixed::from_raw(0xF5C2),
            rope_knocking: 0xFF,
            blood_level: 0xFF,
            unrestrict_rope: false,
            auto_place_worms_by_ally: false,
            no_crate_probability: 0xFF,
            max_crate_count: 5,
            sd_disables_worm_select: true,
            sd_worm_damage_per_turn: 5,
            phased_worms_allied: PhasedWorms::default(),
            phased_worms_enemy: PhasedWorms::default(),
            circular_aim: false,
            anti_lock_aim: false,
            anti_lock_power: false,
            worm_select_no_end_hot_seat: false,
            worm_select_never_cancelled: false,
            batty_rope: false,
            rope_roll_drops: RopeRollDrops::default(),
            x_impact_loss_of_control: 0,
            keep_control_bump_head: false,
            keep_control_skimming: KeepControlAfterSkimming::default(),
            explosion_fall_damage: false,
            explosions_push_all: TriState::default(),
            undetermined_crates: TriState::default(),
            undetermined_fuses: TriState::default(),
            pause_timer_while_firing: true,
            loss_of_control_no_end_turn: false,
            weapon_use_no_end_turn: false,
            weapon_use_no_block: false,
            drill_imparts_velocity: TriState::default(),
            girder_radius_assist: false,
            petrol_turn_decay: 0x3332,
            petrol_touch_decay: 0x1E,
            max_flamelet_count: 0xC8,
            max_projectile_speed: Fixed::from_raw(0x20_0000),
            max_rope_speed: Fixed::from_raw(0x10_0000),
            max_jet_pack_speed: Fixed::from_raw(0x5_0000),
            game_engine_speed: Fixed::from_raw(0x1_0000),
            indian_rope_glitch: TriState::default(),
            herd_doubling_glitch: TriState::default(),
            jet_pack_bungee_glitch: true,
            angle_cheat_glitch: true,
            glide_glitch: true,
            skipwalking: Skipwalking::default(),
            block_roofing: BlockRoofing::default(),
            floating_weapon_glitch: true,
            rubberworm: RubberwormOptions::default(),
            terrain_overlap_glitch: TriState::default(),
            fractional_round_timer: false,
            auto_retreat: false,
            health_crates_cure_poison: HealthCratesCurePoison::default(),
            rw_kaos_mod: 0,
            sheep_heavens_gate: SheepHeavensGate::default(),
            conserve_instant_utilities: false,
            expedite_instant_utilities: false,
            double_time_stack_limit: 1,
        }
    }
}

/// Standard weapon settings present in every scheme version.
#[derive(Debug, Clone, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
pub struct StandardWeapons {
    pub bazooka: WeaponSettings,
    pub homing_missile: WeaponSettings,
    pub mortar: WeaponSettings,
    pub grenade: WeaponSettings,
    pub cluster_bomb: WeaponSettings,
    pub skunk: WeaponSettings,
    pub petrol_bomb: WeaponSettings,
    pub banana_bomb: WeaponSettings,
    pub handgun: WeaponSettings,
    pub shotgun: WeaponSettings,
    pub uzi: WeaponSettings,
    pub minigun: WeaponSettings,
    pub longbow: WeaponSettings,
    pub airstrike: WeaponSettings,
    pub napalm_strike: WeaponSettings,
    pub mine: WeaponSettings,
    pub fire_punch: WeaponSettings,
    pub dragon_ball: WeaponSettings,
    pub kamikaze: WeaponSettings,
    pub prod: WeaponSettings,
    pub battle_axe: WeaponSettings,
    pub blowtorch: WeaponSettings,
    pub pneumatic_drill: WeaponSettings,
    pub girder: WeaponSettings,
    pub ninja_rope: WeaponSettings,
    pub parachute: WeaponSettings,
    pub bungee: WeaponSettings,
    pub teleport: WeaponSettings,
    pub dynamite: WeaponSettings,
    pub sheep: WeaponSettings,
    pub baseball_bat: WeaponSettings,
    pub flame_thrower: WeaponSettings,
    pub homing_pigeon: WeaponSettings,
    pub mad_cow: WeaponSettings,
    pub holy_hand_grenade: WeaponSettings,
    pub old_woman: WeaponSettings,
    pub sheep_launcher: WeaponSettings,
    pub super_sheep: WeaponSettings,
    pub mole_bomb: WeaponSettings,
    pub jet_pack: WeaponSettings,
    pub low_gravity: WeaponSettings,
    pub laser_sight: WeaponSettings,
    pub fast_walk: WeaponSettings,
    pub invisibility: WeaponSettings,
    pub damage_x2: WeaponSettings,
}

/// Super weapon settings added by V2 schemes.
#[derive(Debug, Clone, PartialEq, Eq, Default, DekuRead, DekuWrite, DekuSize)]
pub struct SuperWeapons {
    pub freeze: WeaponSettings,
    pub super_banana_bomb: WeaponSettings,
    pub mine_strike: WeaponSettings,
    pub girder_starter_pack: WeaponSettings,
    pub earthquake: WeaponSettings,
    pub scales_of_justice: WeaponSettings,
    pub ming_vase: WeaponSettings,
    pub mikes_carpet_bomb: WeaponSettings,
    pub patsys_magic_bullet: WeaponSettings,
    pub indian_nuclear_test: WeaponSettings,
    pub select_worm: WeaponSettings,
    pub salvation_army: WeaponSettings,
    pub mole_squadron: WeaponSettings,
    pub mb_bomb: WeaponSettings,
    pub concrete_donkey: WeaponSettings,
    pub suicide_bomber: WeaponSettings,
    pub sheep_strike: WeaponSettings,
    pub mail_strike: WeaponSettings,
    pub armageddon: WeaponSettings,
}

fn read_v3_extended_options<R: deku::no_std_io::Read + deku::no_std_io::Seek>(
    reader: &mut deku::reader::Reader<R>,
    endian: deku::ctx::Endian,
) -> Result<ExtendedOptions, DekuError> {
    let mut bytes = ExtendedOptions::default_bytes();
    let mut len = 0;

    while len < EXTENDED_OPTIONS_SIZE && !reader.end() {
        reader.read_bytes(1, &mut bytes[len..len + 1], deku::ctx::Order::Msb0)?;
        len += 1;
    }

    if !reader.end() {
        return Err(DekuError::Parse("too many V3 extended option bytes"));
    }

    ExtendedOptions::from_bytes_with_endian(bytes.as_slice(), endian)
}

/// Fully typed .wsc scheme.
#[deku_derive(DekuRead, DekuWrite)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[deku(
    magic = b"SCHM",
    ctx = "endian: deku::ctx::Endian",
    ctx_default = "deku::ctx::Endian::Little"
)]
pub struct Scheme {
    #[deku(temp, temp_value = "SchemeVersion::V3")]
    version: SchemeVersion,
    pub options: SchemeOptions,
    pub weapons: StandardWeapons,
    #[deku(
        cond = "*version != SchemeVersion::V1",
        default = "SuperWeapons::default()"
    )]
    pub super_weapons: SuperWeapons,
    #[deku(
        cond = "*version == SchemeVersion::V3",
        default = "ExtendedOptions::default()",
        ctx = "endian",
        reader = "read_v3_extended_options(deku::reader, endian)"
    )]
    pub extended_options: ExtendedOptions,
}

impl Scheme {
    /// Parse a canonical V3 payload section without the `SCHM` header.
    pub fn from_payload_bytes(payload: &[u8]) -> Result<Self, DekuError> {
        if payload.len() != SCHEME_PAYLOAD_V3 {
            return Err(DekuError::Parse("scheme payload must be full V3 size"));
        }

        let mut bytes = Vec::with_capacity(SCHEME_HEADER_SIZE + SCHEME_PAYLOAD_V3);
        bytes.extend_from_slice(&SCHEME_MAGIC);
        bytes.push(SchemeVersion::V3 as u8);
        bytes.extend_from_slice(payload);
        Self::try_from(bytes.as_slice())
    }

    /// Serialize the V3 typed payload section without the `SCHM` header.
    pub fn payload_bytes(&self) -> [u8; SCHEME_PAYLOAD_V3] {
        let bytes = self.to_bytes();
        bytes[SCHEME_HEADER_SIZE..]
            .try_into()
            .expect("canonical V3 payload should be full-sized")
    }

    /// Serialized file size.
    pub fn file_size(&self) -> usize {
        SCHEME_HEADER_SIZE + SCHEME_PAYLOAD_V3
    }

    /// Serialize to .wsc format as a full V3 file.
    pub fn to_bytes(&self) -> Vec<u8> {
        <Self as DekuContainerWrite>::to_bytes(self).expect("scheme should serialize")
    }

    /// Load a scheme file from disk.
    pub fn from_file(path: &std::path::Path) -> Result<Self, SchemeFileError> {
        let data = std::fs::read(path).map_err(SchemeFileError::Io)?;
        Self::try_from(data.as_slice()).map_err(SchemeFileError::Parse)
    }

    /// Save a scheme file to disk.
    pub fn to_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        std::fs::write(path, self.to_bytes())
    }
}

/// Error type for file-based scheme operations.
#[derive(Debug)]
pub enum SchemeFileError {
    Io(std::io::Error),
    Parse(DekuError),
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

    fn scheme_bytes(version: SchemeVersion, payload: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(version as u8);
        data.extend_from_slice(payload);
        data
    }

    fn v1_payload(options: SchemeOptions, weapons: StandardWeapons) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&options.to_bytes().unwrap());
        payload.extend_from_slice(&weapons.to_bytes().unwrap());
        payload
    }

    fn v2_payload(
        options: SchemeOptions,
        weapons: StandardWeapons,
        super_weapons: SuperWeapons,
    ) -> Vec<u8> {
        let mut payload = v1_payload(options, weapons);
        payload.extend_from_slice(&super_weapons.to_bytes().unwrap());
        payload
    }

    fn default_v1_payload() -> Vec<u8> {
        v1_payload(SchemeOptions::default(), StandardWeapons::default())
    }

    fn default_v2_payload() -> Vec<u8> {
        v2_payload(
            SchemeOptions::default(),
            StandardWeapons::default(),
            SuperWeapons::default(),
        )
    }

    #[test]
    fn ammunition_maps_raw_values() {
        let finite = WeaponSettings::try_from([9, 0, 0, 0].as_slice()).unwrap();
        assert_eq!(finite.ammo, Ammunition::Finite(9));

        let infinite_10 = WeaponSettings::try_from([10, 0, 0, 0].as_slice()).unwrap();
        assert_eq!(infinite_10.ammo, Ammunition::Infinite);

        let infinite_high = WeaponSettings::try_from([0x80, 0, 0, 0].as_slice()).unwrap();
        assert_eq!(infinite_high.ammo, Ammunition::Infinite);

        let weapon = WeaponSettings {
            ammo: Ammunition::finite(7).unwrap(),
            ..Default::default()
        };
        assert_eq!(weapon.to_bytes().unwrap()[0], 7);

        let weapon = WeaponSettings {
            ammo: Ammunition::Infinite,
            ..Default::default()
        };
        assert_eq!(weapon.to_bytes().unwrap()[0], 10);

        let weapon = WeaponSettings {
            ammo: Ammunition::Finite(11),
            ..Default::default()
        };
        assert!(weapon.to_bytes().is_err());
    }

    #[test]
    fn parse_v1_synthetic() {
        let data = scheme_bytes(SchemeVersion::V1, &default_v1_payload());
        let scheme = Scheme::try_from(data.as_slice()).unwrap();
        assert_eq!(scheme.to_bytes()[4], SchemeVersion::V3 as u8);
        assert_eq!(scheme.super_weapons, SuperWeapons::default());
        assert_eq!(scheme.extended_options, ExtendedOptions::default());
        assert_eq!(
            Scheme::try_from(scheme.to_bytes().as_slice()).unwrap(),
            scheme
        );
    }

    #[test]
    fn parse_v2_synthetic() {
        let data = scheme_bytes(SchemeVersion::V2, &default_v2_payload());
        let scheme = Scheme::try_from(data.as_slice()).unwrap();
        assert_eq!(scheme.to_bytes()[4], SchemeVersion::V3 as u8);
        assert_eq!(scheme.extended_options, ExtendedOptions::default());
        assert_eq!(
            Scheme::try_from(scheme.to_bytes().as_slice()).unwrap(),
            scheme
        );
    }

    #[test]
    fn roundtrip_synthetic() {
        let data = scheme_bytes(SchemeVersion::V1, &default_v1_payload());
        let scheme = Scheme::try_from(data.as_slice()).unwrap();
        assert_eq!(scheme.to_bytes()[4], SchemeVersion::V3 as u8);
        assert_eq!(
            Scheme::try_from(scheme.to_bytes().as_slice()).unwrap(),
            scheme
        );
    }

    #[test]
    fn error_too_short() {
        assert!(Scheme::try_from(b"SCH".as_slice()).is_err());
    }

    #[test]
    fn error_bad_magic() {
        let mut data = vec![b'N', b'O', b'P', b'E', 0x01];
        data.extend_from_slice(&default_v1_payload());
        assert!(matches!(
            Scheme::try_from(data.as_slice()),
            Err(DekuError::Parse(_))
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
            Scheme::try_from(data.as_slice()),
            Err(DekuError::Incomplete(_))
        ));
    }

    #[test]
    fn v3_variable_length_padded_with_defaults() {
        let mut super_weapons = SuperWeapons::default();
        super_weapons.armageddon.crate_probability = 0xAA;
        let payload = v2_payload(
            SchemeOptions::default(),
            StandardWeapons::default(),
            super_weapons.clone(),
        );
        let data = scheme_bytes(SchemeVersion::V3, &payload);

        let scheme = Scheme::try_from(data.as_slice()).expect("should accept short V3");
        assert_eq!(scheme.super_weapons, super_weapons);
        assert_eq!(scheme.extended_options, ExtendedOptions::default());
    }

    #[test]
    fn v3_full_length_accepted() {
        let mut payload = default_v2_payload();
        payload.extend_from_slice(&ExtendedOptions::default_bytes());
        let data = scheme_bytes(SchemeVersion::V3, &payload);

        let scheme = Scheme::try_from(data.as_slice()).expect("should accept full V3");
        assert_eq!(scheme.extended_options, ExtendedOptions::default());
    }

    #[test]
    fn v3_too_short_rejected() {
        let data = scheme_bytes(SchemeVersion::V3, &default_v1_payload());

        assert!(Scheme::try_from(data.as_slice()).is_err());
    }

    #[test]
    fn default_scheme_is_valid_v3() {
        let scheme = Scheme::default();
        assert_eq!(scheme.extended_options, ExtendedOptions::default());
        assert!(ExtendedOptions::validate_bytes(
            &ExtendedOptions::default_bytes()
        ));
        assert_eq!(scheme.payload_bytes().len(), SCHEME_PAYLOAD_V3);
    }

    #[test]
    fn from_payload_bytes_roundtrips_canonical_payload() {
        let scheme = Scheme::default();

        assert_eq!(
            Scheme::from_payload_bytes(&scheme.payload_bytes()).unwrap(),
            scheme
        );
        assert!(Scheme::from_payload_bytes(&[]).is_err());
    }

    #[test]
    fn payload_bytes_pads_legacy_inputs_for_runtime() {
        let mut standard_weapons = StandardWeapons::default();
        standard_weapons.damage_x2.crate_probability = 0xAA;
        let v1 = scheme_bytes(
            SchemeVersion::V1,
            &v1_payload(SchemeOptions::default(), standard_weapons.clone()),
        );
        let v1 = Scheme::try_from(v1.as_slice()).unwrap();
        assert_eq!(v1.weapons, standard_weapons);
        assert_eq!(v1.super_weapons, SuperWeapons::default());
        assert_eq!(v1.extended_options, ExtendedOptions::default());
        assert_eq!(v1.payload_bytes().len(), SCHEME_PAYLOAD_V3);

        let mut super_weapons = SuperWeapons::default();
        super_weapons.armageddon.crate_probability = 0xBB;
        let v2 = scheme_bytes(
            SchemeVersion::V2,
            &v2_payload(
                SchemeOptions::default(),
                StandardWeapons::default(),
                super_weapons.clone(),
            ),
        );
        let v2 = Scheme::try_from(v2.as_slice()).unwrap();
        assert_eq!(v2.super_weapons, super_weapons);
        assert_eq!(v2.extended_options, ExtendedOptions::default());
        assert_eq!(v2.payload_bytes().len(), SCHEME_PAYLOAD_V3);
    }

    #[test]
    fn v3_writes_through_last_non_default_extended_byte() {
        let mut scheme = Scheme::default();
        scheme.extended_options.double_time_stack_limit = 2;

        assert_eq!(scheme.payload_bytes().last(), Some(&2));
    }

    #[test]
    fn extended_option_nested_defaults_match_rom_defaults() {
        let defaults = ExtendedOptions::default();

        assert_eq!(TriState::default(), TriState::Default);
        assert_eq!(PhasedWorms::default(), PhasedWorms::Off);
        assert_eq!(RopeRollDrops::default(), RopeRollDrops::Disabled);
        assert_eq!(
            KeepControlAfterSkimming::default(),
            KeepControlAfterSkimming::LoseControl
        );
        assert_eq!(Skipwalking::default(), Skipwalking::Possible);
        assert_eq!(BlockRoofing::default(), BlockRoofing::Allow);
        assert_eq!(
            RubberWormGravityType::default(),
            RubberWormGravityType::Unmodified
        );
        assert_eq!(
            HealthCratesCurePoison::default(),
            HealthCratesCurePoison::CollectingWormTeam
        );
        assert_eq!(SheepHeavensGate::default().bits(), 0x07);
        assert!(
            SheepHeavensGate::default().contains(SheepHeavensGate::SHEEP_EXPLODE_FROM_ALL_CRATES)
        );
        assert!(SheepHeavensGate::default().contains(SheepHeavensGate::EXTENDED_SHEEP_FUSE_TIME));
        assert!(
            SheepHeavensGate::default()
                .contains(SheepHeavensGate::BOOST_SHEEP_WEAPON_CRATE_PROBABILITY)
        );
        assert_eq!(RubberwormOptions::default(), defaults.rubberworm);
        assert_eq!(SheepHeavensGate::default(), defaults.sheep_heavens_gate);
    }
}
