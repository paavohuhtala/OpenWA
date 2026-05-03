use super::base::{BaseEntity, SharedDataTable};
use crate::{
    FieldRegistry,
    entity::Entity,
    game::{EntityMessage, message::EntityMessageData},
};
use bytemuck::bytes_of;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "WorldRootEntity" {
        /// WorldRootEntity constructor
        ctor/Stdcall WORLD_ROOT_ENTITY_CTOR = 0x0055B280;
        /// WorldRoot message dispatcher
        fn/Thiscall WORLD_ROOT_HANDLE_MESSAGE = 0x0055DC00;
        /// WorldRoot hurry handler
        fn/Usercall WORLD_ROOT_HURRY_HANDLER = 0x0055E5F0;
        /// WorldRoot auto select teams
        fn WORLD_ROOT_AUTO_SELECT_TEAMS = 0x005611E0;
    }
}

/// Embedded intermediate game-context sub-object within `WorldRootEntity`.
///
/// This is the memory region at `WorldRootEntity+0x30..+0xDB` (0xAC bytes).
/// It is initialised by `TeamEntity__Constructor_Maybe` (0x550EB0), which:
///   1. Calls `BaseEntity::Constructor(this, nullptr, world)`
///   2. Sets the primary vtable to 0x669E34 and class_type to 5
///   3. Sets a **secondary interface vtable** pointer (Ghidra 0x669C44) at +0x30
///      inside the object (i.e. `MatchCtx` base +0x00)
///   4. Copies `landscape_height = GameWorld+0x5E0` as Fixed16.16 to both
///      `land_height` and `land_height_2`
///   5. Writes -1 sentinels to `_sentinel_18`, `_sentinel_28`, `_sentinel_38`
///   6. Writes `team_count = *(byte*)(GameInfo+0x44C)` to `team_count`
///
/// `WorldRootEntity__Constructor` (0x55B2A0) then overrides the primary vtable
/// to 0x669F70 and class_type to 6 but **leaves this sub-object intact**.
///
/// The three -1 sentinels at offsets 0x18 / 0x28 / 0x38 are evenly spaced
/// 0x10 bytes apart. Their role is unknown — candidates are water-level
/// Y-coordinates (initial -1 = "no water"), zone boundaries, or AI markers.
///
/// The final 0x44 bytes (0x68–0xAB, `_unknown_68`) are not initialised by
/// either constructor; they appear to be set by `FUN_005514d0` or similar
/// helpers called later. Observed runtime values: +0xA0 = 15, +0xA4/+0xA8
/// are heap pointers.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MatchCtx {
    /// +0x00 (= WorldRootEntity+0x30): Secondary interface vtable pointer.
    /// Ghidra: 0x00669C44.  Set by both constructors; always 0x669C44 at runtime.
    pub _secondary_vtable: u32,
    /// +0x04: Unknown — not set by constructors (remains 0).
    pub _unknown_04: u32,
    /// +0x08–0x0F: Unknown — explicitly zeroed by constructor.
    pub _unknown_08: [u32; 2],
    /// +0x10: Landscape height as Fixed16.16.  `GameWorld+0x5E0 << 16`.
    pub land_height: Fixed,
    /// +0x14: Landscape height duplicate — same value as `land_height`.
    pub land_height_2: Fixed,
    /// +0x18: Sentinel, always -1 at construction.  Role unknown.
    pub _sentinel_18: i32,
    /// +0x1C–0x27: Unknown — explicitly zeroed by constructor.
    pub _unknown_1c: [u32; 3],
    /// +0x28: Sentinel, always -1 at construction.  Role unknown.
    pub _sentinel_28: i32,
    /// +0x2C–0x37: Unknown — explicitly zeroed by constructor.
    pub _unknown_2c: [u32; 3],
    /// +0x38: Sentinel, always -1 at construction.  Role unknown.
    pub _sentinel_38: i32,
    /// +0x3C–0x4B: Unknown — explicitly zeroed by constructor.
    pub _unknown_3c: [u32; 4],
    /// +0x4C: Number of teams.  `*(byte*)(GameInfo+0x44C)` at construction time.
    pub team_count: u32,
    /// +0x50–0x64: Unknown — explicitly zeroed by constructor (param_1[0x20..0x25]).
    pub _unknown_50: [u32; 6],
    /// +0x68–0x9F: Unknown — all explicitly zeroed by `FUN_005514d0` during game
    /// setup. Purpose unknown; may be per-team or per-worm state slots.
    pub _unknown_68: [u32; 14],
    /// +0xA0: Return value of `FUN_00525f50(0)` stored here during game setup.
    /// Observed value: 15. `FUN_00525f50` is a slot-allocation helper; this may
    /// be a pool slot index or a pre-computed game-state token.
    pub _slot_d0: u32,
    /// +0xA4: `DisplayGfx` textbox handle — created by `DisplayGfx__ConstructTextbox`
    /// with params `(buf, -1280, 2)` if `GameWorld+0x7EF8 != 0` (display active).
    /// Likely the HUD timer textbox.  NULL when display is disabled.
    pub _hud_textbox_a: u32,
    /// +0xA8: `DisplayGfx` textbox handle — created with params `(buf, 8, 4)`.
    /// Likely a secondary HUD element.  NULL when display is disabled.
    pub _hud_textbox_b: u32,
}

const _: () = assert!(core::mem::size_of::<MatchCtx>() == 0xAC);

/// WorldRootEntity vtable — 12 slots. Extends BaseEntity base (8 slots) with turn-game behavior.
///
/// Vtable at Ghidra 0x669F70. Slot 2 (HandleMessage) is the main turn-flow
/// dispatcher (30+ message types).
#[openwa_game::vtable(size = 12, va = 0x00669F70, class = "WorldRootEntity")]
pub struct WorldRootEntityVtable {
    /// HandleMessage — processes messages for turn flow control
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut WorldRootEntity,
        sender: *mut BaseEntity,
        msg_type: EntityMessage,
        size: u32,
        data: *const u8,
    ),
    /// HUD/scoreboard data query — responds to query-style messages such as
    /// msg 0x7D3 (end-of-round data snapshot).
    /// thiscall + 3 stack params (msg, size, buf), RET 0xC.
    #[slot(3)]
    pub hud_data_query: fn(this: *mut WorldRootEntity, msg: u32, size: u32, buf: *mut u8),
    /// ProcessFrame — per-frame turn update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut WorldRootEntity, flags: u32),
}

/// Root turn-controller entity — one instance per game, parent of the entire entity tree.
///
/// Every worm, team, projectile, and environment entity is a child (direct or indirect)
/// of this node.  `WorldRootEntity` drives the turn loop: it processes 50 game frames
/// per second via `WorldRootEntity__TurnManager_ProcessFrame` (0x55FDA0), which is
/// called from HandleMessage case 2 (FrameFinish).
///
/// Inheritance: BaseEntity → TeamEntity → WorldRootEntity.  Class type 6.
/// Constructor: `WorldRootEntity__Constructor` (0x55B2B1).
/// vtable: `WorldRootEntity__vtable` (0x00669F70), 20 slots.
/// Total size: 0x2E0 bytes.
///
/// Key vtable slots:
///   [0] 0x55B5E0 — entity-tree state snapshot serialiser
///   [1] 0x55B540 — destructor / Free
///   [2] 0x55DC00 — HandleMessage (30+ message types)
///   [3] 0x5612E0 — HUD data query (responds to msg 0x7D3)
///
/// All timers decrement by 20 ms per frame (= 1000 ms / 50 fps).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct WorldRootEntity {
    /// 0x00-0x2F: BaseEntity base (vtable, parent, children, shared_data, world, …)
    pub base: BaseEntity<*const WorldRootEntityVtable>,
    /// 0x30–0xDB: Embedded `MatchCtx` sub-object (0xAC bytes).
    /// See [`MatchCtx`] for field details.
    pub game_ctx: MatchCtx,

    // ---- WorldRootEntity-specific fields (0xDC onwards) ----
    pub _unknown_dc: u32,
    pub _unknown_e0: u32,
    /// 0xE4: turn-seed: ~(GameWorld+0x45EC % 9000), used for random initialisation.
    pub _turn_seed: u32,
    /// 0xE8: number of teams in this game (copy of GameWorld+0x44C).
    pub num_teams: u32,
    pub _unknown_ec: [u8; 0x1C],
    /// 0x108: "worm active" flag — non-zero while the current worm is shooting or
    /// moving.  While non-zero, `turn_timer` is paused.
    /// A copy is also written to GameWorld+0x7234 at construction.
    pub worm_active: u32,
    pub _unknown_10c: [u8; 0x20],
    /// 0x12C: active team index, **1-based** (0 = no active team).
    /// Used to index per-team sound tables in GameWorld (stride 0xF0 at GameWorld+0x774C).
    pub current_team: u32,
    /// 0x130: active worm index within the current team, **0-based** (stride 0x9C).
    pub current_worm: u32,
    /// 0x134: arena team index used for TeamArena lookups.
    pub arena_team: u32,
    /// 0x138: arena worm index used for TeamArena lookups.
    pub arena_worm: u32,
    pub _unknown_13c: u32,
    /// 0x140: set to 1 at construction; role unclear.
    pub _init_140: u32,
    pub _unknown_144: u32,
    pub _unknown_148: u32,
    /// 0x14C: set to 1 at construction and reset by turn-flow restart; role unclear.
    pub _init_14c: u32,
    /// 0x150: set to 1 when the active worm's firing phase ends and retreat begins.
    pub turn_ended: u32,
    /// 0x154: set to 1 when the scheme turn time is 0 (no time limit).
    pub no_time_limit: u32,
    pub _unknown_158: [u8; 0x14],
    /// 0x16C: set to 1 once the "5 seconds left" warning sound has played.
    pub warning_sound_played: u32,
    pub _unknown_170: u32,
    /// 0x174: set while a worm is transitioning out of a special state (e.g. landing
    /// after a fly animation); cleared once settled.
    pub _worm_state_transition: u32,
    /// 0x178: retreat-phase countdown (ms).  Initialised from `retreat_time_max` when
    /// firing ends; decrements 20 ms/frame.  Dispatches message 0x35 at zero.
    pub retreat_timer: i32,
    /// 0x17C: initial retreat duration (ms) loaded from the scheme at turn start.
    pub retreat_time_max: i32,
    /// 0x180: last tick when the displayed "seconds remaining" value was updated.
    pub last_second_tick: u32,
    /// 0x184: idle-phase timer (ms).  Initialised to `scheme_turn_time × 1000`.
    /// Decrements 20 ms/frame **only while `worm_active == 0`** (timer pauses when
    /// the worm is moving/shooting).  When it reaches zero, sudden-death is triggered.
    /// This is NOT the per-turn HUD countdown — see `turn_timer` at +0x18C instead.
    pub idle_timer: i32,
    /// 0x188: smoothed display copy of `turn_timer`; tracks it at ≤ 0xCCC ms/frame
    /// to avoid visual jumping on the HUD.
    pub turn_timer_display: i32,
    /// 0x18C: per-turn countdown timer (ms) — this is what the HUD shows.
    /// Decrements 20 ms/frame every game frame.  When it reaches zero,
    /// `FUN_0055f4f0` fires (turn ends / retreat begins).
    pub turn_timer: i32,
    pub _unknown_190: [u8; 0x8],
    /// 0x198: turn-timer visual pulse intensity (Fixed 16.16); grows as time runs low.
    pub timer_flash: u32,
    pub _unknown_19c: [u8; 0x138],
    /// 0x2D4: frame count accumulated while this team held the active worm.
    pub active_worm_frames: u32,
    /// 0x2D8: frame count accumulated during the retreat phase.
    pub retreat_frames: u32,
    /// 0x2DC: timer scale factor derived from scheme (2, 3, or 4; default 4).
    pub _timer_scale: u32,
}

const _: () = assert!(core::mem::size_of::<WorldRootEntity>() == 0x2E0);

impl WorldRootEntity {
    /// SharedData key under which `WorldRootEntity` registers itself.
    ///
    /// As the root of the in-game world, every other entity in the same game
    /// tree can locate it via `(0, 0x14)`. Use [`Self::from_shared_data`]
    /// instead of looking up the raw key.
    pub const SHARED_DATA_KEY: (u32, u32) = (0, 0x14);

    /// Resolve the per-game `WorldRootEntity` instance from any entity in the
    /// same game tree. Returns null if the table has no entry (during
    /// startup/shutdown windows).
    ///
    /// # Safety
    /// `entity` must be a valid entity pointer with an initialised `shared_data`.
    pub unsafe fn from_shared_data(entity: *const BaseEntity) -> *mut WorldRootEntity {
        unsafe {
            let (esi, edi) = Self::SHARED_DATA_KEY;
            SharedDataTable::from_task(entity).lookup(esi, edi) as *mut WorldRootEntity
        }
    }

    pub unsafe fn handle_typed_message_raw<TSender: Entity, TMessage: EntityMessageData>(
        this: *mut Self,
        sender: *mut TSender,
        message: TMessage,
    ) {
        let buf = bytes_of(&message);
        let size = buf.len() as u32;
        unsafe {
            let sender = sender as *mut BaseEntity;
            let buf = if size > 0 {
                buf.as_ptr()
            } else {
                core::ptr::null()
            };
            Self::handle_message_raw(this, sender, TMessage::MESSAGE_TYPE, size, buf);
        }
    }
}

// Generate typed vtable method wrappers: handle_message(), process_frame(), etc.
bind_WorldRootEntityVtable!(WorldRootEntity, base.vtable);
