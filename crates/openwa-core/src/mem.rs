//! Memory inspection utilities — pointer classification and safe read checks.

use crate::address::va;
use openwa_debug_proto::{PointerInfo, PointerKind};

#[cfg(target_os = "windows")]
extern "system" {
    fn IsBadReadPtr(lp: *const u8, ucb: usize) -> i32;
}

/// Check if a pointer is safe to read.
#[cfg(target_os = "windows")]
#[inline]
pub unsafe fn can_read(ptr: u32, size: u32) -> bool {
    ptr >= 0x10000 && IsBadReadPtr(ptr as *const u8, size as usize) == 0
}

/// Stub for non-Windows (always returns false).
#[cfg(not(target_os = "windows"))]
pub unsafe fn can_read(_ptr: u32, _size: u32) -> bool {
    false
}

/// Classify a DWORD value as a pointer kind, if applicable.
///
/// Returns `None` for zero values, small values (< 0x10000), and
/// values that don't point into any known section or readable memory.
///
/// `delta` is `runtime_base - 0x400000` (the ASLR offset).
#[cfg(target_os = "windows")]
pub unsafe fn classify_pointer(value: u32, delta: u32) -> Option<PointerInfo> {
    if value == 0 || value < 0x10000 {
        return None;
    }

    let ghidra_val = value.wrapping_sub(delta);

    // .rdata → Vtable (likely vtable or function pointer table)
    if ghidra_val >= va::RDATA_START && ghidra_val < va::DATA_START {
        let detail = if can_read(value, 4) {
            let vt0 = *(value as *const u32);
            Some(format!("vt[0]=ghidra:0x{:08X}", vt0.wrapping_sub(delta)))
        } else {
            Some("(unreadable)".to_string())
        };
        return Some(PointerInfo {
            offset: 0, // caller fills this in
            raw_value: value,
            ghidra_value: ghidra_val,
            kind: PointerKind::Vtable,
            detail,
        });
    }

    // .text → Code
    if ghidra_val >= va::TEXT_START && ghidra_val <= va::TEXT_END {
        return Some(PointerInfo {
            offset: 0,
            raw_value: value,
            ghidra_value: ghidra_val,
            kind: PointerKind::Code,
            detail: None,
        });
    }

    // .data/.bss → Data
    if ghidra_val >= va::DATA_START && ghidra_val < va::DATA_END {
        return Some(PointerInfo {
            offset: 0,
            raw_value: value,
            ghidra_value: ghidra_val,
            kind: PointerKind::Data,
            detail: None,
        });
    }

    // Heap pointer checks
    if can_read(value, 4) {
        let first = *(value as *const u32);
        let ghidra_first = first.wrapping_sub(delta);

        // Object — heap pointer whose first DWORD is a vtable
        if ghidra_first >= va::RDATA_START && ghidra_first < va::DATA_START {
            let detail = if can_read(first, 4) {
                let vt0 = *(first as *const u32);
                Some(format!(
                    "vtable=ghidra:0x{:08X} vt[0]=ghidra:0x{:08X}",
                    ghidra_first,
                    vt0.wrapping_sub(delta)
                ))
            } else {
                Some(format!("vtable=ghidra:0x{:08X} vt[0]=?", ghidra_first))
            };
            return Some(PointerInfo {
                offset: 0,
                raw_value: value,
                ghidra_value: ghidra_val,
                kind: PointerKind::Object,
                detail,
            });
        }

        // Generic heap pointer
        return Some(PointerInfo {
            offset: 0,
            raw_value: value,
            ghidra_value: ghidra_val,
            kind: PointerKind::Heap,
            detail: Some(format!("*=0x{:08X}", first)),
        });
    }

    None
}

/// Classify all DWORD-aligned values in a byte slice.
///
/// Returns a vec of `PointerInfo` with offsets filled in relative to `base_offset`.
#[cfg(target_os = "windows")]
pub unsafe fn classify_region(
    data: &[u8],
    base_offset: u32,
    delta: u32,
) -> Vec<PointerInfo> {
    let mut pointers = Vec::new();
    let dword_count = data.len() / 4;
    for i in 0..dword_count {
        let byte_offset = i * 4;
        let value = u32::from_le_bytes([
            data[byte_offset],
            data[byte_offset + 1],
            data[byte_offset + 2],
            data[byte_offset + 3],
        ]);
        if let Some(mut info) = classify_pointer(value, delta) {
            info.offset = base_offset + byte_offset as u32;
            pointers.push(info);
        }
    }
    pointers
}
