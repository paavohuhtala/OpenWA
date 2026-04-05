use super::base::DisplayBase;
use super::display_vtable::DisplayVtable;
use crate::bitgrid::DisplayBitGrid;
use core::ffi::c_void;

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
    pub base: DisplayBase<*const DisplayVtable>,

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
    // Bitmap/sprite storage (0x3580 - 0x358B)
    // =========================================================================
    /// 0x3580: Bitmap vector pointer (init 0)
    pub bitmap_ptr: u32,
    /// 0x3584: Bitmap vector end (init 0)
    pub bitmap_end: u32,
    /// 0x3588: Bitmap vector capacity (init 0)
    pub bitmap_capacity: u32,

    // =========================================================================
    // Palette entry table (0x358C - 0x3D8F)
    // =========================================================================
    /// 0x358C: Palette entry count or lead byte
    pub _unknown_358c: u8,
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
    /// 0x3DA8: BitGrid vtable pointer (0x664144). Set in constructor.
    pub bitgrid_vtable: *const c_void,
    /// 0x3DAC: Layer active flag (init 1)
    pub layer_active: u32,
    /// 0x3DB0: Unknown (init 0)
    pub _unknown_3db0: u32,
    /// 0x3DB4: Bit depth (init 8 in constructor — 8bpp paletted mode)
    pub bit_depth: u32,
    /// 0x3DB8 - 0x3DD3: Unknown fields (all init 0 in constructor)
    pub _unknown_3db8: [u8; 0x3DD4 - 0x3DB8],

    // =========================================================================
    // Sprite/bitmap table (0x3DD4 - 0x4DD7)
    // =========================================================================
    /// 0x3DD4: Sprite table — 1024 DWORD entries, zeroed in DisplayGfx__Init.
    /// Used for tracking loaded sprites/bitmaps by ID.
    pub sprite_table: [u32; 0x400],
    /// 0x4DD4: Sprite table metadata field 1 (init 0)
    pub sprite_meta_0: u32,
    /// 0x4DD8: Sprite table metadata field 2 (init 0)
    pub sprite_meta_1: u32,
    /// 0x4DDC - 0x4DF3: Unknown gap
    pub _unknown_4ddc: [u8; 0x4DF4 - 0x4DDC],

    // =========================================================================
    // Color mixing lookup tables (0x4DF4 - 0x24DF3)
    // =========================================================================
    /// 0x4DF4: Additive color mixing LUT (256 × 256 = 0x10000 bytes).
    /// Maps (color_a, color_b) → blended palette index using additive saturation.
    /// Built by DDGame__InitDisplayFinal (0x56A830).
    pub color_add_table: [u8; 0x10000],
    /// 0x14DF4: Gamma-corrected blend LUT (256 × 256 = 0x10000 bytes).
    /// Uses gamma-space blending via sqrt approximation.
    /// Built by DDGame__InitDisplayFinal (0x56A830).
    pub color_blend_table: [u8; 0x10000],

    // =========================================================================
    // Tail fields (0x24DF4 - 0x24E27)
    // =========================================================================
    /// 0x24DF4: Blend mode flag. Controls color distance weighting in
    /// InitDisplayFinal (1 = reduced red weight, else normal).
    pub blend_mode_flag: u32,
    /// 0x24DF8 - 0x24E07: Object vector (std::vector-like).
    /// Holds palette/display objects pushed during DisplayGfx__Init.
    pub object_vector_start: u32,
    /// 0x24DFC: Vector data pointer
    pub object_vector_ptr: u32,
    /// 0x24E00: Vector end pointer
    pub object_vector_end: u32,
    /// 0x24E04: Vector capacity pointer
    pub object_vector_cap: u32,
    /// 0x24E08 - 0x24E27: Remaining tail (0x20 bytes)
    pub _tail: [u8; 0x24E28 - 0x24E08],
}

const _: () = assert!(core::mem::size_of::<DisplayGfx>() == 0x24E28);

impl DisplayGfx {
    /// Allocate and construct a DisplayGfx via WA's native constructor.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::WABox;
        let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
            core::mem::transmute(rb(va::DISPLAYGFX_CTOR) as usize);
        ctor(WABox::<Self>::alloc(0x24E28, 0x24E08).leak())
    }
}
