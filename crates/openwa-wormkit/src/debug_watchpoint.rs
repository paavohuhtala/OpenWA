//! Hardware watchpoint debugger for DDGame constructor field investigation.
//!
//! Uses x86 debug registers (DR0–DR3) to watch DDGame offsets for writes.
//! A Vectored Exception Handler logs the exact instruction address (as Ghidra VA)
//! of every write to the watched offsets.
//!
//! Activated by `OPENWA_VALIDATE=1`. No external debugger needed — the DLL
//! instruments itself by triggering INT3 exceptions whose VEH handler sets
//! the debug registers directly in the thread context.
//!
//! ## How it works
//!
//! 1. `prepare()` registers a first-chance VEH.
//! 2. `on_ddgame_alloc()` is called right after DDGame's `wa_malloc`. It stores
//!    the base address, sets state to `Arming`, and executes `int3`.
//! 3. The VEH catches `EXCEPTION_BREAKPOINT`, writes the target addresses into
//!    DR0–DR3 and the enable/condition bits into DR7 (write-only, 4-byte),
//!    advances EIP past the `int3`, and returns `EXCEPTION_CONTINUE_EXECUTION`.
//!    The CPU loads the modified context — watchpoints are now live.
//! 4. When a watched address is written, the CPU raises `STATUS_SINGLE_STEP`.
//!    The VEH logs the writer's instruction address as a Ghidra VA.
//! 5. `teardown()` fires another `int3` whose handler clears all DR registers,
//!    then removes the VEH.

use crate::log_line;
use openwa_core::address::va;
use openwa_core::rebase::rb;
use std::sync::atomic::{AtomicU32, Ordering};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, RemoveVectoredExceptionHandler, EXCEPTION_POINTERS,
};

/// Offsets to watch. Hardware limit: 4 watchpoints (DR0–DR3).
/// Change these to investigate different fields.
/// NOTE: The base pointer is set by `on_ddgame_alloc()` — can be DDGame or DDGameWrapper.
const WATCH_OFFSETS: [(u32, &str); 1] = [
    (0x3A40, "display+0x3A40"),
];

/// DDGame base address (set when allocation is reported).
static DDGAME_BASE: AtomicU32 = AtomicU32::new(0);

/// VEH handle for cleanup.
static mut VEH_HANDLE: *mut core::ffi::c_void = core::ptr::null_mut();

/// State machine driven by intentional INT3 exceptions.
#[derive(PartialEq)]
enum Phase {
    Idle,
    Arming,
    Active,
    Disarming,
}
static mut PHASE: Phase = Phase::Idle;

/// Saved original DR values for restoration.
static mut SAVED_DR: [u32; 6] = [0; 6]; // DR0, DR1, DR2, DR3, DR6, DR7

// Exception codes (NTSTATUS = i32)
const STATUS_BREAKPOINT: i32 = 0x80000003u32 as i32;
const STATUS_SINGLE_STEP: i32 = 0x80000004u32 as i32;

// ─── VEH Handler ────────────────────────────────────────────────────────────

/// Vectored Exception Handler.
///
/// Handles two exception types:
/// - `EXCEPTION_BREAKPOINT` (INT3): arm or disarm debug registers via context
/// - `STATUS_SINGLE_STEP` (DR hit): log the instruction that wrote to a watched offset
///
/// Data breakpoints are *traps* — EIP points to the instruction **after** the
/// write. The logged Ghidra VA is therefore one instruction past the actual
/// writer. Look at the preceding instruction in Ghidra.
unsafe extern "system" fn veh_handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let ei = &*info;
    let rec = &*ei.ExceptionRecord;
    let ctx = &mut *ei.ContextRecord;

    let phase = core::ptr::read_volatile(&raw const PHASE);
    match (rec.ExceptionCode, phase) {
        // ── INT3: arm watchpoints ──
        (STATUS_BREAKPOINT, Phase::Arming) => {
            // Save original debug registers so we can restore them later
            SAVED_DR = [ctx.Dr0, ctx.Dr1, ctx.Dr2, ctx.Dr3, ctx.Dr6, ctx.Dr7];

            let base = DDGAME_BASE.load(Ordering::Relaxed);
            let dr_regs = [&mut ctx.Dr0, &mut ctx.Dr1, &mut ctx.Dr2, &mut ctx.Dr3];
            for (i, dr) in dr_regs.into_iter().enumerate() {
                if i < WATCH_OFFSETS.len() {
                    *dr = base + WATCH_OFFSETS[i].0;
                }
            }
            ctx.Dr6 = 0;
            ctx.Dr7 = dr7_for_count(WATCH_OFFSETS.len());

            PHASE = Phase::Active;
            ctx.Eip = ctx.Eip.wrapping_add(1); // skip INT3 (1 byte)
            -1 // EXCEPTION_CONTINUE_EXECUTION
        }

        // ── INT3: disarm watchpoints ──
        (STATUS_BREAKPOINT, Phase::Disarming) => {
            ctx.Dr0 = SAVED_DR[0];
            ctx.Dr1 = SAVED_DR[1];
            ctx.Dr2 = SAVED_DR[2];
            ctx.Dr3 = SAVED_DR[3];
            ctx.Dr6 = SAVED_DR[4];
            ctx.Dr7 = SAVED_DR[5];

            PHASE = Phase::Idle;
            ctx.Eip = ctx.Eip.wrapping_add(1);
            -1
        }

        // ── Hardware watchpoint hit ──
        (STATUS_SINGLE_STEP, Phase::Active) => {
            let dr6 = ctx.Dr6;
            let eip = ctx.Eip;
            let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);
            let wa_base = rb(va::IMAGE_BASE);

            let base = DDGAME_BASE.load(Ordering::Relaxed);
            for (i, &(offset, name)) in WATCH_OFFSETS.iter().enumerate() {
                if dr6 & (1 << i) != 0 {
                    let val = *((base + offset) as *const u32);
                    let ghidra_eip = eip.wrapping_sub(delta);

                    // Walk EBP chain for stack trace
                    let mut trace = heapless::String::<512>::new();
                    let mut ebp = ctx.Ebp;
                    let esp = ctx.Esp;
                    for depth in 0..12 {
                        // Validate EBP: must be in plausible stack range, aligned,
                        // and above ESP (stack grows down)
                        if ebp < 0x10000 || ebp > 0x7FFE0000 || (ebp & 3) != 0 {
                            break;
                        }
                        // Safety: check both [ebp] and [ebp+4] are readable
                        if !openwa_core::mem::can_read(ebp, 8) {
                            break;
                        }
                        let ret_addr = *((ebp + 4) as *const u32);
                        let next_ebp = *(ebp as *const u32);
                        let ghidra_ret = ret_addr.wrapping_sub(delta);
                        let in_wa = ret_addr >= wa_base && ret_addr < wa_base + 0x300000;
                        if depth > 0 {
                            let _ = core::fmt::Write::write_str(&mut trace, "<-");
                        }
                        if in_wa {
                            let _ = core::fmt::Write::write_fmt(
                                &mut trace,
                                format_args!("{:08X}", ghidra_ret),
                            );
                        } else {
                            let _ = core::fmt::Write::write_fmt(
                                &mut trace,
                                format_args!("r:{:08X}", ret_addr),
                            );
                        }
                        // EBP must increase (frames go up the stack)
                        if next_ebp <= ebp {
                            break;
                        }
                        ebp = next_ebp;
                    }

                    let _ = log_line(&format!(
                        "[Watchpoint] {} = 0x{:08X}  eip=0x{:08X} stack=[{}]",
                        name, val, ghidra_eip, trace,
                    ));
                }
            }

            ctx.Dr6 = 0;
            -1
        }

        // Not ours — pass to next handler
        _ => 0, // EXCEPTION_CONTINUE_SEARCH
    }
}

/// Compute DR7 value for `n` write-only, 4-byte, locally-enabled watchpoints.
///
/// Per breakpoint `i` (0–3):
/// - Bit `2*i`: local enable (L0–L3)
/// - Bits `16 + 4*i`: condition = 01 (write only)
/// - Bits `18 + 4*i`: length = 11 (4 bytes)
fn dr7_for_count(n: usize) -> u32 {
    let mut dr7 = 0u32;
    for i in 0..n.min(4) {
        dr7 |= 1 << (i * 2); // Local enable
        dr7 |= 0b01 << (16 + i * 4); // Condition: write-only
        dr7 |= 0b11 << (18 + i * 4); // Length: 4 bytes
    }
    dr7
}

// ─── wa_malloc hook (for original constructor instrumentation) ───────────────

/// Size of DDGame allocation — the original allocates 0x98D8 (0x98B8 + 0x20
/// overhead from alloca_probe/SEH), then memsets 0x98B8 of it.
const DDGAME_ALLOC_SIZE: u32 = 0x98D8;

/// Original wa_malloc trampoline (set by minhook).
unsafe extern "cdecl" fn malloc_trampoline_stub(_: u32) -> *mut u8 {
    core::ptr::null_mut()
}
static mut MALLOC_TRAMPOLINE: unsafe extern "cdecl" fn(u32) -> *mut u8 = malloc_trampoline_stub;

/// Hooked wa_malloc — intercepts the DDGame allocation to arm watchpoints.
unsafe extern "cdecl" fn malloc_hook(size: u32) -> *mut u8 {
    let result = MALLOC_TRAMPOLINE(size);
    if size == DDGAME_ALLOC_SIZE && !result.is_null() {
        let _ = log_line(&format!(
            "[Watchpoint] Intercepted wa_malloc(0x{:X}) = 0x{:08X}",
            size,
            result as u32,
        ));
        on_ddgame_alloc(result);
    }
    result
}

/// Whether the wa_malloc hook is currently installed.
static mut MALLOC_HOOKED: bool = false;

/// Install a minhook on wa_malloc (0x5C0AE3) to intercept the DDGame allocation.
unsafe fn hook_wa_malloc() {
    use minhook::MinHook;
    let target = rb(va::WA_MALLOC) as *mut core::ffi::c_void;
    let detour = malloc_hook as *mut core::ffi::c_void;
    match MinHook::create_hook(target, detour) {
        Ok(trampoline) => {
            MALLOC_TRAMPOLINE = core::mem::transmute(trampoline);
            if let Err(e) = MinHook::enable_hook(target) {
                let _ = log_line(&format!("[Watchpoint] MH_EnableHook failed: {:?}", e));
            } else {
                MALLOC_HOOKED = true;
                let _ = log_line("[Watchpoint] wa_malloc hooked for DDGame interception");
            }
        }
        Err(e) => {
            let _ = log_line(&format!("[Watchpoint] MH_CreateHook failed: {:?}", e));
        }
    }
}

/// Remove the wa_malloc hook.
unsafe fn unhook_wa_malloc() {
    if !MALLOC_HOOKED {
        return;
    }
    use minhook::MinHook;
    let target = rb(va::WA_MALLOC) as *mut core::ffi::c_void;
    let _ = MinHook::disable_hook(target);
    let _ = MinHook::remove_hook(target);
    MALLOC_HOOKED = false;
    let _ = log_line("[Watchpoint] wa_malloc hook removed");
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Register the VEH. Call before `create_ddgame`.
pub unsafe fn prepare() {
    VEH_HANDLE = AddVectoredExceptionHandler(1, Some(veh_handler));
    let _ = log_line("[Watchpoint] VEH installed, awaiting DDGame allocation");
}

/// Register the VEH and hook wa_malloc to intercept the DDGame allocation.
/// Use this when instrumenting the **original** WA constructor.
pub unsafe fn prepare_with_malloc_hook() {
    VEH_HANDLE = AddVectoredExceptionHandler(1, Some(veh_handler));
    let _ = log_line("[Watchpoint] VEH installed");
    hook_wa_malloc();
}

/// Called right after DDGame `wa_malloc`. Arms the watchpoints via INT3 → VEH.
///
/// When this function returns, all 4 hardware watchpoints are live on the
/// current thread.
pub unsafe fn on_ddgame_alloc(ddgame: *mut u8) {
    let base = ddgame as u32;
    DDGAME_BASE.store(base, Ordering::Relaxed);

    let _ = log_line(&format!(
        "[Watchpoint] DDGame at 0x{:08X}, arming watchpoints on: {}",
        base,
        WATCH_OFFSETS
            .iter()
            .map(|(off, name)| format!("+0x{:04X}({})", off, name))
            .collect::<Vec<_>>()
            .join(", "),
    ));

    PHASE = Phase::Arming;
    // INT3 triggers our VEH which sets DR0–DR3 and DR7 in the thread context.
    // When execution resumes here, the watchpoints are active.
    core::arch::asm!("int3");
    let _ = log_line("[Watchpoint] Armed!");
}

/// Disarm watchpoints, remove VEH, and unhook wa_malloc if needed.
pub unsafe fn teardown() {
    unhook_wa_malloc();

    if PHASE == Phase::Active {
        PHASE = Phase::Disarming;
        core::arch::asm!("int3");
    }

    if !VEH_HANDLE.is_null() {
        RemoveVectoredExceptionHandler(VEH_HANDLE);
        VEH_HANDLE = core::ptr::null_mut();
    }

    let _ = log_line("[Watchpoint] Disarmed and VEH removed");
}
