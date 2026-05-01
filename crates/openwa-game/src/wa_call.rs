//! Helpers for calling WA.exe functions by Ghidra address.
//!
//! These handle ASLR rebasing and calling convention translation.

use crate::rebase::rb;

/// Read a u32 global variable at a Ghidra address.
#[inline]
pub unsafe fn read_global(ghidra_addr: u32) -> u32 {
    unsafe { *(rb(ghidra_addr) as *const u32) }
}

/// Call a thiscall method (ECX = this) with 1 stack argument.
/// Uses the fastcall trick: ECX = this, EDX = dummy.
#[inline]
pub unsafe fn thiscall_1(ghidra_addr: u32, this: u32, arg1: u32) {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32, u32, u32) = core::mem::transmute(rb(ghidra_addr));
        f(this, 0, arg1);
    }
}

/// Call a thiscall method (ECX = this) with 1 stack argument, returning u32.
#[inline]
pub unsafe fn thiscall_1_ret(ghidra_addr: u32, this: u32, arg1: u32) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32, u32, u32) -> u32 =
            core::mem::transmute(rb(ghidra_addr));
        f(this, 0, arg1)
    }
}

/// Call a thiscall method via an indirect pointer (vtable entry).
/// Reads the function pointer from `vtable_slot_addr` then calls it.
#[inline]
pub unsafe fn thiscall_indirect_1(vtable_slot_addr: u32, this: u32, arg1: u32) {
    unsafe {
        let fn_ptr = *(vtable_slot_addr as *const u32);
        let f: unsafe extern "fastcall" fn(u32, u32, u32) = core::mem::transmute(fn_ptr);
        f(this, 0, arg1);
    }
}

/// Call a stdcall function with 2 arguments.
#[inline]
pub unsafe fn stdcall_2(ghidra_addr: u32, arg1: u32, arg2: u32) {
    unsafe {
        let f: unsafe extern "stdcall" fn(u32, u32) = core::mem::transmute(rb(ghidra_addr));
        f(arg1, arg2);
    }
}

/// Bridge: stdcall(this, stack_param) with ECX preset to `ecx_val`.
///
/// Used to invoke MSVC constructors that read an additional context pointer
/// from ECX on top of the normal stdcall stack args.
#[unsafe(naked)]
pub unsafe extern "C" fn call_ctor_with_ecx(
    _this: *mut u8,
    _stack_param: u32,
    _ecx_val: u32,
    _target: u32,
) -> *mut u8 {
    core::arch::naked_asm!(
        // Stack: [ret_addr] [this] [stack_param] [ecx_val] [target]
        "movl 12(%esp), %ecx", // ECX = ecx_val
        "movl 16(%esp), %eax", // EAX = target address
        "pushl 8(%esp)",       // push stack_param (now at esp+12 due to push)
        "pushl 8(%esp)",       // push this (now at esp+12 due to two pushes)
        "calll *%eax",         // call target (stdcall cleans 8 bytes)
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(ESI=ptr) with one stack arg, plain RET 0x4.
#[unsafe(naked)]
pub unsafe extern "C" fn call_usercall_esi_stack1(
    _ptr: *mut u8,
    _stack_param: u32,
    _target: u32,
) -> *mut u8 {
    core::arch::naked_asm!(
        // [ret@0] [ptr@4] [stack_param@8] [target@12]
        "pushl %esi",          // save ESI (callee-saved)
        "movl 8(%esp), %esi",  // ESI = ptr
        "movl 16(%esp), %eax", // EAX = target
        "pushl 12(%esp)",      // push stack_param
        "calll *%eax",         // stdcall cleans 4 bytes
        "popl %esi",           // restore ESI
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(EAX=eax_val, ESI=esi_val), no stack args, plain RET.
#[unsafe(naked)]
pub unsafe extern "C" fn call_usercall_eax_esi(_eax_val: u32, _esi_val: u32, _target: u32) {
    core::arch::naked_asm!(
        // [ret@0] [eax_val@4] [esi_val@8] [target@12]
        "pushl %esi",
        "movl 8(%esp), %eax",  // EAX = eax_val
        "movl 12(%esp), %esi", // ESI = esi_val
        "movl 16(%esp), %ecx", // ECX (scratch) = target
        "calll *%ecx",
        "popl %esi",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(ESI=esi_val), no stack args, plain RET.
#[unsafe(naked)]
pub unsafe extern "C" fn call_usercall_esi(_esi_val: u32, _target: u32) {
    core::arch::naked_asm!(
        // [ret@0] [esi_val@4] [target@8]
        "pushl %esi",
        "movl 8(%esp), %esi",
        "movl 12(%esp), %eax",
        "calll *%eax",
        "popl %esi",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(EDI=edi_val), no stack args, plain RET.
#[unsafe(naked)]
pub unsafe extern "C" fn call_usercall_edi(_edi_val: u32, _target: u32) {
    core::arch::naked_asm!(
        "pushl %edi",
        "movl 8(%esp), %edi",
        "movl 12(%esp), %eax",
        "calll *%eax",
        "popl %edi",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(EDI=edi_val) + 2 stack args, RET 0x8.
#[unsafe(naked)]
pub unsafe extern "C" fn call_usercall_edi_stack2(
    _edi_val: u32,
    _stack1: u32,
    _stack2: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        // [ret@0] [edi@4] [s1@8] [s2@12] [target@16]
        "pushl %edi",
        "movl 8(%esp), %edi",  // EDI = edi_val
        "movl 20(%esp), %eax", // EAX = target
        "pushl 16(%esp)",      // push stack2
        "pushl 16(%esp)",      // push stack1 (now displaced by 4)
        "calll *%eax",         // stdcall RET 0x8 cleans both
        "popl %edi",
        "retl",
        options(att_syntax),
    );
}
