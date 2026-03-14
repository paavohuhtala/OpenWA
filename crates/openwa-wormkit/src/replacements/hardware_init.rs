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
//!   DDDisplay::Init retry loop (configured → 1024×768 → 800×600 → 640×480)
//!   screen center / cursor setup
//!   DDKeyboard (0x33C, inline) → session+0xA4
//!   Palette (0x28, inline) → session+0xB0
//!   DSSound (0xBE0, usercall ctor + DirectSoundCreate + coop level) → session+0xA8
//!   IF GameInfo.speech_enabled != 0 AND DSSound OK: streaming audio → session+0xB4
//!
//! ELSE (headless):
//!   GameStats (0x3560, stdcall ctor + vtable override) → session+0xAC
//!   session+0xA4/0xA8/0xB0/0xB4 = null
//!
//! ALWAYS:
//!   session+0x28 = (GameInfo.home_lock != 0) ? 1 : 0
//!   DDGameWrapper (0x6F10) → session+0xA0  [via game_session::construct_ddgame_wrapper]
//!   Palette vtable[4/3/2] calls + DDKeyboard poll (normal mode only)
//!   DDNetGameWrapper (0x2C, stdcall ctor) → session+0xC0
//! ```

use openwa_core::address::va;
use openwa_core::rebase::rb;
use openwa_core::game_info::GameInfo;
use openwa_core::game_session::GameSession;
use openwa_core::ddgame_wrapper::DDGameWrapper;
use openwa_core::ddkeyboard::DDKeyboard;
use openwa_core::dddisplay::DDDisplay;
use openwa_core::dssound::DSSound;
use openwa_core::palette::{Palette, PaletteVtable};
use openwa_core::input_ctrl::{InputCtrl, InputCtrlVtable};
use openwa_core::game_timer::GameTimer;
use openwa_core::game_stats::GameStats;
use openwa_core::display_gfx::DisplayGfx;
use openwa_core::streaming_audio::StreamingAudio;
use openwa_core::ddnetgame_wrapper::DDNetGameWrapper;
use openwa_core::wa_alloc::WABox;
use crate::hook;
use crate::log_line;
use super::game_session;

// ─── Entry trampoline state ───────────────────────────────────────────────────

/// Saved return address for the thiscall→cdecl trampoline.
static mut SAVED_RET: u32 = 0;

// ─── Bridge-state statics ─────────────────────────────────────────────────────

/// Function addresses, set in `install()`.
static mut TIMER_CTOR_ADDR: u32 = 0;
static mut DSSOUND_CTOR_ADDR: u32 = 0;
static mut DSSOUND_INIT_BUF_ADDR: u32 = 0;
static mut INPUT_CTRL_INIT_ADDR: u32 = 0;
static mut STREAM_CTOR_ADDR: u32 = 0;
static mut DDISPLAY_INIT_ADDR: u32 = 0;

/// Height passed in ECX to `call_ddisplay_init` — set before each call.
static mut DDISPLAY_INIT_ECX: u32 = 0;

/// Implicit ESI for `call_input_ctrl_init` (set by `impl_init_hardware`).
static mut INPUT_CTRL_ESI: u32 = 0;
/// Saved ESI across the `call_input_ctrl_init` call.
static mut INPUT_CTRL_SAVED_ESI: u32 = 0;

/// Implicit EAX for `call_dssound_init_buffers` (set by `impl_init_hardware`).
static mut DSSOUND_INIT_EAX: u32 = 0;

// ─── Bridges ─────────────────────────────────────────────────────────────────
//
// All bridges use the "pop ECX (save bridge_ret) / call callee / push ECX" idiom
// so the callee sees its actual stack args at [esp+4] / [esp+8] etc. (not
// displaced by an extra return address).

/// Timer constructor: `usercall(ESI=timer_ptr, EAX=crosshair_threshold)`, plain RET.
/// Returns whatever EAX holds after the call.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_timer_ctor(_timer_ptr: *mut GameTimer, _crosshair_threshold: u32) -> u32 {
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

/// DSSound constructor: `usercall(EAX=this)`, plain RET. Void.
///
/// IMPORTANT: FUN_00573D50 clobbers ECX (MOV ECX, 0x1F4 for REP STOSD).
/// Do NOT use ECX to save bridge_ret — read it from the stack instead.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_dssound_ctor(_dssound: *mut DSSound) {
    core::arch::naked_asm!(
        // [esp+0]=bridge_ret, [esp+4]=dssound
        "movl 4(%esp), %eax",    // EAX = dssound (bridge_ret stays on stack untouched)
        "calll *({fn})",          // FUN_00573D50: plain RET; calll pushes continuation here
        // After FUN_00573D50 RET: stack = [bridge_ret, dssound]
        "retl",                  // pop bridge_ret, return; caller cleans dssound
        fn = sym DSSOUND_CTOR_ADDR,
        options(att_syntax),
    );
}

/// Saved bridge_ret for `call_dssound_init_buffers` (can't use ECX — callee clobbers it).
static mut DSSOUND_INIT_SAVED_RET: u32 = 0;

/// FUN_00573E50: `usercall(EAX=dssound)` + `__stdcall(out_0x10, out_0x0C)`, `RET 0x8`.
/// Declared as `extern "stdcall"` so Rust caller does NOT emit `add esp, 8` after the call
/// (callee already cleans via RET 0x8).
/// Caller sets `DSSOUND_INIT_EAX = dssound` before calling.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_dssound_init_buffers(
    _out_primary_buffer: *mut *mut u8,
    _out_primary_caps: *mut u32,
) -> u32 {
    core::arch::naked_asm!(
        // Entry: ESP=E, [E+0]=bridge_ret, [E+4]=out_primary_buffer, [E+8]=out_primary_caps
        "movl {eax_val}, %eax",      // EAX = dssound
        "movl (%esp), %ecx",         // ECX = bridge_ret (temp)
        "movl %ecx, {saved_ret}",    // save to static (ECX clobbered by FUN_00573E50)
        "movl 4(%esp), %ecx",        // ECX = out_primary_buffer
        "movl 8(%esp), %edx",        // EDX = out_primary_caps
        "addl $0xC, %esp",           // ESP = E+12 (discard bridge_ret + 2 args)
        "pushl %edx",                // ESP = E+8,  [E+8]  = out_primary_caps
        "pushl %ecx",                // ESP = E+4,  [E+4]  = out_primary_buffer
        "calll *({fn})",             // ESP = E,    [E]    = cont
                                     // FUN_00573E50 RET 0x8: pops cont → E+4, +8 → E+12
        // At cont: ESP = E+12; EAX = return value
        "pushl {saved_ret}",         // ESP = E+8, [E+8] = bridge_ret
        "retl",                      // pops bridge_ret → ESP = E+12
                                     // stdcall: caller skips cleanup. ESP = E+12 = E+4+8 ✓
        eax_val = sym DSSOUND_INIT_EAX,
        fn = sym DSSOUND_INIT_BUF_ADDR,
        saved_ret = sym DSSOUND_INIT_SAVED_RET,
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

/// DDDisplay::Init — usercall(ECX=height) + stdcall(display_gfx, hwnd, width, flags), RET 0x10.
/// Tail-jump: callee's RET 0x10 cleans the 4 stack args and returns to our caller.
/// Caller must set `DDISPLAY_INIT_ECX = height` before calling.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_ddisplay_init(
    _display_gfx: *mut u8,
    _hwnd: u32,
    _width: u32,
    _flags: u32,
) -> u32 {
    core::arch::naked_asm!(
        // Stack: [ret, display_gfx, hwnd, width, flags]  ECX = whatever
        "movl {ecx_val}, %ecx",  // ECX = height
        "jmpl *({fn})",          // tail-jump; callee RET 0x10 cleans 4 args
        ecx_val = sym DDISPLAY_INIT_ECX,
        fn = sym DDISPLAY_INIT_ADDR,
        options(att_syntax),
    );
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
    let crosshair_threshold = gi.crosshair_overflow_threshold as u32;

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
        let displaygfx_ctor: unsafe extern "stdcall" fn(*mut DisplayGfx) -> *mut DisplayGfx =
            core::mem::transmute(rb(va::DISPLAYGFX_CTOR) as usize);
        let display_gfx = displaygfx_ctor(WABox::<DisplayGfx>::alloc(0x24E28, 0x24E08).leak());
        (*session).display_gfx = display_gfx as *mut u8;

        // ── DDDisplay::Init retry loop ────────────────────────────────────────
        let flags = gi.display_flags;
        let w0 = gi.display_width;
        let h0 = gi.display_height;

        DDISPLAY_INIT_ECX = h0;
        let mut init_ok = call_ddisplay_init(display_gfx as *mut u8, hwnd, w0, flags) != 0;

        if !init_ok {
            let fallbacks: [(u32, u32); 3] = [
                (0x400, 0x300), // 1024×768
                (0x320, 0x258), // 800×600
                (0x280, 0x1E0), // 640×480
            ];
            for &(w, h) in &fallbacks {
                gi.display_width = w;
                gi.display_height = h;
                DDISPLAY_INIT_ECX = h;
                if call_ddisplay_init(display_gfx as *mut u8, hwnd, w, flags) != 0 {
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
                use windows_sys::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClientRect};
                use windows_sys::Win32::Foundation::{HWND, RECT};
                let hwnd_val: HWND = *(rb(va::G_FRONTEND_HWND) as *const HWND);
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd_val, &mut rect);
                let map_fn_ptr = *(rb(va::IAT_MAP_WINDOW_POINTS) as *const usize);
                let map_fn: unsafe extern "stdcall" fn(HWND, HWND, *mut RECT, u32) -> i32 =
                    core::mem::transmute(map_fn_ptr);
                map_fn(hwnd_val, core::ptr::null_mut(), &mut rect, 2);
                ClipCursor(&rect);
            }
        }

        // ── DDKeyboard (inline construction) ──────────────────────────────────
        // WABox zeroes the first 0x31C bytes, covering key_state (+0x11C) and
        // prev_state (+0x21C) — no separate zeroing needed.
        let kb = WABox::<DDKeyboard>::alloc(0x33C, 0x31C).leak();
        (*kb).vtable = rb(va::DDKEYBOARD_VTABLE) as *mut u8;
        (*kb).game_info_input_ptr = &raw mut gi.input_state_f918 as u32;
        (*kb)._field_008 = 1;
        (*kb)._field_014 = 0;
        (*kb)._field_018 = 0;
        (*session).keyboard = kb;

        // ── Palette (inline construction) ─────────────────────────────────────
        let pal = WABox::<Palette>::alloc(0x28, 0).leak();
        (*pal).vtable = rb(va::PALETTE_VTABLE_MAYBE) as *const PaletteVtable;
        (*pal)._field_004 = 0xFFFF_FFFF;
        (*session).palette = pal;

        // ── DSSound ───────────────────────────────────────────────────────────
        (*session).sound = core::ptr::null_mut();
        {
            let snd = WABox::<DSSound>::alloc(0xBE0, 0xBC0).leak();
            call_dssound_ctor(snd);
            (*snd).hwnd = hwnd;
            (*session).sound = snd;

            let ds_create: unsafe extern "stdcall" fn(*const u8, *mut *mut u8, *const u8) -> i32 =
                core::mem::transmute(rb(va::DIRECTSOUND_CREATE) as usize);
            let hr = ds_create(
                core::ptr::null(),
                &mut (*snd).direct_sound,
                core::ptr::null(),
            );

            if hr == 0 {
                DSSOUND_INIT_EAX = snd as u32;
                let hr2 = call_dssound_init_buffers(
                    &mut (*snd).primary_buffer,
                    &mut (*snd).primary_buffer_caps,
                );
                if hr2 == 0 {
                    // IDirectSoundBuffer::Play(this, 0, 0, DSBPLAY_LOOPING=1)
                    const PLAY_VSLOT: usize = 0x30 / 4; // IDirectSoundBuffer vtable slot 12
                    let pds = (*snd).primary_buffer as *const *const usize;
                    let vtbl = *pds;
                    let play: unsafe extern "stdcall" fn(
                        *const *const usize, u32, u32, u32,
                    ) -> i32 = core::mem::transmute(*vtbl.add(PLAY_VSLOT));
                    let hr3 = play(pds, 0, 0, 1);
                    if hr3 == 0 {
                        (*snd).init_success = 1;
                    }
                }
            }
        }

        // ── Streaming audio ───────────────────────────────────────────────────
        (*session).streaming_audio = core::ptr::null_mut();
        if !(*session).sound.is_null() && gi.speech_enabled != 0 {
            let stream = WABox::<StreamingAudio>::alloc(0x354, 0x334).leak();
            let ids = (*(*session).sound).direct_sound;
            call_streaming_audio_ctor(
                stream as *mut u8,
                ids,
                gi.streaming_audio_config.as_mut_ptr(),
            );
            (*session).streaming_audio = stream as *mut u8;
        }
    } else {
        // ── Headless / stats mode ─────────────────────────────────────────────
        let stats = WABox::<GameStats>::alloc(0x3560, 0x3560).leak();
        let gamestats_ctor: unsafe extern "stdcall" fn(*mut GameStats) -> *mut GameStats =
            core::mem::transmute(rb(va::GAMESTATS_CTOR) as usize);
        gamestats_ctor(stats);
        (*stats).vtable = rb(va::GAMESTATS_VTABLE) as *mut u8;
        (*session).display_gfx      = stats as *mut u8;
        (*session).keyboard         = core::ptr::null_mut();
        (*session).sound            = core::ptr::null_mut();
        (*session).palette          = core::ptr::null_mut();
        (*session).streaming_audio  = core::ptr::null_mut();
    }

    // ── Session flags ─────────────────────────────────────────────────────────
    (*session).init_flag = 1;
    (*session).fullscreen_flag = (gi.home_lock != 0) as u32;

    // ── DDGameWrapper (ALWAYS) ────────────────────────────────────────────────
    let wrapper = game_session::construct_ddgame_wrapper(
        game_info as *mut u8,
        WABox::<DDGameWrapper>::alloc(0x6F10, 0x6EF0).leak(),
        (*session).display_gfx as *mut DDDisplay,
        (*session).sound,
        (*session).keyboard as *mut u8,
        (*session).palette,
        (*session).streaming_audio,
        (*session).input_ctrl,
    );
    (*session).ddgame_wrapper = wrapper;

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
            let kb_poll: unsafe extern "stdcall" fn(*mut DDKeyboard) =
                core::mem::transmute(rb(va::DDKEYBOARD_POLL_KEYBOARD_STATE) as usize);
            kb_poll(kb);
        }
    }

    // ── DDNetGameWrapper (ALWAYS) ─────────────────────────────────────────────
    let net_ctor: unsafe extern "stdcall" fn(*mut DDNetGameWrapper) -> *mut DDNetGameWrapper =
        core::mem::transmute(rb(va::DDNETGAME_WRAPPER_CTOR) as usize);
    (*session).net_game = net_ctor(WABox::<DDNetGameWrapper>::alloc(0x2C, 0).leak()) as *mut u8;

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
        TIMER_CTOR_ADDR       = rb(va::GAME_ENGINE_TIMER_CTOR);
        DSSOUND_CTOR_ADDR     = rb(va::CONSTRUCT_DS_SOUND);
        DSSOUND_INIT_BUF_ADDR = rb(va::DSSOUND_INIT_BUFFERS);
        INPUT_CTRL_INIT_ADDR  = rb(va::INPUT_CTRL_INIT);
        STREAM_CTOR_ADDR      = rb(va::STREAMING_AUDIO_CTOR);
        DDISPLAY_INIT_ADDR    = rb(va::DDISPLAY_INIT);

        // Full replacement — trampoline not needed.
        let _ = hook::install(
            "GameEngine__InitHardware",
            va::GAME_ENGINE_INIT_HARDWARE,
            hook_init_hardware as *const (),
        )?;
    }
    Ok(())
}
