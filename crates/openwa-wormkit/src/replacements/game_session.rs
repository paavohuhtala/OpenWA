//! Full Rust replacement for `DDGameWrapper__Constructor` (0x56DEF0).
//!
//! ## Status: FULLY CONVERTED
//!
//! All callers are Rust (`construct_ddgame_wrapper` from `impl_init_hardware`).
//! The original WA function is trapped — panics if called.
//!
//! ## Sub-call conventions
//!
//! - `DDGameWrapper__InitReplay` (0x56F860): usercall(EAX=game_info, ESI=this),
//!   plain RET (no stack args). Bridged via `call_init_replay`.
//! - `DDGame__Constructor` (0x56E220): fully replaced by `create_ddgame()` in openwa-core.
//! - `DDGame__InitGameState` (0x526500): plain stdcall(this), called via transmute.

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::audio::DSSound;
use openwa_core::display::{DDDisplay, Palette};
use openwa_core::engine::ddgame::{create_ddgame, init_constructor_addrs};
use openwa_core::engine::{DDGameWrapper, GameInfo, GameSession};
use openwa_core::rebase::rb;

/// Implicit EDI = game_info pointer, captured from EDI on entry.
static mut GAME_INFO: *mut GameInfo = core::ptr::null_mut();

/// Runtime address of `DDGameWrapper__InitReplay` (set at install time).
static mut INIT_REPLAY_ADDR: u32 = 0;

// ─── Bridge: DDGameWrapper__InitReplay ───────────────────────────────────────
//
// Convention: usercall(EAX=game_info, ESI=this), plain RET (no stack params).
#[unsafe(naked)]
unsafe extern "stdcall" fn call_init_replay(_game_info: *mut GameInfo, _this: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %eax",    // EAX = game_info
        "movl 0xC(%esp), %esi",  // ESI = this
        "calll *({fn})",         // call DDGameWrapper__InitReplay; plain RET
        "popl %esi",
        "retl $8",               // stdcall cleanup: 2 × u32 = 8
        fn = sym INIT_REPLAY_ADDR,
        options(att_syntax),
    );
}

/// Temp: bridge to original DDGame__Constructor for comparison.
#[unsafe(naked)]
unsafe extern "C" fn call_original_ddgame_ctor(
    _wrapper: *mut DDGameWrapper, _display: *mut DDDisplay, _sound: *mut DSSound,
    _keyboard: *mut u8, _palette: *mut Palette, _music: *mut u8,
    _timer: *mut u8, _net_game: *mut u8, _game_info: *mut GameInfo, _input_ctrl: *mut u8,
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
        addr = sym DDGAME_CTOR_ADDR,
    );
}
static mut DDGAME_CTOR_ADDR: u32 = 0;

/// Called by `impl_init_hardware` to construct the DDGameWrapper in-place.
pub(crate) unsafe fn construct_ddgame_wrapper(
    game_info: *mut GameInfo,
    this: *mut DDGameWrapper,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    keyboard: *mut u8,
    palette: *mut Palette,
    streaming_audio: *mut u8,
    input_ctrl: *mut u8,
) -> *mut DDGameWrapper {
    GAME_INFO = game_info;

    // Initialize DDGameWrapper fields (order matches original decompile).
    (*this).ddgame = core::ptr::null_mut();
    (*this).landscape = core::ptr::null_mut();
    (*this).vtable = rb(va::DDGAME_WRAPPER_VTABLE) as *mut u8;
    (*this).sound = sound;
    (*this).display = display;

    // Initialize replay subsystem.  usercall(EAX=game_info, ESI=this), plain RET.
    call_init_replay(game_info, this);

    // Read timer_obj and net_game from the live game session struct.
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    let timer_obj = (*session).timer_obj;
    let net_game = (*session).net_game;

    // Register GameSession as a live object.
    {
        use openwa_core::registry::{self, LiveObject};
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
        crate::debug_watchpoint::on_ddgame_alloc(display as *mut u8);
    }

    // Use env var to switch between original and Rust constructor
    let use_original = std::env::var("OPENWA_USE_ORIG_CTOR").is_ok();
    if use_original {
        call_original_ddgame_ctor(
            this, display, sound, keyboard, palette, streaming_audio,
            timer_obj, net_game, game_info, input_ctrl,
        );
    } else {
        create_ddgame(
            this,
            keyboard as *mut openwa_core::input::DDKeyboard,
            display,
            sound,
            palette,
            streaming_audio as *mut openwa_core::audio::Music,
            timer_obj,
            net_game,
            game_info,
            input_ctrl as u32,
        );

    }

    // Disarm display watchpoint
    if std::env::var("OPENWA_WATCH_DISPLAY").is_ok() {
        crate::debug_watchpoint::teardown();
    }

    // Initialize DDGame's game-state fields.
    let init_state: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_GAME_STATE) as usize);
    init_state(this);


    let _ = log_line(&format!(
        "[GameSession] DDGameWrapper::Constructor done: wrapper=0x{:08X}  ddgame=0x{:08X}",
        this as u32, (*this).ddgame as u32,
    ));

    // Register live objects for pointer identification in debug tools.
    use openwa_core::registry::{self, LiveObject};
    registry::register_live_object(LiveObject {
        ptr: this as u32,
        size: core::mem::size_of::<DDGameWrapper>() as u32,
        class_name: "DDGameWrapper",
        fields: registry::struct_fields_for("DDGameWrapper"),
    });
    if !(*this).ddgame.is_null() {
        registry::register_live_object(LiveObject {
            ptr: (*this).ddgame as u32,
            size: 0x98D8, // DDGame size
            class_name: "DDGame",
            fields: registry::struct_fields_for("DDGame"),
        });
    }

    this
}

pub fn install() -> Result<(), String> {
    unsafe {
        INIT_REPLAY_ADDR = rb(va::DDGAMEWRAPPER_INIT_REPLAY);
        DDGAME_CTOR_ADDR = rb(0x56E220);
        init_constructor_addrs();
        hook::install_trap!("DDGameWrapper__Constructor", va::CONSTRUCT_DD_GAME_WRAPPER);
    }
    Ok(())
}
