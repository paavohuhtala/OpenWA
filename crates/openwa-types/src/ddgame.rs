use crate::task::Ptr32;

/// DDGame — the main game engine object.
///
/// This is a massive ~39KB struct (0x98B8 bytes) that owns all major subsystems:
/// display, landscape, sound, graphics handlers, and task state machines.
///
/// Allocated in DDGame__Constructor (0x56E220).
/// The DDGame pointer is stored at DDGameWrapper+0x488 (DWORD index 0x122).
///
/// PARTIAL: Fields up to 0x510 are densely mapped from the constructor.
/// Beyond that, only scattered fields are known — use the `offsets` module.
///
/// Note on offsets: The constructor accesses DDGame fields via
/// `*(param_2[0x122] + byte_offset)` — these are byte offsets, NOT DWORD-indexed.
/// DWORD indexing only applies to param_2 (DDGameWrapper) itself.
#[repr(C)]
pub struct DDGame {
    /// 0x000: Base value (param_5 from constructor)
    pub _base_000: Ptr32,
    /// 0x004: Context pointer (param_3)
    pub _context: Ptr32,
    /// 0x008: param_4
    pub _param_008: Ptr32,
    /// 0x00C: Allocated object (0x608 bytes, conditional)
    pub _object_00c: Ptr32,
    /// 0x010: param_6
    pub _param_010: Ptr32,
    /// 0x014: param_7
    pub _param_014: Ptr32,
    /// 0x018: param_8
    pub _param_018: Ptr32,
    /// 0x01C: Caller/parent pointer (param_1)
    pub _caller: Ptr32,
    /// 0x020: PCLandscape pointer (copied from DDGameWrapper[0x133])
    pub landscape: Ptr32,
    /// 0x024: Game state pointer (param_10)
    pub game_state: Ptr32,
    /// 0x028: param_9
    pub _param_028: Ptr32,
    /// 0x02C: Secondary GfxDir object (0x70C bytes, conditional on GfxHandler 1)
    pub secondary_gfxdir: Ptr32,
    /// 0x030: Gradient image pointer
    pub gradient_image: Ptr32,
    /// 0x034: Gradient image 2 pointer
    pub gradient_image_2: Ptr32,
    /// 0x038-0x0B4: Arrow sprite object pointers (32 entries)
    pub arrow_sprites: [Ptr32; 32],
    /// 0x0B8-0x134: Arrow GfxDir pointers (32 entries, conditional)
    pub arrow_gfxdirs: [Ptr32; 32],
    /// 0x138: DisplayGfx object pointer (vtable 0x664144)
    pub display_gfx: Ptr32,
    /// 0x13C-0x37F: Unknown
    pub _unknown_13c: [u8; 0x244],
    /// 0x380: TaskStateMachine pointer (vtable 0x664118, 0x2C bytes)
    pub task_state_machine: Ptr32,
    /// 0x384-0x467: Unknown
    pub _unknown_384: [u8; 0xE4],
    /// 0x468: Landscape-derived value
    pub _landscape_val: Ptr32,
    /// 0x46C-0x488: 8 SpriteRegion pointers (0x9C bytes each, vtable 0x66B268)
    /// Created by SpriteRegion__Constructor (0x57DB20).
    /// Each contains 32 TaskStateMachine sub-objects.
    pub sprite_regions: [Ptr32; 8],
    /// 0x48C-0x508: Arrow collision region pointers (32 entries)
    pub arrow_collision_regions: [Ptr32; 32],
    /// 0x50C: Coordinate list object (capacity 600, 0x12C0 data buffer)
    pub coord_list: Ptr32,
    /// 0x510-0x98B7: Remaining fields (sparse — see offsets module)
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
    pub _remaining: [u8; 0x93A8],
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

    // === FUN_00526120 init offsets (stride 0x194, 10 entries) ===
    pub const INIT_TABLE_BASE: usize = 0x379C;
    pub const INIT_TABLE_STRIDE: usize = 0x194;

    // === Sparse fields in upper region ===
    pub const FIELD_64D8: usize = 0x64D8;
    pub const FIELD_72A4: usize = 0x72A4;
    pub const GFX_COLOR_ENTRIES: usize = 0x730C;
    pub const FILL_PIXEL: usize = 0x7338;
    pub const DISPLAY_77C4: usize = 0x77C4;
    pub const FLAG_7EF8: usize = 0x7EF8;
    pub const FIELD_7EFC: usize = 0x7EFC;
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
