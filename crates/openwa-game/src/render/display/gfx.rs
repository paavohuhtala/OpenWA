use super::base::DisplayBase;
use super::vtable::DisplayGfxVtable;
use crate::bitgrid::{BitGrid, BitGridBaseVtable, DisplayBitGrid};
use crate::render::sprite::{CBitmap, LayerSprite};

crate::define_addresses! {
    class "DisplayGfx" {
        /// `DisplayGfx::Destructor` (vtable slot 0 thunk, 0x569CE0) — thiscall.
        /// Calls `DestructorImpl` then `_free(this)` if `flags & 1`.
        fn/Thiscall DISPLAY_GFX_DESTRUCTOR = 0x00569CE0;
        /// `DisplayGfx::DestructorImpl` (0x56A010) — fastcall(ECX=this).
        /// The actual cleanup body. Wrapped in C++ SEH; rebinds vtable to
        /// the most-derived DisplayGfx vtable (standard MSVC pattern), then
        /// frees layer sprites, tile bitmap set, CBitmap vec, layers, hook
        /// vec, embedded BitGrid, and chains to `DisplayBase::Destructor`.
        fn/Fastcall DISPLAY_GFX_DESTRUCTOR_IMPL = 0x0056A010;
        /// `DisplayGfx::FreeLayerSpriteTable` (0x56A280) — fastcall(ECX=this).
        /// Iterates `sprite_table[1..0x3FF]` at `+0x3DD4` and frees each
        /// `LayerSprite` + its `frame_array`.
        fn/Fastcall DISPLAY_GFX_FREE_LAYER_SPRITE_TABLE = 0x0056A280;
        /// `DisplayGfx::DispatchFramePostProcessHooks` (0x0056CDB0) —
        /// stdcall(display), RET 0x4. Per-frame poll/dispatch over the
        /// `FramePostProcessHook*` vector at `+0x24DF8`. Bridged from
        /// `engine::main_loop::render_frame::render_frame`.
        fn/Stdcall DISPLAY_GFX_DISPATCH_FRAME_POST_PROCESS_HOOKS = 0x0056CDB0;
    }

    class "TileBitmapSet" {
        /// `TileBitmapSet::Destructor` (0x569BC0) — fastcall(ECX=this).
        /// Iterates `bitmap_ptrs[0..count]`, calls `vtable[0](1)` on each,
        /// frees `bitmap_ptrs`.
        fn/Fastcall TILE_BITMAP_SET_DESTRUCTOR = 0x00569BC0;
    }

    class "DisplayBase" {
        /// `DisplayBase::Destructor` (0x522F60) — thiscall(ECX=this).
        /// Parent class destructor; rebinds vtable to DisplayBase
        /// (`0x6645F8`) and tears down sprite cache, palette slots, etc.
        /// Chained from `DisplayGfx::DestructorImpl` as the final step.
        fn/Thiscall DISPLAY_BASE_DESTRUCTOR_IMPL = 0x00522F60;
    }
}

/// DisplayGfx — full display/graphics subsystem (derived from DisplayBase).
///
/// Constructor: DisplayGfx__Constructor (0x569C10), stdcall(this) → DisplayGfx*.
/// Initializer: DisplayGfx__Init (0x569D00), usercall.
/// Size: 0x24E28 bytes.
///
/// Inheritance: DisplayBase (0x3560) → DisplayGfx (0x24E28).
/// The constructor calls DisplayBase__Constructor first, then sets the
/// DisplayGfx vtable (0x66A218) and initializes display-specific fields.
///
/// Created by GameEngine__InitHardware in normal (non-headless) mode.
/// Stored in the session's `display` field (shared with DisplayBase in headless).
///
/// ## Memory layout overview
///
/// ```text
/// 0x0000 - 0x355F : DisplayBase (sprite cache, slot table, etc.)
/// 0x3540          : display_initialized flag
/// 0x3548 - 0x355F : display dimensions and clip rect
/// 0x3560 - 0x3577 : camera offset, rendering state
/// 0x3578          : HWND
/// 0x3580 - 0x358B : bitmap vector
/// 0x358C - 0x398C : palette entry table (256 × 4 bytes)
/// 0x3D90 - 0x3D97 : palette metadata
/// 0x3D98          : render lock flag
/// 0x3D9C - 0x3DA7 : three layer object pointers (DisplayGfx vtable 0x664144)
/// 0x3DA8 - 0x3DD3 : DisplayGfx vtable ptr, layer config
/// 0x3DD4 - 0x4DD3 : sprite/bitmap table (1024 entries)
/// 0x4DD4 - 0x4DF3 : sprite table metadata
/// 0x4DF4 - 0x14DF3: color_add_table (256 × 256 additive mixing LUT)
/// 0x14DF4- 0x24DF3: color_blend_table (256 × 256 gamma-corrected blend LUT)
/// 0x24DF4- 0x24E27: tail fields (blend mode flag, object vector)
/// ```
#[repr(C)]
pub struct DisplayGfx {
    // =========================================================================
    // DisplayBase (0x0000 - 0x355F)
    // =========================================================================
    /// 0x0000: DisplayBase fields (vtable, sprite cache, slot table, etc.)
    pub base: DisplayBase<*const DisplayGfxVtable>,

    // =========================================================================
    // Display dimensions and clip rect (0x3560 - 0x357F)
    // =========================================================================
    /// 0x3560: Camera X offset (pixels). DisplayGfx methods add this to coordinates.
    pub camera_x: i32,
    /// 0x3564: Camera Y offset (pixels).
    pub camera_y: i32,
    /// 0x3568: Unknown (set to 0 in InitDisplayFinal)
    pub _unknown_3568: u32,
    /// 0x356C: Unknown (set from FUN_00541340 result in InitDisplayFinal)
    pub _unknown_356c: u32,
    /// 0x3570: Unknown (set to 0 in InitDisplayFinal)
    pub _unknown_3570: u32,
    /// 0x3574: Unknown (set to 0 in DisplayGfx__Init)
    pub _unknown_3574: u32,
    /// 0x3578: Window handle (HWND), used for MoveWindow in DisplayGfx__Init
    pub hwnd: u32,
    /// 0x357C: Unknown
    pub _unknown_357c: u32,

    // =========================================================================
    // Tile-cache bitmap vector (0x3580 - 0x358B)
    // =========================================================================
    /// 0x3580: Tile-cache `CBitmap*` vector — start pointer (init 0).
    /// Populated by `DisplayGfx::DrawTiledBitmap` (slot 11) on its first
    /// call: one entry per 0x400-row strip of the source landscape.
    pub bitmap_ptr: *mut *mut crate::render::sprite::CBitmap,
    /// 0x3584: Tile-cache vector end pointer (init 0). `(end - ptr) >> 2`
    /// = current entry count.
    pub bitmap_end: *mut *mut crate::render::sprite::CBitmap,
    /// 0x3588: Tile-cache vector capacity-end pointer (init 0).
    pub bitmap_capacity: *mut *mut crate::render::sprite::CBitmap,

    // =========================================================================
    // Palette entry table (0x358C - 0x3D8F)
    // =========================================================================
    /// 0x358C: Tile-cache populate flag. Set to 0 by `DrawTiledBitmap`'s
    /// allocate phase, set to 1 once the populate phase has filled every
    /// strip's surface. While 0, the next `DrawTiledBitmap` call repopulates
    /// the cached tile bitmaps from the source descriptor.
    pub tile_cache_populated: u8,
    /// 0x358D - 0x398C: Palette entries (256 × 4 bytes = 0x400 bytes).
    /// Each entry is 4 bytes (R, G, B, flags?). Entry 255 (0x3989) = white.
    pub palette_entries: [u8; 0x400],
    /// 0x398D - 0x3D8F: Unknown region between palette entries and palette metadata
    pub _unknown_398d: [u8; 0x3D90 - 0x398D],

    // =========================================================================
    // Palette metadata and render state (0x3D90 - 0x3D97)
    // =========================================================================
    /// 0x3D90: Lowest dirty palette index (init 0x100 = none dirty).
    /// Updated by `update_palette`, reset to 0x100 after palette commit.
    pub palette_dirty_min: u32,
    /// 0x3D94: Highest dirty palette index (init 0xFFFFFFFF = none dirty).
    /// Updated by `update_palette`, reset to 0xFFFFFFFF after palette commit.
    pub palette_dirty_max: u32,

    // =========================================================================
    // Render state and layer pointers (0x3D98 - 0x3DD3)
    // =========================================================================
    /// 0x3D98: Render lock flag. Set during rendering, cleared by FlushRender.
    pub render_lock: u32,
    /// 0x3D9C: Layer 0 — rendering context BitGrid (vtable 0x664144).
    ///
    /// Allocated as 0x4C bytes (0x2C BitGrid + 0x20 unknown tail), but all observed
    /// access is within BitGrid offsets (0x00-0x28). Initialized with `external_buffer=1`
    /// and `cells_per_unit=8`; the acquire-render-lock helper (0x56A370) populates
    /// `data`/`width`/`height`/`row_stride` from the locked DisplayGfxWrapper surface.
    /// `set_clip_rect` mirrors DisplayBase's clip rect into this BitGrid's clip fields.
    pub layer_0: *mut DisplayBitGrid,
    /// 0x3DA0: Layer 1 — same layout as layer_0.
    pub layer_1: *mut DisplayBitGrid,
    /// 0x3DA4: Layer 2 — same layout, but also initialized via BitGrid::Init(8, 128, 128).
    pub layer_2: *mut DisplayBitGrid,
    /// 0x3DA8 - 0x3DD3: Embedded `BitGrid` sub-object (0x2C bytes).
    ///
    /// A BitGrid allocated INLINE in DisplayGfx, distinct from the three
    /// heap-allocated layer pointers above. Confirmed by `DisplayGfx::DestructorImpl`
    /// (0x56A010), which rebinds `+0x3DA8` to `&BitGrid__vtable` and frees
    /// `+0x3DB0` (= BitGrid::data) if `+0x3DAC` (= BitGrid::external_buffer) is 0.
    /// The constructor sets `+0x3DAC = 1` (external_buffer) and `+0x3DB4 = 8`
    /// (cells_per_unit), matching the standard BitGrid layout exactly.
    pub embedded_bitgrid: BitGrid<*const BitGridBaseVtable>,

    // =========================================================================
    // Sprite/bitmap table (0x3DD4 - 0x4DD7)
    // =========================================================================
    /// 0x3DD4: Bitmap-sprite table — 1024 entries, zeroed in DisplayGfx__Init.
    /// Each non-null entry is a `LayerSprite` allocated by `load_sprite_by_layer`
    /// (vtable slot 37). Read by `GetBitmapSpriteInfo` (slot's helper, used
    /// by the bitmap-sprite branch of `BlitSprite` / slot 19).
    pub sprite_table: [*mut LayerSprite; 0x400],
    // =========================================================================
    // Tile stream config (0x4DD4 - 0x4DF3)
    // =========================================================================
    /// 0x4DD4: Bitmap tile set pointers, indexed by mode (0-based).
    /// draw_tiled_terrain (slot 22) uses mode 1 (index 1, at offset 0x4DD8). Init 0.
    pub tile_bitmap_sets: [*const TileBitmapSet; 2],
    /// 0x4DDC: Total tile grid width in pixels. Inner loop iterates
    /// x from 0 to this value in steps of `tile_col_width`.
    pub tile_total_width: i32,
    /// 0x4DE0: Total tile grid height in pixels. Outer loop iterates
    /// y from 0 to this value in steps of `tile_row_height`.
    /// Also used to clamp the `count` parameter in draw_tiled_terrain.
    pub tile_total_height: i32,
    /// 0x4DE4: Width of each tile column in pixels.
    pub tile_col_width: i32,
    /// 0x4DE8: Height of each tile row in pixels.
    pub tile_row_height: i32,
    /// 0x4DEC - 0x4DF3: Unknown (2 u32s)
    pub _unknown_4dec: [u32; 2],

    // =========================================================================
    // Color mixing lookup tables (0x4DF4 - 0x24DF3)
    // =========================================================================
    /// 0x4DF4: Additive color mixing LUT (256 × 256 = 0x10000 bytes).
    /// Maps (color_a, color_b) → blended palette index using additive saturation.
    /// Built by GameWorld__InitDisplayFinal (0x56A830).
    pub color_add_table: [u8; 0x10000],
    /// 0x14DF4: Gamma-corrected blend LUT (256 × 256 = 0x10000 bytes).
    /// Uses gamma-space blending via sqrt approximation.
    /// Built by GameWorld__InitDisplayFinal (0x56A830).
    pub color_blend_table: [u8; 0x10000],

    // =========================================================================
    // Tail fields (0x24DF4 - 0x24E27)
    // =========================================================================
    /// 0x24DF4: Blend mode flag. Controls color distance weighting in
    /// InitDisplayFinal (1 = reduced red weight, else normal).
    pub blend_mode_flag: u32,
    /// 0x24DF8 - 0x24E07: `std::vector<FramePostProcessHook*>` (16-byte
    /// MSVC 2005 layout with the iterator-debug proxy at offset 0).
    ///
    /// ```text
    /// +0x00 (= +0x24DF8) _Myproxy : *const c_void                       — debug iterator container proxy
    /// +0x04 (= +0x24DFC) _Myfirst : *mut *mut FramePostProcessHook       — start
    /// +0x08 (= +0x24E00) _Mylast  : *mut *mut FramePostProcessHook       — end
    /// +0x0C (= +0x24E04) _Myend   : *mut *mut FramePostProcessHook       — capacity end
    /// ```
    ///
    /// Populated during `DisplayGfx::Init` with the registered post-process
    /// hooks (the only known shipping entry is `ScreenshotHook`, which writes
    /// the rendered frame as a numbered PNG when a screenshot is requested).
    /// Iterated every frame by `DispatchFramePostProcessHooks` (0x56CDB0).
    /// Until the FramePostProcessHook type lands the entry pointer is left
    /// as `*mut u8`.
    pub hook_vec_proxy: *const core::ffi::c_void,
    /// 0x24DFC: hook vec `_Myfirst` — start pointer.
    pub hook_vec_first: *mut *mut u8,
    /// 0x24E00: hook vec `_Mylast` — end pointer.
    pub hook_vec_last: *mut *mut u8,
    /// 0x24E04: hook vec `_Myend` — capacity end pointer.
    pub hook_vec_end: *mut *mut u8,
    /// 0x24E08 - 0x24E27: Remaining tail (0x20 bytes)
    pub _tail: [u8; 0x24E28 - 0x24E08],
}

const _: () = assert!(core::mem::size_of::<DisplayGfx>() == 0x24E28);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, embedded_bitgrid) == 0x3DA8);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, sprite_table) == 0x3DD4);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, tile_bitmap_sets) == 0x4DD4);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, tile_total_width) == 0x4DDC);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, tile_total_height) == 0x4DE0);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, tile_col_width) == 0x4DE4);
const _: () = assert!(core::mem::offset_of!(DisplayGfx, tile_row_height) == 0x4DE8);

impl DisplayGfx {
    /// Allocate and construct a DisplayGfx via WA's native constructor.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        unsafe {
            use crate::address::va;
            use crate::rebase::rb;
            use crate::wa_alloc::wa_malloc_struct_zeroed;
            let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
                core::mem::transmute(rb(va::DISPLAYGFX_CTOR) as usize);
            ctor(wa_malloc_struct_zeroed::<Self>())
        }
    }
}

/// Tile bitmap set — holds an array of bitmap pointers for tiled rendering.
///
/// Pointed to by `DisplayGfx::tile_bitmap_sets`. Used by draw_tiled_terrain (slot 22)
/// to iterate over bitmap tiles in a grid pattern.
///
/// Each `bitmap_ptrs[i]` is a [`CBitmap`] — the same struct slot 11's
/// `bitmap_vec` uses. WA's BlitBitmapClipped (`0x56A700`) and the inner
/// FUN_00403c60 read `+0x4` (the `surface` field) on every entry; the
/// vtable+surface+pad layout matches.
#[repr(C)]
pub struct TileBitmapSet {
    /// 0x00: Bitmap count or total size (observed values: 0x168 = 360)
    pub count: u32,
    /// 0x04: Pointer to array of CBitmap pointers (one per grid tile, row-major).
    pub bitmap_ptrs: *const *mut CBitmap,
}
