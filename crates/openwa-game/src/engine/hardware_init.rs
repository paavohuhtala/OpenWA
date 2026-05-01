//! Rust port of `GameEngine__InitHardware` (0x0056D350) and the in-process
//! helper `GameRuntime__Constructor` (0x0056DEF0).
//!
//! Original `InitHardware` convention: `__thiscall(ECX=*mut GameInfo)` + 3
//! stack params (hwnd, param3, param4), `RET 0xC`. Returns 1 on success, 0
//! on failure. The Rust impl is plain cdecl; the WA-side address is trapped
//! since `run_game_session` calls [`init_hardware`] directly.

use crate::address::va;
use crate::audio::{DSSound, Music};
use crate::engine::game_info::GameInfo;
use crate::engine::game_session::get_game_session;
use crate::engine::runtime::GameRuntime;
use crate::engine::world_constructor::create_game_world;
use crate::engine::{DDNetGameWrapper, GameRuntimeVtable};
use crate::input::{InputCtrl, InputCtrlVtable, Keyboard, MouseInput, init_input_ctrl};
use crate::rebase::rb;
use crate::render::{DisplayBase, DisplayGfx};
use crate::wa::localized_template::LocalizedTemplate;
use crate::wa_alloc::{wa_malloc_struct, wa_malloc_struct_zeroed};

use openwa_core::log::log_line;
use windows_sys::Win32::Foundation::HWND;

// ─── Bridge-state statics ─────────────────────────────────────────────────────

static mut LOCALIZED_TEMPLATE_CTOR_ADDR: u32 = 0;
static mut STREAM_CTOR_ADDR: u32 = 0;
static mut DISPLAY_GFX_INIT_ADDR: u32 = 0;
static mut INIT_REPLAY_ADDR: u32 = 0;

/// Implicit ECX (height) for `call_display_gfx_init` — set per call.
static mut DISPLAY_GFX_INIT_ECX: u32 = 0;

static mut STREAM_CTOR_SAVED_RET: u32 = 0;
static mut STREAM_CTOR_SAVED_ESI: u32 = 0;

// ─── Bridges ─────────────────────────────────────────────────────────────────

/// `LocalizedTemplate__Constructor` (0x0053E950):
/// `usercall(ESI=this, EAX=wa_version_threshold)`, plain RET.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_localized_template_ctor(
    _this: *mut LocalizedTemplate,
    _wa_version_threshold: u32,
) -> u32 {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",
        "movl 0xc(%esp), %eax",
        "calll *({fn})",
        "popl %esi",
        "retl",
        fn = sym LOCALIZED_TEMPLATE_CTOR_ADDR,
        options(att_syntax),
    );
}

/// `FUN_0058BC10`: `usercall(ESI=this)` + 2 stack params, `RET 0x8`.
/// Saves bridge_ret to a static across the call because ECX is the only
/// scratch register left after loading ESI/EDX.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_streaming_audio_ctor(
    _stream: *mut u8,
    _ids: *mut u8,
    _path: *mut u8,
) {
    core::arch::naked_asm!(
        "movl (%esp), %ecx",
        "movl %ecx, {saved_ret}",
        "movl %esi, {saved_esi}",
        "movl 4(%esp), %esi",
        "movl 8(%esp), %ecx",
        "movl 0xc(%esp), %edx",
        "addl $0x10, %esp",
        "pushl %edx",
        "pushl %ecx",
        "calll *({fn})",
        "movl {saved_esi}, %esi",
        "subl $0xc, %esp",
        "pushl {saved_ret}",
        "retl",
        fn = sym STREAM_CTOR_ADDR,
        saved_ret = sym STREAM_CTOR_SAVED_RET,
        saved_esi = sym STREAM_CTOR_SAVED_ESI,
        options(att_syntax),
    );
}

/// `DisplayGfx__Init`: `usercall(ECX=height)` + 4 stack params, `RET 0x10`.
/// Caller sets `DISPLAY_GFX_INIT_ECX` before calling.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_display_gfx_init(
    _display_gfx: *mut u8,
    _hwnd: HWND,
    _width: u32,
    _flags: u32,
) -> u32 {
    core::arch::naked_asm!(
        "movl {ecx_val}, %ecx",
        "jmpl *({fn})",
        ecx_val = sym DISPLAY_GFX_INIT_ECX,
        fn = sym DISPLAY_GFX_INIT_ADDR,
        options(att_syntax),
    );
}

/// `GameRuntime__InitReplay` (0x0056F860): `usercall(EAX=game_info, ESI=this)`,
/// plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_init_replay(_game_info: *mut GameInfo, _this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %eax",
        "movl 0xC(%esp), %esi",
        "calll *({fn})",
        "popl %esi",
        "retl $8",
        fn = sym INIT_REPLAY_ADDR,
        options(att_syntax),
    );
}

// ─── Subsystem creation ───────────────────────────────────────────────────────

/// On COM failure, the partially-initialized DSSound is still returned —
/// matches WA, where downstream code tolerates `init_success == 0`.
unsafe fn create_dssound(hwnd: HWND) -> *mut DSSound {
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Media::Audio::DirectSound::{
            DSBCAPS_PRIMARYBUFFER, DSBPLAY_LOOPING, DSBUFFERDESC, DSSCL_PRIORITY,
            DirectSoundCreate, IDirectSound, IDirectSoundBuffer,
        };

        let snd = wa_malloc_struct::<DSSound>();
        core::ptr::write(snd, DSSound::new(hwnd));

        let mut ds: Option<IDirectSound> = None;
        if DirectSoundCreate(None, &mut ds, None).is_ok() {
            let ds = ds.unwrap();

            let _ = ds.SetCooperativeLevel(HWND(hwnd as _), DSSCL_PRIORITY);

            let desc = DSBUFFERDESC {
                dwSize: core::mem::size_of::<DSBUFFERDESC>() as u32,
                dwFlags: DSBCAPS_PRIMARYBUFFER,
                ..core::mem::zeroed()
            };
            let mut primary: Option<IDirectSoundBuffer> = None;
            if ds.CreateSoundBuffer(&desc, &mut primary, None).is_ok() {
                let primary = primary.unwrap();

                let mut caps =
                    core::mem::zeroed::<windows::Win32::Media::Audio::DirectSound::DSBCAPS>();
                caps.dwSize = core::mem::size_of_val(&caps) as u32;
                let _ = primary.GetCaps(&mut caps);
                (*snd).primary_buffer_caps = caps.dwBufferBytes;

                if primary.Play(0, 0, DSBPLAY_LOOPING).is_ok() {
                    (*snd).init_success = 1;
                }

                (*snd).primary_buffer = core::mem::transmute_copy(&primary);
                core::mem::forget(primary);
            }

            (*snd).direct_sound = core::mem::transmute_copy(&ds);
            core::mem::forget(ds);
        }

        snd
    }
}

// ─── GameRuntime construction ─────────────────────────────────────────────────

/// Rust port of `GameRuntime__Constructor` (0x0056DEF0). WA function is
/// trapped; only caller is [`init_hardware`].
pub unsafe fn construct_runtime(
    game_info: *mut GameInfo,
    this: *mut GameRuntime,
    display: *mut DisplayGfx,
    sound: *mut DSSound,
    keyboard: *mut Keyboard,
    mouse_input: *mut MouseInput,
    music: *mut Music,
    input_ctrl: *mut u8,
) -> *mut GameRuntime {
    unsafe {
        (*this).world = core::ptr::null_mut();
        (*this).landscape = core::ptr::null_mut();
        (*this).vtable = rb(va::GAME_RUNTIME_VTABLE) as *const GameRuntimeVtable;
        (*this).sound = sound;
        (*this).display = display;

        call_init_replay(game_info, this);

        let session = get_game_session();
        let localized_template = (*session).localized_template;
        let net_game = (*session).net_game;

        {
            use crate::registry::{self, LiveObject};
            registry::register_live_object(LiveObject {
                ptr: session as u32,
                size: 0x120,
                class_name: "GameSession",
                fields: registry::struct_fields_for("GameSession"),
            });
        }

        let _ = log_line(&format!(
            "[GameSession] display=0x{:08X}, net_game=0x{:08X}, localized_template=0x{:08X}, game_info=0x{:08X}",
            display as u32, net_game as u32, localized_template as u32, game_info as u32,
        ));

        create_game_world(
            this,
            keyboard,
            display,
            sound,
            mouse_input,
            music,
            localized_template,
            net_game,
            game_info,
            input_ctrl as *mut crate::engine::net_session::NetSession,
        );

        crate::engine::game_state_init::init_game_state(this);

        let _ = log_line(&format!(
            "[GameSession] GameRuntime::Constructor done: wrapper=0x{:08X}  world=0x{:08X}",
            this as u32,
            (*this).world as u32,
        ));

        use crate::registry::{self, LiveObject};
        registry::register_live_object(LiveObject {
            ptr: this as u32,
            size: core::mem::size_of::<GameRuntime>() as u32,
            class_name: "GameRuntime",
            fields: registry::struct_fields_for("GameRuntime"),
        });
        if !(*this).world.is_null() {
            registry::register_live_object(LiveObject {
                ptr: (*this).world as u32,
                size: 0x98D8,
                class_name: "GameWorld",
                fields: registry::struct_fields_for("GameWorld"),
            });
        }

        this
    }
}

// ─── Top-level init ───────────────────────────────────────────────────────────

pub unsafe fn init_hardware(
    game_info: *mut GameInfo,
    hwnd: HWND,
    controlled_displays: *const *mut u8,
    controlled_display_count: u32,
) -> u32 {
    unsafe {
        let _ = log_line("[hardware_init] GameEngine::InitHardware");
        let session = get_game_session();
        let gi = &mut *game_info;
        let game_version = gi.game_version as u32;

        if controlled_display_count == 0 {
            (*session).input_ctrl = core::ptr::null_mut();
        } else {
            let ctrl = wa_malloc_struct_zeroed::<InputCtrl>();
            (*ctrl).field_d74 = 0x3F9;
            (*ctrl).vtable = rb(va::INPUT_CTRL_VTABLE) as *const InputCtrlVtable;
            (*session).input_ctrl = ctrl as *mut u8;

            let ok = init_input_ctrl(
                ctrl,
                game_info,
                controlled_displays,
                controlled_display_count,
            );
            if ok == 0 {
                (*ctrl).destroy(1);
                (*session).input_ctrl = core::ptr::null_mut();
                return 0;
            }
        }

        let localized_template = wa_malloc_struct_zeroed::<LocalizedTemplate>();
        call_localized_template_ctor(localized_template, game_version);
        (*session).localized_template = localized_template;

        let headless = gi.headless_mode != 0;

        if !headless {
            let display_gfx = DisplayGfx::construct();
            (*session).display = display_gfx as *mut u8;

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

            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, SetCursorPos,
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

            let kb = wa_malloc_struct::<Keyboard>();
            core::ptr::write(
                kb,
                Keyboard::new(rb(va::KEYBOARD_VTABLE), &raw mut gi.input_state_f918 as u32),
            );
            (*session).keyboard = kb;

            let mi = wa_malloc_struct::<MouseInput>();
            core::ptr::write(mi, MouseInput::new(rb(va::MOUSE_INPUT_VTABLE)));
            (*session).mouse_input = mi;

            (*session).sound = create_dssound(hwnd);

            (*session).streaming_audio = core::ptr::null_mut();
            if !(*session).sound.is_null() && gi.speech_enabled != 0 {
                let stream = wa_malloc_struct_zeroed::<Music>();
                let ids = (*(*session).sound).direct_sound as *mut u8;
                call_streaming_audio_ctor(
                    stream as *mut u8,
                    ids,
                    gi.streaming_audio_config.as_mut_ptr(),
                );
                (*session).streaming_audio = stream;
            }
        } else {
            (*session).display = DisplayBase::new_headless() as *mut u8;
            (*session).keyboard = core::ptr::null_mut();
            (*session).sound = core::ptr::null_mut();
            (*session).mouse_input = core::ptr::null_mut();
            (*session).streaming_audio = core::ptr::null_mut();
        }

        (*session).init_flag = 1;
        (*session).home_lock_active = (gi.home_lock != 0) as u32;

        let runtime = construct_runtime(
            game_info,
            wa_malloc_struct_zeroed::<GameRuntime>(),
            (*session).display as *mut DisplayGfx,
            (*session).sound,
            (*session).keyboard,
            (*session).mouse_input,
            (*session).streaming_audio,
            (*session).input_ctrl,
        );
        (*session).game_runtime = runtime;

        // Slot 4 is a no-op stub; slot 3 zeros mouse deltas; slot 2 with mask
        // 0x7 disarms LMB/RMB/MMB latch bits that may already be down at
        // startup so the first real press registers.
        if !headless {
            let mi = (*session).mouse_input;
            if !mi.is_null() {
                (*mi).slot_04_noop();
                (*mi).clear_deltas();
                (*mi).ack_button_mask(7);
            }

            let kb = (*session).keyboard;
            if !kb.is_null() {
                (*kb).poll();
            }
        }

        (*session).net_game = DDNetGameWrapper::construct() as *mut u8;

        let _ = log_line("[hardware_init] GameEngine::InitHardware done");
        1
    }
}

pub fn init_addrs() {
    unsafe {
        LOCALIZED_TEMPLATE_CTOR_ADDR = rb(va::LOCALIZED_TEMPLATE_CTOR);
        STREAM_CTOR_ADDR = rb(va::STREAMING_AUDIO_CTOR);
        DISPLAY_GFX_INIT_ADDR = rb(va::DISPLAY_GFX_INIT);
        INIT_REPLAY_ADDR = rb(va::GAME_RUNTIME_INIT_REPLAY);
        crate::input::controller::init_addrs();
    }
}
