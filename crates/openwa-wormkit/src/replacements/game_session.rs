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
//! - `DDGame__Constructor` (0x56E220): **REPLACED** by `create_ddgame()` in
//!   openwa-core. Original function is trapped.
//! - `DDGame__InitGameState` (0x526500): plain stdcall(this), called via transmute.

use openwa_core::address::va;
use openwa_core::audio::DSSound;
use openwa_core::display::{DDDisplay, Palette};
use openwa_core::engine::ddgame::{create_ddgame, init_constructor_addrs};
use openwa_core::engine::{DDGameWrapper, GameInfo, GameSession};
use openwa_core::input::DDKeyboard;
use openwa_core::rebase::rb;
use crate::hook;
use crate::log_line;

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
    let net_game  = (*session).net_game;

    // Create DDGame — replaces the original DDGame__Constructor (0x56E220).
    create_ddgame(
        this,
        keyboard as *mut DDKeyboard,
        display,
        sound,
        palette,
        streaming_audio as *mut openwa_core::audio::Music,
        timer_obj,
        net_game,
        game_info,
        input_ctrl as u32,  // network ECX
    );

    // Initialize DDGame's game-state fields.
    let init_state: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_GAME_STATE) as usize);
    init_state(this);

    let ddgame = (*this).ddgame;
    let _ = log_line(&format!(
        "[GameSession] DDGameWrapper::Constructor: wrapper=0x{:08X}  ddgame=0x{:08X}",
        this as u32, ddgame as u32,
    ));

    this
}

pub fn install() -> Result<(), String> {
    unsafe {
        INIT_REPLAY_ADDR = rb(va::DDGAMEWRAPPER_INIT_REPLAY);
        // Initialize runtime addresses for create_ddgame bridges.
        init_constructor_addrs();
        // Both constructors are fully converted — trap the originals.
        hook::install_trap!("DDGameWrapper__Constructor", va::CONSTRUCT_DD_GAME_WRAPPER);
        hook::install_trap!("DDGame__Constructor", va::CONSTRUCT_DD_GAME);
    }
    Ok(())
}
