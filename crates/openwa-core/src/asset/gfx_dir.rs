//! GfxDir — .dir sprite archive reader.
//!
//! A GfxDir manages a hash table loaded from `.dir` files (e.g. `Gfx.dir`,
//! `Gfx0.dir`). It provides name→resource lookup via a 1024-bucket hash table
//! and delegates file I/O and caching to vtable methods.

#[cfg(target_arch = "x86")]
use core::ffi::{c_char, CStr};

use openwa_core::vtable;

use crate::address::va;
#[cfg(target_arch = "x86")]
use crate::asset::img::DecodedBitGrid;
use crate::bitgrid::BitGrid;
use crate::rebase::rb;
use crate::wa_alloc::{wa_malloc, wa_malloc_struct_zeroed};
#[cfg(target_arch = "x86")]
use crate::{asset, render::palette::PaletteContext};

/// Cache slot within a GfxDir (0x10 bytes).
///
/// Tracks a currently-open stream's read region within the `.dir` archive file.
/// `stream_ptr` is null when the slot is free.
#[repr(C)]
pub struct GfxCacheSlot {
    /// Pointer to the GfxDirStream using this slot, or null if free.
    pub stream_ptr: *mut GfxDirStream,
    /// Absolute file offset of the data region start.
    pub start_offset: u32,
    /// Absolute file offset of the data region end (start + size).
    pub end_offset: u32,
    /// Current read position (between start_offset and end_offset).
    pub current_pos: u32,
}

const _: () = assert!(core::mem::size_of::<GfxCacheSlot>() == 0x10);

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
    /// 0x00C-0x10B: 16 cache slots tracking open stream read regions.
    pub cache_slots: [GfxCacheSlot; 16],
    /// 0x10C-0x14B: Free slot index stack (top at `[slot_count - 1]`).
    pub index_table: [u32; 16],
    /// 0x14C-0x18B: In-use slot index array (count in `inuse_count`).
    pub inuse_slots: [u32; 16],
    /// 0x18C: Number of free cache slots (starts at 16, decremented on alloc).
    pub slot_count: u32,
    /// 0x190: Number of in-use cache slots (starts at 0, incremented on alloc).
    pub inuse_count: u32,
    /// 0x194: Loaded flag (1 after successful gfx_dir_load).
    pub loaded: u32,
    /// 0x198: FILE* handle to the open .dir file.
    pub file_handle: *mut u8,
}

const _: () = assert!(core::mem::size_of::<GfxDir>() == 0x19C);

impl GfxDir {
    /// Allocate and initialize a new GfxDir with the given vtable.
    pub unsafe fn alloc(vtable: *const GfxDirVtable) -> *mut Self {
        let ptr = wa_malloc_struct_zeroed::<Self>();
        if !ptr.is_null() {
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
    /// Slot 1 (0x58BBB0): Absolute seek — `fseek(file, offset, SEEK_SET)`. Returns 1 on success.
    pub seek: fn(this: *mut GfxDir, offset: u32) -> u32,
    /// Slot 2 (0x571AF0): Return cached data pointer for `entry_val`, or null if not mapped.
    pub load_cached: fn(this: *mut GfxDir, entry_val: u32) -> *mut u8,
    /// Slot 3 (0x58BB70): Release resources; if `flags & 1`, free the object.
    pub release: fn(this: *mut GfxDir, flags: u32),
}

bind_GfxDirVtable!(GfxDir, vtable);

/// Entry in a GfxDir hash bucket linked list.
/// Each entry maps a name string to a resource in the `.dir` archive.
#[repr(C)]
pub struct GfxDirEntry {
    pub next: *mut GfxDirEntry,
    /// File offset of the resource data within the `.dir` archive.
    /// Also used as the key for cached resource lookup (vtable[2]).
    pub value: u32,
    /// Size of the resource data in bytes.
    pub data_size: u32,
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
pub unsafe fn gfx_dir_find_entry(name: *const c_char, gfx_dir: *const GfxDir) -> *mut GfxDirEntry {
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
                return entry;
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
        gfx.cache_slots[i as usize].stream_ptr = core::ptr::null_mut();
        gfx.index_table[i as usize] = i;
    }

    gfx.inuse_count = 0;
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

// ─── GfxDir vtable method implementations ───────────────────────────────────

/// Pure Rust implementation of GfxHandler__Read (vtable slot 0, 0x58BBD0).
///
/// `fread(buf, 1, size, self->file_handle)`. Returns `size` on success, 0 on error.
#[cfg(target_arch = "x86")]
pub unsafe extern "thiscall" fn gfx_dir_read(this: *mut GfxDir, buf: *mut u8, size: u32) -> u32 {
    let fread: unsafe extern "cdecl" fn(*mut u8, u32, u32, *mut u8) -> u32 =
        core::mem::transmute(rb(va::WA_FREAD) as usize);
    let bytes_read = fread(buf, 1, size, (*this).file_handle);
    if bytes_read != size {
        0
    } else {
        size
    }
}

/// Pure Rust implementation of GfxHandler__Seek (vtable slot 1, 0x58BBB0).
///
/// `fseek(self->file_handle, offset, SEEK_SET)`. Returns 1 on success, 0 on failure.
#[cfg(target_arch = "x86")]
pub unsafe extern "thiscall" fn gfx_dir_seek(this: *mut GfxDir, offset: u32) -> u32 {
    let fseek: unsafe extern "cdecl" fn(*mut u8, i32, i32) -> i32 =
        core::mem::transmute(rb(va::WA_FSEEK) as usize);
    let result = fseek((*this).file_handle, offset as i32, 0); // SEEK_SET = 0
    if result == 0 {
        1
    } else {
        0
    }
}

/// Pure Rust implementation of GfxHandler__LoadCached (vtable slot 2, 0x571AF0).
///
/// Base implementation is a no-op — always returns null.
pub unsafe extern "thiscall" fn gfx_dir_load_cached(
    _this: *mut GfxDir,
    _entry_val: u32,
) -> *mut u8 {
    core::ptr::null_mut()
}

/// Pure Rust implementation of FUN_566330 (GfxDir cleanup).
///
/// Resets vtable to the "cleaned up" vtable (0x66A1B0), invalidates all
/// active cache slots, and frees the bucket array if it was malloc'd.
#[cfg(target_arch = "x86")]
unsafe fn gfx_dir_cleanup(gfx: *mut GfxDir) {
    // Set vtable to the "cleaned up" vtable (0x66A1B0)
    (*gfx).vtable = rb(0x0066_A1B0) as *const GfxDirVtable;

    if (*gfx).loaded != 0 {
        // Walk in-use slots, invalidate cache entries
        let inuse_count = (*gfx).inuse_count as usize;
        for i in 0..inuse_count {
            let slot_idx = (*gfx).inuse_slots[i] as usize;
            let slot = &mut (*gfx).cache_slots[slot_idx];
            // Clear the stream's backpointer (offset +0x08 in GfxDirStream)
            if !slot.stream_ptr.is_null() {
                (*slot.stream_ptr).gfx_dir = core::ptr::null_mut();
            }
            // Mark slot index as invalid (-1)
            (*slot.stream_ptr).slot_index = 0xFFFF_FFFF;
        }

        if (*gfx).fallback_alloc != 0 {
            crate::wa_alloc::wa_free((*gfx).bucket_array);
        }
    }
}

/// Pure Rust implementation of GfxHandler__Release (vtable slot 3, 0x58BB70).
///
/// Resets vtable, closes the file handle, runs cleanup, and optionally frees self.
#[cfg(target_arch = "x86")]
pub unsafe extern "thiscall" fn gfx_dir_release(this: *mut GfxDir, flags: u32) {
    // Reset vtable to base GfxHandler vtable
    (*this).vtable = rb(va::GFX_DIR_VTABLE) as *const GfxDirVtable;

    if !(*this).file_handle.is_null() {
        let fclose: unsafe extern "cdecl" fn(*mut u8) -> i32 =
            core::mem::transmute(rb(va::WA_FCLOSE) as usize);
        fclose((*this).file_handle);
    }

    gfx_dir_cleanup(this);

    if (flags & 1) != 0 {
        crate::wa_alloc::wa_free(this as *mut u8);
    }
}

/// Pure Rust implementation of IMG__LoadFromDir (0x4F6300).
///
/// Convention: usercall(ECX=gfx_dir, EAX=name) + 1 stack(palette_ctx), RET 0x4.
///
/// Looks up `name` in the GfxDir archive's hash table. If found in cache,
/// decodes via `img_decode_cached`. Otherwise loads from the archive stream
/// and decodes via `img_decode`.
///
/// # Safety
/// All pointers must be valid WA objects.
#[cfg(target_arch = "x86")]
pub unsafe fn img_load_from_dir(
    gfx_dir: *mut GfxDir,
    name: *const c_char,
    palette_ctx: *mut PaletteContext,
) -> *mut BitGrid {
    use crate::asset::img::{img_decode, img_decode_cached};

    // 1. Try FindEntry → cached load → decode from raw buffer
    let entry = gfx_dir_find_entry(name, gfx_dir);
    if !entry.is_null() {
        let entry_val = (*entry).value;
        let cached = GfxDir::load_cached_raw(gfx_dir, entry_val);
        if !cached.is_null() {
            return img_decode_cached(palette_ctx, cached) as *mut BitGrid;
        }
    }

    // 2. Fallback: LoadImage → IMG_Decode from stream
    let raw_image = gfx_dir_load_image(gfx_dir, name);
    if raw_image.is_null() {
        return core::ptr::null_mut();
    }

    let result = match img_decode(palette_ctx, raw_image, 1) {
        Some(decoded) => decoded.as_bitgrid_ptr(),
        None => core::ptr::null_mut(),
    };
    GfxDirStream::destroy_raw(raw_image);

    result
}

/// Helper: find entry in GfxDir and load image, or load directly.
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn call_gfx_find_and_load(
    gfx_dir: *mut GfxDir,
    name: &CStr,
    palette_ctx: *mut PaletteContext,
) -> Option<DecodedBitGrid> {
    let name_ptr = name.as_ptr();
    let entry = gfx_dir_find_entry(name_ptr, gfx_dir);

    if !entry.is_null() {
        let cached = GfxDir::load_cached_raw(gfx_dir, (*entry).value);
        if !cached.is_null() {
            let grid = asset::img::img_decode_cached(palette_ctx, cached);
            if !grid.is_null() {
                return Some(DecodedBitGrid::Display(grid));
            }
            return None;
        }
    }

    // Fallback: load image directly
    call_gfx_load_and_wrap(gfx_dir, name_ptr.cast(), palette_ctx)
}

/// Helper: load image via GfxDir__LoadImage + IMG_Decode.
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn call_gfx_load_and_wrap(
    gfx_dir: *mut GfxDir,
    name: *const c_char,
    palette_ctx: *mut PaletteContext,
) -> Option<DecodedBitGrid> {
    let image = gfx_dir_load_image(gfx_dir, name);
    if image.is_null() {
        return None;
    }
    let result = asset::img::img_decode(palette_ctx, image, 1);
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
    pub gfx_dir: *mut GfxDir,
}

/// Vtable for GfxDirStream (6 slots at 0x66A1C0).
///
/// Provides a sequential read interface over cached resource data within
/// a `.dir` archive. The stream reads from a cache slot in the parent GfxDir.
#[vtable(size = 6, va = 0x0066_A1C0, class = "GfxDirStream")]
pub struct GfxDirStreamVtable {
    /// Slot 0 (0x5661E0): Destructor — releases cache slot, optionally frees.
    pub destructor: fn(this: *mut GfxDirStream, flags: u32),
    /// Slot 1 (0x566210): Returns 1 if current read position < end position.
    pub has_data: fn(this: *mut GfxDirStream) -> u32,
    /// Slot 2 (0x566240): Returns bytes consumed (current_pos - start_offset).
    pub bytes_consumed: fn(this: *mut GfxDirStream) -> u32,
    /// Slot 3 (0x566270): Seek — repositions read cursor within [start, end].
    pub seek: fn(this: *mut GfxDirStream, offset: u32) -> u32,
    /// Slot 4 (0x5662C0): Returns total byte size (end_offset - start_offset).
    pub get_total_size: fn(this: *mut GfxDirStream) -> u32,
    /// Slot 5 (0x5662F0): Read `size` bytes into `dest`.
    pub read: fn(this: *mut GfxDirStream, dest: *mut u8, size: u32),
}

bind_GfxDirStreamVtable!(GfxDirStream, vtable);

impl GfxDirStream {
    /// Destroy the stream reader, releasing its cache slot.
    #[inline]
    pub unsafe fn destroy_raw(this: *mut Self) {
        Self::destructor_raw(this, 1);
    }

    /// Returns a pointer to this stream's cache slot in the parent GfxDir.
    /// Returns null if the stream is invalid (null gfx_dir or slot_index >= 16).
    #[inline]
    unsafe fn cache_slot(this: *mut Self) -> *mut GfxCacheSlot {
        let gfx = (*this).gfx_dir;
        if gfx.is_null() {
            return core::ptr::null_mut();
        }
        let idx = (*this).slot_index;
        if idx >= 16 {
            return core::ptr::null_mut();
        }
        &raw mut (*gfx).cache_slots[idx as usize]
    }
}

// ─── GfxDirStream vtable method implementations ─────────────────────────────

/// Pure Rust implementation of GfxDirStream has_data (vtable slot 1, 0x566210).
///
/// Returns 1 if `current_pos < end_offset`, 0 otherwise.
pub unsafe extern "thiscall" fn gfx_dir_stream_has_data(this: *mut GfxDirStream) -> u32 {
    let slot = GfxDirStream::cache_slot(this);
    if slot.is_null() || (*slot).stream_ptr.is_null() {
        return 0;
    }
    if (*slot).current_pos < (*slot).end_offset {
        1
    } else {
        0
    }
}

/// Pure Rust implementation of GfxDirStream bytes_consumed (vtable slot 2, 0x566240).
///
/// Returns `current_pos - start_offset` (bytes consumed from data start).
pub unsafe extern "thiscall" fn gfx_dir_stream_bytes_consumed(this: *mut GfxDirStream) -> u32 {
    let slot = GfxDirStream::cache_slot(this);
    if slot.is_null() || (*slot).stream_ptr.is_null() {
        return 0;
    }
    (*slot).current_pos.wrapping_sub((*slot).start_offset)
}

/// Pure Rust implementation of GfxDirStream seek (vtable slot 3, 0x566270).
///
/// Repositions the read cursor to `start_offset + offset`, clamped to [start, end].
/// Returns 1 on success, 0 on failure.
pub unsafe extern "thiscall" fn gfx_dir_stream_seek(this: *mut GfxDirStream, offset: u32) -> u32 {
    let slot = GfxDirStream::cache_slot(this);
    if slot.is_null() || (*slot).stream_ptr.is_null() {
        return 0;
    }
    let start = (*slot).start_offset;
    let end = (*slot).end_offset;
    let mut new_pos = start.wrapping_add(offset);
    // Clamp: if wrapping underflow (new_pos < start), use start
    if new_pos < start {
        new_pos = start;
    }
    if new_pos > end {
        new_pos = end;
    }
    (*slot).current_pos = new_pos;
    1
}

/// Pure Rust implementation of GfxDirStream get_total_size (vtable slot 4, 0x5662C0).
///
/// Returns `end_offset - start_offset` (total data size), or -1 if gfx_dir is null.
pub unsafe extern "thiscall" fn gfx_dir_stream_get_total_size(this: *mut GfxDirStream) -> u32 {
    if (*this).gfx_dir.is_null() {
        return 0xFFFF_FFFF; // -1 as u32, matching original
    }
    let slot = GfxDirStream::cache_slot(this);
    if slot.is_null() || (*slot).stream_ptr.is_null() {
        return 0;
    }
    (*slot).end_offset.wrapping_sub((*slot).start_offset)
}

/// Pure Rust implementation of cache slot release (FUN_566640).
///
/// Finds the stream's slot in the parent GfxDir's in-use list, removes it,
/// and returns the slot index to the free list. Clears the slot's stream_ptr.
///
/// Returns 1 on success, 0 if the slot was not found.
pub unsafe fn gfx_dir_release_slot(gfx: *mut GfxDir, stream: *mut GfxDirStream) -> u32 {
    if stream.is_null() {
        return 0;
    }
    let slot_idx = (*stream).slot_index;
    if slot_idx >= 16 || (*gfx).cache_slots[slot_idx as usize].stream_ptr.is_null() {
        return 0;
    }

    // Find slot_idx in the in-use list
    let inuse_count = (*gfx).inuse_count as usize;
    let mut found = false;
    let mut found_pos = 0usize;
    for i in 0..inuse_count {
        if (*gfx).inuse_slots[i] == slot_idx {
            found = true;
            found_pos = i;
            break;
        }
    }
    if !found {
        return 0;
    }

    // Remove from in-use list: swap with last, decrement count
    let new_inuse_count = inuse_count - 1;
    (*gfx).inuse_count = new_inuse_count as u32;
    (*gfx).inuse_slots[found_pos] = (*gfx).inuse_slots[new_inuse_count];

    // Push slot index back to free list
    let free_idx = (*gfx).slot_count as usize;
    (*gfx).index_table[free_idx] = slot_idx;
    (*gfx).slot_count += 1;

    // Clear the cache slot
    (*gfx).cache_slots[slot_idx as usize].stream_ptr = core::ptr::null_mut();

    1
}

/// Pure Rust implementation of GfxDirStream read helper (FUN_566760).
///
/// Reads data from the parent GfxDir's file through its vtable, advancing
/// the cache slot's current read position.
#[cfg(target_arch = "x86")]
unsafe fn gfx_dir_stream_read_inner(
    gfx: *mut GfxDir,
    slot_idx: u32,
    dest: *mut u8,
    size: u32,
) -> u32 {
    if slot_idx >= 16 || (*gfx).cache_slots[slot_idx as usize].stream_ptr.is_null() {
        return 0;
    }

    let slot = &mut (*gfx).cache_slots[slot_idx as usize];
    let current = slot.current_pos;
    let end = slot.end_offset;

    // Clamp read size to remaining bytes
    let mut actual_end = current.wrapping_add(size);
    if actual_end > end {
        actual_end = end;
    }
    let actual_size = actual_end.wrapping_sub(current);
    if actual_size == 0 {
        return 0;
    }

    // Seek parent GfxDir to current position (vtable slot 1 = absolute seek)
    let seek_ok = GfxDir::seek_raw(gfx, current);
    if seek_ok == 0 {
        return 0;
    }

    // Read through parent GfxDir (vtable slot 0)
    let bytes_read = GfxDir::read_raw(gfx, dest, actual_size);
    if bytes_read != actual_size {
        return 0;
    }

    // Advance read position
    slot.current_pos = current.wrapping_add(actual_size);

    actual_size
}

/// Pure Rust implementation of GfxDirStream destructor (vtable slot 0, 0x5661E0).
///
/// Releases the cache slot and optionally frees the stream object.
#[cfg(target_arch = "x86")]
pub unsafe extern "thiscall" fn gfx_dir_stream_destructor(this: *mut GfxDirStream, flags: u32) {
    // Set vtable to GfxDirStream vtable (matches original behavior)
    (*this).vtable = rb(va::GFX_DIR_STREAM_VTABLE) as *const GfxDirStreamVtable;

    // Release the cache slot in the parent GfxDir
    let gfx = (*this).gfx_dir;
    if !gfx.is_null() {
        gfx_dir_release_slot(gfx, this);
    }

    // Reset vtable to the "released" vtable (0x66A198)
    (*this).vtable = rb(0x0066_A198) as *const GfxDirStreamVtable;

    if (flags & 1) != 0 {
        crate::wa_alloc::wa_free(this as *mut u8);
    }
}

/// Pure Rust implementation of GfxDirStream read (vtable slot 5, 0x5662F0).
///
/// Reads `size` bytes from the stream into `dest` by delegating to the
/// parent GfxDir's file I/O vtable methods.
#[cfg(target_arch = "x86")]
pub unsafe extern "thiscall" fn gfx_dir_stream_read(
    this: *mut GfxDirStream,
    dest: *mut u8,
    size: u32,
) {
    let gfx = (*this).gfx_dir;
    if gfx.is_null() {
        return;
    }
    gfx_dir_stream_read_inner(gfx, (*this).slot_index, dest, size);
}

/// Pure Rust implementation of GfxDir__LoadImage (0x5666D0).
///
/// Convention: usercall(ESI=gfx_dir) + 1 stack(name), RET 0x4.
///
/// Looks up `name` in the GfxDir's hash table, allocates a cache slot,
/// creates a GfxDirStream for sequential reading, and returns it.
/// Returns null if no free slots or name not found.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_dir_load_image(gfx_dir: *mut GfxDir, name: *const c_char) -> *mut GfxDirStream {
    // Check if there are free cache slots
    if (*gfx_dir).slot_count == 0 {
        return core::ptr::null_mut();
    }

    // Look up the entry
    let entry = gfx_dir_find_entry(name, gfx_dir);
    if entry.is_null() {
        return core::ptr::null_mut();
    }
    let entry = entry as *const GfxDirEntry;

    // Pop a free slot from index_table
    (*gfx_dir).slot_count -= 1;
    let slot_idx = (*gfx_dir).index_table[(*gfx_dir).slot_count as usize];

    // Push to in-use list
    let inuse_pos = (*gfx_dir).inuse_count as usize;
    (*gfx_dir).inuse_slots[inuse_pos] = slot_idx;
    (*gfx_dir).inuse_count += 1;

    // Allocate the 0xC-byte GfxDirStream object
    let stream = wa_malloc(0xC) as *mut GfxDirStream;
    if stream.is_null() {
        return core::ptr::null_mut();
    }

    // Initialize stream
    (*stream).vtable = rb(va::GFX_DIR_STREAM_VTABLE) as *const GfxDirStreamVtable;
    (*stream).gfx_dir = gfx_dir;
    (*stream).slot_index = slot_idx;

    // Set up cache slot
    let slot = &mut (*gfx_dir).cache_slots[slot_idx as usize];
    slot.stream_ptr = stream;
    slot.start_offset = (*entry).value;
    slot.end_offset = (*entry).value.wrapping_add((*entry).data_size);
    slot.current_pos = (*entry).value;

    stream
}
