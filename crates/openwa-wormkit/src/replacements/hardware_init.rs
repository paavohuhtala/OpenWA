//! Full Rust replacement for `GameEngine__InitHardware` (0x56D350).
//!
//! Orchestrates creation of all game hardware subsystems and stores their
//! pointers into `*G_GAME_SESSION` (`GameSession`).
//!
//! ## Calling convention
//!
//! `__thiscall`: ECX = `*mut GameInfo` (≥0xF914 bytes), 3 stack params
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
//! IF GameInfo+0xF914 == 0 (normal mode):
//!   DisplayGfx (0x24E28, stdcall ctor) → session+0xAC
//!   DDDisplay::Init retry loop (configured → 1024×768 → 800×600 → 640×480)
//!   screen center / cursor setup
//!   DDKeyboard (0x33C, inline) → session+0xA4
//!   Palette (0x28, inline) → session+0xB0
//!   DSSound (0xBE0, usercall ctor + DirectSoundCreate + coop level) → session+0xA8
//!   IF GameInfo+0xDAA4 != 0 AND DSSound OK: streaming audio → session+0xB4
//!
//! ELSE (headless):
//!   GameStats (0x3560, stdcall ctor + vtable override) → session+0xAC
//!   session+0xA4/0xA8/0xB0/0xB4 = null
//!
//! ALWAYS:
//!   session+0x28 = (GameInfo+0xF3B0 != 0) ? 1 : 0
//!   DDGameWrapper (0x6F10) → session+0xA0  [via game_session::construct_ddgame_wrapper]
//!   Palette vtable[4/3/2] calls + DDKeyboard poll (normal mode only)
//!   DDNetGameWrapper (0x2C, stdcall ctor) → session+0xC0
//! ```

use openwa_core::address::va;
use openwa_core::rebase::rb;
use openwa_core::game_session::GameSession;
use openwa_core::ddgame_wrapper::DDGameWrapper;
use openwa_core::dddisplay::DDDisplay;
use openwa_core::dssound::DSSound;
use openwa_core::palette::Palette;
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

/// Timer constructor: `usercall(ESI=timer_ptr, EAX=d778_val)`, plain RET.
/// Returns whatever EAX holds after the call.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_timer_ctor(_timer_ptr: *mut u8, _d778_val: u32) -> u32 {
    core::arch::naked_asm!(
        // [esp+0]=bridge_ret, [esp+4]=timer_ptr, [esp+8]=d778_val
        "pushl %esi",
        // [esp+0]=old_esi, [esp+4]=bridge_ret, [esp+8]=timer_ptr, [esp+c]=d778_val
        "movl 8(%esp), %esi",    // ESI = timer_ptr
        "movl 0xc(%esp), %eax",  // EAX = d778_val
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
unsafe extern "cdecl" fn call_dssound_ctor(_dssound: *mut u8) {
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
    _out_0x10: *mut u32,
    _out_0x0c: *mut u32,
) -> u32 {
    core::arch::naked_asm!(
        // Entry: ESP=E, [E+0]=bridge_ret, [E+4]=out_0x10, [E+8]=out_0x0c
        "movl {eax_val}, %eax",      // EAX = dssound
        "movl (%esp), %ecx",         // ECX = bridge_ret (temp)
        "movl %ecx, {saved_ret}",    // save to static (ECX clobbered by FUN_00573E50)
        "movl 4(%esp), %ecx",        // ECX = out_0x10
        "movl 8(%esp), %edx",        // EDX = out_0x0c
        "addl $0xC, %esp",           // ESP = E+12 (discard bridge_ret + 2 args)
        "pushl %edx",                // ESP = E+8,  [E+8]  = out_0x0c
        "pushl %ecx",                // ESP = E+4,  [E+4]  = out_0x10
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
    _hwnd: u32,
    _param3: u32,
    _d778_val: u32,
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
    game_info: *mut u8,
    hwnd: u32,
    param3: u32,
    param4: u32,
) -> u32 {
    let _ = log_line("[hardware_init] GameEngine::InitHardware");
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    let game_info_d778 = *(game_info.add(0xD778) as *const u32);

    // ── Input controller (if param4 != 0) ────────────────────────────────────
    if param4 == 0 {
        (*session).input_ctrl = core::ptr::null_mut();
    } else {
        let ctrl_ptr = WABox::<u8>::alloc(0x1800, 0x17E0).leak();
        // puVar3[0x35d] = 0x3f9 (original sets this; offset 0xD74)
        *(ctrl_ptr.add(0xD74) as *mut u32) = 0x3F9;
        *(ctrl_ptr as *mut u32) = rb(va::INPUT_CTRL_VTABLE);
        (*session).input_ctrl = ctrl_ptr;

        INPUT_CTRL_ESI = ctrl_ptr as u32;
        let ok = call_input_ctrl_init(game_info.add(4), param3, param4, game_info_d778);
        if ok == 0 {
            let vtbl = *(ctrl_ptr as *const *const usize);
            let dtor: unsafe extern "thiscall" fn(*mut u8, u32) =
                core::mem::transmute(*vtbl);
            dtor(ctrl_ptr, 1);
            (*session).input_ctrl = core::ptr::null_mut();
            return 0;
        }
    }

    // ── Timer object (ALWAYS) ─────────────────────────────────────────────────
    let timer_ptr = WABox::<u8>::alloc(0x30, 0x30).leak();
    call_timer_ctor(timer_ptr, game_info_d778);
    (*session).timer_obj = timer_ptr;

    let headless = *(game_info.add(0xF914) as *const u32) != 0;

    if !headless {
        // ── DisplayGfx ───────────────────────────────────────────────────────
        let displaygfx_ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
            core::mem::transmute(rb(va::DISPLAYGFX_CTOR) as usize);
        let display_gfx = displaygfx_ctor(WABox::<u8>::alloc(0x24E28, 0x24E08).leak());
        (*session).display_gfx = display_gfx;

        // ── DDDisplay::Init retry loop ────────────────────────────────────────
        // DDDisplay::Init is usercall(ECX=height) + stdcall(display_gfx, hwnd, width, flags).
        // Use call_ddisplay_init which sets ECX from DDISPLAY_INIT_ECX before the tail-jump.
        let flags = *(game_info.add(0xF374) as *const u32);
        let w0 = *(game_info.add(0xF3B4) as *const u32);
        let h0 = *(game_info.add(0xF3B8) as *const u32);

        DDISPLAY_INIT_ECX = h0;
        let mut init_ok = call_ddisplay_init(display_gfx, hwnd, w0, flags) != 0;

        if !init_ok {
            let fallbacks: [(u32, u32); 3] = [
                (0x400, 0x300), // 1024×768
                (0x320, 0x258), // 800×600
                (0x280, 0x1E0), // 640×480
            ];
            for &(w, h) in &fallbacks {
                *(game_info.add(0xF3B4) as *mut u32) = w;
                *(game_info.add(0xF3B8) as *mut u32) = h;
                DDISPLAY_INIT_ECX = h;
                if call_ddisplay_init(display_gfx, hwnd, w, flags) != 0 {
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
            let w = *(game_info.add(0xF3B4) as *const i32);
            let h = *(game_info.add(0xF3B8) as *const i32);
            (w / 2, h / 2)
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
                // MapWindowPoints via IAT thunk (avoids adding a new windows-sys feature).
                let map_fn_ptr = *(rb(0x0061_A588) as *const usize);
                let map_fn: unsafe extern "stdcall" fn(HWND, HWND, *mut RECT, u32) -> i32 =
                    core::mem::transmute(map_fn_ptr);
                map_fn(hwnd_val, core::ptr::null_mut(), &mut rect, 2);
                ClipCursor(&rect);
            }
        }

        // ── DDKeyboard (inline construction) ──────────────────────────────────
        let kb_ptr = WABox::<u8>::alloc(0x33C, 0x31C).leak();
        *(kb_ptr as *mut u32) = rb(va::DDKEYBOARD_VTABLE);
        *(kb_ptr.add(4) as *mut u32) = game_info.add(0xF918) as u32;
        *(kb_ptr.add(8) as *mut u32) = 1;
        core::ptr::write_bytes(kb_ptr.add(0x11C), 0, 0x100);
        core::ptr::write_bytes(kb_ptr.add(0x21C), 0, 0x100);
        *(kb_ptr.add(0x14) as *mut u32) = 0;
        *(kb_ptr.add(0x18) as *mut u32) = 0;
        (*session).keyboard = kb_ptr;

        // ── Palette (inline construction) ─────────────────────────────────────
        let pal_ptr = WABox::<u8>::alloc(0x28, 0).leak();
        *(pal_ptr as *mut u32) = rb(va::PALETTE_VTABLE_MAYBE);
        *(pal_ptr.add(4) as *mut u32) = 0xFFFF_FFFF;
        (*session).palette = pal_ptr;

        // ── DSSound ───────────────────────────────────────────────────────────
        (*session).sound = core::ptr::null_mut();
        {
            let snd_mem = WABox::<u8>::alloc(0xBE0, 0xBC0).leak();
            call_dssound_ctor(snd_mem);
            *(snd_mem.add(4) as *mut u32) = hwnd;
            // Store DSSound pointer unconditionally — original does this before init checks.
            // snd_mem+0xBBC = 1 only if all 3 COM init steps succeed; WA code checks this flag.
            (*session).sound = snd_mem;

            let ds_create: unsafe extern "stdcall" fn(*const u8, *mut *mut u8, *const u8) -> i32 =
                core::mem::transmute(rb(va::DIRECTSOUND_CREATE) as usize);
            let hr = ds_create(core::ptr::null(), snd_mem.add(8) as *mut *mut u8, core::ptr::null());

            if hr == 0 {
                DSSOUND_INIT_EAX = snd_mem as u32;
                let hr2 = call_dssound_init_buffers(
                    snd_mem.add(0x10) as *mut u32,
                    snd_mem.add(0x0C) as *mut u32,
                );
                if hr2 == 0 {
                    // vtable[12] (offset 0x30) of the IDirectSoundBuffer* stored at snd_mem+0x10
                    // by DSSOUND_INIT_BUFFERS. IDirectSoundBuffer::Play(this, 0, 0, DSBPLAY_LOOPING=1)
                    let pds = *(snd_mem.add(0x10) as *const *const *const usize);
                    let vtbl = *pds;
                    let play: unsafe extern "stdcall" fn(
                        *const *const usize, u32, u32, u32,
                    ) -> i32 = core::mem::transmute(*vtbl.add(0x30 / 4));
                    let hr3 = play(pds, 0, 0, 1);
                    if hr3 == 0 {
                        *(snd_mem.add(0xBBC) as *mut u32) = 1;
                    }
                }
            }
        }

        // ── Streaming audio ───────────────────────────────────────────────────
        (*session).streaming_audio = core::ptr::null_mut();
        if !(*session).sound.is_null() {
            let speech_flag = *(game_info.add(0xDAA4) as *const u8);
            if speech_flag != 0 {
                let stream_mem = WABox::<u8>::alloc(0x354, 0x334).leak();
                let dssound = (*session).sound;
                let ids = *(dssound.add(8) as *const *mut u8);
                call_streaming_audio_ctor(stream_mem, ids, game_info.add(0xD9E0));
                (*session).streaming_audio = stream_mem;
            }
        }
    } else {
        // ── Headless / stats mode ─────────────────────────────────────────────
        let stats_mem = WABox::<u8>::alloc(0x3560, 0x3560).leak();
        let gamestats_ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
            core::mem::transmute(rb(va::GAMESTATS_CTOR) as usize);
        gamestats_ctor(stats_mem);
        *(stats_mem as *mut u32) = rb(va::GAMESTATS_VTABLE);
        let stats_ptr = stats_mem;
        (*session).display_gfx      = stats_ptr;
        (*session).keyboard         = core::ptr::null_mut();
        (*session).sound            = core::ptr::null_mut();
        (*session).palette          = core::ptr::null_mut();
        (*session).streaming_audio  = core::ptr::null_mut();
    }

    // ── Windowed/fullscreen flag → session+0x28, and session+0x24 = 1 ──────────
    let fullscreen_word = *(game_info.add(0xF3B0) as *const u16);
    *((session as *mut u8).add(0x28) as *mut u32) = (fullscreen_word != 0) as u32;
    // Original always sets session+0x24 = 1 unconditionally (both normal + headless).
    *((session as *mut u8).add(0x24) as *mut u32) = 1;

    // ── DDGameWrapper (ALWAYS) ────────────────────────────────────────────────
    // The 7 explicit args mirror the session fields:
    //   display_gfx (DisplayGfx*), sound (DSSound*), keyboard (DDKeyboard* — called "gfx"),
    //   palette, streaming_audio ("music"), input_ctrl ("network").
    let wrapper = game_session::construct_ddgame_wrapper(
        game_info,
        WABox::<DDGameWrapper>::alloc(0x6F10, 0x6EF0).leak(),
        (*session).display_gfx as *mut DDDisplay,
        (*session).sound as *mut DSSound,
        (*session).keyboard,
        (*session).palette as *mut Palette,
        (*session).streaming_audio,
        (*session).input_ctrl,
    );
    (*session).ddgame_wrapper = wrapper as *mut u8;

    // ── Palette vtable[4/3/2] + keyboard poll (normal mode only) ─────────────
    if !headless {
        let pal = (*session).palette;
        if !pal.is_null() {
            let vtbl = *(pal as *const *const usize);
            let vt4: unsafe extern "thiscall" fn(*mut u8) =
                core::mem::transmute(*vtbl.add(4));
            let vt3: unsafe extern "thiscall" fn(*mut u8) =
                core::mem::transmute(*vtbl.add(3));
            let vt2: unsafe extern "thiscall" fn(*mut u8, u32) =
                core::mem::transmute(*vtbl.add(2));
            vt4(pal);
            vt3(pal);
            vt2(pal, 7);
        }

        let kb = (*session).keyboard;
        if !kb.is_null() {
            let kb_poll: unsafe extern "stdcall" fn(*mut u8) =
                core::mem::transmute(rb(va::DDKEYBOARD_POLL_KEYBOARD_STATE) as usize);
            kb_poll(kb);
        }
    }

    // ── DDNetGameWrapper (ALWAYS) ─────────────────────────────────────────────
    let net_ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
        core::mem::transmute(rb(va::DDNETGAME_WRAPPER_CTOR) as usize);
    (*session).net_game = net_ctor(WABox::<u8>::alloc(0x2C, 0).leak());

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
