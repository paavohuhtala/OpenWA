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
    /// +0x0C: Unknown.
    pub _unknown_0c: i32,
    /// +0x10: Weapon defined flag. Nonzero = weapon exists in table.
    /// Checked by DDGame__CheckWeaponAvail to determine if weapon is valid.
    pub defined: i32,
    /// +0x14-0x23: Unknown.
    pub _unknown_14: [u8; 0x24 - 0x14],
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
    pub fire_subtype_34: i32,
    /// +0x38: Fire subtype for weapon types 1 (projectile) and 2 (rope).
    pub fire_subtype_38: i32,
    /// +0x3C: Fire completion flag / params base.
    /// Set to 0 before dispatch, 1 after. Address also passed to fire handlers.
    pub fire_complete: i32,
    /// +0x40-0x1CF: Unknown fields.
    pub _unknown_40: [u8; 0x1D0 - 0x40],
}
const _: () = assert!(core::mem::size_of::<WeaponEntry>() == 0x1D0);

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
