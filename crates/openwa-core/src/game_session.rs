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
#[repr(C)]
pub struct GameSession {
    /// 0x000: vtable pointer (class `GameSession`, `PTR_FUN_0066b3f8`)
    pub vtable: *mut u8,
    pub _unknown_004: [u8; 4],
    /// 0x008: main window HWND, stored from `hWnd` in `GameSession__Run`
    pub hwnd: u32,
    pub _unknown_00c: [u8; 0x20],
    /// 0x02C: set to 1 on init (role TBD)
    pub flag_2c: u32,
    pub _unknown_030: [u8; 8],
    /// 0x038: back-pointer to the `GameInfo` config struct (ESI from `GameSession__Run`)
    pub config_ptr: *mut u8,
    /// 0x03C: nonzero = exit the game main loop (checked each frame in `GameSession__Run`)
    pub exit_flag: u32,
    /// 0x040: game-end status/result code
    pub flag_40: u32,
    pub _unknown_044: [u8; 0x10],
    /// 0x054: display center X = `display_width / 2`.
    /// Initialized to `0x80000000` by `GameSession__Constructor`, then overwritten.
    pub screen_center_x: i32,
    /// 0x058: display center Y = `display_height / 2`
    pub screen_center_y: i32,
    pub _unknown_05c: [u8; 0x14],
    /// 0x070: cursor X at session start (from `GetCursorPos`)
    pub cursor_initial_x: i32,
    /// 0x074: cursor Y at session start
    pub cursor_initial_y: i32,
    pub _unknown_078: [u8; 8],
    /// 0x080: cursor center X — set to `screen_center_x`, used for `SetCursorPos`
    pub cursor_x: i32,
    /// 0x084: cursor center Y
    pub cursor_y: i32,
    pub _unknown_088: [u8; 8],
    /// 0x090: `QueryPerformanceFrequency` result low DWORD (0 if QPC unavailable)
    pub timer_freq_lo: u32,
    /// 0x094: high DWORD
    pub timer_freq_hi: u32,
    /// 0x098: QPC counter accumulator low DWORD
    pub timer_counter_lo: u32,
    /// 0x09C: high DWORD
    pub timer_counter_hi: u32,
    /// 0x0A0: `DDGameWrapper*` — the main game object wrapper (→ `DDGame` at `+0x488`)
    pub ddgame_wrapper: *mut u8,
    /// 0x0A4: `DDKeyboard*` — 0x33C bytes, vtable `DDKeyboard_vtable` (0x66AEC8)
    pub keyboard: *mut u8,
    /// 0x0A8: `DSSound*` — 0xBE0 bytes, vtable `DSSound_vtable` (0x66AF20)
    pub sound: *mut u8,
    /// 0x0AC: `DisplayGfx*` — 0x24E28 bytes (normal), or `GameStats*` in headless mode
    pub display_gfx: *mut u8,
    /// 0x0B0: `Palette*` — 0x28 bytes, vtable `Palette_vtable_Maybe`
    pub palette: *mut u8,
    /// 0x0B4: streaming audio object — 0x354 bytes (`FUN_0058bc10`)
    pub streaming_audio: *mut u8,
    /// 0x0B8: input controller — 0x1800 bytes; null if `param_4 == 0` at init
    pub input_ctrl: *mut u8,
    /// 0x0BC: timing object — 0x30 bytes (`FUN_0053e950`)
    pub timer_obj: *mut u8,
    /// 0x0C0: `DDNetGameWrapper*` — 0x2C bytes
    pub net_game: *mut u8,
    pub _unknown_0c4: [u8; 0x5C],
}

const _: () = assert!(core::mem::size_of::<GameSession>() == 0x120);
