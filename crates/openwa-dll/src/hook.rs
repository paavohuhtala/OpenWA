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
    // EDX saved/restored: cdecl may clobber it, but original __usercall may not
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 4",
                "pop edx",
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
                "push edx",
                "push [esp+8]",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 8",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 1 register arg (ECX), 1 stack param, ret N — preserves ECX across call.
    // Callers of thiscall functions may rely on ECX being preserved even though
    // the ABI says it's caller-saved (MSVC often generates loops that don't
    // re-set ECX between calls if the callee happens to preserve it).
    (fn $name:ident; impl_fn = $impl:path; reg = ecx;
     stack_params = 1; ret_bytes = $ret:literal; preserve_ecx) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push ecx",                // save ECX for restoration
                "push [esp+12]",           // stack param (shifted +4 by extra push)
                "push ecx",               // cdecl arg (ECX still valid here)
                "call {impl_fn}",
                "add esp, 8",
                "pop ecx",                // restore ECX
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 1 register arg, 2 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident;
     stack_params = 2; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+12]",
                "push [esp+12]",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 12",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 1 register arg, 3 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident;
     stack_params = 3; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+16]",
                "push [esp+16]",
                "push [esp+16]",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 16",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 1 register arg, 4 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident;
     stack_params = 4; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+20]",
                "push [esp+20]",
                "push [esp+20]",
                "push [esp+20]",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 20",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 1 register arg, 5 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; reg = $reg:ident;
     stack_params = 5; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+24]",
                "push [esp+24]",
                "push [esp+24]",
                "push [esp+24]",
                "push [esp+24]",
                concat!("push ", stringify!($reg)),
                "call {impl_fn}",
                "add esp, 24",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 1 stack param, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 1; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+8]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 12",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 2 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 2; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+12]",
                "push [esp+12]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 16",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 0 stack params, plain ret
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident]) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 8",
                "pop edx",
                "ret",
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 4 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 4; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+20]",
                "push [esp+20]",
                "push [esp+20]",
                "push [esp+20]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 24",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 5 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 5; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+24]",
                "push [esp+24]",
                "push [esp+24]",
                "push [esp+24]",
                "push [esp+24]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 28",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 6 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 6; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+28]",
                "push [esp+28]",
                "push [esp+28]",
                "push [esp+28]",
                "push [esp+28]",
                "push [esp+28]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 32",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 7 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 7; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+32]",
                "push [esp+32]",
                "push [esp+32]",
                "push [esp+32]",
                "push [esp+32]",
                "push [esp+32]",
                "push [esp+32]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 36",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 2 register args, 3 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident];
     stack_params = 3; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+16]",
                "push [esp+16]",
                "push [esp+16]",
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 20",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 3 register args, 2 stack params, ret N
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident, $r3:ident];
     stack_params = 2; ret_bytes = $ret:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                "push [esp+12]",
                "push [esp+12]",
                concat!("push ", stringify!($r3)),
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 20",
                "pop edx",
                concat!("ret ", $ret),
                impl_fn = sym $impl,
            );
        }
    };

    // 3 register args, 0 stack params, plain ret
    (fn $name:ident; impl_fn = $impl:path; regs = [$r1:ident, $r2:ident, $r3:ident]) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "push edx",
                concat!("push ", stringify!($r3)),
                concat!("push ", stringify!($r2)),
                concat!("push ", stringify!($r1)),
                "call {impl_fn}",
                "add esp, 12",
                "pop edx",
                "ret",
                impl_fn = sym $impl,
            );
        }
    };
}

pub(crate) use usercall_trampoline;

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
