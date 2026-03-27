/// Opaque sound ID. WA uses a wider range than just the known SFX enum
/// (e.g., speech/voice lines use IDs above 126). This newtype wraps the
/// raw u32 value and can be constructed from [`KnownSoundId`] variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SoundId(pub u32);

impl From<KnownSoundId> for SoundId {
    #[inline]
    fn from(known: KnownSoundId) -> Self {
        Self(known as u32)
    }
}

/// Known sound effect IDs. Range 1-126, contiguous.
///
/// Source: wkJellyWorm Constants.h
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum KnownSoundId {
    Morse = 1,
    CrowdPart1 = 2,
    CrowdPart2 = 3,
    NukePart1 = 4,
    NukePart2 = 5,
    FrenchAnthem = 6,
    IndianAnthem = 7,
    Twang1 = 8,
    Twang2 = 9,
    Twang3 = 10,
    Twang4 = 11,
    Twang5 = 12,
    Twang6 = 13,
    Cough1 = 14,
    Cough2 = 15,
    Cough3 = 16,
    Cough4 = 17,
    Cough5 = 18,
    Cough6 = 19,
    DonorCardAppears = 20,
    DonorCardCollect = 21,
    FlameThrowerAttack = 22,
    FlameThrowerLoop = 23,
    Freeze = 24,
    UnFreeze = 25,
    JetPackStart = 26,
    JetPackFinish = 27,
    LongbowImpact = 28,
    LongbowRelease = 29,
    NukeFlash = 30,
    ScalesOfJustice = 31,
    UnderWaterLoop = 32,
    VikingAxeImpact = 33,
    VikingAxeRelease = 34,
    WormLanding = 35,
    JetPackLoop1 = 36,
    JetPackLoop2 = 37,
    MoleBombDiggingLoop = 38,
    MoleBombWalkLoop = 39,
    MoleBombSqueak = 40,
    SkunkGasLoop = 41,
    SkunkWalkLoop = 42,
    SkunkSqueak = 43,
    Armageddon = 44,
    StartRound = 45,
    CameraPan = 46,
    WalkCompress = 47,
    WalkExpand = 48,
    CowMoo = 49,
    SheepBaa = 50,
    PigeonCoo = 51,
    AirStrike = 52,
    BlowTorch = 53,
    WormPop = 54,
    Sizzle = 55,
    SnotPlop = 56,
    Splash = 57,
    Splish = 58,
    Petrol = 59,
    WormBurned = 60,
    SalvationArmy = 61,
    MagicBullet = 62,
    WormSpring = 63,
    VaseSmash = 64,
    OldWoman = 65,
    Fuse = 66,
    Teleport = 67,
    Communicator = 68,
    Explosion1 = 69,
    Explosion2 = 70,
    Explosion3 = 71,
    ThrowPowerup = 72,
    ThrowRelease = 73,
    RocketPowerup = 74,
    RocketRelease = 75,
    SuperSheepRelease = 76,
    SuperSheepWhoosh = 77,
    UziFire = 78,
    MinigunFire = 79,
    ShotgunFire = 80,
    ShotgunReload = 81,
    HandgunFire = 82,
    Ricochet = 83,
    Drill = 84,
    DrillImpact = 85,
    NinjaRopeFire = 86,
    NinjaRopeImpact = 87,
    MineArm = 88,
    MineTick = 89,
    MineDud = 90,
    WeaponHoming = 91,
    PauseTick = 92,
    TimerTick = 93,
    SuddenDeath = 94,
    KamikazeRelease = 95,
    BaseballBatRelease = 96,
    BaseballBatImpact = 97,
    BaseballBatJingle = 98,
    DragonBallRelease = 99,
    DragonBallImpact = 100,
    FirePunchImpact = 101,
    HolyDonkey = 102,
    HolyDonkeyImpact = 103,
    HolyGrenade = 104,
    HolyGrenadeImpact = 105,
    FrozenWormImpact = 106,
    MineImpact = 107,
    WormImpact = 108,
    CrossImpact = 109,
    CrateImpact = 110,
    BananaImpact = 111,
    GirderImpact = 112,
    GrenadeImpact = 113,
    OilDrumImpact = 114,
    CratePop = 115,
    KeyClick = 116,
    KeyErase = 117,
    WormSelect = 118,
    CursorSelect = 119,
    WarningBeep = 120,
    LoadingTick = 121,
    TeamDrop = 122,
    TeamBounce = 123,
    Collect = 124,
    ThrowPowerdown = 125,
    RocketPowerdown = 126,
}

impl KnownSoundId {
    pub const MIN: u32 = 1;
    pub const MAX: u32 = 126;
}

impl TryFrom<u32> for KnownSoundId {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(unsafe { core::mem::transmute(value) })
        } else {
            Err(value)
        }
    }
}
