use crate::render::RenderQueue;

/// DDGame — the main game engine object.
///
/// This is a massive ~39KB struct (0x98B8 bytes) that owns all major subsystems:
/// display, landscape, sound, graphics handlers, and task state machines.
///
/// Allocated in DDGame__Constructor (0x56E220).
/// The DDGame pointer is stored at DDGameWrapper+0x488 (DWORD index 0x122).
///
/// PARTIAL: Fields up to 0x54C are densely mapped from the constructor.
/// Beyond that, only scattered fields are known — use the `offsets` module.
///
/// Note on offsets: The constructor accesses DDGame fields via
/// `*(param_2[0x122] + byte_offset)` — these are byte offsets, NOT DWORD-indexed.
/// DWORD indexing only applies to param_2 (DDGameWrapper) itself.
#[repr(C)]
pub struct DDGame {
    /// 0x000: Base value (param_5 from constructor)
    pub _base_000: *mut u8,
    /// 0x004: Context pointer (param_3)
    pub _context: *mut u8,
    /// 0x008: param_4
    pub _param_008: *mut u8,
    /// 0x00C: Allocated object (0x608 bytes, conditional)
    pub _object_00c: *mut u8,
    /// 0x010: param_6
    pub _param_010: *mut u8,
    /// 0x014: param_7
    pub _param_014: *mut u8,
    /// 0x018: param_8
    pub _param_018: *mut u8,
    /// 0x01C: Caller/parent pointer (param_1)
    pub _caller: *mut u8,
    /// 0x020: PCLandscape pointer (copied from DDGameWrapper[0x133])
    pub landscape: *mut u8,
    /// 0x024: Game state pointer (param_10)
    pub game_state: *mut u8,
    /// 0x028: param_9
    pub _param_028: *mut u8,
    /// 0x02C: Secondary GfxDir object (0x70C bytes, conditional on GfxHandler 1)
    pub secondary_gfxdir: *mut u8,
    /// 0x030: Gradient image pointer
    pub gradient_image: *mut u8,
    /// 0x034: Gradient image 2 pointer
    pub gradient_image_2: *mut u8,
    /// 0x038-0x0B4: Arrow sprite object pointers (32 entries)
    pub arrow_sprites: [*mut u8; 32],
    /// 0x0B8-0x134: Arrow GfxDir pointers (32 entries, conditional)
    pub arrow_gfxdirs: [*mut u8; 32],
    /// 0x138: DisplayGfx object pointer (vtable 0x664144)
    pub display_gfx: *mut u8,
    /// 0x13C-0x37F: Unknown
    pub _unknown_13c: [u8; 0x244],
    /// 0x380: TaskStateMachine pointer (vtable 0x664118, 0x2C bytes)
    pub task_state_machine: *mut u8,
    /// 0x384-0x467: Unknown
    pub _unknown_384: [u8; 0xE4],
    /// 0x468: Landscape-derived value
    pub _landscape_val: *mut u8,
    /// 0x46C-0x488: 8 SpriteRegion pointers (0x9C bytes each, vtable 0x66B268)
    /// Created by SpriteRegion__Constructor (0x57DB20).
    /// Each contains 32 TaskStateMachine sub-objects.
    pub sprite_regions: [*mut u8; 8],
    /// 0x48C-0x508: Arrow collision region pointers (32 entries)
    pub arrow_collision_regions: [*mut u8; 32],
    /// 0x50C: Coordinate list object (capacity 600, 0x12C0 data buffer)
    pub coord_list: *mut u8,
    /// 0x510: Weapon table pointer
    pub weapon_table: *mut u8,
    /// 0x514-0x523: Unknown
    pub _unknown_514: [u8; 0x10],
    /// 0x524: RenderQueue pointer (passed as `this` to all Draw* functions)
    pub render_queue: *mut RenderQueue,
    /// 0x528-0x547: Unknown
    pub _unknown_528: [u8; 0x20],
    /// 0x548: Weapon panel pointer
    pub weapon_panel: *mut u8,
    /// 0x54C-0x7EFF: Sparse fields (see offsets module)
    ///
    /// Known landmarks:
    /// - 0x64D8: cleared by init
    /// - 0x72A4: cleared by init
    /// - 0x730C-0x732C: 9 GfxDir color entries
    /// - 0x7338: fill pixel value
    /// - 0x77C4: display-related value
    /// - 0x7EF8: flag from game_state+0xF914
    /// - 0x7EFC: init 1
    ///
    /// Also includes FUN_00526120 zeroed offsets at stride 0x194:
    /// 0x379C, 0x3930, 0x3AC4, 0x3C58, 0x3DEC, 0x3F80, 0x4114, 0x42A8, 0x443C, 0x45D0
    pub _unknown_54c: [u8; 0x7F00 - 0x54C],

    // === Sound queue (0x7F00-0x8143) ===
    /// 0x7F00: Sound queue (16 entries, stride 0x24). Appended by PlaySoundGlobal.
    pub sound_queue: [SoundQueueEntry; 16],
    /// 0x8140: Number of entries currently in the sound queue (0–16).
    pub sound_queue_count: i32,

    /// 0x8144-0x98B7: Remaining fields
    ///
    /// Known landmarks:
    /// - 0x8CBC-0x8CF0: 4x 0x10-byte entries (zeroed at +0, +4)
    /// - 0x9850-0x9884: 4x 0x10-byte entries (zeroed at +0, +4)
    pub _unknown_8144: [u8; 0x98B8 - 0x8144],
}

const _: () = assert!(core::mem::size_of::<DDGame>() == 0x98B8);

/// Well-known byte offsets into DDGame, for use with raw pointer access.
///
/// The DDGame pointer is at DDGameWrapper+0x488 (DWORD index 0x122).
pub mod offsets {
    // === Header / init params (0x000-0x02C) ===
    pub const LANDSCAPE: usize = 0x020;
    pub const GAME_STATE: usize = 0x024;
    pub const SECONDARY_GFXDIR: usize = 0x02C;
    pub const GRADIENT_IMAGE: usize = 0x030;

    // === Sprite arrays (0x038-0x138) ===
    pub const ARROW_SPRITES: usize = 0x038;
    pub const ARROW_GFXDIRS: usize = 0x0B8;
    pub const DISPLAY_GFX: usize = 0x138;

    // === Task/state machines (0x380-0x488) ===
    pub const TASK_STATE_MACHINE: usize = 0x380;
    pub const SPRITE_REGIONS: usize = 0x46C;

    // === Arrow collision (0x48C-0x50C) ===
    pub const ARROW_COLLISION_REGIONS: usize = 0x48C;
    pub const COORD_LIST: usize = 0x50C;

    // === WormKit-documented offsets ===
    pub const WEAPON_TABLE: usize = 0x510;
    pub const WEAPON_PANEL: usize = 0x548;

    // === Team weapon state (DDGame + 0x4628) ===
    /// Base of TeamArenaState sub-struct within DDGame.
    /// Callers pass DDGame + TEAM_ARENA_STATE as base pointer to
    /// GetAmmo/AddAmmo/SubtractAmmo.
    pub const TEAM_ARENA_STATE: usize = 0x4628;

    // === Team block array (7 × FullTeamBlock, stride 0x51C) ===
    /// Start of team block array within DDGame (7 blocks, stride 0x51C).
    /// Derived: entry_ptr(team=0) - 0x598 = 0x4628 - 0x598 = 0x4090.
    /// Runtime-confirmed: block[0] is zeroed preamble, blocks[1-6] hold team data.
    pub const TEAM_BLOCKS: usize = 0x4090;

    /// Byte offset from TeamArenaState base back to FullTeamBlock array start.
    /// `blocks_ptr = (tws_base as *const u8).sub(ARENA_TO_BLOCKS) as *const FullTeamBlock`
    ///
    /// entry_ptr(0) = DDGame+0x4628 = TEAM_BLOCKS + 0x598.
    /// 0x598 = sizeof(FullTeamBlock) + 0x7C = one block + offset into sentinel worm[0].
    pub const ARENA_TO_BLOCKS: usize = 0x598;

    // === FUN_00526120 init offsets (stride 0x194, 10 entries) ===
    pub const INIT_TABLE_BASE: usize = 0x379C;
    pub const INIT_TABLE_STRIDE: usize = 0x194;

    // === Sparse fields in upper region ===
    pub const FIELD_64D8: usize = 0x64D8;
    pub const FIELD_72A4: usize = 0x72A4;
    pub const GFX_COLOR_ENTRIES: usize = 0x730C;
    /// Crosshair line color param (DrawPolygon param_2). Part of GfxDir color entries.
    pub const CROSSHAIR_LINE_PARAM_2: usize = 0x7324;
    /// Crosshair line style param (DrawPolygon param_1). Part of GfxDir color entries.
    pub const CROSSHAIR_LINE_PARAM_1: usize = 0x732C;
    pub const FILL_PIXEL: usize = 0x7338;
    pub const DISPLAY_77C4: usize = 0x77C4;
    pub const FLAG_7EF8: usize = 0x7EF8;
    pub const FIELD_7EFC: usize = 0x7EFC;
    /// Scale factor used by DrawCrosshairLine (multiplied by 0x140000).
    pub const CROSSHAIR_SCALE: usize = 0x8150;

    // === Sound queue (DDGame + 0x7F00) ===
    /// Sound enabled flag (i32, nonzero = enabled).
    pub const SOUND_ENABLED: usize = 0x0008;
    /// Sound queue base (16 × SoundQueueEntry, stride 0x24).
    pub const SOUND_QUEUE: usize = 0x7F00;
    /// Sound queue count (i32, 0–16).
    pub const SOUND_QUEUE_COUNT: usize = 0x8140;
}

// ============================================================
// Sound queue entry — 16 entries at DDGame + 0x7F00
// ============================================================

/// Sound queue entry (0x24 = 36 bytes, stride between consecutive entries).
///
/// DDGame maintains a 16-slot sound queue at offset 0x7F00. PlaySoundGlobal
/// appends entries; PlaySoundLocal additionally marks entries as local and
/// stores position via the task's secondary vtable.
#[repr(C)]
pub struct SoundQueueEntry {
    /// Sound effect ID (SoundId enum value).
    pub sound_id: u32,
    /// Flags / priority (e.g. 3=weapon, 7=explosion).
    pub flags: u32,
    /// Volume (Fixed-point, 0x10000 = 1.0).
    pub volume: u32,
    /// Pitch (Fixed-point, 0x10000 = 1.0).
    pub pitch: u32,
    /// Reserved, always 0.
    pub reserved: u32,
    /// 0 = global, 1 = local (has position tracking).
    pub is_local: u8,
    pub _pad: [u8; 3],
    /// Position X (filled by secondary vtable GetPosition for local sounds).
    pub pos_x: u32,
    /// Position Y (filled by secondary vtable GetPosition for local sounds).
    pub pos_y: u32,
    /// Pointer to task's secondary vtable (at task+0xE8) for position updates.
    pub secondary_vtable: u32,
}

const _: () = assert!(core::mem::size_of::<SoundQueueEntry>() == 0x24);

// ============================================================
// Team arena state — sub-struct at DDGame + 0x4628
// ============================================================


// ============================================================
// Per-worm and per-team block structs
// ============================================================

/// Per-worm data entry (0x9C bytes, stride between consecutive worms).
///
/// WA supports up to 8 playable worms per team. The original code accesses
/// worms via raw pointer arithmetic from the team entry pointer, using
/// stride 0x9C. This means the 8th worm crosses the FullTeamBlock boundary
/// into the next block's worms\[0\] — see `TeamArenaRef::team_worm()`.
///
/// Slot 0 of each block is dual-purpose: its high-offset fields (+0x6C, +0x70,
/// +0x74, +0x78) store sentinel/metadata for the team, while its low-offset
/// fields may hold data for the 8th worm of the previous team (when present).
///
/// Field offsets confirmed by runtime memory dump (validator DLL):
/// - state at 0x00: 0x67 = active/selected, 0x65 = idle, 0x80+ = special
/// - active_flag at 0x0C: 1 for alive worms
/// - max_health at 0x58: initial health value (100)
/// - health at 0x5C: current health (100 = full)
/// - name at 0x78: null-terminated worm name string (~20 bytes)
///
/// Sentinel (slot 0) has different layout — see `sentinel_*` methods:
/// - +0x6C (_unknown_60\[0x0C\]): eliminated flag
/// - +0x78 (name\[0..4\]): worm_count
/// - +0x80 (name\[8..12\]): alliance_id
/// - +0x84 (name\[12..\]): team name string
/// - +0x98 (_unknown_90\[8..12\]): value 0x190 (400), unknown purpose
#[repr(C)]
pub struct WormEntry {
    /// 0x00: Worm state machine state.
    /// Values: 0x65=idle, 0x67=active/selected, 0x68=active variant.
    /// Special states {0x80..0x85, 0x89} = dying/drowning/special animation.
    pub state: u32,
    /// 0x04-0x0B: Unknown
    pub _unknown_04: [u8; 8],
    /// 0x0C: Active/alive flag (1 for alive worms in game, 0 otherwise).
    pub active_flag: i32,
    /// 0x10-0x57: Unknown
    pub _unknown_10: [u8; 0x48],
    /// 0x58: Max health (initial health value, typically 100).
    pub max_health: i32,
    /// 0x5C: Current health. Used by GetTeamTotalHealth.
    pub health: i32,
    /// 0x60-0x77: Unknown.
    /// In sentinel: +0x6C = eliminated flag for the team.
    pub _unknown_60: [u8; 0x18],
    /// 0x78: Worm name, null-terminated ASCII string (~20 bytes).
    /// In sentinel: +0x78 = worm_count (i32), +0x80 = alliance_id (i32),
    /// +0x84 = team name string.
    pub name: [u8; 0x18],
    /// 0x90-0x9B: Unknown (zeroed in runtime dump for playable worms).
    /// GetWormPosition reads pos_x/pos_y from +0x90/+0x94 via negative entry_ptr
    /// arithmetic, but values appear transient — not populated at rest.
    /// Actual worm positions live in CGameTask objects (+0x84/+0x88).
    pub _unknown_90: [u8; 0x0C],
}

const _: () = assert!(core::mem::size_of::<WormEntry>() == 0x9C);

impl WormEntry {
    /// Read worm_count from this entry when it's a sentinel (slot 0).
    /// Stored at self.name[0..4] (= WormEntry offset 0x78) as little-endian i32.
    ///
    /// # Safety
    /// Only valid when called on a sentinel worm (slot 0 of a FullTeamBlock).
    pub unsafe fn sentinel_worm_count(&self) -> i32 {
        *(self.name.as_ptr() as *const i32)
    }

    /// Read eliminated flag from this entry when it's a sentinel (slot 0).
    /// Stored at self._unknown_60[0x0C] (= WormEntry offset 0x6C) as i32.
    /// Nonzero = team is eliminated.
    ///
    /// # Safety
    /// Only valid when called on a sentinel worm (slot 0 of a FullTeamBlock).
    pub unsafe fn sentinel_eliminated(&self) -> i32 {
        *(self._unknown_60.as_ptr().add(0x0C) as *const i32)
    }

    /// Read alliance ID from sentinel (slot 0), Pattern B layout.
    /// Stored at self._unknown_60[0x10] (= WormEntry offset 0x70) as i32.
    /// Used by CountTeamsByAlliance and SetActiveWorm_Maybe.
    ///
    /// # Safety
    /// Only valid when called on a sentinel worm (slot 0 of a FullTeamBlock).
    pub unsafe fn sentinel_alliance(&self) -> i32 {
        *(self._unknown_60.as_ptr().add(0x10) as *const i32)
    }

    /// Read active worm index from sentinel (slot 0), Pattern B layout.
    /// Stored at self._unknown_60[0x14] (= WormEntry offset 0x74) as i32.
    /// 0 = no active worm, N = worm N is active.
    /// Used by CountTeamsByAlliance (as alive flag) and SetActiveWorm_Maybe.
    ///
    /// # Safety
    /// Only valid when called on a sentinel worm (slot 0 of a FullTeamBlock).
    pub unsafe fn sentinel_active_worm(&self) -> i32 {
        *(self._unknown_60.as_ptr().add(0x14) as *const i32)
    }

    /// Write active worm index to sentinel (slot 0), Pattern B layout.
    ///
    /// # Safety
    /// Only valid when called on a sentinel worm (slot 0 of a FullTeamBlock).
    pub unsafe fn set_sentinel_active_worm(&mut self, val: i32) {
        *(self._unknown_60.as_mut_ptr().add(0x14) as *mut i32) = val;
    }

    /// Read weapon alliance ID from sentinel (slot 0).
    /// Stored at self.name[8..12] (= WormEntry offset 0x80) as little-endian i32.
    ///
    /// This is the alliance_id used by GetAmmo/AddAmmo/SubtractAmmo to index
    /// into the shared ammo/delay tables. Teams with the same weapon alliance
    /// share ammo pools. Distinct from `sentinel_alliance()` at +0x70 which
    /// is used by CountTeamsByAlliance/SetActiveWorm_Maybe.
    ///
    /// # Safety
    /// Only valid when called on a sentinel worm (slot 0 of a FullTeamBlock).
    pub unsafe fn sentinel_weapon_alliance(&self) -> i32 {
        *(self.name.as_ptr().add(8) as *const i32)
    }
}

/// Full per-team data block (0x51C bytes, 6 teams in DDGame).
///
/// Contains 8 WormEntry slots (0x4E0 bytes) followed by 0x3C bytes of
/// team metadata.
///
/// **Block indexing**: Block 0 is unused (preamble, all zeros). Actual team
/// data starts at block 1. entry_ptr(team=N) = DDGame+0x4628+N*0x51C, which
/// lands at block\[N+1\].worm\[0\]+0x7C. Negative offsets reach back into
/// block\[N\]'s worm data. So entry_ptr(1) accesses block\[1\]'s worms.
///
/// **Worm access**: Playable worms are accessed via raw pointer arithmetic
/// from the team entry pointer (stride 0x9C). For teams with 8 worms, the
/// 8th worm crosses the block boundary — its early fields (state, health)
/// spill into block\[N+1\].worms\[0\], which is also the sentinel. Use
/// `TeamArenaRef::team_worm()` instead of direct array indexing.
///
/// **Sentinel worm\[0\]** stores metadata at high offsets that don't
/// conflict with the 8th worm's early fields:
/// - +0x6C: eliminated flag
/// - +0x70: alliance_id (Pattern B)
/// - +0x74: active_worm (Pattern B)
/// - +0x78: worm_count
/// - +0x84: team name string
#[repr(C)]
pub struct FullTeamBlock {
    /// 0x000-0x4DF: 8 worm entries (stride 0x9C)
    pub worms: [WormEntry; 8],
    /// 0x4E0-0x51B: Team metadata (0x3C bytes)
    pub _metadata: [u8; 0x3C],
}

const _: () = assert!(core::mem::size_of::<FullTeamBlock>() == 0x51C);

/// Team arena state area within DDGame (at DDGame + 0x4628).
///
/// Contains per-team entries and an interleaved ammo/delay table.
/// Used by GetAmmo (0x5225E0), AddAmmo (0x522640), SubtractAmmo (0x522680).
///
/// The ammo/delay table uses stride 142 (= 71 weapons * 2) per alliance.
/// Within each alliance block of 142 entries:
/// - Entries 0..70 are ammo counts (accessed at base + 0x1EB4 + index * 4)
/// - Entries 71..141 are delay flags (accessed at base + 0x1FD0 + index * 4)
///
/// The original code accesses these as two arrays at different base offsets
/// (0x1EB4 and 0x1FD0) using the same index `alliance_id * 142 + weapon_id`.
/// Since 0x1FD0 - 0x1EB4 = 71 * 4, this is equivalent to accessing
/// `weapon_slots[alliance * 142 + weapon]` for ammo and
/// `weapon_slots[alliance * 142 + 71 + weapon]` for delay.
#[repr(C)]
pub struct TeamArenaState {
    /// 0x0000-0x1EAF: Per-team data region (opaque).
    ///
    /// This region contains 7 team entries at stride 0x51C (1-indexed, index 0
    /// is preamble). The original WA.exe accesses team data via raw pointer
    /// arithmetic relative to the arena base. In our Rust code, team data is
    /// accessed through `TeamArenaRef` and `FullTeamBlock` using sentinel
    /// accessors, so this region is treated as opaque padding.
    ///
    /// The 7th entry (team 6, 1-indexed) only has 8 bytes before team_count;
    /// the rest overlaps with weapon_slots.
    pub _teams_region: [u8; 0x1EB0],
    /// 0x1EB0: Number of teams in the game (used by team iteration loops)
    pub team_count: i32,
    /// 0x1EB4: Interleaved ammo/delay slots.
    /// Per alliance: [ammo_0..ammo_70, delay_0..delay_70] = 142 i32 entries.
    /// 6 alliances * 142 = 852 total entries.
    /// Ammo: -1 = unlimited, 0 = none, >0 = count.
    /// Delay: nonzero = weapon on delay.
    pub weapon_slots: [i32; 852],
    /// 0x2C04: Padding between weapon_slots end and game_mode_flag.
    /// weapon_slots: 852 * 4 = 3408 = 0xD50. 0x1EB4 + 0xD50 = 0x2C04.
    pub _pad_2c04: [u8; 8],
    /// 0x2C0C: Game mode flag (nonzero = override weapon delays in certain conditions)
    pub game_mode_flag: i32,
    /// 0x2C10: Unknown padding
    pub _pad_2c10: [u8; 0x18],
    /// 0x2C28: Game phase counter (>=484 = sudden death, >=-2 = normal game)
    pub game_phase: i32,

    // === Alliance tracking (set by CountTeamsByAlliance + SetActiveWorm_Maybe) ===

    /// 0x2C2C: Current alliance being evaluated
    pub current_alliance: i32,
    /// 0x2C30: Count of teams with an active worm (set by SetActiveWorm_Maybe)
    pub active_worm_count: i32,
    /// 0x2C34: Count of active teams with valid alliance (>=0)
    pub active_team_count: i32,
    /// 0x2C38: Count of teams matching current_alliance
    pub same_alliance_count: i32,
    /// 0x2C3C: Count of teams not matching current_alliance
    pub enemy_team_count: i32,
    /// 0x2C40: Last team index set active (written by SetActiveWorm_Maybe)
    pub last_active_team: i32,
    /// 0x2C44: Alliance of last activated team (written by SetActiveWorm_Maybe)
    pub last_active_alliance: i32,
}

const _: () = assert!(core::mem::size_of::<TeamArenaState>() == 0x2C48);

/// Worm state constants and helpers.
pub mod worm {
    // --- Known state values (runtime-validated via validator dumps) ---

    /// Transitional state checked by CheckWormState0x64 (0x5228D0). 11 xrefs.
    /// Appears briefly during turn transitions.
    pub const STATE_TRANSITIONAL: u32 = 0x64;
    /// Idle — worm is waiting, not currently active.
    pub const STATE_IDLE: u32 = 0x65;
    /// Active — worm is currently selected and taking its turn.
    pub const STATE_ACTIVE: u32 = 0x67;
    /// Dead — worm has been killed (hp=0). Persists across turns.
    pub const STATE_DEAD: u32 = 0x87;

    /// Special worm states — worm is dying/drowning/in special animation.
    /// Checked by IsWormInSpecialState (0x5226B0).
    /// All values in the 0x80+ range. 0x87 (dead) is also in this set.
    pub const SPECIAL_STATES: [u32; 6] = [0x80, 0x81, 0x82, 0x83, 0x85, 0x89];

    /// Check if a worm state value is a "special" state.
    pub fn is_special_state(state: u32) -> bool {
        SPECIAL_STATES.contains(&state)
    }
}

/// Game phase thresholds (stored in TeamArenaState::game_phase).
pub const GAME_PHASE_SUDDEN_DEATH: i32 = 0x1E4; // 484
pub const GAME_PHASE_NORMAL_MIN: i32 = -2;


impl TeamArenaState {
    /// Get ammo count for a weapon slot (by flat index).
    pub fn get_ammo(&self, index: usize) -> i32 {
        self.weapon_slots[index]
    }

    /// Get mutable reference to ammo count for a weapon slot.
    pub fn ammo_mut(&mut self, index: usize) -> &mut i32 {
        &mut self.weapon_slots[index]
    }

    /// Get delay flag for a weapon slot (by flat index).
    /// Delay is at +71 offset from the ammo index within the same alliance block.
    pub fn get_delay(&self, index: usize) -> i32 {
        self.weapon_slots[index + 71]
    }
}

/// Typed handle to a TeamArenaState pointer received from WA.exe.
///
/// Wraps the raw `base: u32` from trampoline register captures and provides
/// accessor methods that encapsulate the backward pointer arithmetic to reach
/// FullTeamBlock worm data. The FullTeamBlock array lives 0x598 bytes before
/// the TeamArenaState in DDGame memory.
///
/// # Safety
/// Must only be constructed from a valid DDGame + TEAM_ARENA_STATE pointer.
/// `repr(transparent)` ensures identical ABI to `*const u8` / `u32` on i686,
/// so it can be received directly from usercall trampoline register captures.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct TeamArenaRef {
    base: *const u8,
}

impl TeamArenaRef {
    /// Wrap a raw base pointer (for non-trampoline contexts like validation).
    ///
    /// # Safety
    /// `base` must point to DDGame + TEAM_ARENA_STATE (0x4628).
    #[inline]
    pub unsafe fn from_raw(base: u32) -> Self {
        Self { base: base as *const u8 }
    }

    /// Access the TeamArenaState fields (read-only).
    #[inline]
    pub unsafe fn state(&self) -> &TeamArenaState {
        &*(self.base as *const TeamArenaState)
    }

    /// Access the TeamArenaState fields (mutable).
    #[inline]
    pub unsafe fn state_mut(&self) -> &mut TeamArenaState {
        &mut *(self.base as *mut TeamArenaState)
    }

    /// Get pointer to the FullTeamBlock array base.
    #[inline]
    pub unsafe fn blocks(&self) -> *const FullTeamBlock {
        self.base.sub(offsets::ARENA_TO_BLOCKS) as *const FullTeamBlock
    }

    /// Get the sentinel (metadata) entry for a team.
    ///
    /// Returns `block[team_idx+1].worms[0]`, which holds team metadata:
    /// worm_count (+0x78), eliminated flag (+0x6C).
    #[inline]
    pub unsafe fn team_sentinel(&self, team_idx: usize) -> &WormEntry {
        &(*self.blocks().add(team_idx + 1)).worms[0]
    }

    /// Get a playable worm entry by 1-indexed worm number (1..=8).
    ///
    /// Uses raw pointer arithmetic matching the original WA code:
    /// `base + team_idx * 0x51C + worm_num * 0x9C - 0x598`.
    /// This naturally crosses FullTeamBlock boundaries when worm_num = 8,
    /// since the 8th worm's early fields (state, health) spill into the
    /// next block's worms[0] — which is also the sentinel. The sentinel
    /// metadata lives at high offsets (0x6C, 0x78) that don't conflict.
    #[inline]
    pub unsafe fn team_worm(&self, team_idx: usize, worm_num: usize) -> &WormEntry {
        let ptr = self.base
            .add(team_idx * 0x51C)
            .add(worm_num * 0x9C)
            .sub(0x598);
        &*(ptr as *const WormEntry)
    }

    /// Get a team's worm block and its sentinel in one call.
    ///
    /// Returns `(block[team_idx], block[team_idx+1].worms[0])`.
    /// The block contains worm data (slots 1-7), and the sentinel (slot 0
    /// of the next block) holds team metadata (worm_count, eliminated flag).
    ///
    /// **Note**: For accessing worms, prefer `team_worm()` which handles
    /// 8-worm teams correctly via cross-boundary pointer arithmetic.
    #[inline]
    pub unsafe fn team_and_sentinel(&self, team_idx: usize) -> (&FullTeamBlock, &WormEntry) {
        let blocks = self.blocks();
        let block = &*blocks.add(team_idx);
        let sentinel = &(*blocks.add(team_idx + 1)).worms[0];
        (block, sentinel)
    }

    /// Get mutable sentinel for Pattern B access (alliance/active_worm at +0x70/+0x74).
    ///
    /// Pattern B indexes from block[i+2] for 0-indexed team `i`:
    /// `base + 0x510 + i*0x51C` = `blocks[i+2].worms[0] + 0x70`.
    #[inline]
    pub unsafe fn team_sentinel_b(&self, team_idx: usize) -> &WormEntry {
        &(*self.blocks().add(team_idx + 2)).worms[0]
    }

    /// Get mutable sentinel for Pattern B access.
    #[inline]
    pub unsafe fn team_sentinel_b_mut(&self, team_idx: usize) -> &mut WormEntry {
        &mut (*(self.blocks() as *mut FullTeamBlock).add(team_idx + 2)).worms[0]
    }

    /// Compute the flat index for ammo/delay table access.
    ///
    /// Reads the weapon alliance ID from the sentinel worm for the given
    /// 1-indexed team, then computes `alliance_id * 142 + weapon_id`.
    ///
    /// The weapon_slots array is interleaved: per alliance, 71 ammo slots
    /// then 71 delay slots (stride 142 per alliance).
    /// Ammo: `weapon_slots[alliance_id * 142 + weapon_id]`
    /// Delay: `weapon_slots[alliance_id * 142 + 71 + weapon_id]`
    #[inline]
    pub unsafe fn ammo_index(&self, team_index: usize, weapon_id: u32) -> usize {
        let alliance_id = self.team_sentinel(team_index).sentinel_weapon_alliance() as usize;
        alliance_id * 142 + weapon_id as usize
    }
}
