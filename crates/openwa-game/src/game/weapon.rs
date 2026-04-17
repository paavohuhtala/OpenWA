use core::ffi::c_char;

use openwa_core::fixed::Fixed;

/// Weapon types. Contiguous range 0-70.
///
/// Source: wkJellyWorm Constants.h
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd)]
#[repr(u32)]
pub enum Weapon {
    None = 0,
    Bazooka = 1,
    HomingMissile = 2,
    Mortar = 3,
    HomingPigeon = 4,
    SheepLauncher = 5,
    Grenade = 6,
    ClusterBomb = 7,
    BananaBomb = 8,
    BattleAxe = 9,
    Earthquake = 10,
    Shotgun = 11,
    Handgun = 12,
    Uzi = 13,
    Minigun = 14,
    Longbow = 15,
    FirePunch = 16,
    DragonBall = 17,
    Kamikaze = 18,
    SuicideBomber = 19,
    Prod = 20,
    Dynamite = 21,
    Mine = 22,
    Sheep = 23,
    SuperSheep = 24,
    AquaSheep = 25,
    MoleBomb = 26,
    AirStrike = 27,
    NapalmStrike = 28,
    MailStrike = 29,
    MineStrike = 30,
    MoleSquadron = 31,
    BlowTorch = 32,
    PneumaticDrill = 33,
    Girder = 34,
    BaseballBat = 35,
    GirderPack = 36,
    NinjaRope = 37,
    Bungee = 38,
    Parachute = 39,
    Teleport = 40,
    ScalesOfJustice = 41,
    SuperBanana = 42,
    HolyGrenade = 43,
    FlameThrower = 44,
    SalvationArmy = 45,
    MbBomb = 46,
    PetrolBomb = 47,
    Skunk = 48,
    MingVase = 49,
    SheepStrike = 50,
    CarpetBomb = 51,
    MadCow = 52,
    OldWoman = 53,
    Donkey = 54,
    NuclearTest = 55,
    Armageddon = 56,
    SkipGo = 57,
    Surrender = 58,
    SelectWorm = 59,
    Freeze = 60,
    MagicBullet = 61,
    JetPack = 62,
    LowGravity = 63,
    FastWalk = 64,
    LaserSight = 65,
    Invisibility = 66,
    DamageX2 = 67,
    CrateSpy = 68,
    DoubleTurnTime = 69,
    CrateShower = 70,
}

impl Weapon {
    pub const COUNT: u32 = 71;
}

impl TryFrom<u32> for Weapon {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value < Self::COUNT {
            Ok(unsafe { core::mem::transmute(value) })
        } else {
            Err(value)
        }
    }
}

// ============================================================
// WeaponSpawnData — launch parameters passed to CTaskMissile ctor
// ============================================================

/// Spawn parameters for a weapon projectile (0x2C = 44 bytes, 11 DWORDs).
///
/// Built on the stack by fire sub-functions (ProjectileFire, GrenadeFire, etc.)
/// and passed as param_4 to CTaskMissile::Constructor. Copied verbatim into
/// CTaskMissile at offset 0x130 (spawn_params field).
///
/// Source: runtime inspection via debug CLI + CTaskMissile constructor decompilation.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct WeaponSpawnData {
    /// [0] Team index of the worm that fired this projectile.
    pub owner_id: u32,
    /// [1] Unknown — observed as 1.
    pub _unknown_04: u32,
    /// [2] Fixed16.16 X position at launch.
    pub spawn_x: Fixed,
    /// [3] Fixed16.16 Y position at launch.
    pub spawn_y: Fixed,
    /// [4] Fixed16.16 horizontal velocity. Copied to CGameTask.speed_x.
    pub initial_speed_x: Fixed,
    /// [5] Fixed16.16 vertical velocity. Copied to CGameTask.speed_y.
    pub initial_speed_y: Fixed,
    /// [6] Fixed16.16 aim cursor X at time of fire.
    pub cursor_x: Fixed,
    /// [7] Fixed16.16 aim cursor Y at time of fire.
    pub cursor_y: Fixed,
    /// [8] Index within a cluster volley (0 for single shot, N for Nth sub-pellet).
    /// Determines which half of weapon_data is copied to render_data.
    pub pellet_index: u32,
    /// [9] Fallback timer — copied to render_data[0x19] if that field was zero.
    pub fallback_timer: u32,
    /// [10] Fallback param — copied to render_data[0x11] if that field was zero.
    pub fallback_param: u32,
}
const _: () = assert!(core::mem::size_of::<WeaponSpawnData>() == 0x2C);

// ============================================================
// WeaponEntry — per-weapon data in the weapon table (0x1D0 bytes)
// ============================================================

/// Per-weapon data entry in the weapon table (0x1D0 = 464 bytes).
///
/// Top-level weapon fire type (WeaponEntry+0x30).
///
/// Determines which sub-function handles the weapon fire in FireWeapon dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FireType {
    /// Projectile weapons (Bazooka, Grenade, Shotgun, etc.).
    /// Sub-dispatched by `fire_method`.
    Projectile = 1,
    /// Rope-based weapons (Ninja Rope, Bungee).
    /// Sub-dispatched by `fire_method`.
    Rope = 2,
    /// Strike weapons (Air Strike, Napalm Strike, Mail Strike, etc.).
    /// Uses `special_subtype` as parameter data (not a subtype selector).
    Strike = 3,
    /// Special weapons (melee, utility, powerups).
    /// Sub-dispatched by `special_subtype`.
    Special = 4,
}

impl TryFrom<i32> for FireType {
    type Error = i32;
    fn try_from(v: i32) -> Result<Self, i32> {
        match v {
            1 => Ok(Self::Projectile),
            2 => Ok(Self::Rope),
            3 => Ok(Self::Strike),
            4 => Ok(Self::Special),
            _ => Err(v),
        }
    }
}

/// Fire method for projectile (type 1) and rope (type 2) weapons (WeaponEntry+0x38).
///
/// Selects which sub-function creates the projectile or rope entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FireMethod {
    /// PlacedExplosive: usercall, places mine/dynamite at worm position.
    PlacedExplosive = 1,
    /// ProjectileFire: stdcall, fires projectile with spread/rotation.
    ProjectileFire = 2,
    /// CreateWeaponProjectile: thiscall, allocates CTaskMissile.
    CreateWeaponProjectile = 3,
    /// CreateArrow: thiscall, allocates CTaskArrow (Shotgun, Longbow).
    CreateArrow = 4,
}

impl TryFrom<i32> for FireMethod {
    type Error = i32;
    fn try_from(v: i32) -> Result<Self, i32> {
        match v {
            1 => Ok(Self::PlacedExplosive),
            2 => Ok(Self::ProjectileFire),
            3 => Ok(Self::CreateWeaponProjectile),
            4 => Ok(Self::CreateArrow),
            _ => Err(v),
        }
    }
}

/// Special weapon subtype — the raw `sub34` value from the weapon table,
/// used directly as the switch discriminant in FireWeapon (0x51EE60) case 4.
/// Names are based on confirmed weapon->sub34 mappings from replay test logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum SpecialFireSubtype {
    /// FirePunch weapon (id=16, sub34=1).
    FirePunch = 1,
    /// BaseballBat weapon (id=35, sub34=2). Handler calls PneumaticDrill/SpecialImpact logic.
    BaseballBat = 2,
    /// DragonBall weapon (id=17, sub34=3). Handler allocates CTaskGirder.
    DragonBall = 3,
    /// Kamikaze weapon (id=18, sub34=4).
    Kamikaze = 4,
    /// SuicideBomber weapon (id=19, sub34=5).
    SuicideBomber = 5,
    /// Unknown — no weapon observed using sub34=6 in replay tests.
    Unknown6 = 6,
    // 7: unknown
    /// PneumaticDrill weapon (id=33, sub34=8).
    PneumaticDrill = 8,
    Prod = 9,
    /// Teleport weapon (id=40, sub34=10).
    Teleport = 10,
    /// Blowtorch weapon (id=32, sub34=11).
    Blowtorch = 11,
    /// Parachute weapon (id=39, sub34=12).
    Parachute = 12,
    /// Surrender weapon (id=58, sub34=13). Sends message 0x2B (TaskMessage::Surrender).
    Surrender = 13,
    MailMineMole = 14,
    // 15: unknown
    /// NuclearTest weapon (id=55, sub34=16).
    NuclearTest = 16,
    /// Girder/GirderPack weapons (id=34/36, sub34=17).
    Girder = 17,
    /// Unknown — no weapon observed using sub34=18 in replay tests.
    Unknown18 = 18,
    SkipGo = 19,
    /// Freeze weapon (id=60, sub34=20). Sends message 0x29 (TaskMessage::Freeze).
    Freeze = 20,
    SelectWorm = 21,
    /// ScalesOfJustice weapon (id=41, sub34=22).
    ScalesOfJustice = 22,
    /// JetPack weapon (id=62, sub34=23).
    JetPack = 23,
    /// Armageddon weapon (id=56, sub34=24).
    Armageddon = 24,
}

impl TryFrom<i32> for SpecialFireSubtype {
    type Error = i32;
    fn try_from(v: i32) -> Result<Self, i32> {
        match v {
            1 => Ok(Self::FirePunch),
            2 => Ok(Self::BaseballBat),
            3 => Ok(Self::DragonBall),
            4 => Ok(Self::Kamikaze),
            5 => Ok(Self::SuicideBomber),
            6 => Ok(Self::Unknown6),
            8 => Ok(Self::PneumaticDrill),
            9 => Ok(Self::Prod),
            10 => Ok(Self::Teleport),
            11 => Ok(Self::Blowtorch),
            12 => Ok(Self::Parachute),
            13 => Ok(Self::Surrender),
            14 => Ok(Self::MailMineMole),
            16 => Ok(Self::NuclearTest),
            17 => Ok(Self::Girder),
            18 => Ok(Self::Unknown18),
            19 => Ok(Self::SkipGo),
            20 => Ok(Self::Freeze),
            21 => Ok(Self::SelectWorm),
            22 => Ok(Self::ScalesOfJustice),
            23 => Ok(Self::JetPack),
            24 => Ok(Self::Armageddon),
            _ => Err(v),
        }
    }
}

/// 71 standard entries (indices 0..70), matching the `Weapon` enum.
/// Source: wkJellyWorm/src/CustomWeapons.h (WeaponStruct).
///
/// Known fields from wkJellyWorm and InitWeaponTable (0x53CAB0) analysis.
/// wkJellyWorm copies entire entries via memcpy when creating custom weapons.
#[repr(C)]
pub struct WeaponEntry {
    /// +0x00: Pointer to primary weapon name string.
    pub name1: *const c_char,
    /// +0x04: Pointer to secondary weapon name string.
    pub name2: *const c_char,
    /// +0x08: Panel state (init: 0xFFFFFFFF). wkJellyWorm calls this `panelRow`.
    pub panel_state: i32,
    /// +0x0C: Requires aiming (1 for aimed weapons like Bazooka, 0 for non-aimed like Earthquake).
    /// Runtime-observed: 1 for Bazooka/Grenade/Shotgun, 0 for AirStrike/HomingMissile/Teleport.
    pub requires_aiming: i32,
    /// +0x10: Weapon defined flag. Nonzero = weapon exists in table.
    /// Checked by DDGame__CheckWeaponAvail to determine if weapon is valid.
    pub defined: i32,
    /// +0x14: Shot count per use (1 for most, 2 for Shotgun/Longbow, 5 for GirderPack).
    pub shot_count: i32,
    /// +0x18: Unknown flag (0 or 1). 0 for NinjaRope/Bungee/Parachute/SelectWorm/JetPack/powerups.
    pub _unknown_18: i32,
    /// +0x1C: Retreat timer in ms (0xBB8=3000 for most, 0x1388=5000 for Dynamite/Mine/MingVase,
    /// -1 for non-retreating, -1000 for PneumaticDrill/Teleport, 0 for utility/powerups).
    pub retreat_time: i32,
    /// +0x20: Creates projectile flag (1 for weapons that fire a physical object, 0 for utility).
    /// 0 for Prod/Girder/NinjaRope/Parachute/Teleport/utility weapons.
    pub creates_projectile: i32,
    /// +0x24: Availability flag. Init: 0xFFFFFFFF, then set to 0 (unavailable)
    /// or 1 (available) per weapon. Weapon::None, SkipGo, Surrender default to 0.
    pub availability: i32,
    /// +0x28: Enabled flag (init: 1).
    pub enabled: i32,
    /// +0x2C-0x2F: Unknown.
    pub _unknown_2c: [u8; 4],
    /// +0x30: Weapon fire type (1=projectile, 2=rope, 3=grenade, 4=special).
    /// Read by FireWeapon to dispatch to the correct handler.
    pub fire_type: i32,
    /// +0x34: Fire subtype for weapon types 3 (grenade/mortar) and 4 (special).
    pub special_subtype: i32,
    /// +0x38: Fire subtype for weapon types 1 (projectile) and 2 (rope).
    pub fire_method: i32,
    /// +0x3C: Fire parameters sub-structure. Pointer to this field is passed
    /// to fire sub-functions (PlacedExplosive, Projectile, CreateWeaponProjectile, etc.).
    pub fire_params: WeaponFireParams,
}
const _: () = assert!(core::mem::size_of::<WeaponEntry>() == 0x1D0);

/// Weapon fire parameters — embedded at WeaponEntry+0x3C (0x194 = 404 bytes, 101 DWORDs).
///
/// Pointer to this struct is passed to all fire dispatch sub-functions.
/// The first 94 DWORDs (0x178 bytes) are copied verbatim into CTaskMissile.weapon_data
/// by CTaskMissile__Constructor. The remaining 7 DWORDs are WeaponEntry-only metadata.
///
/// For single-shot projectiles, CTaskMissile.render_data[N] = weapon_data[N+3].
/// For cluster sub-pellets, render_data[N] = weapon_data[N+52].
///
/// Field names confirmed by cross-referencing live memory dumps (debug CLI),
/// CTaskMissile constructor decompilation, and render_data physics usage.
#[repr(C)]
pub struct WeaponFireParams {
    /// [0] +0x3C: Polymorphic.
    /// Missile: pellet count (Bazooka=2, Mortar=2).
    /// Hitscan: shots per trigger (Shotgun=1, Handgun=6, Uzi=10, Minigun=20).
    /// Airstrike: number of projectiles (AirStrike=5).
    pub shot_count: i32,
    /// [1] +0x40: Polymorphic.
    /// Missile: spread angle for multi-pellet (Mortar=100).
    /// Hitscan: spread cone (Shotgun=500, Handgun=500).
    /// Airstrike: spacing (NapalmStrike=48).
    pub spread: i32,
    /// [2] +0x44: Polymorphic.
    /// Missile: cluster flag (Grenade=1, ClusterBomb=1, BananaBomb=1).
    /// Hitscan: fire delay between shots (Handgun=10, Uzi=15, Minigun=25).
    pub _fp_02: i32,
    /// [3] +0x48: Polymorphic.
    /// Missile: collision radius (Bazooka=2.10, HomingPigeon=66.1).
    /// Hitscan: always Fixed(1) — flag.
    pub collision_radius: Fixed,
    /// [4] +0x4C: Missile only: unknown (HomingMissile=5, HomingPigeon=25, Grenade=10).
    pub _fp_04: i32,
    /// [5] +0x50: Polymorphic.
    /// Missile: explosion damage (Bazooka=100, Grenade=100, Mortar=0).
    /// Hitscan: max range (all=66.1 as Fixed16.16).
    pub _fp_05: i32,
    /// [6] +0x54: Polymorphic.
    /// Missile: blast radius (Bazooka=50, Mortar=15, Grenade=50).
    /// Hitscan: impact radius (Shotgun=5, Minigun=20).
    pub _fp_06: i32,
    /// [7] +0x58: Hitscan only: unknown (Shotgun=100, Handgun=50, Uzi/Minigun=100).
    pub _fp_07: i32,
    /// [8] +0x5C: Hitscan only: damage per hit (Shotgun=25, Handgun/Uzi/Minigun=5).
    pub _fp_08: i32,
    /// [9] +0x60: Missile only: sprite/animation ID (Bazooka=48, Grenade=50, HomingPigeon=175).
    pub sprite_id: i32,
    /// [10] +0x64: Shot type/impact type (Missile: Bazooka=2, Grenade=1. Hitscan: Shotgun=2, Uzi=5).
    pub impact_type: i32,
    /// [11] +0x68: Polymorphic.
    /// Missile: unknown (Bazooka=131).
    /// Hitscan: max range in pixels (all=32767).
    pub _fp_11: i32,
    /// [12] +0x6C: Trail effect (Bazooka=50, Mortar=50, most weapons=0).
    pub trail_effect: i32,
    /// [13] +0x70: Gravity percentage (100=normal, 0=no gravity).
    /// → render_data[0x0C] → CGameTask.gravity_factor.
    pub gravity_pct: i32,
    /// [14] +0x74: Wind influence (Bazooka=50, AquaSheep=200).
    pub wind_influence: i32,
    /// [15] +0x78: Bounce percentage (100=normal elastic, 0=no bounce).
    /// → render_data[0x0D] → CGameTask.bounce_factor.
    pub bounce_pct: i32,
    /// [16] +0x7C: Unknown (Bazooka=100, most=0).
    pub _fp_16: i32,
    /// [17] +0x80: Unknown (AirStrike=10, MoleSquadron=50).
    pub _fp_17: i32,
    /// [18] +0x84: Friction percentage (100=normal, 0=no friction).
    /// → render_data[0x0F] → CGameTask.friction_factor.
    pub friction_pct: i32,
    /// [19] +0x88: Explosion delay/fuse timer (Grenade=5000, Dynamite=5000).
    pub explosion_delay: i32,
    /// [20] +0x8C: Fuse timer (Bazooka=9000, HomingMissile=10000, SheepLauncher=20000).
    pub fuse_timer: i32,
    /// [21-25] +0x90-0xA0: Various parameters. Remaining primary fields.
    pub _fp_21_25: [i32; 5],
    /// [26] +0xA4: Missile type discriminator.
    /// 0=None, 1=Homing, 2=Standard, 3=Sheep, 5=SheepLauncher/Cluster.
    /// → render_data[0x17] → CTaskMissile behavior dispatch.
    pub missile_type: i32,
    /// [27] +0xA8: Render size. Bazooka=64.0, Grenade≈66.1.
    pub render_size: Fixed,
    /// [28] +0xAC: Render timer/fuse (Bazooka=1, HomingMissile=58, HomingPigeon=175).
    pub render_timer: i32,
    /// [29-33] +0xB0-0xC0: Homing parameters (only nonzero for homing weapons).
    /// [29]=homing_strength, [30]=homing_sprite, [31]=homing_turn_rate,
    /// [32]=homing_accel, [33]=homing_speed.
    pub homing_params: [i32; 5],
    /// [34-36] +0xC4-0xCC: Additional parameters.
    pub _fp_34_36: [i32; 3],
    /// [37-51] +0xD0-0x10C: Reserved / weapon-specific extended params.
    pub _fp_37_51: [i32; 15],
    /// [52-93] +0x10C-0x1B0: Cluster sub-pellet parameters (mirrors primary [0-41]).
    /// When pellet_index > 0, render_data copies from here instead.
    pub cluster_params: [i32; 42],
    /// [94-100] +0x1B0-0x1CC: WeaponEntry-only metadata (NOT copied to CTaskMissile).
    /// [94]+0x1C8: Power percentage (Bazooka=100, Shotgun=10, NinjaRope=100).
    /// [95]+0x1CC: Unknown (Bazooka=100, Grenade=70, Shotgun=20).
    pub entry_metadata: [i32; 7],
}
const _: () = assert!(core::mem::size_of::<WeaponFireParams>() == 0x1D0 - 0x3C);

/// Weapon table — flat array of 71 entries, no header.
///
/// Allocated by `InitWeaponTable` (0x53CAB0), stored at DDGame+0x510.
/// Total size: 71 × 0x1D0 = 0x80B0 bytes.
#[repr(C)]
pub struct WeaponTable {
    /// Weapon entries array (71 standard weapons, indices 0..70).
    pub entries: [WeaponEntry; 71],
}
const _: () = assert!(core::mem::size_of::<WeaponTable>() == 71 * 0x1D0);
