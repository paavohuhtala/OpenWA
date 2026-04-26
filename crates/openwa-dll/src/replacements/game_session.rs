//! Full Rust replacement for `GameRuntime__Constructor` (0x56DEF0).
//!
//! ## Status: FULLY CONVERTED
//!
//! All callers are Rust (`construct_runtime` from `impl_init_hardware`).
//! The original WA function is trapped — panics if called.
//!
//! ## Sub-call conventions
//!
//! - `GameRuntime__InitReplay` (0x56F860): usercall(EAX=game_info, ESI=this),
//!   plain RET (no stack args). Bridged via `call_init_replay`.
//! - `GameWorld__Constructor` (0x56E220): fully replaced by `create_world()` in openwa-game.
//! - `GameWorld__InitGameState` (0x526500): ported to Rust in `init_game_state()`.

use crate::hook::{self, usercall_trampoline};
use crate::log_line;
use openwa_game::address::va;
use openwa_game::audio::DSSound;
use openwa_game::engine::GameRuntimeVtable;
use openwa_game::engine::create_game_world;
use openwa_game::engine::game_session::get_game_session;
use openwa_game::engine::game_session_run::run_game_session;
use openwa_game::engine::init_constructor_addrs;
use openwa_game::engine::{GameInfo, GameRuntime};
use openwa_game::rebase::rb;
use openwa_game::render::{DisplayGfx, Palette};

/// Implicit EDI = game_info pointer, captured from EDI on entry.
static mut GAME_INFO: *mut GameInfo = core::ptr::null_mut();

/// Runtime address of `GameRuntime__InitReplay` (set at install time).
static mut INIT_REPLAY_ADDR: u32 = 0;

// ─── Bridge: GameRuntime__InitReplay ───────────────────────────────────────
//
// Convention: usercall(EAX=game_info, ESI=this), plain RET (no stack params).
#[unsafe(naked)]
unsafe extern "stdcall" fn call_init_replay(_game_info: *mut GameInfo, _this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %eax",    // EAX = game_info
        "movl 0xC(%esp), %esi",  // ESI = this
        "calll *({fn})",         // call GameRuntime__InitReplay; plain RET
        "popl %esi",
        "retl $8",               // stdcall cleanup: 2 × u32 = 8
        fn = sym INIT_REPLAY_ADDR,
        options(att_syntax),
    );
}

/// Temp: bridge to original GameWorld__Constructor for comparison.
#[unsafe(naked)]
unsafe extern "C" fn call_original_world_ctor(
    _runtime: *mut GameRuntime,
    _display: *mut DisplayGfx,
    _sound: *mut DSSound,
    _keyboard: *mut u8,
    _palette: *mut Palette,
    _music: *mut u8,
    _timer: *mut u8,
    _net_game: *mut u8,
    _game_info: *mut GameInfo,
    _input_ctrl: *mut u8,
) {
    core::arch::naked_asm!(
        "mov ecx, [esp+40]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "push [esp+36]",
        "call [{addr}]",
        "ret",
        addr = sym GAME_WORLD_CTOR_ADDR,
    );
}
static mut GAME_WORLD_CTOR_ADDR: u32 = 0;

/// Called by `impl_init_hardware` to construct the GameRuntime in-place.
pub(crate) unsafe fn construct_runtime(
    game_info: *mut GameInfo,
    this: *mut GameRuntime,
    display: *mut DisplayGfx,
    sound: *mut DSSound,
    keyboard: *mut u8,
    palette: *mut Palette,
    streaming_audio: *mut u8,
    input_ctrl: *mut u8,
) -> *mut GameRuntime {
    unsafe {
        GAME_INFO = game_info;

        // Initialize GameRuntime fields (order matches original decompile).
        (*this).world = core::ptr::null_mut();
        (*this).landscape = core::ptr::null_mut();
        (*this).vtable = rb(va::GAME_RUNTIME_VTABLE) as *const GameRuntimeVtable;
        (*this).sound = sound;
        (*this).display = display;

        // Initialize replay subsystem.  usercall(EAX=game_info, ESI=this), plain RET.
        call_init_replay(game_info, this);

        // Read timer_obj and net_game from the live game session struct.
        let session = get_game_session();
        let timer_obj = (*session).timer_obj;
        let net_game = (*session).net_game;

        // Register GameSession as a live object.
        {
            use openwa_game::registry::{self, LiveObject};
            registry::register_live_object(LiveObject {
                ptr: session as u32,
                size: 0x120,
                class_name: "GameSession",
                fields: registry::struct_fields_for("GameSession"),
            });
        }

        let _ = log_line(&format!(
            "[GameSession] display=0x{:08X}, net_game=0x{:08X}, timer=0x{:08X}, game_info(EDI)=0x{:08X}",
            display as u32, net_game as u32, timer_obj as u32, game_info as u32,
        ));

        // Arm display watchpoint during construction if requested
        if std::env::var("OPENWA_WATCH_DISPLAY").is_ok() {
            crate::debug_watchpoint::prepare();
            crate::debug_watchpoint::on_world_alloc(display as *mut u8);
        }

        // Use env var to switch between original and Rust constructor
        let use_original = std::env::var("OPENWA_USE_ORIG_CTOR").is_ok();
        if use_original {
            call_original_world_ctor(
                this,
                display,
                sound,
                keyboard,
                palette,
                streaming_audio,
                timer_obj,
                net_game,
                game_info,
                input_ctrl,
            );
        } else {
            create_game_world(
                this,
                keyboard as *mut openwa_game::input::DDKeyboard,
                display,
                sound,
                palette,
                streaming_audio as *mut openwa_game::audio::Music,
                timer_obj,
                net_game,
                game_info,
                input_ctrl as *mut openwa_game::engine::net_session::NetSession,
            );
        }

        // Disarm display watchpoint
        if std::env::var("OPENWA_WATCH_DISPLAY").is_ok() {
            crate::debug_watchpoint::teardown();
        }

        // Initialize GameWorld's game-state fields (Rust port).
        openwa_game::engine::game_state_init::init_game_state(this);

        let _ = log_line(&format!(
            "[GameSession] GameRuntime::Constructor done: wrapper=0x{:08X}  world=0x{:08X}",
            this as u32,
            (*this).world as u32,
        ));

        // Register live objects for pointer identification in debug tools.
        use openwa_game::registry::{self, LiveObject};
        registry::register_live_object(LiveObject {
            ptr: this as u32,
            size: core::mem::size_of::<GameRuntime>() as u32,
            class_name: "GameRuntime",
            fields: registry::struct_fields_for("GameRuntime"),
        });
        if !(*this).world.is_null() {
            registry::register_live_object(LiveObject {
                ptr: (*this).world as u32,
                size: 0x98D8, // GameWorld size
                class_name: "GameWorld",
                fields: registry::struct_fields_for("GameWorld"),
            });
        }

        this
    }
}

// ─── GameSession::Run hook ──────────────────────────────────────────────────
//
// __usercall(ESI=GameInfo, stack: arg1..arg4), RET 0x10. Returns 0/1 in EAX.
usercall_trampoline!(fn trampoline_game_session_run;
    impl_fn = run_game_session_impl;
    reg = esi; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn run_game_session_impl(
    game_info: *mut GameInfo,
    arg1_module_state: u32,
    state_buf: *mut u8,
    display_p3: u32,
    display_p4: u32,
) -> u32 {
    unsafe {
        run_game_session(
            game_info,
            arg1_module_state,
            state_buf,
            display_p3,
            display_p4,
        )
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        INIT_REPLAY_ADDR = rb(va::GAME_RUNTIME_INIT_REPLAY);
        GAME_WORLD_CTOR_ADDR = rb(0x56E220);
        init_constructor_addrs();
        hook::install_trap!("GameRuntime__Constructor", va::CONSTRUCT_DD_GAME_WRAPPER);
        hook::install_trap!("GameWorld__InitGameState", va::GAME_WORLD_INIT_GAME_STATE);
        hook::install(
            "GameSession::Run",
            va::GAME_SESSION_RUN,
            trampoline_game_session_run as *const (),
        )?;
    }
    Ok(())
}
