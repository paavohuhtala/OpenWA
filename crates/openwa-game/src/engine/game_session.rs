use crate::FieldRegistry;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::engine::game_info::GameInfo;
use crate::engine::runtime::GameRuntime;
use crate::input::keyboard::DDKeyboard;
use crate::render::display::palette::Palette;

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
    /// 0x000: vtable pointer (class `GameSession`, `PTR_FUN_0066b3f8`)
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 4],
    /// 0x008: main window HWND, stored from `hWnd` in `GameSession__Run`
    pub hwnd: u32,
    /// 0x00C: first param from `GameSession__Run` (passed through from `Frontend__LaunchGameSession`)
    pub run_param_1: u32,
    pub _unknown_010: [u8; 0x14],
    /// 0x024: Always set to 1 by GameEngine__InitHardware.
    pub init_flag: u32,
    /// 0x028: Fullscreen flag — `(GameInfo.home_lock != 0) as u32`.
    pub fullscreen_flag: u32,
    /// 0x02C: set to 1 on init (role TBD)
    pub flag_2c: u32,
    /// 0x030: set to 1 when desktop is unavailable (`OpenInputDesktop` returns null)
    pub desktop_lost: u32,
    /// 0x034: checked in `GameSession__ProcessFrame` for non-engine frame path
    pub flag_34: u32,
    /// 0x038: back-pointer to the `GameInfo` config struct (ESI from `GameSession__Run`)
    pub config_ptr: *mut GameInfo,
    /// 0x03C: nonzero = exit the game main loop (checked each frame in `GameSession__Run`)
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
    /// 0x05C: checked in post-loop cleanup — nonzero triggers keyboard vtable call.
    /// Constructor initializes to 0; `GameSession__ProcessFrame` reads it.
    pub flag_5c: u32,
    /// 0x060: gate flag for engine frame advance in `GameSession__ProcessFrame`.
    /// Constructor initializes to 1.
    pub flag_60: u32,
    /// 0x064: frame state — set to -1 before engine tick, then 1 after.
    pub frame_state: i32,
    /// 0x068: minimize request — when nonzero, posts `WM_SYSCOMMAND SC_MINIMIZE`
    /// to `g_FrontendHwnd` and clears itself.
    pub minimize_request: u32,
    pub _unknown_06c: [u8; 4],
    /// 0x070: cursor X at session start (from `GetCursorPos`)
    pub cursor_initial_x: i32,
    /// 0x074: cursor Y at session start
    pub cursor_initial_y: i32,
    pub _unknown_078: [u8; 8],
    /// 0x080: cursor center X — set to `screen_center_x`, used for `SetCursorPos`
    pub cursor_x: i32,
    /// 0x084: cursor center Y
    pub cursor_y: i32,
    /// 0x088: cleared when desktop is lost and keyboard object exists.
    /// Used for input state tracking.
    pub input_active_flag: u32,
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
    /// 0x0A4: `DDKeyboard*` — 0x33C bytes, vtable `DDKeyboard_vtable` (0x66AEC8)
    pub keyboard: *mut DDKeyboard,
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
    /// 0x0BC: timing object — 0x30 bytes (`FUN_0053e950`)
    pub timer_obj: *mut u8,
    /// 0x0C0: `DDNetGameWrapper*` — 0x2C bytes
    pub net_game: *mut u8,
    pub _unknown_0c4: [u8; 0x5C],
}

const _: () = assert!(core::mem::size_of::<GameSession>() == 0x120);

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
