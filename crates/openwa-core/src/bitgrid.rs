//! BitGrid — general-purpose 2D grid buffer.
//!
//! A polymorphic pixel/bit buffer used across WA's rendering and physics systems.
//! The same struct layout serves multiple roles, distinguished by vtable:
//!
//! | Vtable     | Type                | Role                         |
//! |------------|---------------------|------------------------------|
//! | 0x6640EC   | `BitGrid`           | Base — set by `init`, passive data container |
//! | 0x664118   | `CollisionBitGrid`  | Collision grid (DDGame+0x380), bit-level spatial queries |
//! | 0x664144   | `DisplayBitGrid`    | Display layers and sprite pixel buffers, 8bpp byte-level ops |
//!
//! All three vtables share the same 8-slot interface:
//!
//! | Slot | Method           | Base          | Collision     | Display       |
//! |------|------------------|---------------|---------------|---------------|
//! | 0    | fill_rect        | bit-level     | bit-level     | byte-level    |
//! | 1    | fill_hline       | bit-level *   | bit-level *   | byte-level    |
//! | 2    | fill_vline       | bit-level *   | bit-level *   | byte-level    |
//! | 3    | destructor       | shared *      | shared *      | shared *      |
//! | 4    | get_clipped      | bit-level     | bit-level     | byte-level    |
//! | 5    | put_clipped      | bit-level     | bit-level     | byte-level    |
//! | 6    | get              | stub          | bit-level     | byte-level    |
//! | 7    | put              | stub          | bit-level     | byte-level    |
//!
//! * = same function pointer shared between variants.
//!
//! ## Initialization pattern
//!
//! `BitGrid::init` allocates the data buffer, fills in all fields, and sets
//! the base vtable (0x6640EC). Callers then cast and override the vtable:
//!
//! ```text
//! BitGrid::init(grid, cells_per_unit, width, height);
//! let display_grid = grid as *mut DisplayBitGrid;
//! (*display_grid).vtable = rb_ptr(BIT_GRID_DISPLAY_VTABLE);
//! ```

use crate::rebase::rb;
use crate::task::base::Vtable;
use crate::wa_alloc::{wa_malloc, wa_malloc_struct_zeroed};
use crate::FieldRegistry;

crate::define_addresses! {
    class "BitGrid" {
        /// BitGrid::init (0x4F6370) — allocates buffer, sets base vtable
        fn BIT_GRID_INIT = 0x004F_6370;
        /// Core sprite blit (0x4F6910) — ESI=dst BitGrid, 9 stack params
        fn/Usercall BLIT_SPRITE_RECT = 0x004F_6910;
        /// Clipped line draw on 8bpp BitGrid (0x4F7500)
        fn DRAW_LINE_CLIPPED = 0x004F_7500;
        /// Two-color line draw on 8bpp BitGrid (0x4F7A60)
        fn DRAW_LINE_TWO_COLOR = 0x004F_7A60;
        /// `DisplayBitGrid::SetExternalBuffer` (FUN_004F6470) — fastcall
        /// `(ECX=height, EDX=width, stack=(bitgrid, data, row_stride))`.
        /// Updates an external-buffer bitgrid's data pointer + dimensions
        /// + clip rect. Only known caller is `SpriteBank__GetFrameForBlit`,
        /// which is itself unreachable in shipping WA — see slot 33.
        fn/Fastcall DISPLAY_BIT_GRID_SET_EXTERNAL_BUFFER = 0x004F_6470;
    }
}

/// BitGrid — general-purpose 2D grid buffer.
///
/// Generic over vtable pointer type `V`:
/// - `BitGrid` (default): base form with passive data vtable (0x6640EC).
/// - `CollisionBitGrid`: collision variant for spatial queries (0x664118).
/// - `DisplayBitGrid`: display variant for 8bpp pixel rendering (0x664144).
///
/// Size: 0x2C bytes (base). Display layers are allocated at 0x4C bytes, with
/// the extra 0x20 bytes holding layer-specific state beyond this struct.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct BitGrid<V: Vtable = *const BitGridBaseVtable> {
    /// 0x00: Vtable pointer. Set by `init` to base (0x6640EC),
    /// then overridden to collision (0x664118) or display (0x664144).
    pub vtable: V,
    /// 0x04: External buffer flag. 0 = BitGrid owns data (frees on destroy),
    /// nonzero = external ownership.
    pub external_buffer: u32,
    /// 0x08: Pixel/bit data pointer (row-major, row-aligned to 4 bytes)
    pub data: *mut u8,
    /// 0x0C: Cells per unit — 1 for bit grids, 8 for 8bpp pixel buffers
    pub cells_per_unit: u32,
    /// 0x10: Row stride in bytes
    pub row_stride: u32,
    /// 0x14: Width in pixels/cells
    pub width: u32,
    /// 0x18: Height in pixels/cells
    pub height: u32,
    /// 0x1C: Clip/bounds left (x minimum)
    pub clip_left: u32,
    /// 0x20: Clip/bounds top (y minimum)
    pub clip_top: u32,
    /// 0x24: Clip/bounds right (x maximum)
    pub clip_right: u32,
    /// 0x28: Clip/bounds bottom (y maximum)
    pub clip_bottom: u32,
}

const _: () = assert!(core::mem::size_of::<BitGrid>() == 0x2C);

impl BitGrid {
    /// Pure Rust implementation of BitGrid__Init (0x4F6370).
    ///
    /// Allocates a row-aligned data buffer and initializes all fields.
    /// Sets the base vtable (0x6640EC); callers cast and override to specialize.
    ///
    /// `cells_per_unit`: 1 for bit grids (collision), 8 for 8bpp pixel buffers.
    /// `width` and `height`: dimensions in pixels/cells.
    ///
    /// # Safety
    /// `this` must point to a zero-filled allocation of at least 0x2C bytes.
    pub unsafe fn init(this: *mut BitGrid, cells_per_unit: u32, width: u32, height: u32) {
        let bits = cells_per_unit.wrapping_mul(width).wrapping_add(7) as i32;
        let row_stride = ((bits >> 3) + 3) & !3;
        let total_size = row_stride as u32 * height;

        let alloc_size = ((total_size + 3) & !3) + 0x20;
        let buffer = wa_malloc(alloc_size);

        if buffer.is_null() {
            return;
        }
        if total_size as usize > alloc_size as usize {
            return;
        }

        // Note: the original calls memset twice on the same buffer (possibly a
        // debug/paranoia pattern from MSVC 2005). We only do it once since the
        // second write is a no-op that modern compilers would eliminate anyway.
        core::ptr::write_bytes(buffer, 0, total_size as usize);

        (*this).vtable = rb(BIT_GRID_BASE_VTABLE) as *const BitGridBaseVtable;
        (*this).external_buffer = 0;
        (*this).data = buffer;
        (*this).cells_per_unit = cells_per_unit;
        (*this).row_stride = row_stride as u32;
        (*this).width = width;
        (*this).height = height;
        (*this).clip_left = 0;
        (*this).clip_top = 0;
        (*this).clip_right = width;
        (*this).clip_bottom = height;
    }
}

// =========================================================================
// Base variant (vtable 0x6640EC)
// =========================================================================

/// BitGrid base vtable (0x6640EC, 8 slots).
///
/// Bit-level operations. Used as a passive data container (gradient images)
/// and as the initial vtable set by `BitGrid::init` before callers specialize.
/// Slots 6-7 are stubs (get/put pixel not supported in base form).
#[openwa_core::vtable(size = 8, va = 0x0066_40EC, class = "BitGridBase")]
pub struct BitGridBaseVtable {
    /// fill rectangle — bit-level fill (0x4F6760, RET 0x14)
    #[slot(0)]
    pub fill_rect: fn(this: *mut BitGrid, x1: i32, y1: i32, x2: i32, y2: i32, color: u8),
    /// fill horizontal line — bit-level, shared with collision (0x4F67B0, RET 0x10)
    #[slot(1)]
    pub fill_hline: fn(this: *mut BitGrid, x1: i32, x2: i32, y: i32, color: u8),
    /// fill vertical line — bit-level, shared with collision (0x4F6860, RET 0x10)
    #[slot(2)]
    pub fill_vline: fn(this: *mut BitGrid, x: i32, y1: i32, y2: i32, color: u8),
    /// destructor — shared across all variants (0x4F5DE0, RET 0x4)
    #[slot(3)]
    pub destructor: fn(this: *mut BitGrid, flags: u8) -> *mut BitGrid,
    /// get bit (clipped) — bit-level read (0x505430, RET 0x8)
    #[slot(4)]
    pub get_clipped: fn(this: *mut BitGrid, x: i32, y: i32) -> u8,
    /// set bit (clipped) — bit-level write (0x482860, RET 0xC)
    #[slot(5)]
    pub put_clipped: fn(this: *mut BitGrid, x: i32, y: i32, value: u8),
    // Slots 6-7: stubs (0x5D4E16) — get/put pixel not supported in base form
}

bind_BitGridBaseVtable!(BitGrid, vtable);

impl BitGrid {
    /// Allocate and initialize a base BitGrid on WA's heap.
    ///
    /// Used internally by variant `alloc` methods. For external use,
    /// prefer `CollisionBitGrid::alloc` or `DisplayBitGrid::alloc`.
    /// Returns null if allocation fails.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub(crate) unsafe fn alloc_base(cells_per_unit: u32, width: u32, height: u32) -> *mut BitGrid {
        let grid = wa_malloc_struct_zeroed::<BitGrid>();
        if !grid.is_null() {
            BitGrid::init(grid, cells_per_unit, width, height);
        }
        grid
    }
}

// =========================================================================
// Collision variant (vtable 0x664118)
// =========================================================================

/// Typed alias for collision BitGrids (vtable 0x664118).
///
/// Used at DDGame+0x380 for terrain collision/spatial queries.
/// Bit-level operations — one bit per cell.
pub type CollisionBitGrid = BitGrid<*const BitGridCollisionVtable>;

/// BitGrid collision vtable (0x664118, 8 slots).
///
/// Bit-level spatial query operations. Shares fill_hline/fill_vline
/// implementations with the base vtable, but has distinct get/put methods
/// for bit-level access including unchecked variants.
#[openwa_core::vtable(size = 8, va = 0x0066_4118, class = "BitGridCollision")]
pub struct BitGridCollisionVtable {
    /// fill rectangle — bit-level fill (0x4F8F90, RET 0x14)
    #[slot(0)]
    pub fill_rect: fn(this: *mut CollisionBitGrid, x1: i32, y1: i32, x2: i32, y2: i32, color: u8),
    /// fill horizontal line — bit-level, shared with base (0x4F67B0, RET 0x10)
    #[slot(1)]
    pub fill_hline: fn(this: *mut CollisionBitGrid, x1: i32, x2: i32, y: i32, color: u8),
    /// fill vertical line — bit-level, shared with base (0x4F6860, RET 0x10)
    #[slot(2)]
    pub fill_vline: fn(this: *mut CollisionBitGrid, x: i32, y1: i32, y2: i32, color: u8),
    /// destructor — shared across all variants (0x4F5DE0, RET 0x4)
    #[slot(3)]
    pub destructor: fn(this: *mut CollisionBitGrid, flags: u8) -> *mut CollisionBitGrid,
    /// get bit (clipped) — bit-level read with bounds check (0x4F9020, RET 0x8)
    #[slot(4)]
    pub get_clipped: fn(this: *mut CollisionBitGrid, x: i32, y: i32) -> u8,
    /// set bit (clipped) — bit-level write with bounds check (0x4F9050, RET 0xC)
    #[slot(5)]
    pub put_clipped: fn(this: *mut CollisionBitGrid, x: i32, y: i32, value: u8),
    /// get bit (unchecked) — direct bit read (0x4F5D70, RET 0x8)
    #[slot(6)]
    pub get: fn(this: *mut CollisionBitGrid, x: i32, y: i32) -> u8,
    /// set bit (unchecked) — direct bit write (0x4F5DA0, RET 0xC)
    #[slot(7)]
    pub put: fn(this: *mut CollisionBitGrid, x: i32, y: i32, value: u8),
}

bind_BitGridCollisionVtable!(CollisionBitGrid, vtable);

impl CollisionBitGrid {
    /// Allocate and initialize a collision BitGrid on WA's heap.
    ///
    /// Calls `BitGrid::init` then overrides the vtable to 0x664118.
    /// Returns null if allocation fails.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn alloc(cells_per_unit: u32, width: u32, height: u32) -> *mut CollisionBitGrid {
        let grid = BitGrid::alloc_base(cells_per_unit, width, height);
        if !grid.is_null() {
            let collision = grid as *mut CollisionBitGrid;
            (*collision).vtable = rb(BIT_GRID_COLLISION_VTABLE) as *const BitGridCollisionVtable;
            return collision;
        }
        core::ptr::null_mut()
    }
}

// =========================================================================
// Display layer variant (vtable 0x664144)
// =========================================================================

/// Typed alias for display-layer BitGrids (vtable 0x664144).
///
/// Used as rendering layers in DisplayGfx (+0x3D9C, +0x3DA0, +0x3DA4)
/// and as sprite pixel buffers (Sprite+0x34). Provides typed vtable
/// access and bind wrappers for 8bpp pixel drawing operations.
pub type DisplayBitGrid = BitGrid<*const BitGridDisplayVtable>;

/// BitGrid display-layer vtable (0x664144, 8 slots).
///
/// Byte-level (8bpp) pixel buffer operations for rendering layers.
/// All pixel addressing uses: `data[y * row_stride + x]`
///
/// DisplayGfx creates 3 layer objects in DisplayGfx__Init:
/// - Layer 0 at DisplayGfx+0x3D9C
/// - Layer 1 at DisplayGfx+0x3DA0
/// - Layer 2 at DisplayGfx+0x3DA4
#[openwa_core::vtable(size = 8, va = 0x0066_4144, class = "BitGridDisplay")]
pub struct BitGridDisplayVtable {
    /// fill rectangle — memset rows [y1..y2) from x1 to x2 with color (0x4F9090, RET 0x14)
    #[slot(0)]
    pub fill_rect: fn(this: *mut DisplayBitGrid, x1: i32, y1: i32, x2: i32, y2: i32, color: u8),
    /// fill horizontal line — memset row y from x1 to x2 with color (0x4F90E0, RET 0x10)
    #[slot(1)]
    pub fill_hline: fn(this: *mut DisplayBitGrid, x1: i32, x2: i32, y: i32, color: u8),
    /// fill vertical line — set pixels in column x from y1 to y2 (0x4F9110, RET 0x10)
    #[slot(2)]
    pub fill_vline: fn(this: *mut DisplayBitGrid, x: i32, y1: i32, y2: i32, color: u8),
    /// destructor — shared across all variants (0x4F5DE0, RET 0x4)
    #[slot(3)]
    pub destructor: fn(this: *mut DisplayBitGrid, flags: u8) -> *mut DisplayBitGrid,
    /// get pixel (clipped) — returns 0 if outside clip rect (0x4F9140, RET 0x8)
    #[slot(4)]
    pub get_pixel_clipped: fn(this: *mut DisplayBitGrid, x: i32, y: i32) -> u8,
    /// put pixel (clipped) — no-op if outside clip rect (0x4F9180, RET 0xC)
    ///
    /// This is the main rendering primitive — DisplayGfx dispatches drawing
    /// operations through these layer objects.
    #[slot(5)]
    pub put_pixel_clipped: fn(this: *mut DisplayBitGrid, x: i32, y: i32, color: u8),
    /// get pixel (unchecked) — direct read, no bounds checking (0x4F5E20, RET 0x8)
    #[slot(6)]
    pub get_pixel: fn(this: *mut DisplayBitGrid, x: i32, y: i32) -> u8,
    /// put pixel (unchecked) — direct write, no bounds checking (0x4F5E40, RET 0xC)
    #[slot(7)]
    pub put_pixel: fn(this: *mut DisplayBitGrid, x: i32, y: i32, color: u8),
}

bind_BitGridDisplayVtable!(DisplayBitGrid, vtable);

impl DisplayBitGrid {
    /// Allocate and initialize a display BitGrid on WA's heap.
    ///
    /// Calls `BitGrid::init` then overrides the vtable to 0x664144.
    /// Returns null if allocation fails.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn alloc(cells_per_unit: u32, width: u32, height: u32) -> *mut DisplayBitGrid {
        let grid = BitGrid::alloc_base(cells_per_unit, width, height);
        if !grid.is_null() {
            let display = grid as *mut DisplayBitGrid;
            (*display).vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const BitGridDisplayVtable;
            return display;
        }
        core::ptr::null_mut()
    }
}
