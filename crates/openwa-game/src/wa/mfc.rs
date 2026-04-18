//! MFC library function wrappers.

use crate::address::va;
use crate::rebase::rb;
use crate::wa_call;

/// Zero-cost handle to a CWnd-derived MFC window (raw pointer as u32).
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CWndHandle(pub u32);

impl CWndHandle {
    /// CWnd::EnableWindow (0x5C647A) — thiscall(this, bEnable)
    pub unsafe fn enable_window(&self, enable: bool) {
        unsafe {
            wa_call::thiscall_1(0x5C647A, self.0, enable as u32);
        }
    }
}

/// Zero-cost handle to a CDialog-derived MFC dialog.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CDialogHandle(pub u32);

impl CDialogHandle {
    /// CDialog::EndDialog (0x5CAB72) — thiscall(this, nResult)
    pub unsafe fn end_dialog(&self, result: u32) {
        unsafe {
            wa_call::thiscall_1(0x5CAB72, self.0, result);
        }
    }
}

impl From<CDialogHandle> for CWndHandle {
    fn from(d: CDialogHandle) -> Self {
        CWndHandle(d.0)
    }
}

/// Handle to an MFC `ATL::CSimpleStringT<char,0>` field in a WA struct.
///
/// A CSimpleStringT "object" is just a `char*` data pointer (4 bytes on x86).
/// Metadata lives at negative offsets from the data pointer:
///
/// ```text
/// data - 0x10: pStringMgr* (IAtlStringMgr vtable pointer)
/// data - 0x0C: nDataLength (i32)
/// data - 0x08: nAllocLength (i32)
/// data - 0x04: nRefs (i32, atomically managed)
/// data + 0x00: char data[]
/// ```
///
/// `CStringRef` wraps the address of the CSimpleStringT object (i.e., the
/// address of the `char*` pointer itself), NOT the char data directly.
/// This matches how thiscall methods like operator= expect `this`.
pub struct CStringRef {
    /// Address of the CSimpleStringT object (pointer to the char* data pointer).
    pub ptr: u32,
}

impl CStringRef {
    /// Create a CStringRef from the address of a CString field in a struct.
    /// For example, `CStringRef::new(dest + 0x0C)` for the name field.
    pub fn new(object_addr: u32) -> Self {
        Self { ptr: object_addr }
    }

    /// Read the char* data pointer from the CString object.
    unsafe fn data_ptr(&self) -> u32 {
        unsafe { *(self.ptr as *const u32) }
    }

    /// Get string length from CStringData header (nDataLength at data - 0x0C).
    pub unsafe fn len(&self) -> i32 {
        unsafe {
            let data = self.data_ptr();
            *((data - 0x0C) as *const i32)
        }
    }

    /// Check if the CString is empty.
    pub unsafe fn is_empty(&self) -> bool {
        unsafe { self.len() == 0 }
    }

    /// Assign from another CString via `ATL::CSimpleStringT::operator=` (0x401D20).
    /// thiscall(ECX=this, stack=&src) where both are CSimpleStringT* pointers.
    pub unsafe fn assign_from(&mut self, src: &CStringRef) {
        unsafe {
            wa_call::thiscall_1(va::CSTRING_OPERATOR_ASSIGN, self.ptr, src.ptr);
        }
    }

    /// Assign a localized string resource by ID.
    /// Used when source CString is empty (e.g., resource 0x0E = default scheme name).
    /// FUN_004A39F0: EDX=resource_id, stack param=dest CString object pointer.
    pub unsafe fn assign_resource(&mut self, resource_id: u32) {
        unsafe {
            let addr = rb(va::CSTRING_ASSIGN_RESOURCE);
            let f: unsafe extern "fastcall" fn(u32, u32, u32) = core::mem::transmute(addr);
            // ECX is overwritten by the function (loads from global), so any value works.
            // EDX = resource_id, stack = self.ptr
            f(0, resource_id, self.ptr);
        }
    }
}

/// Release a CString data pointer's refcount.
///
/// Takes the raw `char*` data pointer (NOT the CString object address).
/// Atomically decrements nRefs; if old_ref <= 1, calls the deallocator
/// via `pStringMgr->vtable[1](pCStringData)`.
///
/// This is the standalone cleanup used when a CString data pointer is
/// passed by value (e.g., ScanDirectory's parameter).
pub unsafe fn cstring_release(data_ptr: u32) {
    unsafe {
        let refcount_ptr = (data_ptr - 4) as *mut i32;

        // LOCK XADD [refcount], -1 → returns old value
        let old_ref = core::sync::atomic::AtomicI32::from_ptr(refcount_ptr)
            .fetch_sub(1, core::sync::atomic::Ordering::AcqRel);

        if old_ref <= 1 {
            // pCStringData is at data - 0x10
            let cstring_data = data_ptr - 0x10;
            // pStringMgr is the first field of CStringData
            let string_mgr = *(cstring_data as *const u32);
            // vtable[1] = Free method (thiscall)
            let vtable = *(string_mgr as *const u32);
            let free_fn: unsafe extern "fastcall" fn(u32, u32, u32) =
                core::mem::transmute(*((vtable + 4) as *const u32));
            free_fn(string_mgr, 0, cstring_data);
        }
    }
}
