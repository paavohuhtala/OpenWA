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
