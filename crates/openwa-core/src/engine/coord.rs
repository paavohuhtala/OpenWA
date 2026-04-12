// ============================================================
// Coordinate types — DDGame sub-structs for screen and terrain coords
// ============================================================

/// Coordinate entry used in DDGame viewport/camera tables (stride 0x10).
///
/// Each entry tracks a camera center position as two pairs of Fixed16.16
/// coordinates (current and target). InitGameState initializes all four
/// fields to the level center.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoordEntry {
    /// +0x00: Camera center X (Fixed16.16).
    pub center_x: crate::fixed::Fixed,
    /// +0x04: Camera center Y (Fixed16.16).
    pub center_y: crate::fixed::Fixed,
    /// +0x08: Camera center X target (Fixed16.16).
    pub center_x_target: crate::fixed::Fixed,
    /// +0x0C: Camera center Y target (Fixed16.16).
    pub center_y_target: crate::fixed::Fixed,
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
