//! WA-table-backed sine/cosine lookups.
//!
//! WA stores sine and cosine values as 1025-entry fixed-point 16.16 tables
//! at [`va::G_SIN_TABLE`] and [`va::G_COS_TABLE`]. Each table covers one full
//! period in 1024 steps (2π / 1024 radians per step); the 1025th entry is a
//! sentinel that allows interpolation at the wraparound.
//!
//! The pure interpolation lives in [`openwa_core::trig::trig_lookup`]; this
//! module is the thin WA-specific adapter that supplies the table pointers.

use crate::address::va;
use crate::rebase::rb;
use openwa_core::fixed::Fixed;
pub use openwa_core::trig::trig_lookup;

/// Sine lookup from WA's global sine table.
///
/// # Safety
///
/// Requires ASLR rebase to have been computed (i.e. the DLL is loaded).
#[inline]
pub unsafe fn sin_lookup(angle: u32) -> Fixed {
    unsafe { trig_lookup(rb(va::G_SIN_TABLE) as *const i32, angle) }
}

/// Cosine lookup from WA's global cosine table.
///
/// # Safety
///
/// Requires ASLR rebase to have been computed (i.e. the DLL is loaded).
#[inline]
pub unsafe fn cos_lookup(angle: u32) -> Fixed {
    unsafe { trig_lookup(rb(va::G_COS_TABLE) as *const i32, angle) }
}
