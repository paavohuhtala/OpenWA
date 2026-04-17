//! Runtime validation of the embedded sin/cos tables.
//!
//! The sine/cosine tables and lookup logic live in [`openwa_core::trig`]
//! as byte-for-byte copies of WA.exe's `.rdata`. Callers use
//! `openwa_core::trig::{sin, cos}` directly; this module exists only to
//! house [`validate_against_wa_exe`], the startup check that confirms the
//! embedded tables still match the live binary.

use crate::address::va;
use crate::rebase::rb;
use openwa_core::trig::{COS_TABLE, SIN_TABLE, TABLE_LEN};

/// Verify that the embedded const tables still match WA.exe's `.rdata`
/// tables at runtime. Returns the name of the first differing table,
/// its index, the embedded value, and the live value — or `Ok(())` if
/// the tables are identical.
///
/// # Safety
///
/// Requires ASLR rebase to have been computed (i.e. the DLL is loaded)
/// and the G_SIN_TABLE / G_COS_TABLE addresses to be valid.
pub unsafe fn validate_against_wa_exe() -> Result<(), (&'static str, usize, i32, i32)> {
    let sin_ptr = rb(va::G_SIN_TABLE) as *const i32;
    let cos_ptr = rb(va::G_COS_TABLE) as *const i32;
    for i in 0..TABLE_LEN {
        let live = unsafe { *sin_ptr.add(i) };
        if live != SIN_TABLE[i] {
            return Err(("sin", i, SIN_TABLE[i], live));
        }
        let live = unsafe { *cos_ptr.add(i) };
        if live != COS_TABLE[i] {
            return Err(("cos", i, COS_TABLE[i], live));
        }
    }
    Ok(())
}
