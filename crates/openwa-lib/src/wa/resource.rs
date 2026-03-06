//! Win32 PE resource extraction helpers.

type LPCSTR = *const u8;

extern "system" {
    fn GetModuleHandleA(lpModuleName: *const u8) -> u32;
    fn FindResourceA(hModule: u32, lpName: LPCSTR, lpType: LPCSTR) -> u32;
    fn LoadResource(hModule: u32, hResInfo: u32) -> u32;
    fn SizeofResource(hModule: u32, hResInfo: u32) -> u32;
    fn LockResource(hResData: u32) -> *const u8;
}

#[link(name = "user32")]
extern "system" {
    fn LoadStringA(hInstance: u32, uID: u32, lpBuffer: *mut u8, cchBufferMax: i32) -> i32;
    fn CharLowerBuffA(lpsz: *mut u8, cchLength: u32) -> u32;
}

/// Load a string resource from WA.exe by ID.
/// Returns the string content, or an empty string on failure.
pub unsafe fn load_string_resource(id: u32) -> String {
    let hmodule = GetModuleHandleA(core::ptr::null());
    let mut buf = [0u8; 256];
    let len = LoadStringA(hmodule, id, buf.as_mut_ptr(), buf.len() as i32);
    if len <= 0 {
        return String::new();
    }
    String::from_utf8_lossy(&buf[..len as usize]).into_owned()
}

/// Lowercase an ANSI string using the system's Active Code Page.
pub fn lowercase_ansi(s: &str) -> String {
    let mut buf: Vec<u8> = s.bytes().collect();
    if !buf.is_empty() {
        unsafe {
            CharLowerBuffA(buf.as_mut_ptr(), buf.len() as u32);
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Load a PE resource from WA.exe by type name and integer ID.
/// Returns a slice of the resource data, or None on failure.
/// The returned slice is valid for the lifetime of the module (i.e., forever).
pub unsafe fn load_pe_resource(type_name: &str, id: u32) -> Option<&'static [u8]> {
    let hmodule = GetModuleHandleA(core::ptr::null());

    // MAKEINTRESOURCE(id)
    let lp_name = id as LPCSTR;

    // Type name must be null-terminated
    let mut type_buf = [0u8; 32];
    let type_bytes = type_name.as_bytes();
    if type_bytes.len() >= type_buf.len() {
        return None;
    }
    type_buf[..type_bytes.len()].copy_from_slice(type_bytes);
    // type_buf is already zero-initialized, so null terminator is in place

    let h_res = FindResourceA(hmodule, lp_name, type_buf.as_ptr());
    if h_res == 0 {
        return None;
    }

    let size = SizeofResource(hmodule, h_res) as usize;
    if size == 0 {
        return None;
    }

    let h_data = LoadResource(hmodule, h_res);
    if h_data == 0 {
        return None;
    }

    let ptr = LockResource(h_data);
    if ptr.is_null() {
        return None;
    }

    Some(core::slice::from_raw_parts(ptr, size))
}
