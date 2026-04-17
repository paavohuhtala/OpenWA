//! Interpolated fixed-point trig-table lookup.
//!
//! The lookup angle is a 16-bit quantity where the full range 0..0xFFFF
//! maps to one full period. The upper 10 bits select the table entry and
//! the lower 6 bits are used for linear interpolation between adjacent
//! entries.
//!
//! The table itself (a 1025-entry `[i32; 1025]` sine or cosine array in
//! 16.16 format) is provided by the caller. In `openwa-game`, the tables
//! come from WA.exe's `.rdata` via `rb(va::G_SIN_TABLE)` /
//! `rb(va::G_COS_TABLE)` — see `openwa-game::trig`.

use crate::fixed::Fixed;

/// Interpolated lookup from a 1025-entry fixed-point trig table.
///
/// `table` must point to at least 1025 `i32` values (the standard WA
/// sine/cosine layout). `angle` is a 16-bit angle where the full range
/// 0..0xFFFF maps to one full period.
///
/// Returns a Fixed 16.16 value interpolated between the two nearest
/// table entries.
#[inline]
pub unsafe fn trig_lookup(table: *const i32, angle: u32) -> Fixed {
    let index = ((angle as i32) >> 6) as usize & 0x3FF;
    let frac = Fixed::from_raw(((angle & 0x3F) << 10) as i32);
    let base = Fixed::from_raw(unsafe { *table.add(index) });
    let next = Fixed::from_raw(unsafe { *table.add(index + 1) });
    (next - base).mul_raw(frac) + base
}
