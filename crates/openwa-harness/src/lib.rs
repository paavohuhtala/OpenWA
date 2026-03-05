#![allow(non_snake_case)]

use openwa_types::address::va;

#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExA, DONT_RESOLVE_DLL_REFERENCES};

/// A loaded WA.exe image mapped into our process memory.
///
/// Uses `LoadLibraryExA` with `DONT_RESOLVE_DLL_REFERENCES` to map the PE
/// with relocations applied but no imports resolved and no entry point called.
// SAFETY: WaImage holds a module handle that is valid for the process lifetime.
// The loaded image is read-only (no imports resolved, no entry point called).
unsafe impl Send for WaImage {}
unsafe impl Sync for WaImage {}

pub struct WaImage {
    #[cfg(target_os = "windows")]
    #[allow(dead_code)] // kept to prevent FreeLibrary on the HMODULE
    handle: isize,
    base: usize,
}

impl WaImage {
    /// Load WA.exe from the given path.
    ///
    /// Tries `WAEXE_PATH` env var first, then the provided path.
    #[cfg(target_os = "windows")]
    pub fn load() -> Result<Self, String> {
        let path = std::env::var("WAEXE_PATH").unwrap_or_else(|_| {
            r"I:\games\SteamLibrary\steamapps\common\Worms Armageddon\WA.exe".to_string()
        });

        if !std::path::Path::new(&path).exists() {
            return Err(format!("WA.exe not found at: {path}"));
        }

        let mut path_bytes: Vec<u8> = path.bytes().collect();
        path_bytes.push(0); // null terminator

        let handle = unsafe {
            LoadLibraryExA(
                path_bytes.as_ptr(),
                std::ptr::null_mut(),
                DONT_RESOLVE_DLL_REFERENCES,
            )
        };

        if handle.is_null() {
            let err = std::io::Error::last_os_error();
            return Err(format!("LoadLibraryExA failed: {err}"));
        }

        let handle = handle as isize;
        let base = handle as usize;
        eprintln!(
            "WA.exe loaded at 0x{:08X} (delta from 0x400000: {:+})",
            base,
            base as i64 - va::IMAGE_BASE as i64
        );

        Ok(WaImage { handle, base })
    }

    #[cfg(not(target_os = "windows"))]
    pub fn load() -> Result<Self, String> {
        Err("WaImage only works on Windows".to_string())
    }

    /// Base address of the loaded image.
    pub fn base(&self) -> usize {
        self.base
    }

    /// Convert a Ghidra virtual address to a pointer in the loaded image.
    pub fn ptr(&self, ghidra_addr: u32) -> *const u8 {
        let offset = ghidra_addr.wrapping_sub(va::IMAGE_BASE);
        (self.base + offset as usize) as *const u8
    }

    /// Read a u32 at a Ghidra virtual address.
    pub fn read_u32(&self, ghidra_addr: u32) -> u32 {
        unsafe { (self.ptr(ghidra_addr) as *const u32).read_unaligned() }
    }

    /// Read a byte at a Ghidra virtual address.
    pub fn read_u8(&self, ghidra_addr: u32) -> u8 {
        unsafe { *self.ptr(ghidra_addr) }
    }

    /// Read a slice of bytes at a Ghidra virtual address.
    pub fn read_bytes(&self, ghidra_addr: u32, len: usize) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr(ghidra_addr), len) }
    }

    /// Check if a Ghidra address falls within the .rdata section.
    pub fn is_rdata(&self, ghidra_addr: u32) -> bool {
        ghidra_addr >= va::RDATA_START && ghidra_addr < va::DATA_START
    }

    /// Check if a Ghidra address falls within the .text section.
    pub fn is_text(&self, ghidra_addr: u32) -> bool {
        ghidra_addr >= va::TEXT_START && ghidra_addr <= va::TEXT_END
    }

    /// Convert a relocated runtime address back to a Ghidra address.
    pub fn to_ghidra(&self, runtime_addr: u32) -> u32 {
        (runtime_addr as i64 - self.base as i64 + va::IMAGE_BASE as i64) as u32
    }

    /// Convert a Ghidra address to the relocated runtime address.
    pub fn to_runtime(&self, ghidra_addr: u32) -> u32 {
        (ghidra_addr as i64 - va::IMAGE_BASE as i64 + self.base as i64) as u32
    }

    /// Get a function pointer at a Ghidra address, cast to the desired type.
    ///
    /// # Safety
    /// Caller must ensure the function signature matches and imports are resolved.
    pub unsafe fn func_ptr<T>(&self, ghidra_addr: u32) -> T {
        let ptr = self.ptr(ghidra_addr);
        std::mem::transmute_copy(&ptr)
    }

    /// Patch the import address table, resolving all DLL imports.
    ///
    /// This must be called before calling any WA.exe functions that use
    /// imported APIs (malloc, memset, etc.).
    #[cfg(target_os = "windows")]
    pub fn patch_imports(&self) -> Result<usize, String> {
        unsafe { iat::patch_iat(self.base) }
    }
}

// No Drop impl — FreeLibrary not needed for test harness.
// The OS reclaims resources when the process exits.

// ---------------------------------------------------------------------------
// IAT patching — resolve imports so we can call WA.exe functions
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod iat {
    use std::ffi::CStr;

    /// PE image import descriptor (20 bytes each, array terminated by zeroed entry)
    #[repr(C)]
    struct ImageImportDescriptor {
        original_first_thunk: u32, // RVA to INT (Import Name Table)
        time_date_stamp: u32,
        forwarder_chain: u32,
        name: u32,            // RVA to DLL name string
        first_thunk: u32,     // RVA to IAT (Import Address Table) — this is what we patch
    }

    /// PE import-by-name entry
    #[repr(C)]
    struct ImageImportByName {
        hint: u16,
        name: [u8; 1], // variable length, null-terminated
    }

    /// Patch the IAT of a loaded PE image, resolving imports from our own process.
    ///
    /// For each imported DLL, we call LoadLibraryA to get the real DLL handle,
    /// then GetProcAddress for each import, and write the result into the IAT slot.
    ///
    /// Returns the number of imports patched.
    pub unsafe fn patch_iat(base: usize) -> Result<usize, String> {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
        use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

        // Parse PE headers
        let dos_header = base as *const u8;
        let e_lfanew = *(dos_header.add(0x3C) as *const u32) as usize;
        let pe_sig = base + e_lfanew;

        // Verify PE signature
        if *(pe_sig as *const u32) != 0x4550 {
            return Err("Invalid PE signature".to_string());
        }

        // Optional header starts at PE + 24
        let opt_header = pe_sig + 24;

        // Import directory is data directory entry #1 (at optional_header + 104 for PE32)
        let import_dir_rva = *((opt_header + 104) as *const u32) as usize;
        if import_dir_rva == 0 {
            return Err("No import directory".to_string());
        }

        let mut patched = 0usize;
        let mut desc_ptr = (base + import_dir_rva) as *const ImageImportDescriptor;

        loop {
            let desc = &*desc_ptr;
            if desc.name == 0 {
                break; // End of import descriptor array
            }

            let dll_name_ptr = (base + desc.name as usize) as *const u8;
            let dll_name = CStr::from_ptr(dll_name_ptr as *const i8);

            // Load the actual DLL into our process
            let dll_handle = LoadLibraryA(dll_name_ptr);
            if dll_handle.is_null() {
                eprintln!("  [IAT] Could not load DLL: {}", dll_name.to_string_lossy());
                desc_ptr = desc_ptr.add(1);
                continue;
            }

            // Walk the INT (Original First Thunk) and IAT (First Thunk) in parallel
            let int_rva = if desc.original_first_thunk != 0 {
                desc.original_first_thunk as usize
            } else {
                desc.first_thunk as usize
            };

            let mut int_entry = (base + int_rva) as *const u32;
            let mut iat_entry = (base + desc.first_thunk as usize) as *mut u32;

            let mut dll_count = 0u32;
            loop {
                let thunk_data = *int_entry;
                if thunk_data == 0 {
                    break; // End of import list for this DLL
                }

                let proc_addr = if thunk_data & 0x80000000 != 0 {
                    // Import by ordinal
                    let ordinal = (thunk_data & 0xFFFF) as u16;
                    GetProcAddress(dll_handle, ordinal as *const u8)
                } else {
                    // Import by name
                    let import_by_name = (base + thunk_data as usize) as *const ImageImportByName;
                    let func_name = (*import_by_name).name.as_ptr();
                    GetProcAddress(dll_handle, func_name as *const u8)
                };

                if let Some(addr) = proc_addr {
                    // Make IAT entry writable, write the resolved address
                    let mut old_protect = 0u32;
                    VirtualProtect(iat_entry as *mut _, 4, PAGE_READWRITE, &mut old_protect);
                    *iat_entry = addr as u32;
                    VirtualProtect(iat_entry as *mut _, 4, old_protect, &mut old_protect);
                    dll_count += 1;
                }

                int_entry = int_entry.add(1);
                iat_entry = iat_entry.add(1);
            }

            patched += dll_count as usize;
            desc_ptr = desc_ptr.add(1);
        }

        Ok(patched)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Load WA.exe once for all tests (expensive operation).
    fn wa() -> &'static WaImage {
        static IMAGE: OnceLock<WaImage> = OnceLock::new();
        IMAGE.get_or_init(|| {
            WaImage::load().expect(
                "Failed to load WA.exe. Set WAEXE_PATH or ensure WA.exe exists at default path.",
            )
        })
    }

    /// Mutex to serialize tests that mutate global state (IAT, CRT heap, etc.).
    /// Tests that only read from the loaded image don't need this.
    fn mutation_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn load_wa_exe() {
        let img = wa();
        assert!(img.base() != 0, "base address should be non-zero");
        eprintln!("WA.exe base: 0x{:08X}", img.base());
    }

    // --- Vtable location checks ---

    #[test]
    fn vtable_locations_in_rdata() {
        let img = wa();
        let vtables = [
            ("CTask", va::CTASK_VTABLE),
            ("CGameTask", va::CGAMETASK_VTABLE),
            ("CGameTask2", va::CGAMETASK_VTABLE2),
            ("DDGameWrapper", va::DDGAME_WRAPPER_VTABLE),
            ("GfxHandler", va::GFX_HANDLER_VTABLE),
            ("DisplayGfx", va::DISPLAY_GFX_VTABLE),
            ("PCLandscape", va::PC_LANDSCAPE_VTABLE),
            ("LandscapeShader", va::LANDSCAPE_SHADER_VTABLE),
            ("DSSound", va::DS_SOUND_VTABLE),
            ("TaskStateMachine", va::TASK_STATE_MACHINE_VTABLE),
            ("OpenGLCPU", va::OPENGL_CPU_VTABLE),
            ("WaterEffect", va::WATER_EFFECT_VTABLE),
        ];

        for (name, addr) in vtables {
            assert!(
                img.is_rdata(addr),
                "{name} vtable 0x{addr:08X} should be in .rdata"
            );
        }
    }

    #[test]
    fn vtable_first_entries_point_to_text() {
        let img = wa();
        let vtables = [
            ("CTask", va::CTASK_VTABLE),
            ("CGameTask", va::CGAMETASK_VTABLE),
            ("DDGameWrapper", va::DDGAME_WRAPPER_VTABLE),
            ("GfxHandler", va::GFX_HANDLER_VTABLE),
            ("DisplayGfx", va::DISPLAY_GFX_VTABLE),
            ("PCLandscape", va::PC_LANDSCAPE_VTABLE),
            ("LandscapeShader", va::LANDSCAPE_SHADER_VTABLE),
            ("DSSound", va::DS_SOUND_VTABLE),
            ("TaskStateMachine", va::TASK_STATE_MACHINE_VTABLE),
            ("OpenGLCPU", va::OPENGL_CPU_VTABLE),
            ("WaterEffect", va::WATER_EFFECT_VTABLE),
        ];

        for (name, vtable_addr) in vtables {
            // Vtable entries are relocated pointers — convert back to Ghidra space
            let first_entry_runtime = img.read_u32(vtable_addr);
            let first_entry_ghidra = img.to_ghidra(first_entry_runtime);
            assert!(
                img.is_text(first_entry_ghidra),
                "{name} vtable first entry 0x{first_entry_ghidra:08X} (runtime 0x{first_entry_runtime:08X}) should be in .text"
            );
        }
    }

    // --- CTask vtable method verification ---

    #[test]
    fn ctask_vtable_methods() {
        let img = wa();
        let expected: &[(&str, u32)] = &[
            ("vt0_init", va::CTASK_VT0_INIT),
            ("vt1_Free", va::CTASK_VT1_FREE),
            ("vt2_HandleMessage", va::CTASK_VT2_HANDLE_MESSAGE),
            ("vt3", va::CTASK_VT3),
            ("vt4", va::CTASK_VT4),
            ("vt5", va::CTASK_VT5),
            ("vt6", va::CTASK_VT6),
            ("vt7_ProcessFrame", va::CTASK_VT7_PROCESS_FRAME),
        ];

        for (i, (name, expected_ghidra)) in expected.iter().enumerate() {
            let slot_addr = va::CTASK_VTABLE + (i as u32) * 4;
            let actual_runtime = img.read_u32(slot_addr);
            let actual_ghidra = img.to_ghidra(actual_runtime);
            assert_eq!(
                actual_ghidra, *expected_ghidra,
                "CTask::{name} at vtable+0x{:02X}: expected 0x{expected_ghidra:08X}, got 0x{actual_ghidra:08X}",
                i * 4
            );
        }
    }

    // --- Function prologue checks ---

    #[test]
    fn function_prologues() {
        let img = wa();

        let valid_prologues: &[u8] = &[
            0x55, // push ebp
            0x56, // push esi
            0x57, // push edi
            0x83, // sub esp, ...
            0x8B, // mov reg, ...
            0x6A, // push imm8
            0x81, // sub esp, imm32
            0xB8, // mov eax, imm32
            0x51, // push ecx
            0x52, // push edx
            0x64, // FS: segment prefix (SEH)
            0x85, // test
            0x8D, // lea
            0x53, // push ebx
        ];

        let functions: &[(&str, u32)] = &[
            ("CTask::Constructor", va::CTASK_CONSTRUCTOR),
            ("CGameTask::Constructor", va::CGAMETASK_CONSTRUCTOR),
            ("DDGameWrapper::Constructor", va::CONSTRUCT_DD_GAME_WRAPPER),
            ("DDGame::Constructor", va::CONSTRUCT_DD_GAME),
            ("CreateExplosion", va::CREATE_EXPLOSION),
            ("SpawnObject", va::SPAWN_OBJECT),
            ("WeaponRelease", va::WEAPON_RELEASE),
            ("FireWeapon", va::FIRE_WEAPON),
            ("InitWeaponTable", va::INIT_WEAPON_TABLE),
            ("BlitScreen", va::BLIT_SCREEN),
            ("RenderDrawingQueue", va::RENDER_DRAWING_QUEUE),
            ("DrawLandscape", va::DRAW_LANDSCAPE),
            ("ConstructPCLandscape", va::CONSTRUCT_PC_LANDSCAPE),
            ("ConstructDSSound", va::CONSTRUCT_DS_SOUND),
            ("ShowChatMessage", va::SHOW_CHAT_MESSAGE),
            ("FrontendChangeScreen", va::FRONTEND_CHANGE_SCREEN),
            ("WA_MallocMemset", va::WA_MALLOC_MEMSET),
        ];

        for (name, addr) in functions {
            let first_byte = img.read_u8(*addr);
            assert!(
                valid_prologues.contains(&first_byte),
                "{name} at 0x{addr:08X}: unexpected prologue byte 0x{first_byte:02X}"
            );
        }
    }

    // --- Cross-reference: vtable methods should have valid prologues ---

    #[test]
    fn vtable_method_prologues() {
        let img = wa();

        let valid_prologues: &[u8] = &[
            0x55, 0x56, 0x57, 0x83, 0x8B, 0x6A, 0x81, 0xB8,
            0x51, 0x52, 0x64, 0x85, 0x8D, 0x53, 0xC2, 0xC3, 0x33,
        ];

        // Read all CTask vtable entries and check their prologues
        for i in 0..8 {
            let method_ghidra = img.to_ghidra(img.read_u32(va::CTASK_VTABLE + i * 4));
            let first_byte = img.read_u8(method_ghidra);
            assert!(
                valid_prologues.contains(&first_byte),
                "CTask vtable[{i}] at 0x{method_ghidra:08X}: unexpected byte 0x{first_byte:02X}"
            );
        }

        // CGameTask vtable (20 entries)
        for i in 0..20 {
            let method_ghidra = img.to_ghidra(img.read_u32(va::CGAMETASK_VTABLE + i * 4));
            if img.is_text(method_ghidra) {
                let first_byte = img.read_u8(method_ghidra);
                assert!(
                    valid_prologues.contains(&first_byte),
                    "CGameTask vtable[{i}] at 0x{method_ghidra:08X}: unexpected byte 0x{first_byte:02X}"
                );
            }
        }
    }

    // --- IAT patching ---

    #[test]
    fn patch_imports() {
        let _lock = mutation_lock().lock().unwrap();
        let img = wa();
        let count = img.patch_imports().expect("IAT patching failed");
        eprintln!("Patched {count} IAT entries");
        assert!(count > 100, "Expected many imports, got {count}");
    }

    // --- Function call: WA's malloc wrapper ---

    /// Initialize WA.exe's CRT heap so that _malloc/_free work.
    ///
    /// The statically-linked MSVC CRT checks DAT_006b2198 (heap handle)
    /// and DAT_008c3128 (heap type selector). We set these directly
    /// rather than calling __heap_init.
    unsafe fn init_wa_crt_heap(img: &WaImage) {
        use windows_sys::Win32::System::Memory::GetProcessHeap;

        // DAT_006b2198 = heap handle (HANDLE)
        let heap_handle_ptr = img.ptr(0x006B_2198) as *mut u32;
        // DAT_008c3128 = heap type (1 = simple HeapAlloc mode)
        let heap_type_ptr = img.ptr(0x008C_3128) as *mut u32;

        let heap = GetProcessHeap();
        assert!(!heap.is_null(), "GetProcessHeap failed");

        // Make .data writable (it should already be, but be safe)
        let mut old_protect = 0u32;
        windows_sys::Win32::System::Memory::VirtualProtect(
            heap_handle_ptr as *mut _,
            8,
            windows_sys::Win32::System::Memory::PAGE_READWRITE,
            &mut old_protect,
        );

        *heap_handle_ptr = heap as u32;
        *heap_type_ptr = 1; // Simple HeapAlloc mode — no SBH
        eprintln!("  CRT heap initialized: handle=0x{:08X}, type=1", heap as u32);
    }

    #[test]
    fn call_wa_malloc() {
        let _lock = mutation_lock().lock().unwrap();
        let img = wa();
        img.patch_imports().expect("IAT patching failed");
        unsafe { init_wa_crt_heap(img); }

        // thunk_FUN_005c0ab8 is __cdecl: (size) -> ptr
        // This is WA's malloc wrapper — calls _malloc, checks for null
        const WA_MALLOC: u32 = 0x005C_0AE3;

        type MallocFn = unsafe extern "cdecl" fn(size: u32) -> *mut u8;

        unsafe {
            let wa_malloc: MallocFn = img.func_ptr(WA_MALLOC);

            eprintln!("Calling WA malloc wrapper...");
            let ptr = wa_malloc(64);
            assert!(!ptr.is_null(), "malloc should return non-null for 64 bytes");
            eprintln!("WA malloc(64) returned 0x{:08X}", ptr as u32);

            // Write to it to prove it's real memory
            *ptr = 0x42;
            assert_eq!(*ptr, 0x42);

            eprintln!("WA malloc wrapper works!");
        }
    }

    // --- Function call: CTask::Constructor ---

    #[test]
    fn call_ctask_constructor() {
        let _lock = mutation_lock().lock().unwrap();
        let img = wa();
        img.patch_imports().expect("IAT patching failed");
        unsafe { init_wa_crt_heap(img); }

        // Initialize security cookie (needed for stack protection in SEH functions)
        const SECURITY_INIT_COOKIE: u32 = 0x005E_7EAB;
        type InitCookieFn = unsafe extern "cdecl" fn();
        unsafe {
            let init_cookie: InitCookieFn = img.func_ptr(SECURITY_INIT_COOKIE);
            init_cookie();
            eprintln!("  Security cookie initialized");
        }

        // CTask::Constructor is __stdcall: (this, parent, ddgame) -> this
        type CTaskCtorFn = unsafe extern "stdcall" fn(this: *mut u8, parent: u32, ddgame: u32) -> *mut u8;

        unsafe {
            let ctor: CTaskCtorFn = img.func_ptr(va::CTASK_CONSTRUCTOR);

            // Allocate a buffer for CTask (0x30 bytes, zero-initialized)
            let mut task_buf = vec![0u8; 0x30];
            let task_ptr = task_buf.as_mut_ptr();

            eprintln!("Calling CTask::Constructor...");
            let result = ctor(task_ptr, 0, 0);

            eprintln!("CTask::Constructor returned 0x{:08X}", result as u32);
            assert_eq!(result, task_ptr, "Constructor should return this");

            // Verify vtable was set
            let vtable = *(task_ptr as *const u32);
            let vtable_ghidra = img.to_ghidra(vtable);
            assert_eq!(
                vtable_ghidra, va::CTASK_VTABLE,
                "CTask vtable: expected 0x{:08X}, got 0x{vtable_ghidra:08X}",
                va::CTASK_VTABLE
            );

            // Verify ddgame pointer at offset 0x2C (3rd ctor param, we passed 0)
            let stored_ddgame = *((task_ptr as usize + 0x2C) as *const u32);
            assert_eq!(stored_ddgame, 0, "ddgame should be 0 (we passed 0)");

            // Verify field at 0x08 is 0x10
            let field_08 = *((task_ptr as usize + 0x08) as *const u32);
            assert_eq!(field_08, 0x10, "field 0x08 should be 0x10");

            // Verify children list was allocated (offset 0x14)
            let children = *((task_ptr as usize + 0x14) as *const u32);
            assert_ne!(children, 0, "children list should be non-null");

            eprintln!("CTask::Constructor succeeded!");
            eprintln!("  vtable = 0x{vtable_ghidra:08X}");
            eprintln!("  children = 0x{children:08X}");
            eprintln!("  ddgame = {stored_ddgame}");
        }
    }
}
