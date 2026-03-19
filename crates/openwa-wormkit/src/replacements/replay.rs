//! Hooks for ReplayLoader and ParseReplayPosition.
//!
//! ReplayLoader (0x462DF0): stdcall(param_1, mode), RET 0x8.
//! ParseReplayPosition (0x4E3490): stdcall(input), RET 0x4.

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::replay;

// ─── Trampoline storage ─────────────────────────────────────────────────────

static mut REPLAY_LOADER_ORIG: *const () = core::ptr::null();
#[allow(dead_code)]
static mut PARSE_POSITION_ORIG: *const () = core::ptr::null();

// ─── ReplayLoader passthrough ────────────────────────────────────────────────

/// ReplayLoader: stdcall(param_1: u32, mode: i32) -> u32. RET 0x8.
unsafe extern "stdcall" fn hook_replay_loader(param_1: u32, mode: i32) -> u32 {
    let _ = log_line(&format!(
        "[Replay] ReplayLoader state=0x{param_1:08X} mode={mode}"
    ));

    let orig: unsafe extern "stdcall" fn(u32, i32) -> u32 =
        core::mem::transmute(REPLAY_LOADER_ORIG);
    let result = orig(param_1, mode);

    // Log fields the original populated
    if result == 0 {
        let version = *(param_1 as *const u32).byte_add(0xDB50);
        let replay_active = *(param_1 as *const u8).add(0xDB48);
        let _ = log_line(&format!(
            "[Replay] OK: version={version} active={replay_active}"
        ));
    } else {
        let _ = log_line(&format!(
            "[Replay] ReplayLoader error: {}", result as i32
        ));
    }

    result
}

// ─── ParseReplayPosition full replacement ────────────────────────────────────

/// ParseReplayPosition: stdcall(input: *const u8) -> i32. RET 0x4.
unsafe extern "stdcall" fn hook_parse_replay_position(input: *const u8) -> i32 {
    // Build a safe slice from the null-terminated C string
    let mut len = 0usize;
    while *input.add(len) != 0 {
        len += 1;
        if len > 256 {
            break;
        }
    }
    let slice = core::slice::from_raw_parts(input, len + 1); // include null
    replay::parse_replay_position(slice)
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        REPLAY_LOADER_ORIG = hook::install(
            "ReplayLoader",
            va::REPLAY_LOADER,
            hook_replay_loader as *const (),
        )? as *const ();

        PARSE_POSITION_ORIG = hook::install(
            "ParseReplayPosition",
            va::PARSE_REPLAY_POSITION,
            hook_parse_replay_position as *const (),
        )? as *const ();
    }
    Ok(())
}
