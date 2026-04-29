use bytemuck::{Pod, Zeroable};
use openwa_core::fixed::Fixed;
use openwa_core::weapon::WeaponId;

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
    // TODO: wkJellyworm said this is SkipGo, but in our code this is sent by the function
    // handling strike weapons that drop physics objects (mail, mine, mole)
    SkipGoOrMailMineMole = 40,
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
    // TODO: Figure out which this actually is.
    // wkJellyWorm's Constants.h claims this is Earthquake, but our fire_select_worm sends this message
    EarthquakeOrSelectWorm = 93,
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
    TurnEndMaybe = 117,
    Unknown122 = 122,
    /// 0x82 (130) — broadcast by `GameRuntime::BroadcastFrameTiming` (0x0052A9C0)
    /// last of three. Payload is 24 bytes: `(elapsed_qpc: u64, freq_qpc: u64,
    /// replay_check_flag: u8, /* 3 bytes uninit in WA */, /* 4 bytes uninit in WA */)`.
    /// `replay_check_flag = (frame_delay_counter >= 0 && IsReplayMode())`.
    /// Receivers fall through `WorldRootEntity::HandleMessage`'s default case,
    /// so the message is broadcast to all child entities; no specific handler
    /// has been identified yet.
    Unknown130 = 130,
    /// 0x83 (131) — broadcast by `GameRuntime::BroadcastFrameTiming` (0x0052A9C0)
    /// conditionally (when `world.0x98AC == 0 && replay_flag_a == 0`).
    /// Payload is 12 bytes: `(render_buffer_a_ptr_or_null: u32, fps_scaled: i32,
    /// frame_delay_counter: i32)`.
    Unknown131 = 131,
    /// 0x84 (132) — broadcast by `GameRuntime::BroadcastFrameTiming` (0x0052A9C0)
    /// first of three. Payload is 4 bytes: `(fps_scaled: i32)` (raw 16.16 Fixed
    /// integer, capped at 0x1333 in normal-path frames).
    Unknown132 = 132,
}

impl TryFrom<u32> for TaskMessage {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0..=9
            | 11..=23
            | 26..=41
            | 43..=64
            | 67..=81
            | 84..=86
            | 88..=94
            | 96..=114
            | 117
            | 122
            | 130..=132 => {
                // SAFETY: all matched values correspond to valid variants
                Ok(unsafe { core::mem::transmute(value) })
            }
            _ => Err(value),
        }
    }
}

pub trait TaskMessageData: Pod {
    const MESSAGE_TYPE: TaskMessage;
}

/// Payload for [`TaskMessage::Explosion`]. Built by `create_explosion`
/// and consumed by every `WorldEntity::HandleMessage` reached through the
/// broadcast.
#[repr(C)]
#[derive(Clone, Copy, Debug, Zeroable, Pod)]
pub struct ExplosionMessage {
    /// Always `1` from WA's `CreateExplosion`. Role on the receiver side
    /// unconfirmed — likely a "real vs. cosmetic" discriminator, since
    /// `SpawnEffect` populates the matching slot with its own constant.
    pub flag: u32,
    pub pos_x: Fixed,
    pub pos_y: Fixed,
    pub explosion_id: u32,
    pub damage: u32,
    /// Caller-supplied flag of unknown purpose. Missile contact passes 0,
    /// but other WA call sites pass non-zero values — asserted empirically.
    pub caller_flag: u32,
    pub owner_id: u32,
}

// 7 dwords × 4 bytes = 28 = 0x1C. Matches WA's populated payload range.
const _: () = assert!(core::mem::size_of::<ExplosionMessage>() == 0x1C);

impl TaskMessageData for ExplosionMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Explosion;
}

/// Payload for [`TaskMessage::SpecialImpact`]. Built by `SpecialImpact`
/// (0x005193D0) — drill/prod/baseball-bat-style weapons broadcast it to
/// every receiver inside an axis-aligned box around the hit. WA reports
/// `0x408` for the size (oversized scratch frame) but only this 0x1C
/// prefix is populated.
///
/// The base `WorldEntity::HandleMessage` only reads `impulse_x` /
/// `impulse_y`; subclass overrides (notably `WormEntity`) consume
/// `damage` and `source_team_index` for kill attribution.
#[repr(C)]
#[derive(Clone, Copy, Debug, Zeroable, Pod)]
pub struct SpecialImpactMessage {
    /// Same "real vs. cosmetic" discriminator role as
    /// [`ExplosionMessage::flag`] — empirically.
    pub flag: u32,
    pub pos_x: Fixed,
    pub pos_y: Fixed,
    /// Sign is flipped on the source worm itself so the recoil mirrors
    /// the hit direction; forwarded as-is to every other target.
    pub impulse_x: Fixed,
    pub impulse_y: Fixed,
    /// Already attenuated by the sender's distance-falloff math.
    pub damage: i32,
    pub source_team_index: u32,
}

const _: () = assert!(core::mem::size_of::<SpecialImpactMessage>() == 0x1C);

impl TaskMessageData for SpecialImpactMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::SpecialImpact;
}

/// Empty payload for [`TaskMessage::UpdateNonCritical`]. Broadcast at the
/// head of `reset_frame_state` once per frame.
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct UpdateNonCriticalMessage;

impl TaskMessageData for UpdateNonCriticalMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::UpdateNonCritical;
}

/// Empty payload for [`TaskMessage::TurnEndMaybe`] (msg 0x75). Sent to
/// `WorldRootEntity` at multiple end-of-round transitions.
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct TurnEndMaybeMessage;

impl TaskMessageData for TurnEndMaybeMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::TurnEndMaybe;
}

/// Empty payload for [`TaskMessage::Unknown122`] (msg 0x7A). Sent from
/// `step_frame` when a sentinel field on `GameInfo` matches.
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct Unknown122Message;

impl TaskMessageData for Unknown122Message {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Unknown122;
}

/// Payload for [`TaskMessage::Unknown130`] (msg 0x82). Last of three
/// messages broadcast by `GameRuntime::BroadcastFrameTiming` (0x0052A9C0).
/// Carries the QPC time delta and frequency for the just-completed render
/// frame plus a replay-mode flag. WA leaves the trailing 8 bytes
/// uninitialised; we zero them.
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct Unknown130Message {
    /// QPC ticks since the last render-frame timing reference.
    pub elapsed: u64,
    /// QPC frequency (ticks per second).
    pub freq: u64,
    /// Low byte = `(frame_delay_counter >= 0 && IsReplayMode())`. Upper 3
    /// bytes are stale stack in WA — we always set them to zero.
    pub replay_check_flag: u32,
    /// 4 bytes WA leaves uninit (msg size is 0x18 = 24 bytes total). We
    /// zero them.
    pub _pad: u32,
}

const _: () = assert!(core::mem::size_of::<Unknown130Message>() == 0x18);

impl TaskMessageData for Unknown130Message {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Unknown130;
}

/// Payload for [`TaskMessage::Unknown131`] (msg 0x83). Conditional middle
/// message broadcast by `GameRuntime::BroadcastFrameTiming` (0x0052A9C0) when
/// `world.[0x98AC] == 0 && replay_flag_a == 0`.
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct Unknown131Message {
    /// `runtime.render_buffer_a` if `frame_delay_counter > 0`, else null.
    /// Stored as `u32` rather than `*mut u8` to keep the struct `Pod`.
    pub render_buffer: u32,
    /// Same value as [`Unknown132Message::fps_scaled`].
    pub fps_scaled: i32,
    /// Current `runtime.frame_delay_counter` (-1 = inactive, ≥0 = ticking).
    pub frame_delay: i32,
}

const _: () = assert!(core::mem::size_of::<Unknown131Message>() == 0xC);

impl TaskMessageData for Unknown131Message {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Unknown131;
}

/// Payload for [`TaskMessage::Unknown132`] (msg 0x84). First of three
/// messages broadcast by `GameRuntime::BroadcastFrameTiming` (0x0052A9C0).
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct Unknown132Message {
    /// Raw 16.16 Fixed integer, capped at 0x1333 in normal-path frames —
    /// same value passed to `setup_frame_params`.
    pub fps_scaled: i32,
}

const _: () = assert!(core::mem::size_of::<Unknown132Message>() == 0x4);

impl TaskMessageData for Unknown132Message {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Unknown132;
}

/// Damage report sent up to `WorldRootEntity` when an entity has its
/// `caller_flag` set on an incoming `ExplosionMessage`. The recipient logs
/// the hit for score / kill attribution.
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct ExplosionReportMessage {
    /// Applied damage as a percentage of the explosion's max damage:
    /// `(actual_damage * 100) / max_damage`. Always 0..=100 in normal play.
    pub damage_percent: i32,
}

impl TaskMessageData for ExplosionReportMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::ExplosionReport;
}

/// Payload for [`TaskMessage::DetonateWeapon`] (broadcast by
/// `TeamEntity::HandleMessage` to its children when the team surrenders, on
/// game versions > 0xF4).
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct DetonateWeaponMessage {
    pub team_index: u32,
}

impl TaskMessageData for DetonateWeaponMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::DetonateWeapon;
}

/// Payload for [`TaskMessage::Surrender`] (sent by the Surrender weapon
/// (subtype 13) and by `TeamEntity::HandleMessage` when broadcasting end of
/// turn).
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct SurrenderMessage {
    pub team_index: u32,
}

impl TaskMessageData for SurrenderMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Surrender;
}

/// Payload for [`TaskMessage::Freeze`] (sent by the Freeze weapon, subtype 20).
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct FreezeMessage {
    pub team_index: u32,
}

impl TaskMessageData for FreezeMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Freeze;
}

/// Payload for [`TaskMessage::SkipGoOrMailMineMole`] (sent by the
/// Mail/Mine/Mole weapon family, subtype 14).
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct SkipGoOrMailMineMoleMessage {
    pub team_index: u32,
}

impl TaskMessageData for SkipGoOrMailMineMoleMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::SkipGoOrMailMineMole;
}

/// Payload for [`TaskMessage::EarthquakeOrSelectWorm`] (sent by the Select
/// Worm weapon, subtype 21).
#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod, Debug)]
pub struct SelectWormMessage {
    /// Hard-coded `8` at every observed call site.
    pub unknown1: u32,
    pub team_index: u32,
}

impl TaskMessageData for SelectWormMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::EarthquakeOrSelectWorm;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct PoisonWormMessage {
    pub unknown1: i32,
    // 2 in fire_nuclear_test
    pub unknown2: i32,
    pub team_index: u32,
}

impl TaskMessageData for PoisonWormMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::PoisonWorm;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct RaiseWaterMessage {
    pub fire_method: i32,
    // 8 in fire_nuclear_test
    pub unknown1: i32,
}

impl TaskMessageData for RaiseWaterMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::RaiseWater;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct NukeBlastMessage {
    // 8 in fire_nuclear_test
    pub unknown1: u32,
}

impl TaskMessageData for NukeBlastMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::NukeBlast;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ScalesOfJusticeMessage;

impl TaskMessageData for ScalesOfJusticeMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::ScalesOfJustice;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ArmageddonMessage {
    pub unknown1: i32,
    pub unknown2: i32,
    pub selected_weapon: u32,
    pub team_index: u32,
}

impl TaskMessageData for ArmageddonMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::Armageddon;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct WeaponReleasedMessage {
    pub team_index: u32,
    pub worm_index: u32,
    pub shot_data_1: u32,
    pub shot_data_2: u32,
    pub fire_sync_frame_1: i32,
    pub fire_sync_frame_2: i32,
    pub unknown_flag: u32,
    pub weapon: WeaponId,
}

impl TaskMessageData for WeaponReleasedMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::WeaponReleased;
}

#[repr(C)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct RenderSceneMessage;

impl TaskMessageData for RenderSceneMessage {
    const MESSAGE_TYPE: TaskMessage = TaskMessage::RenderScene;
}
