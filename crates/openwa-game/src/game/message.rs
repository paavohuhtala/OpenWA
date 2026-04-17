/// Inter-task message types for the game's event/message passing system.
///
/// Tasks communicate by sending these messages through the hierarchy.
/// Note: there are gaps in the numbering (10, 24-25, 65-66, 82-83, 87, 95).
///
/// Source: wkJellyWorm Constants.h
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TaskMessage {
    None = 0,
    FrameStart = 1,
    FrameFinish = 2,
    RenderScene = 3,
    ProcessInput = 4,
    UpdateNonCritical = 5,
    MachineFinished = 6,
    CrateCollected = 7,
    StateChecksum = 8,
    MachineReady = 9,
    // gap: 10 is unused
    WormDrowned = 11,
    FrameNumber = 12,
    MachineQuit = 13,
    EnableCheat = 14,
    PlayerChat = 15,
    CameraAuto = 16,
    CursorMoved = 17,
    GirderChanged = 18,
    StrikeChanged = 19,
    TeamVictory = 20,
    GameOver = 21,
    ExitMode = 22,
    Hurry = 23,
    // gap: 24-25 unused
    ThinkingShow = 26,
    ThinkingHide = 27,
    Explosion = 28,
    ExplosionReport = 29,
    MoveLeft = 30,
    MoveRight = 31,
    MoveUp = 32,
    MoveDown = 33,
    FaceLeft = 34,
    FaceRight = 35,
    Jump = 36,
    JumpUp = 37,
    FireWeapon = 38,
    ReleaseWeapon = 39,
    SkipGo = 40,
    Freeze = 41,
    // gap: 42 unused
    Surrender = 43,
    DetonateWeapon = 44,
    MoveWeaponLeft = 45,
    MoveWeaponRight = 46,
    SelectFuse = 47,
    SelectHerd = 48,
    SelectBounce = 49,
    SelectCursor = 50,
    SelectWeapon = 51,
    StartTurn = 52,
    PauseTurn = 53,
    ResumeTurn = 54,
    FinishTurn = 55,
    TurnStarted = 56,
    TurnFinished = 57,
    SuddenDeath = 58,
    DamageWorms = 59,
    RetreatStarted = 60,
    RetreatFinished = 61,
    ApplyPoison = 62,
    SetWorm = 63,
    KillWorm = 64,
    // gap: 65-66 unused
    AdvanceWorm = 67,
    ShowDamage = 68,
    EnableWeapons = 69,
    DisableWeapons = 70,
    WormMoved = 71,
    WormDamaged = 72,
    WeaponReleased = 73,
    WeaponFinished = 74,
    SpecialImpact = 75,
    WeaponCreated = 76,
    WeaponHoming = 77,
    WeaponDestroyed = 78,
    WeaponClaimControl = 79,
    WeaponReleaseControl = 80,
    PoisonWorm = 81,
    // gap: 82-83 unused
    SetWind = 84,
    GameText = 85,
    CreateAnimation = 86,
    // gap: 87 unused
    BringForward = 88,
    RaiseWater = 89,
    NukeBlast = 90,
    Armageddon = 91,
    DetonateCrate = 92,
    Earthquake = 93,
    ScalesOfJustice = 94,
    // gap: 95 unused
    PauseTimer = 96,
    ResumeTimer = 97,
    MoveSpecial = 98,
    StateWrongChecksum = 99,
    UpdateTween = 100,
    ProcessInputTween = 101,
    TimeReceivedObsolete = 102,
    Transferred = 103,
    TransferTimeFreq = 104,
    UpdatePanelTween = 105,
    StateWrongInitChecksum = 106,
    FrameFinishFiller = 107,
    MachineEvent = 108,
    InvalidDataCompressed = 109,
    InvalidDataUncompressed = 110,
    PacketIndexJump = 111,
    CancelPending = 112,
    BulletExplosion = 113,
    FrameNumberWinsock = 114,
}

impl TryFrom<u32> for TaskMessage {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0..=9 | 11..=23 | 26..=41 | 43..=64 | 67..=81 | 84..=86 | 88..=94 | 96..=114 => {
                // SAFETY: all matched values correspond to valid variants
                Ok(unsafe { core::mem::transmute(value) })
            }
            _ => Err(value),
        }
    }
}
