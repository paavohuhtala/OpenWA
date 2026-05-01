//! MFC library function wrappers.

use core::ffi::c_void;

use crate::address::va;
use crate::rebase::rb;
use crate::wa_call;

/// MFC `CWnd` (opaque). The only field we touch directly is `m_hWnd` at +0x20.
///
/// Use as `*mut CWnd` in function signatures; access `m_hWnd` via [`cwnd_hwnd`].
#[repr(C)]
pub struct CWnd {
    /// vtable pointer
    pub vtable: *const c_void,
    _opaque: [u8; 0],
}

/// Read MFC `CWnd::m_hWnd` (offset +0x20 from the CWnd object) as a raw HWND value.
#[inline]
pub unsafe fn cwnd_hwnd(wnd: *const CWnd) -> u32 {
    unsafe { *((wnd as *const u8).add(0x20) as *const u32) }
}

/// MFC `CWinApp` (concrete subclass `CWormsApp`) — singleton at `g_CWinApp`
/// (`0x007C03D0`). Only the fields touched directly by the WA code we've
/// ported are typed here; the rest of CWormsApp is opaque.
///
/// Note: the WA disassembly often "reaches" globals like `g_DisplayModeFlag`
/// (0x0088E485) by adding `+0xCE0B5` to the `CWinApp*`. Those are scattered
/// BSS globals that happen to live at `&g_CWinApp + huge_offset`, NOT real
/// fields of this struct — treat them as named globals (e.g. `va::G_DISPLAY_MODE_FLAG`).
#[repr(C)]
pub struct CWinApp {
    /// vtable pointer (CWormsApp::vtable)
    pub vtable: *const c_void,
    _unknown_04: [u8; 0xa0],
    /// Embedded virtual sub-object at +0xa4. The launch path calls slot 13
    /// (offset 0x34 in the vtable) on it before `GameSession::Run`, and
    /// `GameWorldRenderChildren_Maybe(&self.subobj_a4)` after.
    pub subobj_a4: AppSubObjA4,
    _unknown_a8: [u8; 0xa8],
    /// u32 zeroed on the headful-fullscreen ExitProcess fallback path in
    /// `Frontend::LaunchGameSession`. Other readers in the binary; semantics
    /// TBD.
    pub field_150: u32,
}

/// Embedded virtual sub-object inside `CWinApp` at +0xa4. Constructor
/// (`CWormsApp::Constructor` 0x004E3C04) sets its vtable to `PTR_FUN_00662d48`.
#[repr(C)]
pub struct AppSubObjA4 {
    pub vtable: *const c_void,
    _opaque: [u8; 0],
}

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
