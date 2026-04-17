//! Weapon ID and fire-dispatch enums.
//!
//! Pure, platform-independent data: the `Weapon` ID space (0..70) and the
//! FireType / FireMethod / SpecialFireSubtype discriminants used by WA's
//! FireWeapon dispatch.
//!
//! The runtime-layout structs that hold this data in memory (`WeaponEntry`,
//! `WeaponFireParams`, `WeaponTable`) live in `openwa-game::game::weapon`
//! because they contain `*const c_char` fields whose size is 32-bit on our
//! target but would differ on other platforms.

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
            Ok(unsafe { core::mem::transmute::<u32, Self>(value) })
        } else {
            Err(value)
        }
    }
}

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
