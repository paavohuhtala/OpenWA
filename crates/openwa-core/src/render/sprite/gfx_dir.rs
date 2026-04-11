//! GfxDir — .dir sprite archive reader.
//!
//! A GfxDir manages a hash table loaded from `.dir` files (e.g. `Gfx.dir`,
//! `Gfx0.dir`). It provides name→resource lookup via a 1024-bucket hash table
//! and delegates file I/O and caching to vtable methods.

use openwa_core::vtable;

use crate::address::va;
use crate::rebase::rb;
use crate::wa_alloc::wa_malloc;

/// .dir archive reader (0x19C bytes, vtable 0x66B280).
///
/// Loaded from `.dir` files via `gfx_dir_load`. Contains a 1024-bucket
/// hash table for name→resource lookups, plus a 16-slot cache.
#[repr(C)]
pub struct GfxDir {
    /// 0x000: Vtable pointer (0x66B280).
    pub vtable: *const GfxDirVtable,
    /// 0x004: Bucket array — 1024 pointers to GfxDirEntry linked lists.
    pub bucket_array: *mut u8,
    /// 0x008: 1 if bucket_array was allocated via malloc fallback (needs free).
    pub fallback_alloc: u32,
    /// 0x00C-0x10B: 16 cache slots (0x10 bytes each, first u32 zeroed on reset).
    pub cache_slots: [u8; 0x100],
    /// 0x10C-0x14B: Index table (16 u32 entries, identity permutation on reset).
    pub index_table: [u32; 16],
    /// 0x14C-0x18B: Unknown padding.
    pub _unknown_14c: [u8; 0x40],
    /// 0x18C: Number of cache slots (always 0x10).
    pub slot_count: u32,
    /// 0x190: Unknown (zeroed on reset).
    pub _unknown_190: u32,
    /// 0x194: Loaded flag (1 after successful gfx_dir_load).
    pub loaded: u32,
    /// 0x198: FILE* handle to the open .dir file.
    pub file_handle: *mut u8,
}

const _: () = assert!(core::mem::size_of::<GfxDir>() == 0x19C);

impl GfxDir {
    /// Allocate and initialize a new GfxDir with the given vtable.
    pub unsafe fn alloc(vtable: *const GfxDirVtable) -> *mut Self {
        let ptr = wa_malloc(core::mem::size_of::<Self>() as u32) as *mut Self;
        if !ptr.is_null() {
            core::ptr::write_bytes(ptr as *mut u8, 0, core::mem::size_of::<Self>());
            (*ptr).vtable = vtable;
        }
        ptr
    }
}

/// Vtable for `GfxDir` / GfxHandler (4 slots at 0x66B280).
///
/// Provides sequential I/O into the open `.dir` file and a cached resource
/// access path.
#[vtable(size = 4, va = 0x0066_B280, class = "GfxDir")]
pub struct GfxDirVtable {
    /// Slot 0 (0x58BBD0): Read `size` bytes into `buf`. Returns bytes read.
    pub read: fn(this: *mut GfxDir, buf: *mut u8, size: u32) -> u32,
    /// Slot 1 (0x58BBB0): Seek forward `skip` bytes in the file.
    pub seek: fn(this: *mut GfxDir, skip: u32),
    /// Slot 2 (0x571AF0): Return cached data pointer for `entry_val`, or null if not mapped.
    pub load_cached: fn(this: *mut GfxDir, entry_val: u32) -> *mut u8,
    /// Slot 3 (0x58BB70): Release resources; if `flags & 1`, free the object.
    pub release: fn(this: *mut GfxDir, flags: u32),
}

bind_GfxDirVtable!(GfxDir, vtable);

/// Entry in a GfxDir hash bucket linked list.
/// Each entry maps a name string to a cached resource value.
#[repr(C)]
pub struct GfxDirEntry {
    pub next: *mut GfxDirEntry,
    /// Passed to GfxDir vtable[2] for cached resource lookup.
    pub value: u32,
    pub _unknown_08: u32,
    // +0x0C: null-terminated name (variable-length, not in struct)
}

impl GfxDirEntry {
    /// Name string at +0x0C (immediately after the fixed fields).
    pub unsafe fn name_ptr(&self) -> *const u8 {
        (self as *const Self as *const u8).add(0x0C)
    }
}

/// Hash function for GfxDir entry lookup (FUN_566390).
///
/// 10-bit CRC-like hash over the global name buffer at 0x8ACF58.
/// Operates on the already-lowercased name.
///
/// # Safety
/// `name` must be a valid null-terminated C string pointer.
unsafe fn gfx_dir_hash(name: *const u8) -> u32 {
    let mut hash: u32 = 0;
    let mut p = name;
    while *p != 0 {
        let bit9 = (hash >> 9) & 1;
        hash = ((hash << 1) | bit9) & 0x3FF;
        hash = hash.wrapping_add(*p as u32) & 0x3FF;
        p = p.add(1);
    }
    hash
}

/// Pure Rust implementation of GfxDir__FindEntry (0x566520).
///
/// Convention: usercall(EAX=name) + 1 stack(gfx_dir), RET 0x4.
///
/// Looks up a name in the GfxHandler's hash table. Names are case-insensitive.
/// Supports `|`-separated fallback names (e.g. "path\\file.img|fallback.img").
///
/// Returns entry pointer or null. Entry layout:
/// - entry+0x00: next pointer (linked list)
/// - entry+0x04: value (passed to vtable[2] for cached load)
/// - entry+0x08: unknown
/// - entry+0x0C: name string (null-terminated, lowercase)
///
/// # Safety
/// `gfx_dir` must be a valid GfxHandler with initialized hash table at +0x04.
/// `name` must be a valid null-terminated C string.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_dir_find_entry(
    name: *const core::ffi::c_char,
    gfx_dir: *const GfxDir,
) -> *mut u8 {
    let mut current_name = name as *const u8;

    loop {
        // Copy name to stack buffer (max 0xFF chars + null)
        let mut buf = [0u8; 0x100];
        let mut i = 0usize;
        let mut src = current_name;
        while *src != 0 && i < 0xFF {
            buf[i] = *src;
            src = src.add(1);
            i += 1;
        }
        buf[i] = 0;

        // Find '|' separator in buffer
        let mut pipe_pos: Option<usize> = None;
        for (j, b) in buf[..i].iter_mut().enumerate() {
            if *b == b'|' {
                *b = 0; // truncate at pipe
                pipe_pos = Some(j);
                break;
            }
        }

        // Compute next_name: pointer into original string after '|'
        let next_name: *const u8 = if let Some(pos) = pipe_pos {
            // Offset of '|' in buffer = pos
            // Same offset in original string → current_name + pos + 1
            current_name.add(pos + 1)
        } else {
            core::ptr::null()
        };

        // Lowercase A-Z in buffer
        for b in buf.iter_mut() {
            if *b == 0 {
                break;
            }
            if *b >= b'A' && *b <= b'Z' {
                *b += 0x20;
            }
        }

        // Hash the lowercased name
        let bucket = gfx_dir_hash(buf.as_ptr());

        // Walk linked list in hash bucket
        let bucket_array = (*gfx_dir).bucket_array as *const u32;
        let mut entry = *bucket_array.add(bucket as usize) as *mut GfxDirEntry;

        while !entry.is_null() {
            let entry_name = (*entry).name_ptr();
            let mut match_found = true;
            let mut k = 0usize;
            loop {
                let a = *entry_name.add(k);
                let b = buf[k];
                if a != b {
                    match_found = false;
                    break;
                }
                if a == 0 {
                    break;
                }
                k += 1;
            }

            if match_found {
                return entry as *mut u8;
            }

            entry = (*entry).next;
        }

        // Not found — try fallback name after '|'
        if next_name.is_null() {
            return core::ptr::null_mut();
        }
        current_name = next_name;
    }
}

/// Pure Rust implementation of FUN_5665F0 (GfxHandler reset).
///
/// Convention: usercall(ESI=handler), plain RET. Called at start of LoadDir.
unsafe fn gfx_dir_reset(handler: *mut u8) {
    let gfx = &mut *(handler as *mut GfxDir);
    gfx.bucket_array = core::ptr::null_mut();
    gfx.fallback_alloc = 0;
    gfx.loaded = 0;

    for i in 0..16u32 {
        // Zero first u32 of each cache slot (stride 0x10)
        *(gfx.cache_slots.as_mut_ptr().add(i as usize * 0x10) as *mut u32) = 0;
        gfx.index_table[i as usize] = i;
    }

    gfx._unknown_190 = 0;
    gfx.slot_count = 0x10;
}

/// Pure Rust implementation of GfxHandler__LoadDir (0x5663E0).
///
/// Convention: usercall(EAX=handler), plain RET. Returns 1 on success, 0 on failure.
///
/// Reads a .dir file through the handler's vtable I/O methods:
/// - vtable[0]: read(buf, size) → thiscall, returns bytes read
/// - vtable[1]: seek/reposition(size) → thiscall
/// - vtable[2]: allocate(size) → thiscall, returns buffer or null
///
/// .dir file format:
/// - 4 bytes: magic "DIR\x1A" (0x1A524944)
/// - 4 bytes: total_file_size
/// - 4 bytes: data_size (hash table + entries)
/// - data: 1024-bucket hash table followed by linked list nodes
///   All pointers are relative offsets from (data_start + 4)
///
/// # Safety
/// `handler` must be a valid GfxHandler with file handle at +0x198.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_dir_load_dir(handler: *mut u8) -> i32 {
    gfx_dir_reset(handler);
    let gfx = &mut *(handler as *mut GfxDir);
    let gfx_ptr = handler as *mut GfxDir;

    // Read and validate magic
    let mut magic: u32 = 0;
    if GfxDir::read_raw(gfx_ptr, &mut magic as *mut u32 as *mut u8, 4) != 4 {
        return 0;
    }
    if magic != 0x1A524944 {
        // "DIR\x1A"
        return 0;
    }

    // Read total_file_size and data_size
    let mut total_file_size: u32 = 0;
    if GfxDir::read_raw(gfx_ptr, &mut total_file_size as *mut u32 as *mut u8, 4) != 4 {
        return 0;
    }
    let mut data_size: u32 = 0;
    if GfxDir::read_raw(gfx_ptr, &mut data_size as *mut u32 as *mut u8, 4) != 4 {
        return 0;
    }

    let alloc_size = data_size + 4;

    // Try fast path: vtable[2] load_cached (memory-maps the data)
    let data = GfxDir::load_cached_raw(gfx_ptr, alloc_size);
    gfx.bucket_array = data;

    if data.is_null() {
        // Fallback: seek past header, then malloc + read entire data block
        GfxDir::seek_raw(gfx_ptr, alloc_size);

        let read_size = total_file_size - data_size - 4;
        let malloc_size = ((read_size + 3) & !3) + 0x20;
        let buf = wa_malloc(malloc_size);
        if buf.is_null() {
            return 0;
        }
        core::ptr::write_bytes(buf, 0, read_size as usize);
        gfx.bucket_array = buf;

        let bytes_read = GfxDir::read_raw(gfx_ptr, buf, read_size);
        if bytes_read != read_size {
            crate::wa_alloc::wa_free(buf);
            return 0;
        }

        gfx.fallback_alloc = 1;
    }

    // Fix up relative pointers in the hash table
    // 1024 buckets at data[0..0x1000], each is a pointer to a linked list node
    let data = gfx.bucket_array;
    let base = data as u32;

    for bucket in 0..1024u32 {
        let bucket_ptr = data.add(bucket as usize * 4) as *mut u32;
        let entry_offset = *bucket_ptr;
        if entry_offset == 0 {
            continue;
        }

        // Convert relative offset to absolute: offset + base - 4
        let entry_addr = entry_offset.wrapping_add(base).wrapping_sub(4);
        *bucket_ptr = entry_addr;

        // Walk linked list, fix up each next pointer
        let mut node = entry_addr as *mut u32;
        loop {
            if node.is_null() {
                break;
            }
            let next_offset = *node;
            if next_offset == 0 {
                break;
            }
            let next_addr = next_offset.wrapping_add(base).wrapping_sub(4);
            *node = next_addr;
            node = next_addr as *mut u32;
        }
    }

    gfx.loaded = 1;

    1 // success
}

/// Pure Rust implementation of GfxResource__Create_Maybe (0x4F6300).
///
/// Convention: usercall(ECX=gfx_dir, EAX=name) + 1 stack(output), RET 0x4.
///
/// Looks up `name` in the GfxHandler's directory, tries cached load, wraps
/// as DisplayGfx. Falls back to loading the raw image and decoding it.
///
/// # Safety
/// All pointers must be valid WA objects.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_resource_create(
    gfx_dir: *mut GfxDir,
    name: *const core::ffi::c_char,
    output: *mut u8,
) -> *mut u8 {
    // 1. Try FindEntry → cached load → DisplayGfx wrap
    let entry = gfx_dir_find_entry(name, gfx_dir);
    if !entry.is_null() {
        // gfx_dir->vtable[2](entry->field_4) — cached load
        let entry_val = (*(entry as *const GfxDirEntry)).value;
        let cached = GfxDir::load_cached_raw(gfx_dir, entry_val);
        if !cached.is_null() {
            // DisplayGfx__Constructor_Maybe: stdcall(raw_image), RET 0x4
            let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                core::mem::transmute(rb(va::DISPLAYGFX_CONSTRUCTOR) as usize);
            return ctor(cached);
        }
    }

    // 2. Fallback: LoadImage → IMG_Decode
    let raw_image = call_gfx_load_image(gfx_dir, name);
    if raw_image.is_null() {
        return core::ptr::null_mut();
    }

    // IMG_Decode: stdcall(output, raw_image, 1), RET 0xC
    let decode: unsafe extern "stdcall" fn(*mut u8, *mut u8, i32) -> *mut u8 =
        core::mem::transmute(rb(va::IMG_DECODE) as usize);
    let result = decode(output, raw_image as *mut u8, 1);

    GfxDirStream::destroy_raw(raw_image);

    result
}

/// Helper: find entry in GfxDir and load image, or load directly.
/// Returns a DisplayGfx/sprite pointer or null.
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn call_gfx_find_and_load(
    gfx_dir: *mut GfxDir,
    name: &core::ffi::CStr,
    display_ctx: *mut u8,
) -> *mut u8 {
    let name_ptr = name.as_ptr() as *const u8;
    let entry = gfx_dir_find_entry(name_ptr.cast(), gfx_dir);

    if !entry.is_null() {
        // Try cached load: gfx_dir->vtable[2](entry->field_4)
        let cached = GfxDir::load_cached_raw(gfx_dir, (*(entry as *const GfxDirEntry)).value);
        if !cached.is_null() {
            // Wrap with DisplayGfx__Constructor_Maybe (0x4F5E80)
            // This is stdcall(1 param), RET 0x4
            let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                core::mem::transmute(rb(va::DISPLAYGFX_CONSTRUCTOR) as usize);
            return ctor(cached);
        }
    }

    // Fallback: load image directly
    call_gfx_load_and_wrap(gfx_dir, name_ptr.cast(), display_ctx)
}

/// Helper: load image via GfxDir__LoadImage + wrap as DisplayGfx.
/// Used by arrow sprite loop when GfxDir__FindEntry returns null.
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn call_gfx_load_and_wrap(
    gfx_dir: *mut GfxDir,
    name: *const core::ffi::c_char,
    display_ctx: *mut u8,
) -> *mut u8 {
    let image = call_gfx_load_image(gfx_dir, name);
    if image.is_null() {
        return core::ptr::null_mut();
    }
    // FUN_004F5F80(display_ctx, image, 1) — stdcall, RET 0xC (3 params)
    let f: unsafe extern "stdcall" fn(*mut u8, *mut u8, u32) -> *mut u8 =
        core::mem::transmute(rb(va::IMG_DECODE) as usize);
    let result = f(display_ctx, image as *mut u8, 1);
    GfxDirStream::destroy_raw(image);
    result
}

/// Stream reader returned by `GfxDir__LoadImage` (vtable 0x66A1C0).
///
/// A 12-byte object wrapping a cached read position within a `.dir` archive.
/// Created by `GfxDir__LoadImage`, destroyed by calling `vtable[0](1)`.
///
/// The vtable provides a sequential read interface over the cached resource data.
#[repr(C)]
pub struct GfxDirStream {
    /// 0x00: Vtable pointer (0x66A1C0).
    pub vtable: *const GfxDirStreamVtable,
    /// 0x04: Cache slot index within the parent GfxDir.
    pub slot_index: u32,
    /// 0x08: Parent GfxDir pointer.
    pub gfx_dir: *mut u8,
}

/// Vtable for GfxDirStream (6 slots at 0x66A1C0).
#[repr(C)]
pub struct GfxDirStreamVtable {
    /// Slot 0 (0x5661E0): Destructor — releases cache slot, optionally frees.
    pub destructor: unsafe extern "thiscall" fn(this: *mut GfxDirStream, flags: u32),
    /// Slot 1 (0x566210): Returns 1 if read position < end position.
    pub has_data: unsafe extern "thiscall" fn(this: *mut GfxDirStream) -> u32,
    /// Slot 2 (0x566240): Returns remaining bytes (end - current position).
    pub remaining: unsafe extern "thiscall" fn(this: *mut GfxDirStream) -> u32,
    /// Slot 3 (0x566270): Unknown.
    pub _slot_3: usize,
    /// Slot 4 (0x5662C0): Return total byte size of this stream's data region.
    pub get_total_size: unsafe extern "thiscall" fn(this: *mut GfxDirStream) -> u32,
    /// Slot 5 (0x5662F0): Read `size` bytes into `dest`.
    pub read: unsafe extern "thiscall" fn(this: *mut GfxDirStream, dest: *mut u8, size: u32),
}

impl GfxDirStream {
    /// Read `size` bytes from the stream into `dest`.
    ///
    /// # Safety
    /// `this` must be a valid stream pointer. `dest` must have room for `size` bytes.
    #[inline]
    pub unsafe fn read_raw(this: *mut Self, dest: *mut u8, size: u32) {
        ((*(*this).vtable).read)(this, dest, size);
    }

    /// Returns the number of bytes remaining in the stream.
    #[inline]
    pub unsafe fn remaining_raw(this: *mut Self) -> u32 {
        ((*(*this).vtable).remaining)(this)
    }

    /// Returns the total byte size of this stream's data region.
    #[inline]
    pub unsafe fn total_size_raw(this: *mut Self) -> u32 {
        ((*(*this).vtable).get_total_size)(this)
    }

    /// Destroy the stream reader, releasing its cache slot.
    #[inline]
    pub unsafe fn destroy_raw(this: *mut Self) {
        ((*(*this).vtable).destructor)(this, 1);
    }
}

static mut GFX_LOAD_DIR_ADDR: u32 = 0;

// GfxDir__LoadImage is usercall(ESI=gfx_dir) + 1 stack(name), RET 0x4.
// Returns raw image pointer or null.
static mut GFX_LOAD_IMAGE_ADDR: u32 = 0;

/// Bridge to GfxHandler__LoadDir (0x5663E0).
/// Convention: usercall(EAX=handler), plain RET. Returns nonzero on success.
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn call_gfx_load_dir(handler: *mut u8, addr: u32) -> i32 {
    let result: i32;
    core::arch::asm!(
        "call {addr}",
        addr = in(reg) addr,
        inlateout("eax") handler => result,
        out("ecx") _,
        out("edx") _,
        clobber_abi("C"),
    );
    result
}

#[cfg(target_arch = "x86")]
#[unsafe(naked)]
pub unsafe extern "C" fn call_gfx_load_image(
    _gfx_dir: *mut GfxDir,
    _name: *const core::ffi::c_char,
) -> *mut GfxDirStream {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",     // ESI = gfx_dir
        "pushl 12(%esp)",         // push name (callee cleans via RET 0x4)
        "calll *({addr})",
        "popl %esi",
        "retl",
        addr = sym GFX_LOAD_IMAGE_ADDR,
        options(att_syntax),
    );
}

/// Initialize runtime addresses for GfxHandler bridges.
/// Must be called once at DLL startup.
pub fn init_addrs() {
    unsafe {
        GFX_LOAD_IMAGE_ADDR = rb(va::GFX_DIR_LOAD_IMAGE);
        GFX_LOAD_DIR_ADDR = rb(va::GFX_DIR_LOAD_DIR);
    }
}

/// Returns the rebased runtime address for GfxHandler__LoadDir.
/// Used by callers that pass the address to `call_gfx_load_dir`.
pub fn gfx_load_dir_addr() -> u32 {
    unsafe { GFX_LOAD_DIR_ADDR }
}
