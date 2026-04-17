use crate::{asset::gfx_dir::GfxDir, engine::ddgame::DDGame};

/// PCLandscape — terrain/landscape subsystem (0xB40 bytes).
///
/// Created by `PCLandscape__Constructor` (0x57ACB0). Vtable: 0x66B208 (32 slots).
///
/// Manages terrain pixel data (8-bit indexed/paletted), collision bitmap,
/// water effects, and level graphics. Loads `Water.dir` and `Level.dir`.
///
/// ## Rendering pipeline
/// - Terrain is stored as multiple pixel layers (DisplayGfx objects)
/// - Collision uses a 1-bit-per-pixel packed bitmap at +0x0D0
/// - Dirty rects queued via `RedrawLandRegion` (up to 256, 8 bytes each)
/// - `DrawLandscape` (0x5A2790) blits landscape pixels to framebuffer
///   (memcpy for opaque, per-pixel color-key for transparent)
/// - `WriteLandRaw` (0x57C300) modifies terrain across all layers + collision
///
/// ## Key dimensions (stored in DDGame, set by constructor)
/// - DDGame+0x77C0: level width
/// - DDGame+0x77C4: level height
/// - DDGame+0x77C8: width × height (total pixels)
///
/// Ghidra uses DWORD-indexed offsets (param_1[N] = byte offset N*4).
#[repr(C)]
pub struct PCLandscape {
    /// 0x000: Vtable pointer (0x66B208)
    pub vtable: *const PCLandscapeVtable,
    /// 0x004: Parent DDGame pointer (param_1[1])
    pub ddgame: *mut DDGame,
    /// 0x008-0x0CB: Pre-rendered crater sprites for 15 explosion sizes.
    /// param_1[2..17]: crater image ptrs, param_1[18..33]: secondary ptrs.
    /// Indexed by `explosion_size * 15 / 100`.
    pub crater_sprites: [*mut u8; 16],
    pub crater_sprites_secondary: [*mut u8; 16],
    /// 0x088-0x0CB: Unknown
    pub _unknown_088: [u8; 0x44],
    /// 0x0CC: Resource handle (param_1[0x33])
    pub resource_handle: *mut u8,
    /// 0x0D0: Collision bitmap pointer — 1 bit per pixel, packed into DWORDs.
    /// Width = (level_width + 7) / 8 rounded to 4-byte alignment.
    pub collision_bitmap: *mut u8,
    /// 0x0D4: Dirty rect array — 256 entries, each 8 bytes (x1,y1,x2,y2 as u16).
    /// Queued by `RedrawLandRegion`, flushed during frame render.
    pub dirty_rects: [DirtyRect; 256],
    /// 0x8D4: Number of dirty rects queued (max 256, overflows call flush)
    pub dirty_rect_count: u32,
    /// 0x8D8: Dirty flag — set to 1 when any rect is queued
    pub dirty_flag: u8,
    /// 0x8D9-0x8EB: Unknown
    pub _unknown_8d9: [u8; 0x13],
    /// 0x8EC: Unknown (zero at runtime)
    pub _unknown_8ec: u32,
    /// 0x8F0: Unknown (0x9B at runtime — small integer, not pointer)
    pub _unknown_8f0: u32,
    /// 0x8F4: Unknown (zero at runtime)
    pub _unknown_8f4: u32,
    /// 0x8F8-0x8FF: Unknown
    pub _unknown_8f8: [u8; 8],
    /// 0x900: Unknown (NOT DDGame — runtime value 0x13300048 doesn't match DDGame ptr)
    pub _unknown_900: *mut u8,
    /// 0x904: Initialized flag (param_1[0x241], set to 1)
    pub initialized: u32,
    /// 0x908: Terrain layer 0 — DisplayGfx* (collision visual / background)
    pub layer_0: *mut u8,
    /// 0x90C: Terrain layer 1 — DisplayGfx*
    pub layer_1: *mut u8,
    /// 0x910: Terrain layer 2 — DisplayGfx* (main terrain image).
    /// Pixel data at `*(layer_2 + 8)`, width at `*(layer_2 + 0x14)`,
    /// height at `*(layer_2 + 0x18)`, stride at `*(layer_2 + 0x10)`.
    pub layer_terrain: *mut u8,
    /// 0x914: Terrain layer 3 — DisplayGfx* (edge/shading layer)
    pub layer_edges: *mut u8,
    /// 0x918-0x91B: Unknown
    pub _unknown_918: [u8; 4],
    /// 0x91C: Terrain layer 4 — DisplayGfx* (shadow/overlay)
    pub layer_shadow: *mut u8,
    /// 0x920: Terrain layer 5 — DisplayGfx*
    pub layer_5: *mut u8,
    /// 0x924: Level directory path (char buffer, ~0x100 bytes)
    pub level_dir_path: [u8; 0x100],
    /// 0xA24: Theme/data directory path (char buffer, 0x100 bytes)
    pub theme_dir_path: [u8; 0x100],
    /// 0xB24-0xB2F: Visible bounds (left, top, right, bottom)
    pub visible_left: u32,
    pub visible_top: u32,
    pub visible_right: u32,
    pub visible_bottom: u32,
    /// 0xB34: GfxHandler for Level.dir (param_1[0x2CD])
    pub level_gfx_dir: *mut GfxDir,
    /// 0xB38: GfxHandler for Water.dir (param_1[0x2CE])
    pub water_gfx_dir: *mut GfxDir,
    /// 0xB3C: Control flag set by vtable slot 1 (donkey mode / landscape control)
    pub control_flag: u32,
}

const _: () = assert!(core::mem::size_of::<PCLandscape>() == 0xB40);

/// A dirty rectangle entry in the PCLandscape dirty rect queue.
/// Coordinates are in landscape pixel space.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirtyRect {
    pub x1: u16,
    pub y1: u16,
    pub x2: u16,
    pub y2: u16,
}

const _: () = assert!(core::mem::size_of::<DirtyRect>() == 8);

/// PCLandscape vtable (32 slots at 0x66B208).
#[openwa_game::vtable(size = 32, va = 0x0066_B208, class = "PCLandscape")]
pub struct PCLandscapeVtable {
    /// destructor (0x57B540, RET 0x4)
    #[slot(0)]
    pub destructor: fn(this: *mut PCLandscape, flags: u32) -> *mut PCLandscape,
    /// set control flag at +0xB3C (0x57BD10, RET 0x4)
    #[slot(1)]
    pub set_control_flag: fn(this: *mut PCLandscape, flag: u32),
    /// apply explosion crater (0x57C820, RET 0xC) — terrain destruction
    #[slot(2)]
    pub apply_explosion: fn(this: *mut PCLandscape, p1: u32, p2: u32, p3: u32),
    /// init landscape borders and layers (0x57D7F0, RET 0x20)
    #[slot(6)]
    pub init_borders: fn(
        this: *mut PCLandscape,
        p1: u32,
        p2: u32,
        p3: u32,
        p4: u32,
        p5: u32,
        p6: u32,
        p7: u32,
        p8: u32,
    ),
    /// redraw single row (0x57CF60, RET 0x4)
    #[slot(8)]
    pub redraw_row: fn(this: *mut PCLandscape, row: u32),
    /// get frame checksum component (0x57D540, plain RET)
    #[slot(18)]
    pub get_frame_checksum: fn(this: *mut PCLandscape) -> u32,
}

bind_PCLandscapeVtable!(PCLandscape, vtable);
