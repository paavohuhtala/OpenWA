//! Game session lifecycle hook.
//!
//! Passthrough hook on `DDGameWrapper__Constructor` (0x56DEF0) that fires once
//! per game session start, logging the `DDGameWrapper` and `DDGame` addresses.
//!
//! ## Calling convention notes
//!
//! `DDGameWrapper__Constructor` is `__stdcall` with 7 stack params, BUT it also
//! uses `EDI` as an **implicit register param** (Ghidra: `unaff_EDI`), which it
//! passes as the 9th argument to `DDGame__Constructor`. A regular `extern "stdcall"`
//! hook function would clobber EDI before calling the original (the Rust compiler
//! uses it as a scratch register), causing a null-pointer crash deep in DDGame init.
//!
//! The naked trampoline below avoids touching EDI entirely:
//! 1. Swaps the caller's return address with our continuation label.
//! 2. Tail-calls the original via `jmp` (not `call`) — EDI is unchanged.
//! 3. After the original returns (via our continuation), calls a cdecl logger.
//! 4. Jumps back to the real caller.

use openwa_core::address::va;
use crate::hook;
use crate::log_line;

/// Trampoline address of the original DDGameWrapper__Constructor.
/// Set by `install()` via MinHook, read by the naked trampoline.
static mut ORIG_VAL: u32 = 0;

/// Caller's return address, saved by the naked trampoline before the tail-call
/// to orig so we can return there after the logger runs.
/// Non-reentrant (single-threaded game init; DDGameWrapper::Constructor is
/// called at most once per game session).
static mut SAVED_RET: u32 = 0;

/// Called from the naked trampoline after the original constructor returns.
/// `wrapper` = the DDGameWrapper pointer (orig's return value = `this`).
unsafe extern "cdecl" fn on_ddgame_wrapper_created(wrapper: u32) {
    // DDGame* lives at DDGameWrapper+0x488
    let ddgame = *(wrapper as *const u32).add(0x488 / 4);
    let _ = log_line(&format!(
        "[GameSession] DDGameWrapper ctor: wrapper=0x{wrapper:08X}  ddgame=0x{ddgame:08X}"
    ));
}

/// Naked trampoline for `DDGameWrapper__Constructor`.
///
/// Stack layout on entry (stdcall 7 params):
/// ```text
/// [esp+00] = caller_ret
/// [esp+04] = this  (*mut DDGameWrapper)
/// [esp+08] = display
/// [esp+0C] = sound
/// [esp+10] = gfx
/// [esp+14] = palette
/// [esp+18] = music
/// [esp+1C] = network
/// EDI      = implicit param (passed down to DDGame__Constructor as 9th arg)
/// ```
#[unsafe(naked)]
unsafe extern "C" fn hook_ddgame_wrapper_ctor() {
    core::arch::naked_asm!(
        // --- Save EAX and swap return address ---
        "pushl %eax",
        // Stack: [esp+0]=old_eax, [esp+4]=caller_ret, [esp+8]=this, ..., [esp+20]=network
        "movl 4(%esp), %eax",          // eax = caller_ret
        "movl %eax, {ret}",            // save caller_ret → SAVED_RET
        "movl $1f, %eax",              // eax = address of our continuation (AT&T: $ = immediate)
        "movl %eax, 4(%esp)",          // replace caller_ret with continuation address
        "popl %eax",                   // restore eax; stack: [cont, this, ..., network]
        // Stack is now exactly: [esp+0]=continuation, [esp+4]=this, ..., [esp+1C]=network
        // EDI is unchanged — the original function will read it directly.

        // --- Tail-call to original (jmp, not call, to avoid pushing a return address) ---
        "movl {orig}, %eax",           // eax = trampoline address (value at ORIG_VAL)
        "jmpl *%eax",

        // --- Continuation: entered when orig does its `ret 0x1C` ---
        // At this point:
        //   EAX = wrapper pointer (orig's return value)
        //   ESP = caller's stack (orig's ret 0x1C popped 7 params + return addr)
        "1:",
        "pushl %eax",                  // save wrapper (return value)
        "pushl %eax",                  // arg: wrapper ptr → on_ddgame_wrapper_created
        "calll {log_fn}",
        "addl $4, %esp",               // clean cdecl arg
        "popl %eax",                   // restore wrapper → EAX (caller's expected return value)
        "jmpl *{ret}",                 // jump to saved caller_ret

        orig = sym ORIG_VAL,
        ret = sym SAVED_RET,
        log_fn = sym on_ddgame_wrapper_created,
        options(att_syntax),
    );
}

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline = hook::install(
            "DDGameWrapper__Constructor",
            va::CONSTRUCT_DD_GAME_WRAPPER,
            hook_ddgame_wrapper_ctor as *const (),
        )?;
        ORIG_VAL = trampoline as u32;
    }
    Ok(())
}
