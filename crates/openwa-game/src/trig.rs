//! Interpolated fixed-point trig table lookup.
//!
//! WA stores sine and cosine values as 1025-entry fixed-point 16.16 tables
//! at [`va::G_SIN_TABLE`] and [`va::G_COS_TABLE`]. Each table covers one full
//! period in 1024 steps (2π / 1024 radians per step); the 1025th entry is a
//! sentinel that allows interpolation at the wraparound.
//!
//! The lookup angle is a 16-bit quantity where the full range 0..0xFFFF
//! maps to one full period. The upper 10 bits select the table entry and
//! the lower 6 bits are used for linear interpolation between adjacent
//! entries.

use crate::fixed::Fixed;
use crate::rebase::rb;

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
    let base = Fixed::from_raw(*table.add(index));
    let next = Fixed::from_raw(*table.add(index + 1));
    (next - base).mul_raw(frac) + base
}

/// Sine lookup from WA's global sine table.
///
/// # Safety
///
/// Requires ASLR rebase to have been computed (i.e. the DLL is loaded).
#[inline]
pub unsafe fn sin_lookup(angle: u32) -> Fixed {
    trig_lookup(rb(crate::address::va::G_SIN_TABLE) as *const i32, angle)
}

/// Cosine lookup from WA's global cosine table.
///
/// # Safety
///
/// Requires ASLR rebase to have been computed (i.e. the DLL is loaded).
#[inline]
pub unsafe fn cos_lookup(angle: u32) -> Fixed {
    trig_lookup(rb(crate::address::va::G_COS_TABLE) as *const i32, angle)
}
