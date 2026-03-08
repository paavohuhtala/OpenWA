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
    /// 0x54C-0x98B7: Remaining fields (sparse — see offsets module)
    ///
    /// Known landmarks in this region:
    /// - 0x64D8: cleared by init
    /// - 0x72A4: cleared by init
    /// - 0x730C-0x732C: 9 GfxDir color entries
    /// - 0x7338: fill pixel value
    /// - 0x77C4: display-related value
    /// - 0x7EF8: flag from game_state+0xF914
    /// - 0x7EFC: init 1
    /// - 0x8CBC-0x8CF0: 4x 0x10-byte entries (zeroed at +0, +4)
    /// - 0x9850-0x9884: 4x 0x10-byte entries (zeroed at +0, +4)
    ///
    /// Also includes FUN_00526120 zeroed offsets at stride 0x194:
    /// 0x379C, 0x3930, 0x3AC4, 0x3C58, 0x3DEC, 0x3F80, 0x4114, 0x42A8, 0x443C, 0x45D0
    pub _remaining: [u8; 0x936C],
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
    /// Base of TeamWeaponState sub-struct within DDGame.
    /// Callers pass DDGame + TEAM_WEAPON_STATE as base pointer to
    /// GetAmmo/AddAmmo/SubtractAmmo.
    pub const TEAM_WEAPON_STATE: usize = 0x4628;

    // === Team block array (7 × FullTeamBlock, stride 0x51C) ===
    /// Start of team block array within DDGame (7 blocks, stride 0x51C).
    /// Derived: entry_ptr(team=0) - 0x598 = 0x4628 - 0x598 = 0x4090.
    /// Runtime-confirmed: block[0] is zeroed preamble, blocks[1-6] hold team data.
    pub const TEAM_BLOCKS: usize = 0x4090;

    /// Byte offset from TeamWeaponState base back to FullTeamBlock array start.
    /// `blocks_ptr = (tws_base as *const u8).sub(TWS_TO_BLOCKS) as *const FullTeamBlock`
    ///
    /// entry_ptr(0) = DDGame+0x4628 = TEAM_BLOCKS + 0x598.
    /// 0x598 = sizeof(FullTeamBlock) + 0x7C = one block + offset into sentinel worm[0].
    pub const TWS_TO_BLOCKS: usize = 0x598;

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
}

// ============================================================
// Team weapon state — sub-struct at DDGame + 0x4628
// ============================================================

/// Per-team entry within the TeamWeaponState area.
///
/// Located at TeamWeaponState base + team_index * 0x51C.
/// Each team has an alliance_id that maps into shared ammo/delay tables.
///
/// Worm data lives BEFORE this entry:
/// - worm_count at offset -0x4
/// - worm array at offset -0x4A0 (stride 0x9C, health at [0])
#[repr(C)]
pub struct TeamEntry {
    pub _unknown_000: [u8; 4],
    /// Alliance ID — teams with the same alliance share ammo pools.
    /// Index into ammo/delay tables: alliance_id * 142 + weapon_id
    pub alliance_id: i32,
    pub _unknown_008: [u8; 0x514],
}

const _: () = assert!(core::mem::size_of::<TeamEntry>() == 0x51C);

// ============================================================
// Per-worm and per-team block structs
// ============================================================

/// Per-worm data entry (0x9C bytes, stride between consecutive worms).
///
/// 8 slots per team. Slot 0 is a sentinel/header (team metadata), slots 1-7
/// hold playable worm data. GetTeamTotalHealth iterates starting from slot 1.
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
}

/// Full per-team data block (0x51C bytes, 6 teams in DDGame).
///
/// Contains 8 WormEntry slots (0x4E0 bytes) followed by 0x3C bytes of
/// team metadata. Slot 0 is a sentinel/header; slots 1-7 are playable worms.
///
/// **Block indexing**: Block 0 is unused (preamble, all zeros). Actual team
/// data starts at block 1. entry_ptr(team=N) = DDGame+0x4628+N*0x51C, which
/// lands at block\[N+1\].worm\[0\]+0x7C. Negative offsets reach back into
/// block\[N\]'s worm data. So entry_ptr(1) accesses block\[1\]'s worms.
///
/// **Sentinel worm\[0\]** stores metadata for the team accessed by the
/// PREVIOUS entry_ptr index:
/// - +0x78: worm_count (accessed as entry_ptr-4)
/// - +0x80: alliance_id (accessed as entry_ptr+4)
/// - +0x84: team name string
#[repr(C)]
pub struct FullTeamBlock {
    /// 0x000-0x4DF: 8 worm entries (stride 0x9C)
    pub worms: [WormEntry; 8],
    /// 0x4E0-0x51B: Team metadata (0x3C bytes)
    pub _metadata: [u8; 0x3C],
}

const _: () = assert!(core::mem::size_of::<FullTeamBlock>() == 0x51C);

/// Team weapon state area within DDGame (at DDGame + 0x4628).
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
pub struct TeamWeaponState {
    /// 0x0000: Per-team entries (6 teams, stride 0x51C = 1308 bytes each)
    pub teams: [TeamEntry; 6],
    /// 0x1EA8: Padding
    pub _pad_1ea8: [u8; 0x8],
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

    // === Alliance tracking (set by CountTeamsByAlliance, 0x522030) ===

    /// 0x2C2C: Current alliance being evaluated
    pub current_alliance: i32,
    /// 0x2C30: Unknown padding
    pub _pad_2c30: [u8; 4],
    /// 0x2C34: Count of teams alive with valid alliance (>=0)
    pub active_team_count: i32,
    /// 0x2C38: Count of teams matching current_alliance
    pub same_alliance_count: i32,
    /// 0x2C3C: Count of teams not matching current_alliance
    pub enemy_team_count: i32,
}

const _: () = assert!(core::mem::size_of::<TeamWeaponState>() == 0x2C40);

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

/// Game phase thresholds (stored in TeamWeaponState::game_phase).
pub const GAME_PHASE_SUDDEN_DEATH: i32 = 0x1E4; // 484
pub const GAME_PHASE_NORMAL_MIN: i32 = -2;

/// Team data offsets within TeamWeaponState (relative to base pointer).
/// Used by CountTeamsByAlliance which accesses a different sentinel layout
/// than the entry_ptr-based functions (offset +0x70/+0x74 vs +0x78/+0x80).
pub mod team_data {
    /// Offset to first team's per-team data block (from TWS base).
    /// Maps to block[2].worms[0]+0x70 — a separate alliance/alive pair
    /// distinct from the entry_ptr-based alliance_id at worms[0]+0x80.
    pub const BASE_OFFSET: usize = 0x510;
    /// Alive flag within per-team data block (at +4 from team data start)
    pub const ALIVE_FLAG: usize = 4;
}

impl TeamWeaponState {
    /// Compute the flat index for ammo/delay table access.
    ///
    /// The weapon_slots array is interleaved: per alliance, 71 ammo slots
    /// then 71 delay slots (stride 142 per alliance).
    /// Ammo: `weapon_slots[alliance_id * 142 + weapon_id]`
    /// Delay: `weapon_slots[alliance_id * 142 + 71 + weapon_id]`
    pub unsafe fn ammo_index(&self, team_index: usize, weapon_id: u32) -> usize {
        let alliance_id = self.teams[team_index].alliance_id as usize;
        alliance_id * 142 + weapon_id as usize
    }

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
