//! WA-internal frontend function wrappers.

use core::mem::transmute;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::engine::game_info_snapshot;
use crate::engine::game_session_run::run_game_session;
use crate::rebase::rb;
use crate::render::display::context::{FastcallResult, RenderContext};
use crate::wa::mfc::{AppSubObjA4, CDialogHandle, CWinApp, CWnd, CWndHandle, cwnd_hwnd};
use crate::wa_call;

/// When set, [`launch_game_session`] skips the `subobj_a4` vtable slot 13
/// pre-game UI hook AND the paired `DXCtl::sub_40CAA0` post-game
/// render-children walk.
///
/// Slot 13 expects to be called from within an MFC dialog-handler stack
/// frame (where MFC has set up some thread-local context); our custom
/// frontend invokes `launch_game_session` from a Win32 message hook
/// instead, where slot 13 crashes. The post-game walk is paired with it —
/// it iterates render-tree children that slot 13 attaches pre-game, so
/// skipping one without the other leaves the walk traversing stale
/// nodes. Normal WA-frontend dialog handlers leave this flag false so
/// both hooks fire as designed.
///
/// Set this *before* calling `launch_game_session` and clear it after.
pub static SUPPRESS_PRE_LAUNCH_VT13: AtomicBool = AtomicBool::new(false);

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

const FCS_DIALOG_FLAGS: usize = 0x3C;
const FCS_DIALOG_SCREEN_ID: usize = 0x44;
const FCS_DIALOG_PALETTE_OBJ: usize = 0x12C;
const FCS_DIALOG_PALETTE_PARAM: usize = 0x134;
const FCS_VTABLE_TRANSITION_METHOD: u32 = 0x15C;
const FCS_FLAG_INIT_BITS: u32 = 0x18;
const FCS_FLAG_CLEAR_BIT: u32 = 0x10;

/// Rust port of `FrontendChangeScreen` (0x00447A20). Original is usercall
/// (ESI = dialog, screen_id on stack); this takes `dialog` explicitly.
///
/// Drops `screen_id == 25` (IntroMovie). Only WA-side caller is
/// `CMainMenu::OnTimer` (0x00486D20), an idle screensaver triggered 60s after
/// each fresh MainMenu — undesirable in OpenWA, and the custom launcher
/// re-arms it on every post-match return.
pub unsafe fn frontend_change_screen(dialog: *mut CWnd, screen_id: u32) {
    unsafe {
        if screen_id == 25 {
            return;
        }

        let g_frontend_frame = wa_call::read_global(va::G_FRONTEND_FRAME);
        let dialog_addr = dialog as usize;

        if g_frontend_frame == 0 {
            let flags = *((dialog_addr + FCS_DIALOG_FLAGS) as *const u32);
            if (flags & FCS_FLAG_INIT_BITS) != 0 {
                *((dialog_addr + FCS_DIALOG_SCREEN_ID) as *mut u32) = screen_id;
                *((dialog_addr + FCS_DIALOG_FLAGS) as *mut u32) = flags & !FCS_FLAG_CLEAR_BIT;
            }
            return;
        }

        let wnd = CWndHandle(dialog as u32);
        let dlg = CDialogHandle(dialog as u32);

        wnd.enable_window(false);

        let palette_param = *((dialog_addr + FCS_DIALOG_PALETTE_PARAM) as *const u32);
        let eax_value = *((dialog_addr + FCS_DIALOG_PALETTE_OBJ) as *const u32);
        palette_animation(eax_value, palette_param);

        let vtable = *(dialog as *const u32);
        for i in 1u32..=2 {
            wa_call::thiscall_indirect_1(vtable + FCS_VTABLE_TRANSITION_METHOD, dialog as u32, i);
        }

        wnd.enable_window(true);
        dlg.end_dialog(screen_id);
    }
}

// ─── Frontend::LaunchGameSession (0x004EC540) ───────────────────────────────
//
// `__stdcall(CWinApp* app, CWnd* dialog, int p3, int p4)`, RET 0x10. The
// frontend's single funnel into `GameSession::Run`. Wraps the run with audio
// pause/resume, display-mode toggle, mouse cursor management, and frontend
// window hide/restore. WA-side has 11 callers (frontend dialog handlers),
// so the WA address is hooked as a full replacement.
//
// The first arg is the MFC application singleton `&g_CWinApp` (Ghidra's
// prototype calls it "DDGame"; that's misleading — every caller passes the
// MFC `theApp`). MSVC reaches several scattered BSS globals via
// `app + huge_offset` base-relative addressing rather than absolute loads;
// in WA's disassembly those look like fields of the param, but they are NOT:
//
//   +0xCE0B4 (= 0x0088E484) → `g_AltDisplaySurfaceAllocated`
//   +0xCE0B5 (= 0x0088E485) → `g_DisplayModeFlag` (== headless mode)
//   +0xCE0B6 (= 0x0088E486) → `g_DisplayModeFlagAtGameStart`
//   +0xCE0B7 (= 0x0088E487) → `g_MouseModeReentryLatch`
//
// Real CWinApp fields used here: vtable @ +0x00, embedded sub-object @ +0xa4
// (slot 13 called pre-game, walked post-game), u32 @ +0x150 (zeroed on the
// headful-fullscreen ExitProcess fallback).

/// Read a `*mut GameInfo` from a global slot.
#[inline]
unsafe fn read_global_ptr<T>(addr: u32) -> *mut T {
    unsafe { *(rb(addr) as *const *mut T) }
}

/// CWnd::ShowWindow(this, n_cmd_show) — thiscall.
#[inline]
unsafe fn cwnd_show_window(cwnd: *mut CWnd, n_cmd_show: i32) {
    unsafe {
        wa_call::thiscall_1(va::CWND_SHOW_WINDOW, cwnd as u32, n_cmd_show as u32);
    }
}

/// CWnd::MoveWindow(this, x, y, w, h, repaint) — thiscall, 5 stack args.
#[inline]
unsafe fn cwnd_move_window(cwnd: *mut CWnd, x: i32, y: i32, w: i32, h: i32, repaint: bool) {
    unsafe {
        let f: unsafe extern "fastcall" fn(*mut CWnd, u32, i32, i32, i32, i32, u32) =
            transmute(rb(va::CWND_MOVE_WINDOW));
        f(cwnd, 0, x, y, w, h, repaint as u32);
    }
}

/// CWnd::SetFocus(this) — thiscall.
#[inline]
unsafe fn cwnd_set_focus(cwnd: *mut CWnd) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(*mut CWnd, u32) -> u32 =
            transmute(rb(va::CWND_SET_FOCUS));
        f(cwnd, 0)
    }
}

/// CWnd::FromHandle(hwnd) — static stdcall.
#[inline]
unsafe fn cwnd_from_handle(hwnd: u32) -> *mut CWnd {
    unsafe {
        let f: unsafe extern "stdcall" fn(u32) -> *mut CWnd = transmute(rb(va::CWND_FROM_HANDLE));
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
pub unsafe extern "stdcall" fn launch_game_session(
    app: *mut CWinApp,
    dialog: *mut CWnd,
    peer_connections: *const *mut crate::input::PeerState,
    peer_connection_count: u32,
) {
    unsafe {
        // [ESP+0x18] in the original — a 4-byte scratch for the audio/mouse
        // bridges that take an out-pointer in ESI/EDI.
        let mut audio_state_local: u32 = 0;

        // Capture GameInfo for replay by custom frontends. Idempotent —
        // first launch wins. Only useful when a real frontend dialog has
        // populated GameInfo; cheap when not.
        game_info_snapshot::capture();

        let console_mode = *(rb(va::G_CONSOLE_MODE) as *const u32);

        if console_mode == 0 {
            // ── Pre-launch teardown ────────────────────────────────────────
            let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
            *frame.add(0x58) = 1;

            if !dialog.is_null() {
                let audio_table = *(rb(va::G_AUDIO_HANDLE_TABLE_PTR) as *const *mut u8);
                let wav_handle = *(audio_table.add(0x128) as *const u32);

                wa_call::call_usercall_eax_esi(
                    wav_handle,
                    &raw mut audio_state_local as u32,
                    rb(va::WAV_PLAYER_CHECK_OR_BIND_MAYBE),
                );

                // `g_MainFrontend` is never written; this comparison is
                // always false and the gated `WavPlayer_PreparePlay` is dead
                // code. Preserved for fidelity.
                let main_frontend = *(rb(va::G_MAIN_FRONTEND) as *const *mut CWinApp);
                if app == main_frontend {
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

                // Vtable call on the embedded sub-object at +0xa4, slot 13.
                // Skipped when called from a custom frontend (see flag docs).
                if !SUPPRESS_PRE_LAUNCH_VT13.load(Ordering::Acquire) {
                    let subobj: *mut AppSubObjA4 = &raw mut (*app).subobj_a4;
                    let vt = (*subobj).vtable as *const usize;
                    let slot13: usize = *vt.add(0xd); // offset 0x34 / 4
                    let f: unsafe extern "fastcall" fn(*mut AppSubObjA4, u32) = transmute(slot13);
                    f(subobj, 0);
                }

                // RenderContext::release_frame_buffer(this, !headless)
                let ctx = read_global_ptr::<RenderContext>(va::G_RENDER_CONTEXT);
                let headless = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
                let mut buf = FastcallResult::default();
                RenderContext::release_frame_buffer_raw(ctx, &mut buf, (headless == 0) as i32);
            }

            *(rb(va::G_FRONTEND_TICK_LATCH_MAYBE) as *mut u8) = 0;
            *(rb(va::G_IN_GAME_SESSION_FLAG) as *mut u8) = 1;

            // Snapshot the headless flag for the duration of the session.
            let headless = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
            *(rb(va::G_DISPLAY_MODE_FLAG_AT_GAME_START) as *mut u8) = headless;

            if headless == 0 {
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
        let game_info = rb(va::G_GAME_INFO) as *mut GameInfo;
        let state_buf = rb(va::G_GAME_SESSION_STATE_BUFFER) as *mut u8;
        run_game_session(
            game_info,
            module_arg,
            state_buf,
            peer_connections,
            peer_connection_count,
        );

        // ── Post-game restore ──────────────────────────────────────────────
        let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
        if !frame.is_null() {
            *frame.add(0x58) = 1;
        }
        *(rb(va::G_PENDING_INPUT_FLAG_MAYBE) as *mut u32) = 0;

        if console_mode != 0 {
            return;
        }

        let headless = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
        if headless == 0 {
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

        if dialog.is_null() {
            return;
        }

        let headless = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
        let mut headful_fullscreen_exit = false;

        if headless == 0 {
            let f: unsafe extern "stdcall" fn() =
                transmute(rb(va::MOUSE_CURSOR_SNAP_TO_SCREEN_CENTER_MAYBE));
            f();

            let alt_surface = *(rb(va::G_ALT_DISPLAY_SURFACE_ALLOCATED) as *const u8);
            if alt_surface != 0 {
                let ctx = read_global_ptr::<RenderContext>(va::G_RENDER_CONTEXT);
                let mut buf = FastcallResult::default();
                RenderContext::construct_frame_buffer_raw(ctx, &mut buf, 0x280, 0x1E0);

                // Stock WA follows up with `if (app != g_GameWorldInstance) {
                // release_frame_buffer; OnGraphicsInitError(app, app);
                // ExitProcess(1); }`. Two latent bugs make that branch
                // unconditionally fatal:
                //   1. `g_GameWorldInstance` (0x007C03CC) is never written —
                //      it's zero-init data, so the guard is effectively
                //      `app != NULL`, always true.
                //   2. `OnGraphicsInitError` expects a `GLibError*` but WA
                //      passes `app`; reading `*app` returns the CWormsApp
                //      vtable, which `Localization__FormatGLibError` then
                //      feeds to `Localization__GetString` as a token id — AV.
                // Stock WA's outer `__try/__except` (in the dialog handler
                // calling `LaunchGameSession`) swallows the AV; ExitProcess
                // never fires. Our hook breaks the SEH chain, so the AV
                // escapes to WA's UnhandledExceptionFilter and writes
                // ERRORLOG. We skip the broken branch outright — the
                // post-game framebuffer rebuild above is the only useful
                // work this block does.
            }

            if *(rb(va::G_FULLSCREEN_FLAG) as *const u32) == 0 {
                let frame_p = *(rb(va::G_FRONTEND_FRAME) as *const *mut CWnd);
                cwnd_move_window(frame_p, 0, 0, 0x280, 0x1E0, true);
            }

            let alt_surface = *(rb(va::G_ALT_DISPLAY_SURFACE_ALLOCATED) as *const u8);
            if alt_surface == 0 {
                *(rb(va::G_MOUSE_MODE_REENTRY_LATCH) as *mut u8) = 1;
                let f: unsafe extern "stdcall" fn(*mut CWinApp) =
                    transmute(rb(va::MOUSE_MODE_ENTER_WINDOWED_MAYBE));
                f(app);
                *(rb(va::G_MOUSE_MODE_REENTRY_LATCH) as *mut u8) = 0;
            }

            // DXCtl__sub_40CAA0 — thiscall(this=&app->subobj_a4) + 1 stack arg
            // (same value). Walks the render tree's children. Paired with
            // the pre-game slot 13 hook; if we skipped that, the children
            // were never attached and walking them crashes — skip too.
            if !SUPPRESS_PRE_LAUNCH_VT13.load(Ordering::Acquire) {
                let subobj = &raw mut (*app).subobj_a4 as u32;
                wa_call::thiscall_1(va::GAME_WORLD_RENDER_CHILDREN_MAYBE, subobj, subobj);
            }
        } else {
            headful_fullscreen_exit = true;
        }

        let frame = *(rb(va::G_FRONTEND_FRAME) as *const *mut u8);
        *frame.add(0x58) = 0;

        let headless = *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8);
        if headless == 0 {
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
                rb(crate::generated::addresses::WavPlayer__Play),
            );
        }

        cwnd_show_window(dialog, 5); // SW_SHOW

        if !headful_fullscreen_exit {
            let dialog_hwnd = cwnd_hwnd(dialog);
            let prev_hwnd = iat_call_1(va::IAT_SET_ACTIVE_WINDOW, dialog_hwnd);
            let _ = cwnd_from_handle(prev_hwnd); // discard return — original code does the same
            let _ = cwnd_set_focus(dialog);
        } else {
            let ctx = read_global_ptr::<RenderContext>(va::G_RENDER_CONTEXT);
            let mut buf = FastcallResult::default();
            RenderContext::renderer_slot8_raw(ctx, &mut buf);
            (*app).field_150 = 0;
            *(rb(va::G_FULLSCREEN_RESTORE_FLAG_MAYBE) as *mut u32) = 0;
            *(rb(va::G_MOUSE_MODE_REENTRY_LATCH) as *mut u8) = 1;
        }
    }
}
