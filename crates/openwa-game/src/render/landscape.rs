use core::arch::naked_asm;

use crate::{
    asset::gfx_dir::GfxDir,
    bitgrid::DisplayBitGrid,
    engine::{ddgame::DDGame, ddgame_wrapper::DDGameWrapper},
    rebase::rb,
};

crate::define_addresses! {
    /// `Landscape__FlushDirtyRects` (usercall, EDI=this, plain RET).
    /// Drains all queued dirty rects through `Landscape+0x900->vtable[39]`.
    fn/Usercall LANDSCAPE_FLUSH_DIRTY_RECTS = 0x0057CBA0;
}

/// Landscape — terrain/landscape subsystem (0xB40 bytes).
///
/// Created by `Landscape__Constructor` (0x57ACB0). Vtable: 0x66B208 (32 slots).
///
/// Manages terrain pixel data (8-bit indexed/paletted), collision bitmap,
/// water effects, and level graphics. Loads `Water.dir` and `Level.dir`.
///
/// ## Rendering pipeline
/// - Terrain is stored as multiple 8bpp pixel layers (`DisplayBitGrid`)
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
pub struct Landscape {
    /// 0x000: Vtable pointer (0x66B208)
    pub vtable: *const LandscapeVtable,
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
    /// 0x8D8: "Incremental dirty pending" flag.
    /// `RedrawLandRegion` (0x57CC10) sets this to 1 after queueing a small
    /// region. `Landscape::init_borders` (0x57D7F0) clears it to 0 after
    /// queueing the full-screen rect — the full redraw supersedes any
    /// incremental tracking, so there's nothing for the renderer to chase.
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
    /// 0x908: Terrain layer 0 — written by the landscape image loader
    /// (`SoundEmitter__Constructor_Maybe` at the top of the Landscape ctor —
    /// name appears mislabeled in Ghidra; it returns 8bpp BitGrid layers).
    pub layer_0: *mut DisplayBitGrid,
    /// 0x90C: Terrain layer 1 — `wa_malloc(0x4C) + BitGrid::init(8)` with
    /// the DisplayGfx (BitGridDisplay) vtable.
    pub layer_1: *mut DisplayBitGrid,
    /// 0x910: Main terrain image (8bpp). The Landscape ctor reads width and
    /// height from this layer directly into `DDGame.level_width / level_height`.
    pub layer_terrain: *mut DisplayBitGrid,
    /// 0x914: Edge / collision-mask layer. `Landscape::init_borders` stamps
    /// `1` into every border-region pixel here.
    pub layer_edges: *mut DisplayBitGrid,
    /// 0x918-0x91B: Unknown
    pub _unknown_918: [u8; 4],
    /// 0x91C: Shader / overlay layer. The vtable can be `LandscapeShader`
    /// (offline path — slot 5 is a no-op stub) or `BitGridDisplay`
    /// (online path) depending on the value of `DDGame+0x2C` at constructor
    /// time. Both share the BitGrid base layout, so a `DisplayBitGrid` typing
    /// works for either via `put_pixel_clipped_raw` dispatch.
    pub layer_shadow: *mut DisplayBitGrid,
    /// 0x920: Secondary shader / overlay layer — same conditional construction
    /// as `layer_shadow`.
    pub layer_5: *mut DisplayBitGrid,
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

const _: () = assert!(core::mem::size_of::<Landscape>() == 0xB40);

/// A dirty rectangle entry in the Landscape dirty rect queue.
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

/// Landscape vtable (32 slots at 0x66B208).
#[openwa_game::vtable(size = 32, va = 0x0066B208, class = "Landscape")]
pub struct LandscapeVtable {
    /// destructor (0x57B540, RET 0x4)
    #[slot(0)]
    pub destructor: fn(this: *mut Landscape, flags: u32) -> *mut Landscape,
    /// set control flag at +0xB3C (0x57BD10, RET 0x4)
    #[slot(1)]
    pub set_control_flag: fn(this: *mut Landscape, flag: u32),
    /// apply explosion crater (0x57C820, RET 0xC) — terrain destruction
    #[slot(2)]
    pub apply_explosion: fn(this: *mut Landscape, p1: u32, p2: u32, p3: u32),
    /// Stamp the indestructible-borders pattern onto the terrain layers.
    /// (0x57D7F0, RET 0x20)
    ///
    /// For each enabled side, paints an 8-pixel-wide diagonal-stripe brick
    /// pattern across all three landscape layers (`+0x910` terrain, `+0x914`
    /// edges, `+0x91C` shader/shadow). The selector is uniform across all 4
    /// sides: `((x - y + 4) >> 3) & 1` — sel=0 picks the "_a" colors,
    /// sel=1 picks the "_b" colors. The edges layer is stamped with the
    /// constant `1`. After painting, appends a full-screen dirty rect.
    #[slot(6)]
    pub init_borders: fn(
        this: *mut Landscape,
        left: u32,
        right: u32,
        top: u32,
        bottom: u32,
        terrain_color_b: u32,
        terrain_color_a: u32,
        shader_color_b: u32,
        shader_color_a: u32,
    ),
    /// redraw single row (0x57CF60, RET 0x4)
    #[slot(8)]
    pub redraw_row: fn(this: *mut Landscape, row: u32),
    /// get frame checksum component (0x57D540, plain RET)
    #[slot(18)]
    pub get_frame_checksum: fn(this: *mut Landscape) -> u32,
}

bind_LandscapeVtable!(Landscape, vtable);

/// Pure Rust implementation of `InitLandscapeBorders` (0x00528480).
///
/// Applies the scheme's cavern / indestructible-borders flag to the landscape.
/// Convention: usercall(EAX = `this`), plain RET. `this` is a CTaskTurnGame
/// (`DDGameWrapper`); the body only touches `this->ddgame` (+0x488).
///
/// If the scheme byte at `GameInfo+0xD94B` is set, dispatches
/// `Landscape::init_borders(1,1,1,1, colors)` and flips `ddgame.is_cavern` on.
/// Otherwise, if `is_cavern` was previously set, dispatches
/// `init_borders(0,0,1,0, colors)` to tear down the borders.
pub unsafe fn init_landscape_borders(wrapper: *mut DDGameWrapper) {
    unsafe {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info;

        let scheme_flag = (*game_info).landscape_scheme_flag;

        let terrain_color_b = (*ddgame).gfx_color_table[3];
        let terrain_color_a = (*ddgame).gfx_color_table[0];
        let shader_color_b = (*ddgame).border_shader_color_b;
        let shader_color_a = (*ddgame).border_shader_color_a;

        let landscape = (*ddgame).landscape;

        if scheme_flag != 0 {
            Landscape::init_borders_raw(
                landscape,
                1,
                1,
                1,
                1,
                terrain_color_b,
                terrain_color_a,
                shader_color_b,
                shader_color_a,
            );
            (*ddgame).is_cavern = 1;
        } else if (*ddgame).is_cavern != 0 {
            Landscape::init_borders_raw(
                landscape,
                0,
                0,
                1,
                0,
                terrain_color_b,
                terrain_color_a,
                shader_color_b,
                shader_color_a,
            );
        }
    }
}

/// Bridge to WA's `Landscape__FlushDirtyRects` (0x57CBA0). Usercall: EDI = this,
/// plain RET, no other args. Drains the dirty-rect queue through
/// `landscape+0x900->vtable[39]`, then resets `dirty_rect_count` to 0.
///
/// Wraps a naked cdecl trampoline that loads `this` into EDI and calls the
/// rebased target address (passed as a parameter per the project's bridge rule).
unsafe fn flush_dirty_rects(this: *mut Landscape) {
    unsafe {
        flush_dirty_rects_trampoline(this, rb(LANDSCAPE_FLUSH_DIRTY_RECTS));
    }
}

#[unsafe(naked)]
unsafe extern "cdecl" fn flush_dirty_rects_trampoline(_this: *mut Landscape, _addr: u32) {
    naked_asm!(
        "push edi",
        "mov edi, [esp + 8]",  // this
        "mov eax, [esp + 12]", // target
        "call eax",
        "pop edi",
        "ret",
    );
}

/// Pure Rust implementation of `Landscape::init_borders` (0x57D7F0, vtable
/// slot 6). See [`LandscapeVtable::init_borders`] for semantics.
///
/// Calling convention: thiscall (ECX = this) + 8 stack args, RET 0x20.
pub unsafe extern "thiscall" fn landscape_init_borders_impl(
    this: *mut Landscape,
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
    terrain_color_b: u32,
    terrain_color_a: u32,
    shader_color_b: u32,
    shader_color_a: u32,
) {
    unsafe {
        let layer_terrain = (*this).layer_terrain;
        let layer_edges = (*this).layer_edges;
        let layer_shader = (*this).layer_shadow;

        let width = (*layer_terrain).width as i32;
        let height = (*layer_terrain).height as i32;

        let terrain_pair = [terrain_color_a as u8, terrain_color_b as u8];
        let shader_pair = [shader_color_a as u8, shader_color_b as u8];

        // The diagonal stripe selector is uniform across all 4 border regions:
        // sel = ((x - y + 4) >> 3) & 1. (WA stores the same value as a per-row
        // counter that decrements in sync with y; both forms are equivalent.)
        let stamp = |x: i32, y: i32| {
            let sel = (((x - y + 4) >> 3) & 1) as usize;
            DisplayBitGrid::put_pixel_clipped_raw(layer_edges, x, y, 1);
            DisplayBitGrid::put_pixel_clipped_raw(layer_terrain, x, y, terrain_pair[sel]);
            DisplayBitGrid::put_pixel_clipped_raw(layer_shader, x, y, shader_pair[sel]);
        };

        if left != 0 {
            (*this).visible_left = 8;
            for y in 0..height {
                for x in 0..8i32 {
                    stamp(x, y);
                }
            }
        }

        if right != 0 {
            let x_start = width - 8;
            (*this).visible_right = x_start as u32;
            if x_start < width {
                for y in 0..height {
                    for x in x_start..width {
                        stamp(x, y);
                    }
                }
            }
        }

        if top != 0 {
            (*this).visible_top = 8;
            for y in 0..8i32 {
                for x in 0..width {
                    stamp(x, y);
                }
            }
        }

        if bottom != 0 {
            let y_start = height - 8;
            (*this).visible_bottom = y_start as u32;
            if y_start < height {
                for y in y_start..height {
                    for x in 0..width {
                        stamp(x, y);
                    }
                }
            }
        }

        push_dirty_rect(this, 0, 0, width as u16, height as u16);
        // Full-screen rect supersedes any pending incremental work — see the
        // `dirty_flag` field comment.
        (*this).dirty_flag = 0;
    }
}

/// Append a dirty rect to the Landscape's queue, draining first if it's
/// already at capacity. Caller is responsible for `dirty_flag` semantics.
unsafe fn push_dirty_rect(this: *mut Landscape, x1: u16, y1: u16, x2: u16, y2: u16) {
    unsafe {
        if (*this).dirty_rect_count >= 256 {
            flush_dirty_rects(this);
        }
        let idx = (*this).dirty_rect_count as usize;
        (*this).dirty_rects[idx] = DirtyRect { x1, y1, x2, y2 };
        (*this).dirty_rect_count += 1;
    }
}
