//! Registry-driven startup validation of WA.exe addresses.
//!
//! Runs unconditionally at DLL load (after hook installation). Checks that
//! all registered vtables, functions, and constructors point to valid memory
//! regions in the live binary. Fully auto-generated from `define_addresses!`
//! and `#[vtable(...)]` metadata — no hardcoded address lists.

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::rebase::rb;
use openwa_game::registry::{self, AddrKind};

/// Valid first bytes for x86 function prologues (MSVC).
/// This covers standard prologues plus common non-standard function starts.
const VALID_PROLOGUES: &[u8] = &[
    0x2B, // sub reg, r/m32 (e.g. SpriteBank__GetInfo starts with SUB EAX, [ECX+8])
    0x33, // xor reg, reg
    0x3C, // cmp al, imm8 (usercall with AL input, e.g. Font__GetMetric)
    0x51, // push ecx
    0x52, // push edx
    0x53, // push ebx
    0x55, // push ebp
    0x56, // push esi
    0x57, // push edi
    0x64, // fs: prefix (SEH setup)
    0x66, // operand-size prefix
    0x6A, // push imm8
    0x80, // cmp byte, imm8
    0x81, // sub esp, imm32
    0x83, // sub esp, imm8
    0x85, // test reg, reg
    0x89, // mov reg, reg
    0x8B, // mov reg, ...
    0x8D, // lea reg, ...
    0x0F, // two-byte opcode prefix (e.g. movzx, movsx)
    0xA1, // mov eax, [imm32]
    0xB8, // mov eax, imm32
    0xC1, // shr/shl reg, imm8
    0xE8, // call rel32
    0xE9, // jmp rel32 (MinHook trampoline — function is hooked)
    0xEB, // jmp rel8 (short jump)
    0xF6, // test byte ptr [mem], imm8 (e.g. FramePostProcessHook__Destructor's flag check)
    0xFF, // call/jmp [mem]
];

#[inline]
fn is_in_text(addr: u32) -> bool {
    addr >= rb(va::TEXT_START) && addr <= rb(va::TEXT_END)
}

#[inline]
fn is_in_rdata(addr: u32) -> bool {
    addr >= rb(va::RDATA_START) && addr < rb(va::DATA_START)
}

/// Check if an address points to executable memory (any module).
/// Uses VirtualQuery to inspect the memory protection flags.
#[inline]
fn is_executable(addr: u32) -> bool {
    use windows_sys::Win32::System::Memory::{
        MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
        PAGE_EXECUTE_WRITECOPY, VirtualQuery,
    };
    unsafe {
        let mut mbi: MEMORY_BASIC_INFORMATION = core::mem::zeroed();
        let size = VirtualQuery(
            addr as *const _,
            &mut mbi,
            core::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        );
        if size == 0 {
            return false;
        }
        matches!(
            mbi.Protect,
            PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY
        )
    }
}

/// Run all startup checks. Returns the number of failures.
pub fn run() -> u32 {
    let mut pass = 0u32;
    let mut fail = 0u32;

    // 1. Vtable location checks: all registered vtables should be in .rdata
    for entry in registry::entries_by_kind(AddrKind::Vtable) {
        let addr = rb(entry.va);
        if is_in_rdata(addr) {
            pass += 1;
        } else {
            fail += 1;
            let _ = log_line(&format!(
                "[CHECK FAIL] {} location: 0x{:08X} (ghidra 0x{:08X}) NOT in .rdata",
                entry.name, addr, entry.va
            ));
        }
    }

    // 2. Vtable first-entry checks: first slot should point to executable code.
    //    May be in WA.exe .text or in our DLL (if we replaced the vtable slot).
    for entry in registry::entries_by_kind(AddrKind::Vtable) {
        let addr = rb(entry.va);
        unsafe {
            let first_entry = *(addr as *const u32);
            if is_executable(first_entry) {
                pass += 1;
            } else {
                fail += 1;
                let _ = log_line(&format!(
                    "[CHECK FAIL] {} first entry: [0x{:08X}] = 0x{:08X} NOT executable",
                    entry.name, addr, first_entry
                ));
            }
        }
    }

    // 3. Typed vtable slot checks: named slots should point to executable code.
    //    Slots may point to WA.exe .text or to our DLL (replaced vtable methods).
    for info in registry::all_vtable_info() {
        if info.ghidra_va == 0 {
            continue;
        }
        let vt_base = rb(info.ghidra_va);
        for slot in info.slots {
            unsafe {
                let slot_addr = vt_base + slot.index * 4;
                let fn_ptr = *(slot_addr as *const u32);
                if is_executable(fn_ptr) {
                    pass += 1;
                } else {
                    fail += 1;
                    let _ = log_line(&format!(
                        "[CHECK FAIL] {}::{} [slot {}]: 0x{:08X} NOT executable",
                        info.class_name, slot.name, slot.index, fn_ptr
                    ));
                }
            }
        }
    }

    // 4. Function/constructor prologue checks
    let prologue_kinds = [AddrKind::Function, AddrKind::Constructor];
    for kind in &prologue_kinds {
        for entry in registry::entries_by_kind(*kind) {
            let addr = rb(entry.va);
            if !is_in_text(addr) {
                fail += 1;
                let _ = log_line(&format!(
                    "[CHECK FAIL] {} prologue: 0x{:08X} (ghidra 0x{:08X}) NOT in .text",
                    entry.name, addr, entry.va
                ));
                continue;
            }
            unsafe {
                let first_byte = *(addr as *const u8);
                if VALID_PROLOGUES.contains(&first_byte) {
                    pass += 1;
                } else {
                    fail += 1;
                    let _ = log_line(&format!(
                        "[CHECK FAIL] {} prologue: 0x{:08X} first byte 0x{:02X} unexpected",
                        entry.name, addr, first_byte
                    ));
                }
            }
        }
    }

    // 5. Trig table equality: the sin/cos tables embedded in openwa-core must
    //    match WA.exe's .rdata byte-for-byte. A mismatch means either the
    //    binary changed or our copy is stale — fail loudly.
    unsafe {
        match openwa_game::trig::validate_against_wa_exe() {
            Ok(()) => pass += 1,
            Err((which, idx, embedded, live)) => {
                fail += 1;
                let _ = log_line(&format!(
                    "[CHECK FAIL] trig {which} table entry {idx}: embedded 0x{embedded:08X} != WA.exe 0x{live:08X}"
                ));
            }
        }
    }

    let total = pass + fail;
    let _ = log_line(&format!("[CHECK] {pass}/{total} passed, {fail} failed"));
    fail
}
