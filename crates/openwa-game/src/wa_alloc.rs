//! WA heap allocation utilities.

use crate::address::va;
use crate::rebase::rb;

/// Allocate `size` bytes from WA's statically-linked CRT heap.
///
/// # Safety
/// Must only be called from within the WA.exe process (game or injected DLL).
pub unsafe fn wa_malloc(size: u32) -> *mut u8 {
    let f: unsafe extern "cdecl" fn(u32) -> *mut u8 =
        core::mem::transmute(rb(va::WA_MALLOC) as usize);
    f(size)
}

/// Allocate space for a `T` on WA's heap and return a pointer to it.
///
/// Equivalent to `wa_malloc(size_of::<T>())` with a cast. The allocation is uninitialized.
///
/// # Safety
/// Must only be called from within the WA.exe process (game or injected DLL).
pub unsafe fn wa_malloc_struct<T>() -> *mut T {
    wa_malloc(core::mem::size_of::<T>() as u32) as *mut T
}

/// Allocate space for a `T` on WA's heap, zero-initialize it, and return a pointer to it.
/// Equivalent to `wa_malloc_zeroed(size_of::<T>())` with a cast. The allocation is zero-initialized.
///
/// # Safety
/// Must only be called from within the WA.exe process (game or injected DLL).
pub unsafe fn wa_malloc_struct_zeroed<T>() -> *mut T {
    wa_malloc_zeroed(core::mem::size_of::<T>() as u32) as *mut T
}

/// Allocate `size` bytes from WA's CRT heap and zero-initialize them.
///
/// Equivalent to `wa_malloc(size)` followed by `write_bytes(ptr, 0, size)`.
///
/// # Safety
/// Must only be called from within the WA.exe process.
pub unsafe fn wa_malloc_zeroed(size: u32) -> *mut u8 {
    let ptr = wa_malloc(size);
    if !ptr.is_null() {
        core::ptr::write_bytes(ptr, 0, size as usize);
    }
    ptr
}

/// Free a pointer allocated by [`wa_malloc`] (WA's statically-linked CRT `free`).
///
/// # Safety
/// `ptr` must have been returned by `wa_malloc` (or null, which is a no-op).
pub unsafe fn wa_free<T>(ptr: *mut T) {
    let f: unsafe extern "cdecl" fn(*mut u8) = core::mem::transmute(rb(va::WA_FREE) as usize);
    f(ptr as *mut u8);
}
