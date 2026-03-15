//! Replay fast-forward via DDGame+0x98B0.
//!
//! When `OPENWA_REPLAY_TEST=1`, hooks TurnManager_ProcessFrame (0x55FDA0)
//! and sets DDGame+0x98B0 (fast-forward active flag) each frame.
//!
//! When this flag is set, FUN_005307A0 processes up to 50 game frames per
//! render cycle. Sound is suppressed (FUN_00546B50) and rendering is skipped
//! (FUN_00529F30). The flag gets cleared at turn boundaries (FUN_00534540,
//! FUN_0055BDD0), so we re-set it every frame.
//!
//! This is the same mechanism triggered by key 0x35 (spacebar) during replay.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::hook;
use crate::log_line;
use openwa_core::rebase::rb;
use openwa_core::address::va;
use openwa_core::engine::{DDGame, DDGameWrapper};

extern "system" {
    fn IsBadReadPtr(lp: *const u8, ucb: u32) -> i32;
}

/// Trampoline to the original TurnManager_ProcessFrame.
static ORIG_TURN_MANAGER: AtomicU32 = AtomicU32::new(0);

/// Whether to set fast-forward flag (only in replay test mode).
static FAST_FORWARD: AtomicBool = AtomicBool::new(false);

/// Get the DDGame pointer (session+0xA0 → DDGameWrapper.ddgame).
#[inline]
unsafe fn get_ddgame() -> *mut DDGame {
    let session = *(rb(va::G_GAME_SESSION) as *const u32);
    if session == 0 {
        return core::ptr::null_mut();
    }
    let wrapper_ptr = *((session + 0xA0) as *const *const DDGameWrapper);
    if wrapper_ptr.is_null() {
        return core::ptr::null_mut();
    }
    (*wrapper_ptr).ddgame
}

/// Check if a pointer is safe to read.
#[inline]
pub unsafe fn can_read(ptr: u32, size: u32) -> bool {
    ptr >= 0x10000 && IsBadReadPtr(ptr as *const u8, size) == 0
}

/// Dump a memory region as DWORDs with automatic classification.
///
/// Each non-zero DWORD is classified as:
/// - `[VTABLE]`  — points into .rdata (likely a vtable or function pointer table);
///   also dereferences `vt[0]` if readable
/// - `[CODE]`    — points into .text
/// - `[DATA]`    — points into .data/.bss
/// - `[OBJECT]`  — heap pointer whose first DWORD is a vtable; prints vtable address
/// - `[ptr]`     — any other readable heap pointer; prints the dereferenced DWORD
/// - `[small=N]` — value < 0x10000 (integer / enum / flag)
/// - `[value]`   — anything else
///
/// # Parameters
/// - `base_ptr`: start of the object to dump (e.g. `turngame as *const u8`)
/// - `offset`: byte offset within `base_ptr` to start the dump
/// - `size`: number of bytes to dump (must be a multiple of 4)
/// - `struct_name`: used as the prefix in log lines (e.g. `"CTaskTurnGame"`)
pub unsafe fn dump_region(base_ptr: *const u8, offset: usize, size: usize, struct_name: &str) {
    let wa_base = rb(va::IMAGE_BASE);
    let delta = wa_base.wrapping_sub(va::IMAGE_BASE);

    let _ = log_line(&format!("\n=== {}+0x{:04X}..0x{:04X} ===", struct_name, offset, offset + size));

    let dword_count = size / 4;
    for i in 0..dword_count {
        let field_offset = offset + i * 4;
        let val = *(base_ptr.add(field_offset) as *const u32);
        if val == 0 {
            continue; // Skip zeros to reduce noise
        }

        let ghidra_val = val.wrapping_sub(delta);

        // Check if value itself is in .rdata (direct vtable pointer)
        if ghidra_val >= va::RDATA_START && ghidra_val < va::DATA_START {
            if can_read(val, 4) {
                let vt0 = *(val as *const u32);
                let _ = log_line(&format!(
                    "  +0x{:04X}: 0x{:08X} [VTABLE] g:0x{:08X} vt[0]=g:0x{:08X}",
                    field_offset, val, ghidra_val, vt0.wrapping_sub(delta)
                ));
            } else {
                let _ = log_line(&format!(
                    "  +0x{:04X}: 0x{:08X} [VTABLE] g:0x{:08X} (unreadable)",
                    field_offset, val, ghidra_val
                ));
            }
        } else if ghidra_val >= va::TEXT_START && ghidra_val <= va::TEXT_END {
            let _ = log_line(&format!(
                "  +0x{:04X}: 0x{:08X} [CODE] g:0x{:08X}",
                field_offset, val, ghidra_val
            ));
        } else if ghidra_val >= va::DATA_START && ghidra_val < 0x008C5000 {
            let _ = log_line(&format!(
                "  +0x{:04X}: 0x{:08X} [DATA] g:0x{:08X}",
                field_offset, val, ghidra_val
            ));
        } else if val < 0x10000 {
            let _ = log_line(&format!(
                "  +0x{:04X}: 0x{:08X} [small={}]",
                field_offset, val, val
            ));
        } else if can_read(val, 4) {
            // Heap pointer — safely read first DWORD to check for vtable
            let first = *(val as *const u32);
            let ghidra_first = first.wrapping_sub(delta);
            if ghidra_first >= va::RDATA_START && ghidra_first < va::DATA_START {
                // It's an object with a vtable!
                let vt0_str = if can_read(first, 4) {
                    let vt0 = *(first as *const u32);
                    format!("vt[0]=g:0x{:08X}", vt0.wrapping_sub(delta))
                } else {
                    "vt[0]=?".to_string()
                };
                let _ = log_line(&format!(
                    "  +0x{:04X}: 0x{:08X} [OBJECT] vtable=g:0x{:08X} {}",
                    field_offset, val, ghidra_first, vt0_str
                ));
            } else {
                let _ = log_line(&format!(
                    "  +0x{:04X}: 0x{:08X} [ptr] *=0x{:08X}",
                    field_offset, val, first
                ));
            }
        } else {
            let _ = log_line(&format!(
                "  +0x{:04X}: 0x{:08X} [value]",
                field_offset, val
            ));
        }
    }
}

/// Hook for TurnManager_ProcessFrame (stdcall, 1 param = TurnGame*).
unsafe extern "stdcall" fn hook_turn_manager(turngame: u32) {
    // Call original first
    let orig: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(ORIG_TURN_MANAGER.load(Ordering::Relaxed));
    orig(turngame);

    let ddgame = get_ddgame();
    if ddgame.is_null() {
        return;
    }

    // Fast-forward for replay test
    if FAST_FORWARD.load(Ordering::Relaxed) {
        (*ddgame).fast_forward_active = 1;
    }
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_REPLAY_TEST").is_ok() {
        FAST_FORWARD.store(true, Ordering::Relaxed);
        let _ = log_line("[Input] Replay test mode — fast-forward enabled");
    }

    let _ = log_line("[Input] Hooking TurnManager_ProcessFrame");

    unsafe {
        let trampoline = hook::install(
            "TurnManager_ProcessFrame",
            va::TURN_MANAGER_PROCESS_FRAME,
            hook_turn_manager as *const (),
        )?;
        ORIG_TURN_MANAGER.store(trampoline as u32, Ordering::Relaxed);
    }

    Ok(())
}
