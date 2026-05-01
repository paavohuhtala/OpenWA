//! WA-internal frontend function wrappers.

use core::mem::transmute;

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::engine::game_session_run::run_game_session;
use crate::rebase::rb;
use crate::render::display::context::{FastcallResult, RenderContext};
use crate::wa_call;

/// Frontend__PaletteAnimation (0x422180)
///
/// `__usercall`: EAX = implicit param (from dialog+0x12c),
/// 2 stack params: &DAT_007be560 (palette data), palette_param (from dialog+0x134).
pub unsafe fn palette_animation(eax_value: u32, palette_param: u32) {
    unsafe {
        let addr = rb(va::FRONTEND_PALETTE_ANIMATION);
        let data = rb(0x7be560);
        core::arch::asm!(
            "push {param}",
            "push {palette}",
            "call {func}",
            param = in(reg) palette_param,
            palette = in(reg) data,
            func = in(reg) addr,
            in("eax") eax_value,
            clobber_abi("C"),
        );
    }
}

// ─── Frontend::LaunchGameSession (0x004EC540) ───────────────────────────────
//
// `__stdcall(CGameWorld* game_world, CWnd* dialog, int p3, int p4)`, RET 0x10.
// The frontend's single funnel into `GameSession::Run`. Wraps the run with
// audio pause/resume, display-mode toggle, mouse cursor management, and
// frontend window hide/restore. WA-side has 11 callers (frontend dialog
// handlers), so the WA address is hooked as a full replacement.
//
// `game_world` is a `CGameWorld*` (WA's old codename was "DDGame"). The full
// MFC class isn't typed yet; we keep it as `u32` and use byte-offset
// accessors for the four flag fields:
//   +0xCE0B4 — alt-display surface allocated (set by `FUN_004EBB70`)
//   +0xCE0B5 — "skip frontend teardown" master flag
//   +0xCE0B6 — copy of +0xCE0B5 captured at game-start (not used here)
//   +0xCE0B7 — re-entry latch around `MOUSE_MODE_ENTER_WINDOWED_MAYBE`

/// Read a `*mut GameInfo` from a global slot.
#[inline]
unsafe fn read_global_ptr<T>(addr: u32) -> *mut T {
    unsafe { *(rb(addr) as *const *mut T) }
}

/// CWnd::ShowWindow(this, n_cmd_show) — thiscall.
#[inline]
unsafe fn cwnd_show_window(cwnd: u32, n_cmd_show: i32) {
    unsafe {
        wa_call::thiscall_1(va::CWND_SHOW_WINDOW, cwnd, n_cmd_show as u32);
    }
}

/// CWnd::MoveWindow(this, x, y, w, h, repaint) — thiscall, 5 stack args.
#[inline]
unsafe fn cwnd_move_window(cwnd: u32, x: i32, y: i32, w: i32, h: i32, repaint: bool) {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32, u32, i32, i32, i32, i32, u32) =
            transmute(rb(va::CWND_MOVE_WINDOW));
        f(cwnd, 0, x, y, w, h, repaint as u32);
    }
}

/// CWnd::SetFocus(this) — thiscall.
#[inline]
unsafe fn cwnd_set_focus(cwnd: u32) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32, u32) -> u32 = transmute(rb(va::CWND_SET_FOCUS));
        f(cwnd, 0)
    }
}

/// CWnd::FromHandle(hwnd) — static stdcall.
#[inline]
unsafe fn cwnd_from_handle(hwnd: u32) -> u32 {
    unsafe {
        let f: unsafe extern "stdcall" fn(u32) -> u32 = transmute(rb(va::CWND_FROM_HANDLE));
        f(hwnd)
    }
}

/// AfxGetModuleState() — cdecl, returns AFX_MODULE_STATE*.
#[inline]
unsafe fn afx_get_module_state() -> *mut u8 {
    unsafe {
        let f: unsafe extern "cdecl" fn() -> *mut u8 = transmute(rb(va::AFX_GET_MODULE_STATE));
        f()
    }
}

#[inline]
unsafe fn iat_call_1(iat_addr: u32, arg: u32) -> u32 {
    unsafe {
        let fn_ptr = *(rb(iat_addr) as *const usize);
        let f: unsafe extern "stdcall" fn(u32) -> u32 = transmute(fn_ptr);
        f(arg)
    }
}

/// Rust port of `Frontend::LaunchGameSession` (0x004EC540).
///
/// Pre-launch: hide the dialog window, snapshot/stop wav players, switch the
/// display out of frontend mode. Body: `GameSession::Run`. Post-launch:
/// rebuild the framebuffer, re-show + reactivate the dialog, restore audio.
///
/// On the headful-fullscreen ExitProcess path (different DDDisplay singleton
/// detected after the game), bails via `ExitProcess(1)` faithfully.
pub unsafe extern "stdcall" fn launch_game_session(game_world: u32, dialog: u32, p3: u32, p4: u32) {
    unsafe {
        // [ESP+0x18] in the original — a 4-byte scratch for the audio/mouse
        // bridges that take an out-pointer in ESI/EDI.
        let mut audio_state_local: u32 = 0;

        let console_mode = *(rb(va::G_CONSOLE_MODE) as *const u32);

        if console_mode == 0 {
            // ── Pre-launch teardown ────────────────────────────────────────
            let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
            *frame.add(0x58) = 1;

            if dialog != 0 {
                let audio_table = *(rb(va::G_AUDIO_HANDLE_TABLE_PTR) as *const *mut u8);
                let wav_handle = *(audio_table.add(0x128) as *const u32);

                wa_call::call_usercall_eax_esi(
                    wav_handle,
                    &raw mut audio_state_local as u32,
                    rb(va::WAV_PLAYER_CHECK_OR_BIND_MAYBE),
                );

                let main_frontend = *(rb(va::G_MAIN_FRONTEND) as *const u32);
                if game_world == main_frontend {
                    wa_call::call_usercall_eax_esi(
                        wav_handle,
                        &raw mut audio_state_local as u32,
                        rb(va::WAV_PLAYER_PREPARE_PLAY),
                    );
                }

                let f: unsafe extern "stdcall" fn() = transmute(rb(va::WAV_BANK_RELEASE_ALL_MAYBE));
                f();

                let f: unsafe extern "stdcall" fn(u32) = transmute(rb(va::STOP_ALL_WAV_PLAYERS_2));
                f(rb(va::G_WAV_PLAYER_RING_BASE_MAYBE));

                wa_call::call_usercall_edi(
                    &raw mut audio_state_local as u32,
                    rb(va::WAV_ACTIVE_CHANNELS_STOP_MAYBE),
                );

                cwnd_show_window(dialog, 0); // SW_HIDE

                // Vtable call on `(game_world+0xa4)`, slot at offset 0x34.
                let subobj = (game_world as *mut u8).add(0xa4) as *mut u32;
                let subobj_vt = *(subobj as *const *const u32);
                let slot13: usize = *subobj_vt.add(0xd) as usize; // offset 0x34 / 4
                let f: unsafe extern "fastcall" fn(u32, u32) = transmute(slot13);
                f(subobj as u32, 0);

                // RenderContext::release_frame_buffer(this, ce0b5 == 0)
                let ctx = read_global_ptr::<RenderContext>(va::G_RENDER_CONTEXT);
                let ce0b5 = *(game_world as *const u8).add(0xCE0B5);
                let mut buf = FastcallResult::default();
                RenderContext::release_frame_buffer_raw(ctx, &mut buf, (ce0b5 == 0) as i32);
            }

            *(rb(va::G_FRONTEND_TICK_LATCH_MAYBE) as *mut u8) = 0;
            *(rb(va::G_IN_GAME_SESSION_FLAG) as *mut u8) = 1;

            let ce0b5 = *(game_world as *const u8).add(0xCE0B5);
            *(game_world as *mut u8).add(0xCE0B6) = ce0b5;

            if ce0b5 == 0 {
                let f: unsafe extern "stdcall" fn(u32) =
                    transmute(rb(va::MOUSE_CURSOR_RECENTER_ON_WINDOW_MAYBE));
                f(0);

                // Win32 SetActiveWindow + SetFocus on the frontend HWND.
                let hwnd = *(rb(va::G_FRONTEND_HWND) as *const u32);
                let _ = iat_call_1(va::IAT_SET_ACTIVE_WINDOW, hwnd);
                let hwnd = *(rb(va::G_FRONTEND_HWND) as *const u32);
                let _ = iat_call_1(va::IAT_SET_FOCUS, hwnd);
            }

            let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
            *frame.add(0x58) = 0;
        }

        // ── Run game session ───────────────────────────────────────────────
        // WA: ESI = `0x0087D438` (the GameInfo singleton, currently aliased
        // as `G_TEAM_SECONDARY_DATA`); stack args = (AfxGetModuleState()+8,
        // &g_GameSessionStateBuffer, p3, p4). We call the Rust port directly.
        let module_state = afx_get_module_state();
        let module_arg = *(module_state.add(8) as *const u32);
        let game_info = rb(va::G_TEAM_SECONDARY_DATA) as *mut GameInfo;
        let state_buf = rb(va::G_GAME_SESSION_STATE_BUFFER) as *mut u8;
        run_game_session(game_info, module_arg, state_buf, p3, p4);

        // ── Post-game restore ──────────────────────────────────────────────
        let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
        if !frame.is_null() {
            *frame.add(0x58) = 1;
        }
        *(rb(va::G_PENDING_INPUT_FLAG_MAYBE) as *mut u32) = 0;

        if console_mode != 0 {
            return;
        }

        let ce0b5 = *(game_world as *const u8).add(0xCE0B5);
        if ce0b5 == 0 {
            let fs = *(rb(va::G_FULLSCREEN_FLAG) as *const u32) != 0;
            if fs {
                let _ = iat_call_1(va::IAT_CLIP_CURSOR, 0);
            }
        }

        let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
        if frame.is_null() {
            return;
        }

        if *(rb(va::G_POST_GAME_RESTORE_FLAG_MAYBE) as *const u32) != 0 {
            *(rb(va::G_POST_GAME_RESTORE_FLAG_MAYBE) as *mut u32) = 0;
            let f: unsafe extern "stdcall" fn() = transmute(rb(va::DINPUT_MOUSE_ACQUIRE_MAYBE));
            f();
        }
        *(rb(va::G_IN_GAME_SESSION_FLAG) as *mut u8) = 0;

        let update_cursor: unsafe extern "stdcall" fn(u32) =
            transmute(rb(va::FRONTEND_DIALOG_UPDATE_CURSOR));
        update_cursor(rb(va::G_INGAME_FRONTEND_DIALOG));

        if dialog == 0 {
            return;
        }

        let ce0b5 = *(game_world as *const u8).add(0xCE0B5);
        let mut headful_fullscreen_exit = false;

        if ce0b5 == 0 {
            let f: unsafe extern "stdcall" fn() =
                transmute(rb(va::MOUSE_CURSOR_SNAP_TO_SCREEN_CENTER_MAYBE));
            f();

            let ce0b4 = *(game_world as *const u8).add(0xCE0B4);
            if ce0b4 != 0 {
                let ctx = read_global_ptr::<RenderContext>(va::G_RENDER_CONTEXT);
                let mut buf = FastcallResult::default();
                RenderContext::construct_frame_buffer_raw(ctx, &mut buf, 0x280, 0x1E0);

                // Stock WA follows up with `if (game_world != g_GameWorldInstance) {
                // release_frame_buffer; OnGraphicsInitError(game_world,
                // game_world); ExitProcess(1); }`. Two latent bugs make that
                // branch unconditionally fatal:
                //   1. `g_GameWorldInstance` (0x007C03CC) is never written —
                //      it's zero-init data, so the guard is effectively
                //      `game_world != NULL`, always true for a live frontend.
                //   2. `OnGraphicsInitError` expects a `GLibError*` but WA
                //      passes `game_world`; reading `*game_world` returns the
                //      GameRuntime vtable, which `Localization__FormatGLibError`
                //      then feeds to `Localization__GetString` as a token id —
                //      AV.
                // Stock WA's outer `__try/__except` (in the dialog handler
                // calling `LaunchGameSession`) swallows the AV; ExitProcess
                // never fires. Our hook breaks the SEH chain, so the AV
                // escapes to WA's UnhandledExceptionFilter and writes
                // ERRORLOG. We skip the broken branch outright — the
                // post-game framebuffer rebuild above is the only useful
                // work this block does.
            }

            if *(rb(va::G_FULLSCREEN_FLAG) as *const u32) == 0 {
                let frame_p = *(rb(va::G_FRONTEND_FRAME) as *const u32);
                cwnd_move_window(frame_p, 0, 0, 0x280, 0x1E0, true);
            }

            let ce0b4 = *(game_world as *const u8).add(0xCE0B4);
            if ce0b4 == 0 {
                *(game_world as *mut u8).add(0xCE0B7) = 1;
                let f: unsafe extern "stdcall" fn(u32) =
                    transmute(rb(va::MOUSE_MODE_ENTER_WINDOWED_MAYBE));
                f(game_world);
                *(game_world as *mut u8).add(0xCE0B7) = 0;
            }

            // FUN_0040CAA0 — thiscall(this=game_world+0xa4) + 1 stack arg
            // (same value). Walks the render tree's children.
            let subobj_addr = game_world.wrapping_add(0xa4);
            wa_call::thiscall_1(
                va::GAME_WORLD_RENDER_CHILDREN_MAYBE,
                subobj_addr,
                subobj_addr,
            );
        } else {
            headful_fullscreen_exit = true;
        }

        let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
        *frame.add(0x58) = 0;

        let ce0b5 = *(game_world as *const u8).add(0xCE0B5);
        if ce0b5 == 0 {
            wa_call::call_usercall_esi(
                &raw mut audio_state_local as u32,
                rb(va::DSOUND_CHANNEL_ACQUIRE_MAYBE),
            );

            let f: unsafe extern "stdcall" fn() = transmute(rb(va::WAV_BANK_LOAD_ALL_MAYBE));
            f();

            let audio_table = *(rb(va::G_AUDIO_HANDLE_TABLE_PTR) as *const *mut u8);
            let wav_handle = *(audio_table.add(0x128) as *const u32);
            wa_call::call_usercall_edi_stack2(
                &raw mut audio_state_local as u32,
                wav_handle,
                1,
                rb(va::WAV_PLAYER_PLAY),
            );
        }

        cwnd_show_window(dialog, 5); // SW_SHOW

        if !headful_fullscreen_exit {
            let dialog_hwnd = *((dialog as *const u8).add(0x20) as *const u32);
            let prev_hwnd = iat_call_1(va::IAT_SET_ACTIVE_WINDOW, dialog_hwnd);
            let _ = cwnd_from_handle(prev_hwnd); // discard return — original code does the same
            let _ = cwnd_set_focus(dialog);
        } else {
            let ctx = read_global_ptr::<RenderContext>(va::G_RENDER_CONTEXT);
            let mut buf = FastcallResult::default();
            RenderContext::renderer_slot8_raw(ctx, &mut buf);
            *((game_world as *mut u8).add(0x150) as *mut u32) = 0;
            *(rb(va::G_FULLSCREEN_RESTORE_FLAG_MAYBE) as *mut u32) = 0;
            *(game_world as *mut u8).add(0xCE0B7) = 1;
        }
    }
}
