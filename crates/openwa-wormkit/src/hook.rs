//! MinHook helper for installing inline hooks on WA.exe functions.
//!
//! Also provides the [`usercall_trampoline!`] macro for generating naked asm
//! trampolines that capture implicit register params from MSVC `__usercall`
//! functions and forward them to cdecl Rust implementations.

/// Generate a naked trampoline for a `__usercall` function.
///
/// Captures one implicit register param and optionally forwards stack params
/// to a `cdecl` Rust implementation function.
///
/// The impl_fn signature must be: `extern "cdecl" fn(reg_value, stack_args...) -> R`
///
/// # Variants
///
/// ```ignore
/// // 1 register, 0 stack params, plain ret
/// usercall_trampoline!(fn name; impl_fn = path; reg = eax);
///
/// // 1 register, 1 stack param, ret with cleanup
/// usercall_trampoline!(fn name; impl_fn = path; reg = esi;
///     stack_params = 1; ret_bytes = "0x4");
/// ```
macro_rules! usercall_trampoline {
    // 1 register arg, 0 stack params, plain ret
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 4",
                "ret",
                impl_fn = sym $impl,
            );
        }
    };

    // 1 register arg, 1 stack param, ret N
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident;
     stack_params = 1; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push [esp+4]",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 8",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };
}

pub(crate) use usercall_trampoline;

use std::ffi::c_void;

use minhook::MinHook;

use crate::log_line;
use crate::rebase::rb;

/// Install a MinHook inline hook on a WA.exe function.
///
/// - `name`: human-readable name for log/error messages
/// - `ghidra_addr`: Ghidra VA of the target function (rebased automatically)
/// - `detour`: replacement function pointer
///
/// Returns the trampoline pointer (for calling the original).
/// Caller can store it in an `AtomicU32` if needed, or discard with `let _ =`.
pub unsafe fn install(
    name: &str,
    ghidra_addr: u32,
    detour: *const (),
) -> Result<*mut c_void, String> {
    let target = rb(ghidra_addr) as *mut c_void;
    let detour = detour as *mut c_void;

    let trampoline = MinHook::create_hook(target, detour)
        .map_err(|e| format!("MinHook create_hook failed for {name}: {e}"))?;

    MinHook::enable_hook(target)
        .map_err(|e| format!("MinHook enable_hook failed for {name}: {e}"))?;

    let _ = log_line(&format!(
        "  [REPLACE] {name}: target 0x{:08X}, trampoline 0x{:08X}",
        target as u32, trampoline as u32
    ));

    Ok(trampoline)
}
