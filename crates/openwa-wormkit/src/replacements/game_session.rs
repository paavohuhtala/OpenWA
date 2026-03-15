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
//! - `DDGame__Constructor` (0x56E220): stdcall 9 params + implicit ECX=network.
//!   Bridged via `ddgame_constructor_call` which sets ECX then **tail-jumps**,
//!   preserving the exact stack so DDGame::ctor's `RET 0x24` returns to ctor_impl.
//! - `DDGame__InitGameState` (0x526500): plain stdcall(this), called via transmute.

use openwa_core::address::va;
use openwa_core::engine::GameInfo;
use openwa_core::rebase::rb;
use openwa_core::engine::DDGameWrapper;
use openwa_core::display::DDDisplay;
use openwa_core::audio::DSSound;
use openwa_core::engine::GameSession;
use openwa_core::display::Palette;
use crate::hook;
use crate::log_line;

/// Implicit EDI = game_info pointer, captured from EDI on entry.
static mut GAME_INFO: *mut GameInfo = core::ptr::null_mut();

/// Implicit ECX = network pointer for `DDGame__Constructor`. Set in `ctor_impl`
/// just before calling `ddgame_constructor_call`.
static mut DDGAME_CTOR_ECX: u32 = 0;

/// Runtime address of `DDGameWrapper__InitReplay` (set at install time).
static mut INIT_REPLAY_ADDR: u32 = 0;

/// Runtime address of `DDGame__Constructor` (set at install time).
static mut DDGAME_CTOR_ADDR: u32 = 0;

// ─── Bridge: DDGameWrapper__InitReplay ───────────────────────────────────────
//
// Convention: usercall(EAX=game_info, ESI=this), plain RET (no stack params).
// Declared as stdcall(game_info, this) so the Rust caller pushes both; the
// naked body loads EAX/ESI from the stack and calls via INIT_REPLAY_ADDR.
//
// Stack on entry (after stdcall push):
//   [esp+0] = ret addr
//   [esp+4] = game_info (arg 1, pushed last by stdcall)
//   [esp+8] = this      (arg 2, pushed first by stdcall)
//
// After `pushl %esi`:
//   [esp+0] = old_esi,  [esp+4] = ret addr,  [esp+8] = game_info,  [esp+0xC] = this
#[unsafe(naked)]
unsafe extern "stdcall" fn call_init_replay(_game_info: *mut GameInfo, _this: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %eax",    // EAX = game_info
        "movl 0xC(%esp), %esi",  // ESI = this
        "calll *({fn})",         // call DDGameWrapper__InitReplay; plain RET from callee
        "popl %esi",             // restore ESI
        "retl $8",               // stdcall callee-cleanup: 2 × u32 = 8 bytes
        fn = sym INIT_REPLAY_ADDR,
        options(att_syntax),
    );
}

// ─── Bridge: DDGame__Constructor ─────────────────────────────────────────────
//
// Convention: stdcall 9 params + implicit ECX=network.
//
// Rust calls this as a normal stdcall(9 params). The naked body loads ECX from
// DDGAME_CTOR_ECX and then **tail-jumps** (jmp, not call) to DDGame::ctor.
//
// Tail-jump rationale: the call instruction in ctor_impl already pushed the
// return address at [esp+0]. After the tail-jump, DDGame::ctor sees exactly:
//   [esp+0]  = return address back into ctor_impl
//   [esp+4]  = this  … [esp+24] = game_info   (9 stdcall args)
//   ECX      = network (implicit)
// DDGame::ctor's `RET 0x24` pops 0x24 bytes and returns to ctor_impl. ✓
#[unsafe(naked)]
unsafe extern "stdcall" fn ddgame_constructor_call(
    _this: *mut DDGameWrapper,
    _display: *mut DDDisplay,
    _sound: *mut DSSound,
    _keyboard: *mut u8,
    _palette: *mut Palette,
    _streaming_audio: *mut u8,
    _timer_obj: *mut u8,
    _net_game: *mut u8,
    _game_info: *mut GameInfo,
) -> *mut u8 {
    core::arch::naked_asm!(
        "movl {ecx_val}, %ecx",  // ECX = input_ctrl (implicit param for DDGame::ctor)
        "jmpl *({fn})",          // tail-jump; DDGame::ctor's RET 0x24 returns to caller
        ecx_val = sym DDGAME_CTOR_ECX,
        fn = sym DDGAME_CTOR_ADDR,
        options(att_syntax),
    );
}

/// Called by `impl_init_hardware` to construct the DDGameWrapper in-place.
///
/// Sets `GAME_INFO` (read by `ctor_impl`) and delegates directly to `ctor_impl`,
/// bypassing the naked entry trampoline which is designed as a hook target, not a
/// callable subroutine.
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

    let this = this;

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

    // Store input_ctrl as the implicit ECX for DDGame::ctor, then tail-jump-call.
    DDGAME_CTOR_ECX = input_ctrl as u32;
    ddgame_constructor_call(
        this, display, sound, keyboard, palette, streaming_audio,
        timer_obj, net_game, game_info,
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
        DDGAME_CTOR_ADDR = rb(va::CONSTRUCT_DD_GAME);
        // Fully converted — only called from impl_init_hardware (Rust).
        // Trap panics if WA.exe unexpectedly calls the original.
        hook::install_trap!("DDGameWrapper__Constructor", va::CONSTRUCT_DD_GAME_WRAPPER);
    }
    Ok(())
}
