//! Full Rust replacement for `DDGameWrapper__Constructor` (0x56DEF0).
//!
//! ## Original calling convention
//!
//! `DDGameWrapper__Constructor` is `__stdcall` with 7 explicit stack params, plus
//! an **implicit EDI** register param (`game_info` / `unaff_EDI` in Ghidra).
//! EDI is passed as the 9th argument to `DDGame__Constructor`.
//!
//! ## Implementation strategy
//!
//! A naked trampoline handles the unconventional calling convention:
//! 1. Saves EDI (game_info) to a static before any Rust code can touch registers.
//! 2. Pops the caller return address to another static (to simulate stdcall callee
//!    cleanup from outside the cdecl implementation).
//! 3. Calls the cdecl `ctor_impl` with the 7 stack args already in position.
//! 4. After `ctor_impl` returns, skips 0x1C bytes (7 × 4) and jumps to the saved
//!    return address — exactly what `stdcall RET 0x1C` would do.
//!    EAX = `this` (ctor_impl's return value) is preserved for the caller.
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
use openwa_core::rebase::rb;
use openwa_core::ddgame_wrapper::DDGameWrapper;
use openwa_core::dddisplay::DDDisplay;
use openwa_core::dssound::DSSound;
use openwa_core::game_session::GameSession;
use openwa_core::palette::Palette;
use crate::hook;
use crate::log_line;

/// Caller's return address, saved by the naked trampoline so we can do the
/// stdcall callee-cleanup (arg pop + return) after the cdecl impl returns.
static mut SAVED_RET: u32 = 0;

/// Implicit EDI = game_info pointer, captured from EDI on entry.
static mut GAME_INFO: u32 = 0;

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
unsafe extern "stdcall" fn call_init_replay(_game_info: *mut u8, _this: *mut DDGameWrapper) {
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
    _game_info: *mut u8,
) -> *mut u8 {
    core::arch::naked_asm!(
        "movl {ecx_val}, %ecx",  // ECX = input_ctrl (implicit param for DDGame::ctor)
        "jmpl *({fn})",          // tail-jump; DDGame::ctor's RET 0x24 returns to caller
        ecx_val = sym DDGAME_CTOR_ECX,
        fn = sym DDGAME_CTOR_ADDR,
        options(att_syntax),
    );
}

// ─── Implementation ───────────────────────────────────────────────────────────

/// Core Rust implementation of `DDGameWrapper__Constructor`.
///
/// Called by the naked trampoline via `calll`. The 7 stdcall args are already
/// on the stack in cdecl position (the trampoline popped the original return
/// address into `SAVED_RET`, leaving [this, display, …, network] at [esp+4..]).
unsafe extern "cdecl" fn ctor_impl(
    this: *mut DDGameWrapper,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    keyboard: *mut u8,
    palette: *mut Palette,
    streaming_audio: *mut u8,
    input_ctrl: *mut u8,
) -> *mut DDGameWrapper {
    let game_info = GAME_INFO as *mut u8;

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

// ─── Naked entry trampoline ───────────────────────────────────────────────────
//
// Stack on entry (stdcall 7 params + implicit EDI=game_info):
//   [esp+0x00] = caller_ret
//   [esp+0x04] = this
//   [esp+0x08] = display
//   [esp+0x0C] = sound
//   [esp+0x10] = gfx
//   [esp+0x14] = palette
//   [esp+0x18] = music
//   [esp+0x1C] = network
//   EDI        = game_info (implicit, must not be modified)
//
// Steps:
//   1. Capture EDI → GAME_INFO via EAX scratch (EDI itself is never written).
//   2. Pop caller_ret → SAVED_RET.
//   3. calll ctor_impl — args are in place; cdecl so callee doesn't clean stack.
//   4. addl $0x1C, %esp — simulate stdcall callee cleanup of 7 args.
//   5. jmpl *SAVED_RET — return to caller; EAX = this (ctor_impl's return value).
#[unsafe(naked)]
unsafe extern "C" fn ddgamewrapper_constructor() {
    core::arch::naked_asm!(
        // Use EAX as scratch to save EDI without touching EDI.
        "pushl %eax",
        // [esp+0]=old_eax, [esp+4]=caller_ret, [esp+8]=this, ..., [esp+20]=network
        "movl %edi, %eax",
        "movl %eax, {game_info}",    // GAME_INFO = EDI
        "popl %eax",                 // restore EAX; stack = [caller_ret, this, ..., network]
        "popl %eax",                 // EAX = caller_ret; stack = [this, display, ..., network]
        "movl %eax, {saved_ret}",    // SAVED_RET = caller_ret
        "calll {impl_fn}",           // ctor_impl(this, display, sound, gfx, palette, music, network)
        // cdecl: stack unchanged after call; EAX = this.
        "addl $0x1c, %esp",          // stdcall callee-cleanup: discard 7 × u32 args
        "jmpl *{saved_ret}",         // return to caller; EAX = this
        game_info = sym GAME_INFO,
        saved_ret = sym SAVED_RET,
        impl_fn   = sym ctor_impl,
        options(att_syntax),
    );
}

/// Called by `impl_init_hardware` to construct the DDGameWrapper in-place.
///
/// Sets `GAME_INFO` (read by `ctor_impl`) and delegates directly to `ctor_impl`,
/// bypassing the naked entry trampoline which is designed as a hook target, not a
/// callable subroutine.
pub(crate) unsafe fn construct_ddgame_wrapper(
    game_info: *mut u8,
    this: *mut DDGameWrapper,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    keyboard: *mut u8,
    palette: *mut Palette,
    streaming_audio: *mut u8,
    input_ctrl: *mut u8,
) -> *mut DDGameWrapper {
    GAME_INFO = game_info as u32;
    ctor_impl(this, display, sound, keyboard, palette, streaming_audio, input_ctrl)
}

pub fn install() -> Result<(), String> {
    unsafe {
        INIT_REPLAY_ADDR = rb(va::DDGAMEWRAPPER_INIT_REPLAY);
        DDGAME_CTOR_ADDR = rb(va::CONSTRUCT_DD_GAME);
        // Full replacement — trampoline not needed.
        let _ = hook::install(
            "DDGameWrapper__Constructor",
            va::CONSTRUCT_DD_GAME_WRAPPER,
            ddgamewrapper_constructor as *const (),
        )?;
    }
    Ok(())
}
