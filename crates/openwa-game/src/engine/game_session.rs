use windows_sys::Win32::Foundation::POINT;

use crate::FieldRegistry;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::engine::game_info::GameInfo;
use crate::engine::runtime::GameRuntime;
use crate::input::keyboard::Keyboard;
use crate::render::display::palette::Palette;

/// `GameSession` vtable (`PTR_FUN_0066b3f8`, 1 slot — scalar deleting dtor).
///
/// Followed immediately at 0x0066B3FC by `INPUT_CTRL_VTABLE`, confirming the
/// 1-slot size.
#[openwa_game::vtable(size = 1, va = 0x0066B3F8, class = "GameSession")]
pub struct GameSessionVtable {
    /// scalar deleting destructor (0x0058C040, RET 0x4) — `flags & 1` frees
    /// the heap allocation after running the C++ destructor body.
    #[slot(0)]
    pub destructor: fn(this: *mut GameSession, flags: u32) -> *mut GameSession,
}

/// Top-level game session context, allocated once per game run.
///
/// `G_GAME_SESSION` (0x7A0884) stores a pointer to this struct. Created by
/// `GameSession__Constructor` (0x58BFA0, usercall EAX=this), populated by
/// `GameEngine__InitHardware` (0x56D350), and destroyed by
/// `GameEngine__Shutdown` (0x56DCD0).
///
/// Lifecycle:
/// ```text
/// GameSession__Run (0x572F50, ESI=GameInfo)
///   ├─ alloc 0x120 bytes → GameSession__Constructor (0x58BFA0)
///   ├─ G_GAME_SESSION ← &this
///   ├─ GameEngine__InitHardware → fills subsystem pointers (0xA0–0xC0)
///   ├─ game main loop (exit when exit_flag != 0)
///   └─ GameEngine__Shutdown → destroys all subsystems
/// ```
///
/// In headless/stats mode (`GameInfo+0xF914 != 0`), `display_gfx` holds a
/// `GameStats` object with a DDInput vtable instead of a real `DisplayGfx`.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct GameSession {
    /// 0x000: vtable pointer (`PTR_FUN_0066b3f8` — single scalar deleting dtor slot)
    pub vtable: *const GameSessionVtable,
    pub _unknown_004: [u8; 4],
    /// 0x008: main window HWND, stored from `hWnd` in `GameSession__Run`
    pub hwnd: u32,
    /// 0x00C: first param from `GameSession__Run` (passed through from `Frontend__LaunchGameSession`)
    pub run_param_1: u32,
    pub _unknown_010: [u8; 0x14],
    /// 0x024: Always set to 1 by GameEngine__InitHardware.
    pub init_flag: u32,
    /// 0x028: Home-lock-active flag — `(GameInfo.home_lock != 0) as u32`.
    /// When set, the engine WindowProc *ignores* keyboard/mouse input and
    /// instead sets `exit_flag = 1` on any input message — i.e. attended
    /// "watcher" mode that aborts on any user interaction. The fullscreen
    /// state lives elsewhere in the global `g_FullscreenFlag` (0x88E484);
    /// this field was historically misnamed `fullscreen_flag`.
    pub home_lock_active: u32,
    /// 0x02C: Mouse acquired flag — set to 1 on init and by
    /// `Mouse::PollAndAcquire` (0x00572620); cleared by
    /// `Mouse::ReleaseAndCenter` (0x005725B0) and the headless pre-loop.
    /// WindowProc gates all WM_MOUSE* processing on this; click-without-acquire
    /// re-routes through `Mouse::PollAndAcquire` to re-grab the cursor.
    pub mouse_acquired: u32,
    /// 0x030: set to 1 when desktop is unavailable (`OpenInputDesktop` returns null)
    pub desktop_lost: u32,
    /// 0x034: checked in `GameSession__ProcessFrame` for non-engine frame path
    pub flag_34: u32,
    /// 0x038: back-pointer to the `GameInfo` config struct (ESI from `GameSession__Run`)
    pub config_ptr: *mut GameInfo,
    /// 0x03C: nonzero = exit the game main loop (checked each frame in
    /// `GameSession__Run`). `GameSession::WindowProc` sets it to 1 on:
    ///  - any keyboard/mouse message while `home_lock_active` is set
    ///    (watcher-mode "abort on user interaction")
    ///  - Alt+F4 with no Ctrl held (frontend-exit signal)
    pub exit_flag: u32,
    /// 0x040: game-end status/result code
    pub flag_40: u32,
    /// 0x044: Replay-active flag — DispatchFrame sets this to 1 while the
    /// replay speed accumulator is advancing a frame.
    pub replay_active_flag: u32,
    pub _unknown_048: [u8; 4],
    /// 0x04C: copied from `GameInfo+0xF39C` (display param)
    pub display_param_1: u32,
    /// 0x050: copied from `GameInfo+0xF3A0` (display param)
    pub display_param_2: u32,
    /// 0x054: display center X = `display_width / 2`.
    /// Initialized to `0x80000000` by `GameSession__Constructor`, then overwritten.
    pub screen_center_x: i32,
    /// 0x058: display center Y = `display_height / 2`
    pub screen_center_y: i32,
    /// 0x05C: "engine suspended" flag — set when the game enters a
    /// minimized / headless-like state (no rendering, no engine tick, no
    /// palette work). Initialised to 0 by the constructor; set to 1 by
    /// `GameSession::OnHeadlessPreLoop_Maybe` (headless startup +
    /// SYSCOMMAND minimize paths); cleared by the keyboard re-acquire path
    /// when the window is restored. Readers gate gameplay/render work:
    ///  - `GameSession::ProcessFrame` skips the engine frame advance
    ///  - `dispatch_frame` skips render work
    ///  - `GameSession::WindowProc` WM_PALETTECHANGED skips palette work
    ///  - `GameSession::Run` post-loop alert path checks it
    pub flag_5c: u32,
    /// 0x060: gate flag for engine frame advance in `GameSession__ProcessFrame`.
    /// Constructor initializes to 1.
    pub flag_60: u32,
    /// 0x064: frame state — set to -1 before engine tick, then 1 after.
    pub frame_state: i32,
    /// 0x068: minimize request — when nonzero, posts `WM_SYSCOMMAND SC_MINIMIZE`
    /// to `g_FrontendHwnd` and clears itself.
    pub minimize_request: u32,
    /// 0x06C: Cursor recenter request. Read by the engine WindowProc's
    /// WM_MOUSEMOVE handler — when nonzero, calls `SetCursorPos` to the
    /// screen center and clears the flag. Set to `1` by `Keyboard::AcquireInput`
    /// on focus regain so the very next mouse-move snaps the cursor back
    /// to center (preventing a spurious cursor jump on alt-tab).
    pub cursor_recenter_request: u32,
    /// 0x070: cursor position at session start (a Win32 `POINT`,
    /// populated in one shot by `GetCursorPos` in `GameSession::Run`).
    pub cursor_initial: POINT,
    /// 0x078: Mouse delta X accumulator (since last gameplay consumer poll).
    /// WM_MOUSEMOVE adds `(new_screen_x - cursor_x)` here. Zeroed by the
    /// constructor / `Run` startup / mouse release path. Consumed by
    /// `DDNetGameWrapper::Mouse__ConsumeDeltaAndButtons` (vtable slot 3,
    /// 0x0056D2E0), which copies the delta into a caller buffer without
    /// clearing — pair with `Mouse__ClearDeltas` (vtable slot 5, 0x0056D340)
    /// for read-then-zero. The wrapper bridges local mouse input into the
    /// per-frame network game state (`SendGameState`).
    pub mouse_delta_x: i32,
    /// 0x07C: Mouse delta Y accumulator (companion to `mouse_delta_x`,
    /// same producer/consumer pair).
    pub mouse_delta_y: i32,
    /// 0x080: Last cursor position (screen coords) seen by
    /// `GameSession::WindowProc`'s WM_MOUSEMOVE handler — used to compute
    /// `(new - prev)` deltas added to `mouse_delta_x/_y`. Initialised to
    /// `screen_center_x` by `GameEngine::InitHardware` (so the first move
    /// after startup measures from the cursor-recenter target), then
    /// overwritten on every WM_MOUSEMOVE.
    pub cursor_x: i32,
    /// 0x084: Last cursor Y (companion to `cursor_x`).
    pub cursor_y: i32,
    /// 0x088: Mouse button state bitmask. Bit 0 = LMB, bit 1 = RMB, bit 2 = MMB.
    /// WM_LBUTTONDOWN/RBUTTONDOWN/MBUTTONDOWN OR in the bit; corresponding
    /// UP messages clear it; WM_MOUSEMOVE *replaces* the low 3 bits from the
    /// `MK_LBUTTON|MK_RBUTTON|MK_MBUTTON` flags in wParam (so a fast
    /// click+release between two MOVE events can be lost). Cleared by the
    /// release-input + headless pre-loop paths.
    ///
    /// Consumed by `DDNetGameWrapper::Mouse__ConsumeDeltaAndButtons` (vtable
    /// slot 3, 0x0056D2E0) as a debounced click detector: the wrapper holds a
    /// per-button "armed" latch in its own `[this+4]` field, ANDs that latch
    /// with the current bitmask to report fresh clicks, then re-arms any bit
    /// that's currently unpressed. So a held button only registers once.
    pub mouse_button_state: u32,
    pub _unknown_08c: [u8; 4],
    /// 0x090: `QueryPerformanceFrequency` result (ticks per second),
    /// or `0` to request the synthetic-clock path (`GetTickCount`
    /// at 1 MHz scaled units). WA deliberately zeroes this in headless
    /// / deterministic-replay modes.
    pub timer_freq: u64,
    /// 0x098: QPC counter accumulator.
    pub timer_counter: u64,
    /// 0x0A0: `GameRuntime*` — the main game object wrapper (→ `GameWorld` at `+0x488`)
    pub game_runtime: *mut GameRuntime,
    /// 0x0A4: `Keyboard*` — 0x33C bytes, vtable `Keyboard_vtable` (0x66AEC8)
    pub keyboard: *mut Keyboard,
    /// 0x0A8: `DSSound*` — 0xBE0 bytes, vtable `DSSound_vtable` (0x66AF20)
    pub sound: *mut DSSound,
    /// 0x0AC: Polymorphic display — `DisplayGfx*` (normal) or `DisplayBase*` (headless).
    /// Stays `*mut u8` because the concrete type depends on mode.
    pub display: *mut u8,
    /// 0x0B0: `Palette*` — 0x28 bytes, vtable `Palette_vtable_Maybe`
    pub palette: *mut Palette,
    /// 0x0B4: Music object — 0x354 bytes (constructor 0x58BC10, vtable 0x66B3E0).
    /// Combines playlist controller + embedded streaming audio engine.
    pub streaming_audio: *mut Music,
    /// 0x0B8: input controller — 0x1800 bytes; null if `param_4 == 0` at init
    pub input_ctrl: *mut u8,
    /// 0x0BC: Localized-template resolver — 0x30 bytes
    /// (constructed by `LocalizedTemplate__Constructor` at 0x0053E950).
    /// Copied into [`GameWorld`](crate::engine::GameWorld)`+0x18` on world
    /// construction. See [`LocalizedTemplate`](crate::wa::localized_template::LocalizedTemplate).
    pub localized_template: *mut crate::wa::localized_template::LocalizedTemplate,
    /// 0x0C0: `DDNetGameWrapper*` — 0x2C bytes
    pub net_game: *mut u8,
    pub _unknown_0c4: [u8; 0x5C],
}

const _: () = assert!(core::mem::size_of::<GameSession>() == 0x120);

bind_GameSessionVtable!(GameSession, vtable);

// ─── Runtime accessors (DLL-injected context only) ──────────────────────

#[cfg(target_arch = "x86")]
use crate::address::va;
#[cfg(target_arch = "x86")]
use crate::engine::world::GameWorld;
#[cfg(target_arch = "x86")]
use crate::rebase::rb;

/// Get a pointer to the global `GameSession` struct from `G_GAME_SESSION`.
///
/// Can be null if called before the game session is initialized.
#[cfg(target_arch = "x86")]
#[inline]
pub unsafe fn get_game_session() -> *mut GameSession {
    unsafe { *(rb(va::G_GAME_SESSION) as *const *mut GameSession) }
}

/// Get the GameRuntime pointer from the global game session.
///
/// Returns null if the session or wrapper hasn't been initialized yet.
#[cfg(target_arch = "x86")]
#[inline]
pub unsafe fn get_runtime() -> *mut GameRuntime {
    unsafe {
        let session: *mut GameSession = get_game_session();
        if session.is_null() {
            return core::ptr::null_mut();
        }
        (*session).game_runtime
    }
}

/// Get the GameWorld pointer from the global game session.
///
/// Follows the chain: G_GAME_SESSION → GameSession.runtime → GameRuntime.world.
/// Returns null if any link in the chain is uninitialized.
#[cfg(target_arch = "x86")]
#[inline]
pub unsafe fn get_game_world() -> *mut GameWorld {
    unsafe {
        let runtime = get_runtime();
        if runtime.is_null() {
            return core::ptr::null_mut();
        }
        (*runtime).world
    }
}
