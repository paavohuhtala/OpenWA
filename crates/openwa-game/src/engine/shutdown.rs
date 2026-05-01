//! Rust port of `GameEngine::Shutdown` (0x0056DCD0).
//!
//! Tears down the subsystems constructed by `engine::hardware_init`:
//! input controller, streaming audio, game runtime, display, keyboard,
//! mouse, net wrapper, sound, and localized template.

use crate::address::va;
use crate::engine::game_session::get_game_session;
use crate::rebase::rb;

/// Invoke vtable slot `slot` on `this` as a thiscall(this, flags=arg).
/// Generic `vtable[N](this, arg)` dispatcher used by [`shutdown`].
#[inline]
unsafe fn vcall1(this: u32, slot: usize, arg: u32) {
    unsafe {
        let vtable = *(this as *const *const u32);
        let f: unsafe extern "fastcall" fn(u32, u32, u32) = core::mem::transmute(*vtable.add(slot));
        f(this, 0, arg); // ECX=this, EDX=dummy, stack=arg → thiscall(this, arg)
    }
}

/// Invoke vtable slot `slot` on `this` as a thiscall with no args.
#[inline]
unsafe fn vcall0(this: u32, slot: usize) {
    unsafe {
        let vtable = *(this as *const *const u32);
        let f: unsafe extern "fastcall" fn(u32, u32) = core::mem::transmute(*vtable.add(slot));
        f(this, 0); // ECX=this, EDX=dummy → thiscall(this)
    }
}

/// Rust port of `GameEngine::Shutdown` (0x0056DCD0).
///
/// Stdcall(state_buf), RET 0x4. The destruction order matches the original
/// decompile exactly:
///
/// 1. Clear `session.init_flag`.
/// 2. If `input_ctrl != 0`: call `SHUTDOWN_INPUT_CTRL_HELPER_MAYBE` (cdecl, 0 args).
/// 3. If `streaming_audio != 0`: `streaming_audio.vtable[5](this)` (Music::stop_and_cleanup).
/// 4. `game_runtime.vtable[8](this, state_buf)` — slot 8 (`SaveReplayState_Maybe`,
///    0x528A30) — **no null check on `game_runtime`** in the original.
/// 5. If `game_runtime != 0`: `game_runtime.vtable[6](this, 1)` — `EndGame` (0x56DF90).
/// 6. If `display != 0`: `display.vtable[0](this, 1)` — destructor.
/// 7. If `keyboard != 0`: `keyboard.vtable[0](this, 1)`.
/// 8. If `mouse_input != 0`: `mouse_input.vtable[0](this, 1)`.
/// 9. If `net_game != 0`: `net_game.vtable[0](this, 1)`.
/// 10. If `streaming_audio != 0`: `streaming_audio.vtable[0](this, 1)` — scalar deleting dtor.
/// 11. If `input_ctrl != 0`: `input_ctrl.vtable[0](this, 1)`.
/// 12. If `sound != 0`: `sound.vtable[0](this, 1)` — DSSound destructor.
/// 13. If `localized_template != 0`: `LOCALIZED_TEMPLATE_DTOR_BODY_MAYBE` + `wa_free`.
pub unsafe extern "stdcall" fn shutdown(state_buf: *mut u8) {
    unsafe {
        let session = get_game_session();

        let game_runtime = (*session).game_runtime as u32;
        let keyboard = (*session).keyboard as u32;
        let sound = (*session).sound as u32;
        let display = (*session).display as u32;
        let mouse_input = (*session).mouse_input as u32;
        let music = (*session).streaming_audio as u32;
        let input_ctrl = (*session).input_ctrl as u32;
        let net_game = (*session).net_game as u32;
        let localized_template = (*session).localized_template as u32;

        (*session).init_flag = 0;

        if input_ctrl != 0 {
            // Usercall(EDI=g_GameSession). The function reads `[EDI+0xB8]`
            // (input_ctrl) on entry; passing the session pointer matches WA.
            crate::wa_call::call_usercall_edi(
                session as u32,
                rb(va::SHUTDOWN_INPUT_CTRL_HELPER_MAYBE),
            );
        }

        if music != 0 {
            vcall0(music, 5);
        }

        // Original code calls this without a null check; preserve that.
        vcall1(game_runtime, 8, state_buf as u32);

        if game_runtime != 0 {
            vcall1(game_runtime, 6, 1);
        }
        if display != 0 {
            vcall1(display, 0, 1);
        }
        if keyboard != 0 {
            vcall1(keyboard, 0, 1);
        }
        if mouse_input != 0 {
            vcall1(mouse_input, 0, 1);
        }
        if net_game != 0 {
            vcall1(net_game, 0, 1);
        }
        if music != 0 {
            vcall1(music, 0, 1);
        }
        if input_ctrl != 0 {
            vcall1(input_ctrl, 0, 1);
        }
        if sound != 0 {
            vcall1(sound, 0, 1);
        }

        if localized_template != 0 {
            // Usercall(EDI=this) — the function reads `[EDI+4]` immediately.
            crate::wa_call::call_usercall_edi(
                localized_template,
                rb(va::LOCALIZED_TEMPLATE_DTOR_BODY_MAYBE),
            );
            crate::wa_alloc::wa_free(localized_template as *mut u8);
        }
    }
}
