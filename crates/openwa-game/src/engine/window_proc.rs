//! Rust port of `GameSession::WindowProc` (0x00572660).
//!
//! Engine-mode `WNDPROC` installed by `FUN_004ECD40` via `SetWindowLongA`,
//! replacing the standard MFC `WindowProcA` for the game window. The
//! original MFC WNDPROC is cached at [`va::G_MFC_WNDPROC`] for the
//! outer-guard fall-through — anything we don't actively handle chains
//! through `CallWindowProcA` so MFC keeps working for non-input messages.
//!
//! Handles three message families when `g_InGameLoop != 0` and
//! `g_InputHookMode == Off`:
//!   - Keyboard (`WM_KEYFIRST..=WM_KEYLAST`, 0x100..=0x109)
//!   - Mouse (`WM_MOUSEFIRST..=WM_MOUSELAST`, 0x200..=0x20E)
//!   - Palette (`WM_PALETTECHANGED`, 0x311)
//!
//! Replay tests bypass the message pump entirely — this whole module is
//! headful-only. See `project_mouse_subsystem.md` for the design notes
//! and headful test plan.

use core::ffi::c_void;
use core::mem::transmute;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcA, DefWindowProcA, MB_ICONHAND, MB_OK, SetCursorPos, WM_CHAR, WM_DEADCHAR,
    WM_KEYDOWN, WM_KEYFIRST, WM_KEYLAST, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEFIRST, WM_MOUSELAST, WM_MOUSEMOVE, WM_PALETTECHANGED,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSCHAR, WM_SYSDEADCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDPROC,
};

use crate::address::va;
use crate::engine::game_session::{GameSession, get_game_session};
use crate::input::keyboard::Keyboard;
use crate::input::mouse::{mouse_poll_and_acquire, mouse_release_and_center};
use crate::rebase::rb;

// ─── Win32 raw bindings (not exposed by windows-sys 0.61 in stable paths) ─

#[repr(C)]
#[allow(clippy::upper_case_acronyms)] // Mirrors the Win32 typedef name.
struct OFSTRUCT {
    cBytes: u8,
    fFixedDisk: u8,
    nErrCode: u16,
    Reserved1: u16,
    Reserved2: u16,
    szPathName: [u8; 128],
}

const OF_EXIST: u32 = 0x4000;

unsafe extern "system" {
    fn ClientToScreen(hwnd: HWND, lp: *mut POINT) -> i32;
    fn CreateDirectoryA(lpPathName: *const u8, lpSecurityAttributes: *const c_void) -> i32;
    fn OpenFile(lpFileName: *const u8, lpReOpenBuff: *mut OFSTRUCT, uStyle: u32) -> i32;
}

// ─── Inline constants ────────────────────────────────────────────────────

// MK_* mouse-button flags (wParam in WM_MOUSE* messages).
const MK_LBUTTON: u32 = 0x0001;
const MK_RBUTTON: u32 = 0x0002;
const MK_MBUTTON: u32 = 0x0010;

// Virtual-key codes used in match arms (windows-sys VK_* are u16, doesn't
// pattern-match against u32; defining our own avoids the cast/binding hazard).
const VK_PAUSE: u32 = 0x13;
const VK_G: u32 = 0x47;
const VK_F4: u32 = 0x73;
const VK_SCROLL: u32 = 0x91;
// Modifier VKs (passed to GetKeyState which takes i32).
const VK_SHIFT_I: i32 = 0x10;
const VK_CONTROL_I: i32 = 0x11;
const VK_MENU_I: i32 = 0x12;

// ─── Modifier helpers ─────────────────────────────────────────────────────

#[inline]
unsafe fn shift_held() -> bool {
    unsafe { GetKeyState(VK_SHIFT_I) < 0 }
}
#[inline]
unsafe fn ctrl_held() -> bool {
    unsafe { GetKeyState(VK_CONTROL_I) < 0 }
}
#[inline]
unsafe fn alt_held() -> bool {
    unsafe { GetKeyState(VK_MENU_I) < 0 }
}

// ─── Bridges (kept in WA) ─────────────────────────────────────────────────

#[inline]
unsafe fn palette_log_change(hwnd: HWND) {
    unsafe {
        let f: unsafe extern "stdcall" fn(HWND) = transmute(rb(va::PALETTE_LOG_CHANGE) as usize);
        f(hwnd);
    }
}

/// Bridge for `Palette::RealizeFromSystem` (0x005A1110),
/// `__usercall(EDI=palette_buf, [ESP+4]=*out_counter), RET 0x4` — stdcall
/// for the stack arg, with EDI as an implicit register parameter. Compares
/// the live system palette (1024 bytes from `palette_buf`) against the
/// cached reference at `DAT_008ac8c8`; if it differs, increments a
/// "changed" counter and copies it in. Always writes
/// `*out_counter = DAT_008ac8c4` before returning.
///
/// Naked trampoline: load EDI from the first arg, push the second arg,
/// call the WA-side entry. The callee's `RET 0x4` already pops the
/// pushed `out_counter`, so we don't add to ESP afterward — only restore
/// EDI and return.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_palette_realize_from_system(
    _palette_buf: *mut u8,
    _out_counter: *mut u32,
) {
    core::arch::naked_asm!(
        // [esp+0]=ret, [esp+4]=palette_buf, [esp+8]=out_counter
        "pushl %edi",                  // save caller's EDI
        // After push: [esp+0]=saved_edi, [esp+4]=ret, [esp+8]=palette_buf, [esp+c]=out_counter
        "movl 8(%esp), %edi",          // EDI = palette_buf
        "movl 0xc(%esp), %eax",        // EAX = out_counter
        "pushl %eax",                  // push out_counter as stack arg
        "calll *({addr})",             // callee RET 0x4 cleans the pushed arg
        "popl %edi",                   // restore EDI
        "retl",                        // cdecl: outer caller cleans 2 args
        addr = sym PALETTE_REALIZE_FROM_SYSTEM_ADDR,
        options(att_syntax),
    );
}

/// Resolved at install time (set in `init_window_proc_addrs`).
static mut PALETTE_REALIZE_FROM_SYSTEM_ADDR: u32 = 0;

/// Offset within `DisplayGfx` of the 1024-byte cached system-palette buffer
/// passed to `Palette::RealizeFromSystem` via EDI. Verified at the two
/// `WindowProc` call sites (`0x0057284E` Shift+Pause; `0x00572B8C`
/// WM_PALETTECHANGED): both `MOV EDI, [g_GameSession+0xAC]; ADD EDI, 0x358D`.
const DISPLAY_PALETTE_BUF_OFFSET: usize = 0x358D;

/// Wrapper that pulls the palette buffer out of the live `DisplayGfx` and
/// invokes the trampoline. Returns silently if the display pointer is null
/// (shouldn't happen in normal play; defensive).
#[inline]
unsafe fn palette_realize_from_system(session: *mut GameSession, out_counter: *mut u32) {
    unsafe {
        let display = (*session).display;
        if display.is_null() {
            return;
        }
        call_palette_realize_from_system(display.add(DISPLAY_PALETTE_BUF_OFFSET), out_counter);
    }
}

#[inline]
unsafe fn screenshot_save_png(path: *const u8, display_param: u32) -> u32 {
    unsafe {
        let f: unsafe extern "stdcall" fn(*const u8, u32) -> u32 =
            transmute(rb(va::SCREENSHOT_SAVE_PNG) as usize);
        f(path, display_param)
    }
}

#[inline]
unsafe fn map_save_png_capture(runtime: *mut u8) {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut u8) =
            transmute(rb(va::MAP_SAVE_PNG_CAPTURE) as usize);
        f(runtime);
    }
}

// ─── Sub-handler: WM_PALETTECHANGED (0x311) ────────────────────────────────

unsafe fn handle_palette_changed(hwnd: HWND, wparam: WPARAM) {
    unsafe {
        let session = get_game_session();
        if session.is_null() {
            return;
        }
        // While the engine is suspended (minimised / headless pre-loop),
        // skip both the diagnostic log and the system-palette realize.
        if (*session).flag_5c != 0 {
            return;
        }
        palette_log_change(hwnd);
        // Ignore self-realisation events.
        if wparam as u32 == (*session).hwnd {
            return;
        }
        // Skip while a foreground display-mode change is still in flight.
        if *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8) != 0 {
            return;
        }
        let mut counter: u32 = 0;
        palette_realize_from_system(session, &raw mut counter);
    }
}

// ─── Sub-handler: WM_MOUSEMOVE (0x200) ─────────────────────────────────────

unsafe fn handle_mouse_move(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    unsafe {
        let session = get_game_session();
        if session.is_null() || (*session).mouse_acquired == 0 {
            return None; // fall through to MFC WNDPROC
        }
        // Pre-init sentinel — InitHardware hasn't written the screen
        // centre yet; swallow the event.
        if (*session).screen_center_x == i32::MIN {
            return Some(1);
        }

        let mut pt = POINT {
            x: (lparam & 0xFFFF) as i16 as i32,
            y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
        };
        let _ = hwnd; // we read the session's cached hwnd, matching WA exactly
        ClientToScreen((*session).hwnd as HWND, &raw mut pt);

        // First post-acquire move snaps the cursor back to centre and
        // suppresses the delta this tick (prevents a spurious jump when
        // alt-tabbing back into the game).
        if (*session).cursor_recenter_request != 0 {
            SetCursorPos((*session).screen_center_x, (*session).screen_center_y);
            (*session).cursor_recenter_request = 0;
            return Some(1);
        }

        if pt.x == (*session).cursor_x && pt.y == (*session).cursor_y {
            return Some(1);
        }

        // WA reconstructs the button bitmask from MK_* on every move. Click
        // handlers only flip individual bits, so a click+release between
        // two moves is lost; this is a deliberate WA quirk we preserve.
        let wp = wparam as u32;
        let buttons = (wp & MK_LBUTTON as u32)
            | ((wp & MK_RBUTTON as u32) >> 1) << 1
            | ((wp & MK_MBUTTON as u32) >> 4) << 2;
        (*session).mouse_button_state = buttons;

        (*session).mouse_delta_x = (*session)
            .mouse_delta_x
            .wrapping_add(pt.x - (*session).cursor_x);
        (*session).mouse_delta_y = (*session)
            .mouse_delta_y
            .wrapping_add(pt.y - (*session).cursor_y);
        (*session).cursor_x = pt.x;
        (*session).cursor_y = pt.y;
        Some(1)
    }
}

// ─── Sub-handler: mouse buttons (0x201..=0x208 via jump tbl @ 0x572c60) ────

/// Handles WM_LBUTTONDOWN / WM_RBUTTONDOWN / WM_MBUTTONDOWN with bit `bit`
/// (0=L, 1=R, 2=M). Returns the LRESULT to send back, or `None` to fall
/// through to MFC.
unsafe fn handle_button_down(bit: u32) -> Option<LRESULT> {
    unsafe {
        let session = get_game_session();
        if session.is_null() {
            return None;
        }
        if (*session).mouse_acquired == 0 {
            // First click after focus regain — re-grab and swallow.
            mouse_poll_and_acquire();
            return Some(0);
        }
        if (*session).home_lock_active != 0 {
            // Watcher mode: any input aborts the loop.
            (*session).exit_flag = 1;
            return Some(0);
        }
        (*session).mouse_button_state |= 1 << bit;
        Some(0)
    }
}

unsafe fn handle_button_up(bit: u32) -> Option<LRESULT> {
    unsafe {
        let session = get_game_session();
        if session.is_null() {
            return None;
        }
        if (*session).home_lock_active != 0 {
            (*session).exit_flag = 1;
            return Some(0);
        }
        (*session).mouse_button_state &= !(1 << bit);
        Some(0)
    }
}

// ─── Sub-handlers: keyboard ────────────────────────────────────────────────

/// Common DOWN tail: writes the per-VK key_state byte to 1, edge-clears
/// prev_state if it was sitting at 1. Skipped when home_lock_active.
unsafe fn write_key_down(session: *mut GameSession, vk: u8) -> LRESULT {
    unsafe {
        if (*session).home_lock_active != 0 {
            (*session).exit_flag = 1;
            return 0;
        }
        let kb = (*session).keyboard;
        if !kb.is_null() {
            let idx = vk as usize;
            if (*kb).prev_state[idx] == 1 {
                (*kb).prev_state[idx] = 0;
            }
            (*kb).key_state[idx] = 1;
        }
        0
    }
}

/// Common UP tail: clears both key_state and prev_state for the VK.
unsafe fn write_key_up(session: *mut GameSession, vk: u8) -> LRESULT {
    unsafe {
        if (*session).home_lock_active != 0 {
            (*session).exit_flag = 1;
            return 0;
        }
        let kb = (*session).keyboard;
        if !kb.is_null() {
            let idx = vk as usize;
            (*kb).prev_state[idx] = 0;
            (*kb).key_state[idx] = 0;
        }
        0
    }
}

/// VK_PAUSE handler: Shift → palette realize + beep. Alt → save level
/// map. None → save backbuffer screenshot. Always falls into the DOWN
/// tail (per-VK key state write).
unsafe fn key_pause(session: *mut GameSession) {
    unsafe {
        if shift_held() {
            if *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8) == 0 {
                let mut counter: u32 = 0;
                palette_realize_from_system(session, &raw mut counter);
            }
            MessageBeep(MB_OK);
            return;
        }
        if alt_held() {
            // WA passes session->game_runtime to the map writer.
            map_save_png_capture((*session).game_runtime as *mut u8);
            return;
        }

        // Bare Pause → screenshot. Build path: User\Capture\screenNNNN.png
        // by scanning for the first index with no existing file.
        const PREFIX: &[u8] = b"User\\Capture\\\0";
        // Ensure the directory exists; ignore failure (might already exist).
        let _ = CreateDirectoryA(PREFIX.as_ptr(), core::ptr::null());

        let mut path: [u8; 260] = [0; 260];
        let mut idx: u32 = 0;
        loop {
            // "User\Capture\screenNNNN.png" — fixed format, MAX_PATH-safe.
            let len = format_screenshot_path(&mut path, idx);
            // Truncate-write null terminator (already there from init).
            let _ = len;
            let mut ofs: OFSTRUCT = core::mem::zeroed();
            let exists = OpenFile(path.as_ptr(), &raw mut ofs, OF_EXIST);
            if exists == -1 {
                // No file at this index — use it.
                break;
            }
            idx += 1;
            if idx > 9999 {
                // Safety cap: 10k screenshots per session is more than
                // enough; bail rather than spin forever.
                MessageBeep(MB_ICONHAND);
                return;
            }
        }

        if screenshot_save_png(path.as_ptr(), (*session).display_param_1) == 0 {
            MessageBeep(MB_ICONHAND);
        }
    }
}

/// Format `User\Capture\screenNNNN.png\0` into `buf`. Returns the byte
/// length (excluding the trailing NUL). Caller's buffer must be at least
/// 28 bytes; we use a 260-byte stack buffer in practice (MAX_PATH).
fn format_screenshot_path(buf: &mut [u8; 260], n: u32) -> usize {
    use core::fmt::Write;
    struct Cursor<'a> {
        buf: &'a mut [u8; 260],
        pos: usize,
    }
    impl Write for Cursor<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let end = self.pos + bytes.len();
            if end >= self.buf.len() {
                return Err(core::fmt::Error);
            }
            self.buf[self.pos..end].copy_from_slice(bytes);
            self.pos = end;
            Ok(())
        }
    }
    let mut c = Cursor { buf, pos: 0 };
    let _ = write!(c, "User\\Capture\\screen{:04}.png", n);
    let pos = c.pos;
    buf[pos] = 0;
    pos
}

/// VK_G ('G') handler: in fullscreen, Ctrl+!Alt+!Shift releases the
/// cursor grab. Returns `Some(0)` if handled, `None` to fall through to
/// the regular DOWN tail.
unsafe fn key_g(session: *mut GameSession) -> Option<LRESULT> {
    unsafe {
        let _ = session;
        let fullscreen = *(rb(va::G_FULLSCREEN_FLAG) as *const u32) != 0;
        if fullscreen && ctrl_held() && !alt_held() && !shift_held() {
            mouse_release_and_center();
            return Some(0);
        }
        None
    }
}

/// VK_F4 handler: Alt+!Ctrl signals frontend exit (Alt+F4).
unsafe fn key_f4(session: *mut GameSession) -> Option<LRESULT> {
    unsafe {
        if alt_held() && !ctrl_held() {
            (*session).exit_flag = 1;
            return Some(0);
        }
        None
    }
}

/// VK_SCROLL handler: writes the Ctrl-state into a 1-byte field on the
/// Keyboard object (offset +0x0C). Always falls into the DOWN tail.
unsafe fn key_scroll(session: *mut GameSession) {
    unsafe {
        let kb = (*session).keyboard;
        if kb.is_null() {
            return;
        }
        // +0x0C is currently `_unknown_00c[0]` on Keyboard; written here
        // as 1/0 reflecting Ctrl-while-Scroll-Lock-pressed.
        let p = (kb as *mut u8).add(0x0C);
        *p = if ctrl_held() { 1 } else { 0 };
    }
}

unsafe fn handle_key_down(session: *mut GameSession, vk: u8) -> LRESULT {
    unsafe {
        match vk as u32 {
            VK_PAUSE => {
                key_pause(session);
                write_key_down(session, vk)
            }
            VK_G => {
                if let Some(r) = key_g(session) {
                    return r;
                }
                write_key_down(session, vk)
            }
            VK_F4 => {
                if let Some(r) = key_f4(session) {
                    return r;
                }
                write_key_down(session, vk)
            }
            VK_SCROLL => {
                key_scroll(session);
                write_key_down(session, vk)
            }
            _ => write_key_down(session, vk),
        }
    }
}

unsafe fn handle_char(session: *mut GameSession, wparam: WPARAM) -> LRESULT {
    unsafe {
        if (*session).home_lock_active != 0 {
            (*session).exit_flag = 1;
            return 0;
        }
        let kb = (*session).keyboard;
        if kb.is_null() {
            return 0;
        }
        let ch = wparam as u8;
        if ch == 0 {
            return 0;
        }
        let head = (*kb).ring_head;
        let new_head = (head + 1) & 0xFF;
        if new_head == (*kb).ring_tail {
            // Buffer full — drop.
            return 0;
        }
        (*kb).ring_buffer[head as usize] = ch;
        (*kb).ring_head = new_head;
        0
    }
}

// ─── Top-level entry ───────────────────────────────────────────────────────

/// Rust port of `GameSession::WindowProc` (0x00572660).
///
/// `unsafe extern "system"` matches the Win32 `WNDPROC` ABI (stdcall on
/// x86, `RET 0x10`).
pub unsafe extern "system" fn engine_wnd_proc(
    hwnd: HWND,
    mut msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        // Outer guard. Anything outside our three message families, or
        // any message arriving while a modal-dialog input grab is active,
        // chains through to the cached MFC WNDPROC.
        let in_game_loop = *(rb(va::G_IN_GAME_LOOP) as *const u32) != 0;
        let input_hook_off = *(rb(va::G_INPUT_HOOK_MODE) as *const u32) == 0;
        // WA's range is `WM_KEYFIRST..=WM_KEYLAST + 1` — one past WM_KEYLAST
        // so newer messages added after WM_SYSDEADCHAR (e.g. WM_UNICHAR =
        // 0x109) are also accepted. Match that convention.
        let in_keyboard_range = (WM_KEYFIRST..=WM_KEYLAST + 1).contains(&msg);
        let in_mouse_range = (WM_MOUSEFIRST..=WM_MOUSELAST).contains(&msg);
        let is_palette = msg == WM_PALETTECHANGED;
        if !(in_game_loop && input_hook_off && (in_keyboard_range || in_mouse_range || is_palette))
        {
            let mfc: WNDPROC = Some(transmute::<usize, _>(
                *(rb(va::G_MFC_WNDPROC) as *const usize),
            ));
            return CallWindowProcA(mfc, hwnd, msg, wparam, lparam);
        }

        let session = get_game_session();
        if session.is_null() {
            return DefWindowProcA(hwnd, msg, wparam, lparam);
        }

        // Modifier-aware msg remap: when mouse_acquired, fold SYSKEY*/
        // DEADCHAR* down to KEY*/CHAR (subtract 4). 0x104→0x100, 0x105→
        // 0x101, 0x106→0x102, 0x107→0x103.
        if (*session).mouse_acquired != 0 {
            let off = msg.wrapping_sub(WM_SYSKEYDOWN);
            if off < 4 {
                msg -= 4;
            }
        }

        // Palette branch.
        if is_palette {
            handle_palette_changed(hwnd, wparam);
            return DefWindowProcA(hwnd, msg, wparam, lparam);
        }

        // Mouse branch.
        if msg == WM_MOUSEMOVE {
            return match handle_mouse_move(hwnd, wparam, lparam) {
                Some(r) => r,
                None => DefWindowProcA(hwnd, msg, wparam, lparam),
            };
        }
        if (WM_LBUTTONDOWN..=WM_MBUTTONUP).contains(&msg) {
            return match msg {
                WM_LBUTTONDOWN => handle_button_down(0),
                WM_LBUTTONUP => handle_button_up(0),
                WM_RBUTTONDOWN => handle_button_down(1),
                WM_RBUTTONUP => handle_button_up(1),
                WM_MBUTTONDOWN => handle_button_down(2),
                WM_MBUTTONUP => handle_button_up(2),
                _ => None, // LBUTTONDBLCLK / RBUTTONDBLCLK fall through
            }
            .unwrap_or_else(|| DefWindowProcA(hwnd, msg, wparam, lparam));
        }
        if (WM_MBUTTONDBLCLK..=WM_MOUSELAST).contains(&msg) {
            // M-doubleclick / X-buttons / wheels aren't handled.
            return DefWindowProcA(hwnd, msg, wparam, lparam);
        }

        // Keyboard branch.
        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if (*session).mouse_acquired == 0 {
                    return DefWindowProcA(hwnd, msg, wparam, lparam);
                }
                handle_key_down(session, wparam as u8)
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if (*session).mouse_acquired == 0 {
                    return DefWindowProcA(hwnd, msg, wparam, lparam);
                }
                write_key_up(session, wparam as u8)
            }
            WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR => {
                if (*session).mouse_acquired == 0 {
                    return DefWindowProcA(hwnd, msg, wparam, lparam);
                }
                handle_char(session, wparam)
            }
            _ => DefWindowProcA(hwnd, msg, wparam, lparam),
        }
    }
}

/// Install-time initializer: caches the rebased address of
/// `Palette::RealizeFromSystem` so the naked trampoline can `CALL` it via
/// a static rather than going through `rb()` at every call.
pub unsafe fn init_window_proc_addrs() {
    unsafe {
        PALETTE_REALIZE_FROM_SYSTEM_ADDR = rb(va::PALETTE_REALIZE_FROM_SYSTEM);
    }
}

// Compile-time offset checks — guard against accidental Keyboard layout drift.
const _: () = {
    assert!(core::mem::offset_of!(Keyboard, ring_head) == 0x14);
    assert!(core::mem::offset_of!(Keyboard, ring_tail) == 0x18);
    assert!(core::mem::offset_of!(Keyboard, ring_buffer) == 0x1C);
    assert!(core::mem::offset_of!(Keyboard, key_state) == 0x11C);
    assert!(core::mem::offset_of!(Keyboard, prev_state) == 0x21C);
};
