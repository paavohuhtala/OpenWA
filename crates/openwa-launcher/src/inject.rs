use std::ffi::CString;
use std::mem;
use std::ptr;

// Declare only what we need directly, avoiding windows-sys feature-flag issues.
#[allow(non_camel_case_types)]
type HANDLE = *mut core::ffi::c_void;
#[allow(non_camel_case_types)]
type LPVOID = *mut core::ffi::c_void;
#[allow(non_camel_case_types)]
type LPCVOID = *const core::ffi::c_void;
#[allow(non_camel_case_types)]
type SIZE_T = usize;
#[allow(non_camel_case_types)]
type DWORD = u32;
#[allow(non_camel_case_types)]
type BOOL = i32;
type FARPROC = unsafe extern "system" fn() -> isize;

extern "system" {
    fn VirtualAllocEx(
        hProcess: HANDLE, lpAddress: LPVOID, dwSize: SIZE_T,
        flAllocationType: DWORD, flProtect: DWORD,
    ) -> LPVOID;
    fn VirtualFreeEx(hProcess: HANDLE, lpAddress: LPVOID, dwSize: SIZE_T, dwFreeType: DWORD) -> BOOL;
    fn WriteProcessMemory(
        hProcess: HANDLE, lpBaseAddress: LPVOID, lpBuffer: LPCVOID,
        nSize: SIZE_T, lpNumberOfBytesWritten: *mut SIZE_T,
    ) -> BOOL;
    fn GetModuleHandleA(lpModuleName: *const u8) -> HANDLE;
    fn GetProcAddress(hModule: HANDLE, lpProcName: *const u8) -> Option<FARPROC>;
    fn CreateRemoteThread(
        hProcess: HANDLE, lpThreadAttributes: LPVOID, dwStackSize: SIZE_T,
        lpStartAddress: unsafe extern "system" fn(LPVOID) -> DWORD,
        lpParameter: LPVOID, dwCreationFlags: DWORD, lpThreadId: *mut DWORD,
    ) -> HANDLE;
    fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
    fn CloseHandle(hObject: HANDLE) -> BOOL;
}

const MEM_COMMIT: DWORD  = 0x1000;
const MEM_RESERVE: DWORD = 0x2000;
const MEM_RELEASE: DWORD = 0x8000;
const PAGE_READWRITE: DWORD = 0x04;
const INFINITE: DWORD = 0xFFFF_FFFF;

/// Inject `dll_path` into `process` by creating a remote thread that calls `LoadLibraryA`.
///
/// Blocks until `DllMain` returns (i.e. all hooks are installed) before returning,
/// so the caller can safely resume the main thread.
pub unsafe fn inject_dll(process: HANDLE, dll_path: &str) -> Result<(), String> {
    let path_cstr = CString::new(dll_path)
        .map_err(|e| format!("bad DLL path: {e}"))?;
    let path_bytes = path_cstr.as_bytes_with_nul();

    // Allocate memory in the target process for the DLL path string.
    let remote_buf = VirtualAllocEx(
        process, ptr::null_mut(), path_bytes.len(),
        MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE,
    );
    if remote_buf.is_null() {
        return Err("VirtualAllocEx failed".to_string());
    }

    // Write the path into the target process.
    let ok = WriteProcessMemory(
        process, remote_buf,
        path_bytes.as_ptr().cast(), path_bytes.len(),
        ptr::null_mut(),
    );
    if ok == 0 {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("WriteProcessMemory failed".to_string());
    }

    // LoadLibraryA lives at the same address in every process (kernel32 is always mapped
    // at the same base across all processes on the same OS session due to ASLR sharing).
    let k32 = GetModuleHandleA(b"kernel32.dll\0".as_ptr());
    if k32.is_null() {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("GetModuleHandleA(kernel32) failed".to_string());
    }
    let load_library = GetProcAddress(k32, b"LoadLibraryA\0".as_ptr());
    let Some(load_library) = load_library else {
        VirtualFreeEx(process, remote_buf, 0, MEM_RELEASE);
        return Err("GetProcAddress(LoadLibraryA) failed".to_string());
    };

    // Spin up a remote thread that calls LoadLibraryA(dll_path).
    // This triggers DllMain(DLL_PROCESS_ATTACH) which installs all our hooks.
    let thread = CreateRemoteThread(
        process, ptr::null_mut(), 0,
        mem::transmute(load_library),
        remote_buf, 0, ptr::null_mut(),
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
