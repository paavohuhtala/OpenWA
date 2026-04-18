//! Passthrough param logging for `DDGameWrapper__SetupFrameParams` (0x534CA0)
//! and `DDGameWrapper__AdvanceFrameCounters` (0x52AAA0).
//!
//! Installed only when `OPENWA_DISPATCH_TRACE=1` is set. Logs every call's
//! `this` + stack params to a dedicated file so the Rust port can be compared
//! against the original (`OPENWA_DISPATCH_ORIGINAL=1`) per-call.
//!
//! Both targets are `__usercall` + `stdcall`; we install a naked passthrough
//! that preserves full register context, calls a small cdecl logger, then
//! `JMP`s to the MinHook trampoline so the original runs with its
//! `RET N` returning directly to the WA caller.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::hook;
use crate::log_line;
use openwa_game::address::va;
use openwa_game::engine::ddgame_wrapper::DDGameWrapper;

static ORIG_AFC: AtomicU32 = AtomicU32::new(0);
static ORIG_SFP: AtomicU32 = AtomicU32::new(0);

static TRACE_LOG: Mutex<Option<std::io::BufWriter<std::fs::File>>> = Mutex::new(None);

/// Current frame counter for tagging log lines. Reads DDGame.frame_counter
/// via the wrapper pointer passed to the hook.
#[inline]
unsafe fn frame_counter(wrapper: u32) -> i32 {
    unsafe {
        if wrapper == 0 {
            return -1;
        }
        let w = wrapper as *const DDGameWrapper;
        let ddgame = (*w).ddgame;
        if ddgame.is_null() {
            return -1;
        }
        (*ddgame).frame_counter
    }
}

unsafe extern "C" fn log_afc(this: u32, p1: i32, p2: i32, p3: i32, p4: i32, p5: u32) {
    unsafe {
        let fc = frame_counter(this);
        if let Ok(mut guard) = TRACE_LOG.lock() {
            if let Some(w) = guard.as_mut() {
                let _ = writeln!(
                    w,
                    "fc={:>5} AFC p1={:>6} p2=0x{:08X} p3={:>6} p4=0x{:08X} p5=0x{:08X}",
                    fc, p1, p2 as u32, p3, p4 as u32, p5
                );
            }
        }
    }
}

unsafe extern "C" fn log_sfp(this: u32, p1: i32, p2: i32, p3: i32) {
    unsafe {
        let fc = frame_counter(this);
        if let Ok(mut guard) = TRACE_LOG.lock() {
            if let Some(w) = guard.as_mut() {
                let _ = writeln!(
                    w,
                    "fc={:>5} SFP p1={:>6} p2={:>6} p3=0x{:08X}",
                    fc, p1, p2, p3 as u32
                );
            }
        }
    }
}

// AdvanceFrameCounters: usercall ESI=this, 5 stdcall params, RET 0x14.
//
// After MinHook's JMP detours here, stack is [retaddr, p1..p5], ESI=this.
// We PUSHAD (32 bytes), then stack becomes:
//   [esp+0x00..+0x1C]  saved GPRs
//   [esp+0x20]          retaddr
//   [esp+0x24..+0x34]   p1..p5
// Push cdecl args (p5 down to this), call logger, cleanup, POPAD, and
// tail-JMP into the trampoline (which holds the overwritten prologue bytes
// plus a JMP back into AFC's body). The original's RET 0x14 returns to
// the WA caller.
#[unsafe(naked)]
unsafe extern "C" fn hook_afc() {
    core::arch::naked_asm!(
        "pushad",
        "push dword ptr [esp+0x34]",  // p5
        "push dword ptr [esp+0x34]",  // p4 (prev p4 slot post-push)
        "push dword ptr [esp+0x34]",  // p3
        "push dword ptr [esp+0x34]",  // p2
        "push dword ptr [esp+0x34]",  // p1
        "push esi",                    // this
        "call {log_fn}",
        "add esp, 24",
        "popad",
        "jmp dword ptr [{orig}]",
        log_fn = sym log_afc,
        orig = sym ORIG_AFC,
    );
}

// SetupFrameParams: usercall EAX=this, 3 stdcall params, RET 0xC.
// After PUSHAD (32 bytes) and retaddr, p1..p3 are at [esp+0x24..+0x2C].
// EAX still holds `this` at this point (PUSHAD saved it but doesn't clear
// live registers, and `push [mem]` doesn't touch GPRs), so we push it
// directly instead of reading back the saved copy.
#[unsafe(naked)]
unsafe extern "C" fn hook_sfp() {
    core::arch::naked_asm!(
        "pushad",
        "push dword ptr [esp+0x2C]",       // p3
        "push dword ptr [esp+0x2C]",       // p2
        "push dword ptr [esp+0x2C]",       // p1
        "push eax",                         // this
        "call {log_fn}",
        "add esp, 16",
        "popad",
        "jmp dword ptr [{orig}]",
        log_fn = sym log_sfp,
        orig = sym ORIG_SFP,
    );
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_DISPATCH_TRACE").is_err() {
        return Ok(());
    }

    let path = std::env::var("OPENWA_DISPATCH_TRACE_PATH")
        .unwrap_or_else(|_| "dispatch_trace.log".to_string());

    let file = std::fs::File::create(&path)
        .map_err(|e| format!("Failed to create dispatch trace log {path}: {e}"))?;
    *TRACE_LOG.lock().unwrap() = Some(std::io::BufWriter::new(file));

    let mode = if std::env::var("OPENWA_DISPATCH_ORIGINAL").is_ok() {
        "original"
    } else {
        "rust"
    };
    let _ = log_line(&format!(
        "[DispatchTrace] Logging AFC/SFP params to {path} (dispatch_frame mode: {mode})"
    ));

    unsafe {
        let afc = hook::install(
            "DDGameWrapper__AdvanceFrameCounters",
            va::DDGAMEWRAPPER_ADVANCE_FRAME_COUNTERS,
            hook_afc as *const (),
        )?;
        ORIG_AFC.store(afc as u32, Ordering::Relaxed);

        let sfp = hook::install(
            "DDGameWrapper__SetupFrameParams",
            va::DDGAMEWRAPPER_SETUP_FRAME_PARAMS,
            hook_sfp as *const (),
        )?;
        ORIG_SFP.store(sfp as u32, Ordering::Relaxed);
    }

    Ok(())
}

pub fn flush() {
    if let Ok(mut guard) = TRACE_LOG.lock() {
        if let Some(writer) = guard.as_mut() {
            let _ = writer.flush();
        }
        *guard = None;
    }
}
