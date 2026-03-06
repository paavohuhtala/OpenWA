//! MFC library function wrappers.

use crate::wa_call;

/// Zero-cost handle to a CWnd-derived MFC window (raw pointer as u32).
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CWndHandle(pub u32);

impl CWndHandle {
    /// CWnd::EnableWindow (0x5C647A) — thiscall(this, bEnable)
    pub unsafe fn enable_window(&self, enable: bool) {
        wa_call::thiscall_1(0x5C647A, self.0, enable as u32);
    }
}

/// Zero-cost handle to a CDialog-derived MFC dialog.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CDialogHandle(pub u32);

impl CDialogHandle {
    /// CDialog::EndDialog (0x5CAB72) — thiscall(this, nResult)
    pub unsafe fn end_dialog(&self, result: u32) {
        wa_call::thiscall_1(0x5CAB72, self.0, result);
    }
}

impl From<CDialogHandle> for CWndHandle {
    fn from(d: CDialogHandle) -> Self {
        CWndHandle(d.0)
    }
}
