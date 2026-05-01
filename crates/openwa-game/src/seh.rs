//! Structured Exception Handling for x86 MSVC, in Rust.
//!
//! Implements a `__try { ... } __except (EXCEPTION_EXECUTE_HANDLER) { ... }`
//! equivalent for catching access violations (and similar non-recoverable
//! exceptions) from within Rust code. Used to wrap fragile WA-side code paths
//! that stock WA tolerates via its own SEH frames — most notably
//! `DSSound::Destructor` (0x00573DD0), where the `IDirectSoundBuffer::Release`
//! call on the primary buffer is unstable on some configs (driver / refcount
//! quirk) and stock WA's `__try/__except` swallows the AV.
//!
//! ## Implementation
//!
//! Pure Rust + Win32: a per-thread "current jump buffer" is set by
//! [`try_seh`] before invoking the user closure, and a process-wide
//! [Vectored Exception Handler] hooks AVs and, if the faulting thread has an
//! active jump buffer, modifies the `CONTEXT` to long-jump back to the
//! [`try_seh`] return point with `EAX=1`. The setjmp/longjmp pair are
//! implemented with `naked_asm!` to avoid relying on the absent CRT
//! `_setjmp`/`longjmp` shims.
//!
//! ## Caveats
//!
//! - **No Drop unwinding.** When an AV is caught, the closure's pending
//!   `Drop`s do *not* run. Treat the closure body as if it could `abort()`
//!   at any point — don't rely on RAII for crucial cleanup.
//! - **x86 MSVC only.** The naked asm assumes the i686 calling convention.
//! - **AV only.** Other exception codes (illegal instruction, stack overflow,
//!   etc.) propagate through unchanged. Add more cases in [`av_handler`] if
//!   needed.

use core::cell::Cell;
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::EXCEPTION_ACCESS_VIOLATION;
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH,
    EXCEPTION_POINTERS,
};

use openwa_core::log::log_line;

/// Saved register state for a setjmp / longjmp pair.
///
/// Layout matches what [`rust_setjmp`] writes and [`av_handler`] reads back
/// into the `CONTEXT` record. Order: ebx, esi, edi, ebp, esp, eip.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct JmpBuf {
    pub ebx: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebp: u32,
    pub esp: u32,
    pub eip: u32,
}

thread_local! {
    /// Current jump buffer for this thread (raw pointer to a `JmpBuf` on the
    /// caller's stack). `None` when no `try_seh` is active. Nested calls
    /// save the previous value into a local on entry, so chains compose
    /// correctly.
    static CURRENT_JMP: Cell<*mut JmpBuf> = const { Cell::new(core::ptr::null_mut()) };
}

/// VEH installed exactly once for the lifetime of the process.
static VEH_HANDLE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
static VEH_INIT: Once = Once::new();

unsafe extern "system" fn av_handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    unsafe {
        let exc = (*info).ExceptionRecord;
        if (*exc).ExceptionCode != EXCEPTION_ACCESS_VIOLATION as i32 {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let env_ptr = CURRENT_JMP.with(|c| c.get());
        if env_ptr.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        // Long-jump: restore registers + ESP + EIP from the active JmpBuf.
        let env = &*env_ptr;
        let ctx = (*info).ContextRecord;
        (*ctx).Ebx = env.ebx;
        (*ctx).Esi = env.esi;
        (*ctx).Edi = env.edi;
        (*ctx).Ebp = env.ebp;
        (*ctx).Esp = env.esp;
        (*ctx).Eip = env.eip;
        (*ctx).Eax = 1; // longjmp return value (rust_setjmp returns 0 on first entry)
        EXCEPTION_CONTINUE_EXECUTION
    }
}

fn ensure_veh_installed() {
    VEH_INIT.call_once(|| unsafe {
        let h = AddVectoredExceptionHandler(1, Some(av_handler));
        VEH_HANDLE.store(h, Ordering::Release);
        let _ = log_line("[seh] vectored exception handler installed (AV swallow)");
    });
}

/// `setjmp`-equivalent: saves callee-saved registers + ESP + return address
/// into `*env`. Returns 0 when called normally; returns 1 when reached via
/// the AV handler's long-jump.
#[unsafe(naked)]
unsafe extern "C" fn rust_setjmp(_env: *mut JmpBuf) -> u32 {
    core::arch::naked_asm!(
        // [esp+0] = return addr, [esp+4] = env
        "movl 4(%esp), %eax",  // EAX = env
        "movl %ebx, (%eax)",   // env.ebx = EBX
        "movl %esi, 4(%eax)",  // env.esi = ESI
        "movl %edi, 8(%eax)",  // env.edi = EDI
        "movl %ebp, 12(%eax)", // env.ebp = EBP
        "leal 4(%esp), %ecx",  // ECX = ESP at caller's point (after our `ret`)
        "movl %ecx, 16(%eax)", // env.esp = ECX
        "movl (%esp), %ecx",   // ECX = return address (call site)
        "movl %ecx, 20(%eax)", // env.eip = ECX
        "xorl %eax, %eax",     // return 0
        "retl",
        options(att_syntax),
    );
}

/// Run `f` under SEH protection. Returns `Ok(value)` if `f` completed
/// normally, or `Err(())` if an access violation was caught.
///
/// **Side effects on AV**: `f`'s pending `Drop`s do not run. Heap memory
/// allocated within `f` is leaked. Use only for cases where the closure body
/// is a "best-effort cleanup that may legitimately fail" — matching WA's
/// `__except (EXCEPTION_EXECUTE_HANDLER) { /* ignore */ }` pattern.
///
/// # Safety
/// Only safe on `i686-pc-windows-msvc`. The closure must not contain
/// references that need to outlive an unwinding return (e.g., locks,
/// half-initialized resources whose `Drop` is load-bearing).
#[allow(clippy::result_unit_err)]
pub unsafe fn try_seh<R>(f: impl FnOnce() -> R) -> Result<R, ()> {
    ensure_veh_installed();
    let mut env = JmpBuf::default();
    let env_ptr = &raw mut env;
    let prev = CURRENT_JMP.with(|c| c.replace(env_ptr));

    let setjmp_result = unsafe { rust_setjmp(env_ptr) };
    if setjmp_result == 0 {
        // First-pass: run the closure. If it AVs, control jumps back here
        // with EAX=1 (the `else` branch).
        let r = f();
        CURRENT_JMP.with(|c| c.set(prev));
        Ok(r)
    } else {
        // Reached by long-jump from `av_handler`.
        CURRENT_JMP.with(|c| c.set(prev));
        Err(())
    }
}
