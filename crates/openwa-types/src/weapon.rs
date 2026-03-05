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
