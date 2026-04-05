//! Full Rust replacement for `GameEngine__InitHardware` (0x56D350).
//!
//! Orchestrates creation of all game hardware subsystems and stores their
//! pointers into `*G_GAME_SESSION` (`GameSession`).
//!
//! ## Calling convention
//!
//! `__thiscall`: ECX = `*mut GameInfo` (≥0xF91C bytes), 3 stack params
//! (hwnd, param3, param4), `RET 0xC`. Returns 1 on success, 0 on failure.
//!
//! The naked entry trampoline captures ECX, pops the return address → `SAVED_RET`,
//! pushes ECX as the first cdecl arg, calls `impl_init_hardware`, cleans 4 × u32,
//! and jumps to the saved return address.
//!
//! ## Initialization order
//!
//! ```text
//! ALWAYS:
//!   timer (0x30 bytes, FUN_0053E950 usercall ESI=this EAX=d778_val) → session+0xBC
//!
//! IF param4 != 0:
//!   input ctrl (0x1800 bytes, inline vtable, FUN_0058C0D0 usercall ESI=this) → session+0xB8
//!
//! IF GameInfo.headless_mode == 0 (normal mode):
//!   DisplayGfx (0x24E28, stdcall ctor) → session+0xAC
//!   DisplayGfx::Init retry loop (configured → 1024×768 → 800×600 → 640×480)
//!   screen center / cursor setup
//!   DDKeyboard (0x33C, inline) → session+0xA4
//!   Palette (0x28, inline) → session+0xB0
//!   DSSound (0xBE0, usercall ctor + DirectSoundCreate + coop level) → session+0xA8
//!   IF GameInfo.speech_enabled != 0 AND DSSound OK: streaming audio → session+0xB4
//!
//! ELSE (headless):
//!   DisplayBase (0x3560, stdcall ctor + vtable override) → session+0xAC
//!   session+0xA4/0xA8/0xB0/0xB4 = null
//!
//! ALWAYS:
//!   session+0x28 = (GameInfo.home_lock != 0) ? 1 : 0
//!   DDGameWrapper (0x6F10) → session+0xA0  [via game_session::construct_ddgame_wrapper]
//!   Palette vtable[4/3/2] calls + DDKeyboard poll (normal mode only)
//!   DDNetGameWrapper (0x2C, stdcall ctor) → session+0xC0
//! ```

use super::game_session;
use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::audio::{DSSound, Music};
use openwa_core::display::{DisplayBase, DisplayGfx, Palette};
use openwa_core::engine::{DDGameWrapper, DDNetGameWrapper, GameInfo, GameSession, GameTimer};
use openwa_core::input::{DDKeyboard, InputCtrl, InputCtrlVtable};
use openwa_core::rebase::rb;
use openwa_core::wa_alloc::WABox;

// ─── Entry trampoline state ───────────────────────────────────────────────────

/// Saved return address for the thiscall→cdecl trampoline.
static mut SAVED_RET: u32 = 0;

// ─── Bridge-state statics ─────────────────────────────────────────────────────

/// Function addresses, set in `install()`.
static mut TIMER_CTOR_ADDR: u32 = 0;
static mut INPUT_CTRL_INIT_ADDR: u32 = 0;
static mut STREAM_CTOR_ADDR: u32 = 0;
static mut DISPLAY_GFX_INIT_ADDR: u32 = 0;

/// Height passed in ECX to `call_display_gfx_init` — set before each call.
static mut DISPLAY_GFX_INIT_ECX: u32 = 0;

/// Implicit ESI for `call_input_ctrl_init` (set by `impl_init_hardware`).
static mut INPUT_CTRL_ESI: u32 = 0;
/// Saved ESI across the `call_input_ctrl_init` call.
static mut INPUT_CTRL_SAVED_ESI: u32 = 0;

// ─── Bridges ─────────────────────────────────────────────────────────────────
//
// All bridges use the "pop ECX (save bridge_ret) / call callee / push ECX" idiom
// so the callee sees its actual stack args at [esp+4] / [esp+8] etc. (not
// displaced by an extra return address).

/// Timer constructor: `usercall(ESI=timer_ptr, EAX=crosshair_threshold)`, plain RET.
/// Returns whatever EAX holds after the call.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_timer_ctor(
    _timer_ptr: *mut GameTimer,
    _crosshair_threshold: u32,
) -> u32 {
    core::arch::naked_asm!(
        // [esp+0]=bridge_ret, [esp+4]=timer_ptr, [esp+8]=crosshair_threshold
        "pushl %esi",
        // [esp+0]=old_esi, [esp+4]=bridge_ret, [esp+8]=timer_ptr, [esp+c]=d778_val
        "movl 8(%esp), %esi",    // ESI = timer_ptr
        "movl 0xc(%esp), %eax",  // EAX = crosshair_threshold
        "calll *({fn})",          // FUN_0053E950: plain RET (no stack args)
        "popl %esi",
        "retl",                   // cdecl; caller cleans 2 × u32
        fn = sym TIMER_CTOR_ADDR,
        options(att_syntax),
    );
}

/// FUN_0058C0D0: `usercall(ESI=input_ctrl)` + stdcall(4 params), RET 0x10.
/// Caller sets `INPUT_CTRL_ESI = input_ctrl` before calling.
/// FUN_0058C0D0's `RET 0x10` cleans all 4 args, so `retl` here needs no cleanup.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_input_ctrl_init(
    _game_info_p4: *mut u8,
    _param3: u32,
    _param4: u32,
    _crosshair_threshold: u32,
) -> u32 {
    core::arch::naked_asm!(
        // [esp+0]=bridge_ret, [esp+4]=gip4, [esp+8]=hwnd, [esp+c]=p3, [esp+10]=d778
        "movl %esi, {saved_esi}",
        "movl {esi_val}, %esi",
        "popl %ecx",                   // ECX = bridge_ret; stack: [esp+0]=gip4, ...
        "calll *({fn})",               // RET 0x10 cleans 4 args, returns to `pushl %ecx`
        "pushl %ecx",
        "movl {saved_esi}, %esi",
        "retl",                         // stdcall: args already cleaned by RET 0x10
        saved_esi = sym INPUT_CTRL_SAVED_ESI,
        esi_val = sym INPUT_CTRL_ESI,
        fn = sym INPUT_CTRL_INIT_ADDR,
        options(att_syntax),
    );
}

/// Saved bridge_ret for `call_streaming_audio_ctor`.
static mut STREAM_CTOR_SAVED_RET: u32 = 0;
/// Saved ESI across the `call_streaming_audio_ctor` call.
static mut STREAM_CTOR_SAVED_ESI: u32 = 0;

/// FUN_0058BC10: `usercall(ESI=this)` + 2 stack(param_1, param_2) + `RET 0x8`, void.
/// Caller sets ESI = stream pointer; callee cleans the 2 stack params.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_streaming_audio_ctor(
    _stream: *mut u8,
    _ids: *mut u8,
    _path: *mut u8,
) {
    core::arch::naked_asm!(
        // Entry: ESP=E, [E+0]=bridge_ret, [E+4]=stream, [E+8]=ids, [E+C]=path
        "movl (%esp), %ecx",
        "movl %ecx, {saved_ret}",    // save bridge_ret to static (ECX is scratch)
        "movl %esi, {saved_esi}",    // save caller's ESI
        "movl 4(%esp), %esi",        // ESI = stream (this)
        "movl 8(%esp), %ecx",        // ECX = ids
        "movl 0xc(%esp), %edx",      // EDX = path
        // Discard our 4 cdecl slots, then push the 2 callee args + let calll push cont
        "addl $0x10, %esp",          // ESP = E+0x10
        "pushl %edx",                // [E+0xC] = path,  ESP = E+0xC
        "pushl %ecx",                // [E+0x8] = ids,   ESP = E+0x8
        "calll *({fn})",             // [E+0x4] = cont,  ESP = E+0x4; calls FUN_0058BC10
        // FUN_0058BC10 RET 0x8 → cont:  ESP = E+0x10
        "movl {saved_esi}, %esi",    // restore ESI
        "subl $0xc, %esp",           // ESP = E+0x4
        "pushl {saved_ret}",         // ESP = E+0x0
        "retl",                      // ESP = E+0x4 ✓  (caller cleans 3 × u32)
        fn = sym STREAM_CTOR_ADDR,
        saved_ret = sym STREAM_CTOR_SAVED_RET,
        saved_esi = sym STREAM_CTOR_SAVED_ESI,
        options(att_syntax),
    );
}

/// DisplayGfx::Init — usercall(ECX=height) + stdcall(display_gfx, hwnd, width, flags), RET 0x10.
/// Tail-jump: callee's RET 0x10 cleans the 4 stack args and returns to our caller.
/// Caller must set `DISPLAY_GFX_INIT_ECX = height` before calling.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_display_gfx_init(
    _display_gfx: *mut u8,
    _hwnd: u32,
    _width: u32,
    _flags: u32,
) -> u32 {
    core::arch::naked_asm!(
        // Stack: [ret, display_gfx, hwnd, width, flags]  ECX = whatever
        "movl {ecx_val}, %ecx",  // ECX = height
        "jmpl *({fn})",          // tail-jump; callee RET 0x10 cleans 4 args
        ecx_val = sym DISPLAY_GFX_INIT_ECX,
        fn = sym DISPLAY_GFX_INIT_ADDR,
        options(att_syntax),
    );
}

// ─── Subsystem creation ───────────────────────────────────────────────────────

/// Allocate and initialize a DSSound object with DirectSound COM setup.
///
/// Creates the DSSound, calls DirectSoundCreate, initializes primary buffer,
/// and starts looping playback. Sets `init_success` if all COM steps succeed.
/// The returned pointer is always valid (sound may be partially initialized
/// if COM steps fail, matching original WA behavior).
unsafe fn create_dssound(hwnd: u32) -> *mut DSSound {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Media::Audio::DirectSound::{
        DirectSoundCreate, IDirectSound, IDirectSoundBuffer, DSBCAPS_PRIMARYBUFFER,
        DSBPLAY_LOOPING, DSBUFFERDESC, DSSCL_PRIORITY,
    };

    // Pure Rust construction — replaces call_dssound_ctor bridge.
    let snd = WABox::<DSSound>::from_value(DSSound::new(hwnd)).leak();

    // DirectSound COM initialization — replaces call_dssound_init_buffers bridge.
    let mut ds: Option<IDirectSound> = None;
    if DirectSoundCreate(None, &mut ds, None).is_ok() {
        let ds = ds.unwrap();

        // SetCooperativeLevel(hwnd, DSSCL_PRIORITY)
        let _ = ds.SetCooperativeLevel(HWND(hwnd as _), DSSCL_PRIORITY);

        // CreateSoundBuffer with DSBCAPS_PRIMARYBUFFER (no format — primary buffer)
        let desc = DSBUFFERDESC {
            dwSize: core::mem::size_of::<DSBUFFERDESC>() as u32,
            dwFlags: DSBCAPS_PRIMARYBUFFER,
            ..core::mem::zeroed()
        };
        let mut primary: Option<IDirectSoundBuffer> = None;
        if ds.CreateSoundBuffer(&desc, &mut primary, None).is_ok() {
            let primary = primary.unwrap();

            // GetCaps to populate primary_buffer_caps
            let mut caps =
                core::mem::zeroed::<windows::Win32::Media::Audio::DirectSound::DSBCAPS>();
            caps.dwSize = core::mem::size_of_val(&caps) as u32;
            let _ = primary.GetCaps(&mut caps);
            (*snd).primary_buffer_caps = caps.dwBufferBytes;

            // Start primary buffer looping
            if primary.Play(0, 0, DSBPLAY_LOOPING).is_ok() {
                (*snd).init_success = 1;
            }

            // Store COM pointers as raw u32 (WA owns the references)
            (*snd).primary_buffer = core::mem::transmute_copy(&primary);
            core::mem::forget(primary);
        }

        (*snd).direct_sound = core::mem::transmute_copy(&ds);
        core::mem::forget(ds);
    }

    snd
}

// ─── Implementation ───────────────────────────────────────────────────────────

unsafe extern "cdecl" fn impl_init_hardware(
    game_info: *mut GameInfo,
    hwnd: u32,
    param3: u32,
    param4: u32,
) -> u32 {
    let _ = log_line("[hardware_init] GameEngine::InitHardware");
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    let gi = &mut *game_info;
    let crosshair_threshold = gi.game_version as u32;

    // ── Input controller (if param4 != 0) ────────────────────────────────────
    if param4 == 0 {
        (*session).input_ctrl = core::ptr::null_mut();
    } else {
        let ctrl = WABox::<InputCtrl>::alloc(0x1800, 0x17E0).leak();
        (*ctrl)._field_d74 = 0x3F9;
        (*ctrl).vtable = rb(va::INPUT_CTRL_VTABLE) as *const InputCtrlVtable;
        (*session).input_ctrl = ctrl as *mut u8;

        // Original passes GameInfo+4 (skips first DWORD of unknown padding).
        let game_info_plus_4 = (game_info as *mut u8).add(4);
        INPUT_CTRL_ESI = ctrl as u32;
        let ok = call_input_ctrl_init(game_info_plus_4, param3, param4, crosshair_threshold);
        if ok == 0 {
            (*ctrl).destroy(1);
            (*session).input_ctrl = core::ptr::null_mut();
            return 0;
        }
    }

    // ── Timer object (ALWAYS) ─────────────────────────────────────────────────
    let timer = WABox::<GameTimer>::alloc(0x30, 0x30).leak();
    call_timer_ctor(timer, crosshair_threshold);
    (*session).timer_obj = timer as *mut u8;

    let headless = gi.headless_mode != 0;

    if !headless {
        // ── DisplayGfx ───────────────────────────────────────────────────────
        let display_gfx = DisplayGfx::construct();
        (*session).display = display_gfx as *mut u8;

        // ── DisplayGfx::Init retry loop ────────────────────────────────────────
        let flags = gi.display_flags;
        let w0 = gi.display_width;
        let h0 = gi.display_height;

        DISPLAY_GFX_INIT_ECX = h0;
        let mut init_ok = call_display_gfx_init(display_gfx as *mut u8, hwnd, w0, flags) != 0;

        if !init_ok {
            let fallbacks: [(u32, u32); 3] = [
                (0x400, 0x300), // 1024×768
                (0x320, 0x258), // 800×600
                (0x280, 0x1E0), // 640×480
            ];
            for &(w, h) in &fallbacks {
                gi.display_width = w;
                gi.display_height = h;
                DISPLAY_GFX_INIT_ECX = h;
                if call_display_gfx_init(display_gfx as *mut u8, hwnd, w, flags) != 0 {
                    init_ok = true;
                    break;
                }
            }
        }

        if !init_ok {
            return 0;
        }

        // ── Screen center and cursor ──────────────────────────────────────────
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SetCursorPos, SM_CXSCREEN, SM_CYSCREEN,
        };

        let fullscreen = *(rb(va::G_FULLSCREEN_FLAG) as *const u32) != 0;
        let (cx, cy): (i32, i32) = if fullscreen {
            let w = GetSystemMetrics(SM_CXSCREEN);
            let h = GetSystemMetrics(SM_CYSCREEN);
            (w / 2, h / 2)
        } else {
            (gi.display_width as i32 / 2, gi.display_height as i32 / 2)
        };

        (*session).screen_center_x = cx;
        (*session).screen_center_y = cy;
        (*session).cursor_x = cx;
        (*session).cursor_y = cy;

        let suppress = *(rb(va::G_SUPPRESS_CURSOR) as *const u8);
        if suppress == 0 {
            SetCursorPos(cx, cy);
            if fullscreen {
                use windows_sys::Win32::Foundation::{HWND, RECT};
                use windows_sys::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClientRect};
                let hwnd_val: HWND = *(rb(va::G_FRONTEND_HWND) as *const HWND);
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                GetClientRect(hwnd_val, &mut rect);
                let map_fn_ptr = *(rb(va::IAT_MAP_WINDOW_POINTS) as *const usize);
                let map_fn: unsafe extern "stdcall" fn(HWND, HWND, *mut RECT, u32) -> i32 =
                    core::mem::transmute(map_fn_ptr);
                map_fn(hwnd_val, core::ptr::null_mut(), &mut rect, 2);
                ClipCursor(&rect);
            }
        }

        // ── DDKeyboard (inline construction) ──────────────────────────────────
        let kb = WABox::from_value(DDKeyboard::new(
            rb(va::DDKEYBOARD_VTABLE),
            &raw mut gi.input_state_f918 as u32,
        ))
        .leak();
        (*session).keyboard = kb;

        // ── Palette (inline construction) ─────────────────────────────────────
        let pal = WABox::from_value(Palette::new(rb(va::PALETTE_VTABLE))).leak();
        (*session).palette = pal;

        // ── DSSound ───────────────────────────────────────────────────────────
        (*session).sound = create_dssound(hwnd);

        // ── Streaming audio ───────────────────────────────────────────────────
        (*session).streaming_audio = core::ptr::null_mut();
        if !(*session).sound.is_null() && gi.speech_enabled != 0 {
            let stream = WABox::<Music>::alloc(0x354, 0x334).leak();
            let ids = (*(*session).sound).direct_sound as *mut u8;
            call_streaming_audio_ctor(
                stream as *mut u8,
                ids,
                gi.streaming_audio_config.as_mut_ptr(),
            );
            (*session).streaming_audio = stream;
        }
    } else {
        // ── Headless / stats mode ─────────────────────────────────────────────
        (*session).display = DisplayBase::new_headless() as *mut u8;
        (*session).keyboard = core::ptr::null_mut();
        (*session).sound = core::ptr::null_mut();
        (*session).palette = core::ptr::null_mut();
        (*session).streaming_audio = core::ptr::null_mut();
    }

    // ── Session flags ─────────────────────────────────────────────────────────
    (*session).init_flag = 1;
    (*session).fullscreen_flag = (gi.home_lock != 0) as u32;

    // ── DDGameWrapper (ALWAYS) ────────────────────────────────────────────────
    let _ = crate::log_line("[hardware_init] Creating DDGameWrapper");
    let wrapper = game_session::construct_ddgame_wrapper(
        game_info,
        WABox::<DDGameWrapper>::alloc(0x6F10, 0x6EF0).leak(),
        (*session).display as *mut DisplayGfx,
        (*session).sound,
        (*session).keyboard as *mut u8,
        (*session).palette,
        (*session).streaming_audio as *mut u8,
        (*session).input_ctrl,
    );
    (*session).ddgame_wrapper = wrapper;
    let _ = crate::log_line("[hardware_init] DDGameWrapper created OK");

    // ── Palette vtable[4/3/2] + keyboard poll (normal mode only) ─────────────
    if !headless {
        let pal = (*session).palette;
        if !pal.is_null() {
            (*pal).reset();
            (*pal).init();
            (*pal).set_mode(7);
        }

        let kb = (*session).keyboard;
        if !kb.is_null() {
            (*kb).poll();
        }
    }

    // ── DDNetGameWrapper (ALWAYS) ─────────────────────────────────────────────
    (*session).net_game = DDNetGameWrapper::construct() as *mut u8;

    let _ = log_line("[hardware_init] GameEngine::InitHardware done");
    1
}

// ─── Naked entry trampoline ───────────────────────────────────────────────────
//
// Stack on entry (thiscall 3 params):
//   [esp+0x00] = caller_ret
//   [esp+0x04] = hwnd     (param_2)
//   [esp+0x08] = param3   (param_3)
//   [esp+0x0C] = param4   (param_4)
//   ECX        = game_info (thiscall this, implicit)
//
// Steps:
//   1. Pop caller_ret → SAVED_RET.
//   2. Push ECX so stack = [game_info, hwnd, param3, param4].
//   3. Call impl_init_hardware (cdecl, 4 args).
//   4. ADD ESP, 0x10 — clean 4 × u32.
//   5. JMP *SAVED_RET — return to caller; EAX = 1 or 0.
#[unsafe(naked)]
unsafe extern "C" fn hook_init_hardware() {
    core::arch::naked_asm!(
        "popl %eax",              // EAX = caller_ret
        "movl %eax, {saved_ret}",
        "pushl %ecx",             // push game_info; stack: [game_info, hwnd, param3, param4]
        "calll {impl_fn}",
        "addl $0x10, %esp",       // clean 4 × u32
        "jmpl *{saved_ret}",
        saved_ret = sym SAVED_RET,
        impl_fn   = sym impl_init_hardware,
        options(att_syntax),
    );
}

pub fn install() -> Result<(), String> {
    unsafe {
        TIMER_CTOR_ADDR = rb(va::GAME_ENGINE_TIMER_CTOR);
        INPUT_CTRL_INIT_ADDR = rb(va::INPUT_CTRL_INIT);
        STREAM_CTOR_ADDR = rb(va::STREAMING_AUDIO_CTOR);
        DISPLAY_GFX_INIT_ADDR = rb(va::DISPLAY_GFX_INIT);

        // Full replacement — trampoline not needed.
        let _ = hook::install(
            "GameEngine__InitHardware",
            va::GAME_ENGINE_INIT_HARDWARE,
            hook_init_hardware as *const (),
        )?;

        // Trap functions whose only caller was GameEngine__InitHardware (now Rust).
        hook::install_trap!("DSSound__Constructor", va::CONSTRUCT_DS_SOUND);
        hook::install_trap!("DSSOUND_INIT_BUFFERS", va::DSSOUND_INIT_BUFFERS);
    }
    Ok(())
}
