//! Hook wiring for ReplayLoader and ParseReplayPosition.
//!
//! Thin shim — all loader/writer logic lives in `openwa_game::engine::replay_loader`
//! and the stream/types live in `openwa_game::engine::replay`.
//!
//! ReplayLoader (0x462DF0): stdcall(state, mode), RET 0x8.
//! ParseReplayPosition (0x4E3490): stdcall(input), RET 0x4.

use crate::hook;
use openwa_game::address::va;
use openwa_game::engine::game_info::GameInfo;
use openwa_game::engine::replay;
use openwa_game::engine::replay_loader::{self, OriginalReplayLoader};

static mut REPLAY_LOADER_ORIG: *const () = core::ptr::null();
#[allow(dead_code)]
static mut PARSE_POSITION_ORIG: *const () = core::ptr::null();

unsafe extern "stdcall" fn original_replay_loader(gi: *mut GameInfo, mode: i32) -> u32 {
    unsafe {
        let orig: OriginalReplayLoader = core::mem::transmute(REPLAY_LOADER_ORIG);
        orig(gi, mode)
    }
}

unsafe extern "stdcall" fn hook_replay_loader(state: *mut GameInfo, mode: i32) -> u32 {
    unsafe {
        if mode != 1 {
            return original_replay_loader(state, mode);
        }
        match replay_loader::play_replay(state, original_replay_loader) {
            Ok(()) => 0u32,
            Err(e) => e as i32 as u32,
        }
    }
}

unsafe extern "stdcall" fn hook_parse_replay_position(input: *const u8) -> i32 {
    unsafe {
        let mut len = 0usize;
        while *input.add(len) != 0 {
            len += 1;
            if len > 256 {
                break;
            }
        }
        let slice = core::slice::from_raw_parts(input, len + 1);
        replay::parse_replay_position(slice)
    }
}

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
