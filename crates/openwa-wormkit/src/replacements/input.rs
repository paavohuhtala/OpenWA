//! Input hooks for replay fast-forward.
//!
//! When `OPENWA_REPLAY_TEST=1`, posts WM_KEYDOWN/WM_KEYUP messages for
//! spacebar to the game's window, advancing the replay one turn per press.
//!
//! The key-down and key-up must land in SEPARATE game frames so the game
//! sees a stable "pressed" state for at least one frame before release.
//! WA runs at 50fps (~20ms/frame), so we use 50ms gaps between down/up.
//!
//! To work without stealing focus, we subclass the game's WndProc to
//! intercept WM_ACTIVATE/WM_ACTIVATEAPP and make WA think it always has
//! focus. This allows PostMessage input to work even when the window is
//! in the background.
//!
//! Activation is delayed ~3s after DLL load to let WA reach gameplay.

use crate::log_line;

const VK_SPACE: u32 = 0x20;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_ACTIVATE: u32 = 0x0006;
const WM_ACTIVATEAPP: u32 = 0x001C;
const WA_ACTIVE: u32 = 1;
const GWL_WNDPROC: i32 = -4;
const SW_RESTORE: i32 = 9;

/// Original WndProc, saved when we subclass the window.
static mut ORIG_WNDPROC: u32 = 0;

/// Our WndProc wrapper: intercepts deactivation messages so WA thinks
/// it always has focus, then forwards everything to the original.
unsafe extern "system" fn hook_wndproc(hwnd: u32, msg: u32, wparam: u32, lparam: u32) -> u32 {
    extern "system" {
        fn CallWindowProcA(lpPrevWndFunc: u32, hWnd: u32, msg: u32, wParam: u32, lParam: u32) -> u32;
    }

    match msg {
        WM_ACTIVATE => {
            // Always tell WA it's being activated (WA_ACTIVE=1)
            return CallWindowProcA(ORIG_WNDPROC, hwnd, msg, WA_ACTIVE, lparam);
        }
        WM_ACTIVATEAPP => {
            // Always tell WA the app is being activated (wParam=TRUE)
            return CallWindowProcA(ORIG_WNDPROC, hwnd, msg, 1, lparam);
        }
        _ => {}
    }

    CallWindowProcA(ORIG_WNDPROC, hwnd, msg, wparam, lparam)
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_REPLAY_TEST").is_err() {
        return Ok(());
    }

    let _ = log_line("[Input] Replay test mode — will post spacebar messages for fast-forward");

    extern "system" {
        fn FindWindowA(lpClassName: *const u8, lpWindowName: *const u8) -> u32;
        fn PostMessageA(hWnd: u32, msg: u32, wParam: u32, lParam: u32) -> i32;
        fn SetForegroundWindow(hWnd: u32) -> i32;
        fn ShowWindow(hWnd: u32, nCmdShow: i32) -> i32;
        fn SetWindowLongA(hWnd: u32, nIndex: i32, dwNewLong: u32) -> u32;
    }

    std::thread::spawn(move || {
        let _ = log_line("[Input] Waiting 1s before enabling fast-forward...");
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Find the WA game window
        let hwnd = unsafe { FindWindowA(std::ptr::null(), b"Worms Armageddon\0".as_ptr()) };
        if hwnd == 0 {
            let _ = log_line("[Input] ERROR: Could not find game window");
            return;
        }
        let _ = log_line(&format!("[Input] Found game window: HWND=0x{:08X}", hwnd));

        // Subclass the WndProc to intercept deactivation messages
        unsafe {
            ORIG_WNDPROC = SetWindowLongA(hwnd, GWL_WNDPROC, hook_wndproc as *const () as u32);
            if ORIG_WNDPROC == 0 {
                let _ = log_line("[Input] ERROR: SetWindowLongA failed");
                return;
            }
        }
        let _ = log_line("[Input] WndProc subclassed — WA will think it always has focus");

        // Restore and focus window once to kick things off
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
        let _ = log_line("[Input] Fast-forward enabled — posting spacebar messages");

        // lParam encodes scan code (0x39 for space) and repeat count
        let down_lparam: u32 = 0x0039_0001; // scan=0x39, repeat=1, bit30=0 (first press)
        let up_lparam: u32 = 0xC039_0001; // scan=0x39, repeat=1, bit30+31 set (release)

        loop {
            unsafe {
                // Send key DOWN — must be processed in its own frame
                let r = PostMessageA(hwnd, WM_KEYDOWN, VK_SPACE, down_lparam);
                if r == 0 {
                    let _ = log_line("[Input] PostMessage failed — window closed");
                    break;
                }
            }
            // Wait >1 frame (50ms > 20ms/frame) so game processes key-down
            std::thread::sleep(std::time::Duration::from_millis(50));

            unsafe {
                // Send key UP — processed in a subsequent frame
                let r = PostMessageA(hwnd, WM_KEYUP, VK_SPACE, up_lparam);
                if r == 0 {
                    break;
                }
            }
            // Wait for the release to be processed before next press
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    Ok(())
}
