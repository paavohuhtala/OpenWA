// ============================================================
// Coordinate types — DDGame sub-structs for screen and terrain coords
// ============================================================

/// Coordinate entry used in DDGame screen coordinate tables (stride 0x10).
///
/// InitFields zeroes x and y; at runtime they contain fixed-point screen
/// coordinates used for camera tracking and rendering regions.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoordEntry {
    pub x: i32,
    pub y: i32,
    pub _unknown: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<CoordEntry>() == 0x10);

// ============================================================
// CoordList — dynamic array of packed terrain coordinates
// ============================================================

/// Packed terrain coordinate entry (8 bytes).
///
/// `coord` packs x and y as `x * 0x10000 + y` (fixed-point).
/// `flag` is always 1 for populated entries.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoordListEntry {
    pub coord: u32,
    pub flag: u32,
}
const _: () = assert!(core::mem::size_of::<CoordListEntry>() == 8);

/// Dynamic coordinate array header (12 bytes).
///
/// Allocated at DDGame+0x50C during `init_graphics_and_resources`.
/// Data buffer is a separate allocation of `capacity * 8` bytes.
/// Used for terrain coordinate lookups (spawning, aiming, collision).
#[repr(C)]
pub struct CoordList {
    /// Number of entries currently stored.
    pub count: u32,
    /// Maximum number of entries (600).
    pub capacity: u32,
    /// Pointer to the data buffer (`capacity * sizeof(CoordListEntry)` bytes).
    pub data: *mut CoordListEntry,
}
const _: () = assert!(core::mem::size_of::<CoordList>() == 12);
