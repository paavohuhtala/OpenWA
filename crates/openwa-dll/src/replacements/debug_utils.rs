//! Debug memory inspection utilities.
//!
//! Contains `dump_region` for logging classified DWORD dumps of memory regions.
//! Used by the validation module for struct inspection.

use crate::log_line;
use openwa_core::address::va;
use openwa_core::rebase::rb;

/// Dump a memory region as DWORDs with automatic pointer classification.
///
/// Uses `openwa_core::mem::classify_pointer` for pointer detection.
#[allow(dead_code)]
pub unsafe fn dump_region(base_ptr: *const u8, offset: usize, size: usize, struct_name: &str) {
    use openwa_core::mem;
    use openwa_debug_proto::PointerKind;

    let wa_base = rb(va::IMAGE_BASE);
    let delta = wa_base.wrapping_sub(va::IMAGE_BASE);

    let _ = log_line(&format!(
        "\n=== {}+0x{:04X}..0x{:04X} ===",
        struct_name,
        offset,
        offset + size
    ));

    let dword_count = size / 4;
    for i in 0..dword_count {
        let field_offset = offset + i * 4;
        let val = *(base_ptr.add(field_offset) as *const u32);
        if val == 0 {
            continue;
        }

        if let Some(info) = mem::classify_pointer(val, delta) {
            let detail_str = info.detail.as_deref().unwrap_or("");
            match info.kind {
                PointerKind::Vtable => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [VTABLE] g:0x{:08X} {}",
                        field_offset, val, info.ghidra_value, detail_str
                    ));
                }
                PointerKind::Code => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [CODE] g:0x{:08X}",
                        field_offset, val, info.ghidra_value
                    ));
                }
                PointerKind::Data => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [DATA] g:0x{:08X}",
                        field_offset, val, info.ghidra_value
                    ));
                }
                PointerKind::Object => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [OBJECT] {}",
                        field_offset, val, detail_str
                    ));
                }
                PointerKind::Heap => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [ptr] {}",
                        field_offset, val, detail_str
                    ));
                }
            }
        } else if val < 0x10000 {
            let _ = log_line(&format!(
                "  +0x{:04X}: 0x{:08X} [small={}]",
                field_offset, val, val
            ));
        } else {
            let _ = log_line(&format!("  +0x{:04X}: 0x{:08X} [value]", field_offset, val));
        }
    }
}
