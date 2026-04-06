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

/// An allocation from WA's heap.
///
/// Owns a pointer obtained from [`wa_malloc`]. Call [`.leak()`](WABox::leak) to transfer
/// ownership to WA code; call [`.as_ptr()`](WABox::as_ptr) to borrow.
///
/// There is **no automatic free on drop** — the WA heap is not under Rust's control.
/// If a `WABox` is dropped without being leaked, the allocation silently leaks.
/// This is intentional: all allocation sites either `.leak()` into a WA struct field or
/// reach an early-return via the C++ destructor path.
#[must_use]
pub struct WABox<T> {
    ptr: core::ptr::NonNull<T>,
}

impl<T> WABox<T> {
    /// Allocate `alloc_size` bytes from WA's heap, zeroing the first `zero_size` bytes.
    ///
    /// Panics if `wa_malloc` returns null.
    ///
    /// `zero_size` may be less than `alloc_size` when the tail of the allocation is
    /// populated by a constructor — e.g. `alloc(0x24E28, 0x24E08)` zeroes all but the
    /// last 0x20 bytes, matching the original `_memset(pvVar8, 0, 0x24e08)`.
    ///
    /// # Safety
    /// Must only be called from within the WA.exe process.
    pub unsafe fn alloc(alloc_size: u32, zero_size: u32) -> Self {
        let raw = wa_malloc(alloc_size);
        if raw.is_null() {
            panic!("wa_malloc({alloc_size}) returned null");
        }
        if zero_size > 0 {
            core::ptr::write_bytes(raw, 0, zero_size as usize);
        }
        Self {
            ptr: core::ptr::NonNull::new_unchecked(raw as *mut T),
        }
    }

    /// Return a raw pointer to the allocation without consuming `self`.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Allocate space for `T` on WA's heap and write `val` into it.
    ///
    /// Equivalent to `alloc(size_of::<T>(), 0)` followed by `ptr::write(ptr, val)`.
    /// The value is fully initialized on the stack first, then moved to the heap.
    ///
    /// # Safety
    /// Must only be called from within the WA.exe process.
    pub unsafe fn from_value(val: T) -> Self {
        let size = core::mem::size_of::<T>() as u32;
        let raw = wa_malloc(size);
        if raw.is_null() {
            panic!("wa_malloc({size}) returned null");
        }
        let ptr = raw as *mut T;
        core::ptr::write(ptr, val);
        Self {
            ptr: core::ptr::NonNull::new_unchecked(ptr),
        }
    }

    /// Consume the box and return the raw pointer, relinquishing Rust ownership.
    ///
    /// The caller is now responsible for the memory — typically by storing the pointer
    /// in a WA struct field that will eventually be freed by a C++ destructor.
    pub fn leak(self) -> *mut T {
        let ptr = self.ptr.as_ptr();
        let _ = core::mem::ManuallyDrop::new(self);
        ptr
    }
}
