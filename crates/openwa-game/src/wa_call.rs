//! Helpers for calling WA.exe functions by Ghidra address.
//!
//! These handle ASLR rebasing and calling convention translation.

use crate::rebase::rb;

/// Read a u32 global variable at a Ghidra address.
#[inline]
pub unsafe fn read_global(ghidra_addr: u32) -> u32 {
    *(rb(ghidra_addr) as *const u32)
}

/// Call a thiscall method (ECX = this) with 1 stack argument.
/// Uses the fastcall trick: ECX = this, EDX = dummy.
#[inline]
pub unsafe fn thiscall_1(ghidra_addr: u32, this: u32, arg1: u32) {
    let f: unsafe extern "fastcall" fn(u32, u32, u32) = core::mem::transmute(rb(ghidra_addr));
    f(this, 0, arg1);
}

/// Call a thiscall method (ECX = this) with 1 stack argument, returning u32.
#[inline]
pub unsafe fn thiscall_1_ret(ghidra_addr: u32, this: u32, arg1: u32) -> u32 {
    let f: unsafe extern "fastcall" fn(u32, u32, u32) -> u32 =
        core::mem::transmute(rb(ghidra_addr));
    f(this, 0, arg1)
}

/// Call a thiscall method via an indirect pointer (vtable entry).
/// Reads the function pointer from `vtable_slot_addr` then calls it.
#[inline]
pub unsafe fn thiscall_indirect_1(vtable_slot_addr: u32, this: u32, arg1: u32) {
    let fn_ptr = *(vtable_slot_addr as *const u32);
    let f: unsafe extern "fastcall" fn(u32, u32, u32) = core::mem::transmute(fn_ptr);
    f(this, 0, arg1);
}

/// Call a stdcall function with 2 arguments.
#[inline]
pub unsafe fn stdcall_2(ghidra_addr: u32, arg1: u32, arg2: u32) {
    let f: unsafe extern "stdcall" fn(u32, u32) = core::mem::transmute(rb(ghidra_addr));
    f(arg1, arg2);
}
