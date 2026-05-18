//! MinHook helper for installing inline hooks on WA.exe functions.

/// Walk the EBP chain and log a symbolicated stack trace.
///
/// `ebp` is the current frame pointer; the function walks up to 12 frames,
/// resolving return addresses via the address registry.
pub unsafe fn log_stack_trace(name: &str, ebp: u32) {
    unsafe {
        use openwa_game::address::va;
        use openwa_game::rebase::rb;
        let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);
        let wa_base = rb(va::IMAGE_BASE);

        let mut trace = String::new();
        let mut ebp = ebp;
        for depth in 0..12 {
            if !(0x10000..=0x7FFE0000).contains(&ebp) || (ebp & 3) != 0 {
                break;
            }
            if !openwa_game::mem::can_read(ebp, 8) {
                break;
            }
            let ret_addr = *((ebp + 4) as *const u32);
            let next_ebp = *(ebp as *const u32);
            let ghidra_ret = ret_addr.wrapping_sub(delta);
            if depth > 0 {
                trace.push_str("<-");
            }
            let in_wa = ret_addr >= wa_base && ret_addr < wa_base + 0x300000;
            if in_wa {
                trace.push_str(&openwa_game::registry::format_va(ghidra_ret));
            } else {
                use core::fmt::Write;
                let _ = write!(trace, "r:{:08X}", ret_addr);
            }
            if next_ebp <= ebp {
                break;
            }
            ebp = next_ebp;
        }

        let _ = crate::log_line(&format!("[TRAP] {} stack=[{}]", name, trace));
    }
}

/// Install a panic trap on a fully-converted WA function.
///
/// Use this when ALL callers of a WA function have been replaced with Rust code.
/// The trap verifies our caller analysis by panicking if WA.exe unexpectedly
/// calls the function. Each invocation generates a unique trap function with
/// the function name baked into the panic message, plus a stack trace.
///
/// ```ignore
/// install_trap!("GameRuntime__Constructor", va::CONSTRUCT_GAME_RUNTIME);
/// ```
macro_rules! install_trap {
    ($name:literal, $addr:expr_2021) => {{
        unsafe extern "C" fn trap() {
            unsafe {
                let ebp: u32;
                core::arch::asm!("mov {}, ebp", out(reg) ebp);
                hook::log_stack_trace($name, ebp);
                panic!(concat!(
                    "TRAP: ",
                    $name,
                    " called by WA.exe — all callers should be Rust"
                ));
            }
        }
        let _ = hook::install(concat!($name, " [TRAP]"), $addr, trap as *const ())?;
    }};
}

pub(crate) use install_trap;

use std::ffi::c_void;

use minhook::MinHook;

use openwa_game::rebase::rb;

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
    unsafe {
        let target = rb(ghidra_addr) as *mut c_void;
        let detour = detour as *mut c_void;

        let trampoline = MinHook::create_hook(target, detour)
            .map_err(|e| format!("MinHook create_hook failed for {name}: {e}"))?;

        MinHook::queue_enable_hook(target)
            .map_err(|e| format!("MinHook queue_enable_hook failed for {name}: {e}"))?;

        Ok(trampoline)
    }
}
