//! Game state snapshot trait and helpers.
//!
//! Types implement [`Snapshot`] to produce canonicalized, diff-friendly text
//! representations of their state. Pointer values are replaced with `<ptr>`
//! or `null`, keeping only deterministic simulation state.

use core::fmt;

use crate::mem;
use crate::rebase::rb;
use crate::address::va;

/// Trait for types that can write a canonicalized snapshot.
///
/// Implementations skip or canonicalize pointer fields and format values
/// for human-readable diffing. The output should be deterministic across
/// runs (same game state → same text).
pub trait Snapshot {
    /// Write a human-readable, canonicalized representation.
    ///
    /// # Safety
    /// `self` must point to valid, initialized memory.
    unsafe fn write_snapshot(&self, w: &mut dyn fmt::Write, indent: usize) -> fmt::Result;
}

// ── Helpers ──

/// Write `indent * 2` spaces.
pub fn write_indent(w: &mut dyn fmt::Write, indent: usize) -> fmt::Result {
    for _ in 0..indent {
        w.write_str("  ")?;
    }
    Ok(())
}

/// Format a pointer field: `<ptr>` if non-null, `null` if null.
pub fn fmt_ptr(ptr: *const u8) -> &'static str {
    if ptr.is_null() { "null" } else { "<ptr>" }
}

/// Format a pointer field for a raw u32 that might be a pointer.
pub fn fmt_ptr32(val: u32) -> &'static str {
    if val == 0 { "null" } else { "<ptr>" }
}

/// Dump a raw memory region as canonicalized hex lines.
///
/// Each line: `+OFFSET: XX XX XX XX  XX XX XX XX  XX XX XX XX  XX XX XX XX`
///
/// DWORD-aligned values that look like pointers (per `mem::classify_pointer`)
/// are replaced with `[ptr---]` to canonicalize heap addresses.
///
/// # Safety
/// `base` must point to `size` readable bytes.
#[cfg(target_arch = "x86")]
pub unsafe fn write_raw_region(
    w: &mut dyn fmt::Write,
    base: *const u8,
    size: usize,
    indent: usize,
) -> fmt::Result {
    let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);

    for row_start in (0..size).step_by(16) {
        write_indent(w, indent)?;
        write!(w, "+{:04X}:", row_start)?;

        let row_end = (row_start + 16).min(size);

        // Process as DWORDs for pointer detection
        for dw_off in (row_start..row_end).step_by(4) {
            w.write_char(' ')?;
            if dw_off + 4 <= size {
                let val = *(base.add(dw_off) as *const u32);
                if is_likely_pointer(val, delta) {
                    w.write_str("[ptr---]")?;
                } else {
                    write!(w, "{:08X}", val)?;
                }
            } else {
                // Partial DWORD at end — dump remaining bytes
                for b in dw_off..row_end {
                    write!(w, "{:02X}", *base.add(b))?;
                }
            }
        }
        w.write_char('\n')?;
    }
    Ok(())
}

/// Heuristic: is this value likely a heap/code/data pointer?
#[cfg(target_arch = "x86")]
fn is_likely_pointer(val: u32, delta: u32) -> bool {
    if val < 0x10000 {
        return false;
    }
    // Check if it's in a known WA section (.text through .data end)
    let ghidra = val.wrapping_sub(delta);
    if ghidra >= va::TEXT_START && ghidra < va::DATA_END {
        return true;
    }
    // Heap pointer heuristic: > 1MB and readable
    if val > 0x100000 {
        return unsafe { mem::can_read(val, 4) };
    }
    false
}
