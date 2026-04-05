use std::ffi::CString;
use std::mem;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{CreateRemoteThread, WaitForSingleObject, INFINITE};

/// Inject `dll_path` into `process` by creating a remote thread that calls `LoadLibraryA`.
///
/// Blocks until `DllMain` returns (i.e. all hooks are installed) before returning,
/// so the caller can safely resume the main thread.
pub unsafe fn inject_dll(process: HANDLE, dll_path: &str) -> Result<(), String> {
    let path_cstr = CString::new(dll_path).map_err(|e| format!("bad DLL path: {e}"))?;
    let path_bytes = path_cstr.as_bytes_with_nul();

    // Allocate memory in the target process for the DLL path string.
    let remote_buf = VirtualAllocEx(
        process,
        ptr::null(),
        path_bytes.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );
    if remote_buf.is_null() {
        return Err("VirtualAllocEx failed".to_string());
    }

    // Write the path into the target process.
    let ok = WriteProcessMemory(
        process,
        remote_buf,
        path_bytes.as_ptr().cast(),
        path_bytes.len(),
        ptr::null_mut(),
    );
    if ok == 0 {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("WriteProcessMemory failed".to_string());
    }

    // LoadLibraryA lives at the same address in every process (kernel32 is always mapped
    // at the same base across all processes on the same OS session due to ASLR sharing).
    let k32 = GetModuleHandleA(c"kernel32.dll".as_ptr().cast());
    if k32.is_null() {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("GetModuleHandleA(kernel32) failed".to_string());
    }
    let load_library = GetProcAddress(k32, c"LoadLibraryA".as_ptr().cast());
    let Some(load_library) = load_library else {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("GetProcAddress(LoadLibraryA) failed".to_string());
    };

    // Spin up a remote thread that calls LoadLibraryA(dll_path).
    // This triggers DllMain(DLL_PROCESS_ATTACH) which installs all our hooks.
    let thread = CreateRemoteThread(
        process,
        ptr::null(),
        0,
        Some(mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(*mut core::ffi::c_void) -> u32,
        >(load_library)),
        remote_buf,
        0,
        ptr::null_mut(),
    );
    if thread.is_null() {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("CreateRemoteThread failed".to_string());
    }

    // Wait for DllMain to finish — hooks are installed when this returns.
    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
    VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);

    Ok(())
}
