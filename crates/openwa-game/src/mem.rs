//! Memory inspection utilities — pointer classification and safe read checks.

use crate::address::va;
use crate::registry;
use openwa_debug_proto::{PointerInfo, PointerKind};

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn IsBadReadPtr(lp: *const u8, ucb: usize) -> i32;
}

/// Check if a pointer is safe to read.
#[cfg(target_os = "windows")]
#[inline]
pub unsafe fn can_read(ptr: u32, size: u32) -> bool {
    unsafe { ptr >= 0x10000 && IsBadReadPtr(ptr as *const u8, size as usize) == 0 }
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
    unsafe {
        if value == 0 || value < 0x10000 {
            return None;
        }

        let ghidra_val = value.wrapping_sub(delta);

        // .rdata → Vtable (likely vtable or function pointer table)
        if (va::RDATA_START..va::DATA_START).contains(&ghidra_val) {
            let name = registry::format_va(ghidra_val);
            let detail = if can_read(value, 4) {
                let vt0 = *(value as *const u32);
                Some(format!(
                    "{} vt[0]=ghidra:0x{:08X}",
                    name,
                    vt0.wrapping_sub(delta)
                ))
            } else {
                Some(format!("{} (unreadable)", name))
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
        if (va::TEXT_START..=va::TEXT_END).contains(&ghidra_val) {
            let detail = registry::lookup_va(ghidra_val).map(|r| {
                if r.offset == 0 {
                    r.entry.name.to_string()
                } else {
                    format!("{}+0x{:X}", r.entry.name, r.offset)
                }
            });
            return Some(PointerInfo {
                offset: 0,
                raw_value: value,
                ghidra_value: ghidra_val,
                kind: PointerKind::Code,
                detail,
            });
        }

        // .data/.bss → Data
        if (va::DATA_START..va::DATA_END).contains(&ghidra_val) {
            let detail = registry::lookup_va_exact(ghidra_val).map(|e| e.name.to_string());
            return Some(PointerInfo {
                offset: 0,
                raw_value: value,
                ghidra_value: ghidra_val,
                kind: PointerKind::Data,
                detail,
            });
        }

        // Heap pointer checks
        if can_read(value, 4) {
            let first = *(value as *const u32);
            let ghidra_first = first.wrapping_sub(delta);

            // Object — heap pointer whose first DWORD is a vtable
            if (va::RDATA_START..va::DATA_START).contains(&ghidra_first) {
                let class = registry::vtable_class_name(ghidra_first);
                let detail = match class {
                    Some(name) => Some(format!("{}* (vtable=0x{:08X})", name, ghidra_first)),
                    None if can_read(first, 4) => {
                        let vt0 = *(first as *const u32);
                        Some(format!(
                            "vtable=ghidra:0x{:08X} vt[0]=ghidra:0x{:08X}",
                            ghidra_first,
                            vt0.wrapping_sub(delta)
                        ))
                    }
                    None => Some(format!("vtable=ghidra:0x{:08X} vt[0]=?", ghidra_first)),
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
}

/// Rich pointer identification result.
#[derive(Debug)]
pub struct PointerIdentity {
    /// Raw runtime value.
    pub raw_value: u32,
    /// Ghidra VA (raw_value - ASLR delta).
    pub ghidra_value: u32,
    /// Memory segment classification.
    pub segment: PointerKind,
    /// Human-readable name (e.g., "BASE_ENTITY_CONSTRUCTOR" or "WormEntity*").
    pub name: Option<String>,
    /// Class name if this is a vtable or an object with a known vtable.
    pub class_name: Option<&'static str>,
    /// Extra context detail.
    pub detail: Option<String>,
}

/// Given an arbitrary runtime pointer value, identify what it is.
///
/// Combines the static address registry with segment-based classification
/// and vtable-based object identification for heap pointers.
///
/// `delta` is `runtime_base - 0x400000` (the ASLR offset).
#[cfg(target_os = "windows")]
pub unsafe fn identify_pointer(value: u32, delta: u32) -> Option<PointerIdentity> {
    unsafe {
        if value == 0 || value < 0x10000 {
            return None;
        }

        let ghidra_val = value.wrapping_sub(delta);

        // 1. Check static registry for a known address
        if let Some(resolved) = registry::lookup_va(ghidra_val)
            && resolved.offset < 0x1000
        {
            let segment = match resolved.entry.kind {
                registry::AddrKind::Vtable | registry::AddrKind::VtableMethod => {
                    PointerKind::Vtable
                }
                registry::AddrKind::Function | registry::AddrKind::Constructor => PointerKind::Code,
                _ => PointerKind::Data,
            };
            let name = if resolved.offset == 0 {
                resolved.entry.name.to_string()
            } else {
                format!("{}+0x{:X}", resolved.entry.name, resolved.offset)
            };
            return Some(PointerIdentity {
                raw_value: value,
                ghidra_value: ghidra_val,
                segment,
                name: Some(name),
                class_name: resolved.entry.class_name,
                detail: None,
            });
        }

        // 2. Check if pointer falls inside a tracked live object
        if let Some(m) = registry::identify_live_pointer(value) {
            let name = match m.field {
                Some(field) if m.offset == field.offset => {
                    format!("{}.{}", m.object.class_name, field.name)
                }
                Some(field) => {
                    let inner_off = m.offset - field.offset;
                    format!("{}.{}+0x{:X}", m.object.class_name, field.name, inner_off)
                }
                None => format!("{}+0x{:X}", m.object.class_name, m.offset),
            };
            return Some(PointerIdentity {
                raw_value: value,
                ghidra_value: ghidra_val,
                segment: PointerKind::Object,
                name: Some(name),
                class_name: Some(m.object.class_name),
                detail: Some(format!(
                    "base=0x{:08X} offset=0x{:X}",
                    m.object.ptr, m.offset
                )),
            });
        }

        // 3. If heap pointer, check if first DWORD is a known vtable
        if can_read(value, 4) {
            let first = *(value as *const u32);
            let ghidra_first = first.wrapping_sub(delta);
            if let Some(class) = registry::vtable_class_name(ghidra_first) {
                return Some(PointerIdentity {
                    raw_value: value,
                    ghidra_value: ghidra_val,
                    segment: PointerKind::Object,
                    name: Some(format!("{}*", class)),
                    class_name: Some(class),
                    detail: Some(format!("vtable=0x{:08X}", ghidra_first)),
                });
            }
        }

        // 4. Fall back to segment-based classification
        classify_pointer(value, delta).map(|info| PointerIdentity {
            raw_value: info.raw_value,
            ghidra_value: info.ghidra_value,
            segment: info.kind,
            name: None,
            class_name: None,
            detail: info.detail,
        })
    }
}

/// Classify all DWORD-aligned values in a byte slice.
///
/// Returns a vec of `PointerInfo` with offsets filled in relative to `base_offset`.
#[cfg(target_os = "windows")]
pub unsafe fn classify_region(data: &[u8], base_offset: u32, delta: u32) -> Vec<PointerInfo> {
    unsafe {
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
}
