use core::ffi::c_char;

/// Weapon types. Contiguous range 0-70.
///
/// Source: wkJellyWorm Constants.h
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
// WeaponEntry — per-weapon data in the weapon table (0x1D0 bytes)
// ============================================================

/// Per-weapon data entry in the weapon table (0x1D0 = 464 bytes).
///
/// 71 standard entries (indices 0..70), matching the `Weapon` enum.
/// Source: wkJellyWorm/src/CustomWeapons.h (WeaponStruct).
///
/// Only 3 fields are confirmed; the rest is opaque. wkJellyWorm copies
/// entire entries via memcpy when creating custom weapons.
#[repr(C)]
pub struct WeaponEntry {
    /// +0x00: Pointer to primary weapon name string.
    /// Non-null means the weapon is defined/available.
    pub name1: *const c_char,
    /// +0x04: Pointer to secondary weapon name string.
    pub name2: *const c_char,
    /// +0x08: Weapon panel row index (0..12).
    pub panel_row: i32,
    /// +0x0C: Unknown.
    pub _unknown_0c: i32,
    /// +0x10-0x1CF: Unknown fields (113 × i32).
    pub _unknown_10: [u8; 0x1D0 - 0x10],
}
const _: () = assert!(core::mem::size_of::<WeaponEntry>() == 0x1D0);

/// Weapon table header (0x10 = 16 bytes before the first entry).
///
/// The table is allocated by `InitWeaponTable` (0x53CAB0) and stored
/// at DDGame+0x510. Layout: 16-byte header + 71 × WeaponEntry.
#[repr(C)]
pub struct WeaponTable {
    /// +0x00-0x0F: Header (purpose unknown).
    pub _header: [u8; 0x10],
    /// +0x10: Weapon entries array (71 standard weapons).
    pub entries: [WeaponEntry; 71],
}
const _: () = assert!(
    core::mem::size_of::<WeaponTable>() == 0x10 + 71 * 0x1D0
);
