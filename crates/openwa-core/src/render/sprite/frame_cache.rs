//! LRU ring-buffer allocator for sprite subframe payloads.
//!
//! Pure-Rust port of `FrameCache__Allocate` (0x4FA950). The shared
//! [`FrameCache`] holds decompressed pixel data for the most recently
//! requested sprite/sprite-bank subframes. When the buffer fills, the
//! oldest entry is evicted and its owner is notified via
//! `vtable[1]` (`on_subframe_evicted`) so it can clear its
//! `decoded_ptr` and re-request decompression on the next render.
//!
//! Each allocated entry has a 16-byte header ([`FrameCacheEntry`]) followed
//! inline by the payload bytes. The returned pointer is the payload start
//! (`entry + 0x10`).
//!
//! See the slot 33 docstring in [`crate::render::display::vtable`] for the
//! full reverse-engineering notes and the `Sprite__GetFrameForBlit` /
//! `SpriteBank__GetFrameForBlit` callers.

use core::ffi::c_void;
use core::ptr::null_mut;

use crate::render::SpriteCache;
#[cfg(test)]
use crate::render::display::base::FrameCache;
use crate::render::display::base::FrameCacheEntry;

crate::define_addresses! {
    class "FrameCache" {
        /// `FrameCache__Allocate` (FUN_004FA950) — usercall LRU
        /// allocator. `EAX = entry_size`, stack args
        /// `(context_ptr, owner, frame_idx)`. Ported to
        /// [`frame_cache_allocate`]; only callers were
        /// `Sprite__GetFrameForBlit` and `SpriteBank__GetFrameForBlit`,
        /// both bypassed by the slot 33 vtable_replace.
        fn/Usercall FRAME_CACHE_ALLOCATE = 0x004F_A950;
    }
}

/// Eviction callback signature — `vtable[1]` of both `Sprite` and
/// `SpriteBank`. Called when the oldest entry is being dropped from the
/// LRU ring buffer; the callback clears the owner's per-subframe
/// `decoded_ptr` so the next render re-decompresses.
///
/// `this` is `victim.owner`. `subframe_idx` is `victim.frame_idx` (the
/// caller's third stack arg from when the entry was allocated). `payload`
/// is `victim + 0x10` — the payload start, which the original asm pushes
/// but neither callback uses.
type EvictCallback =
    unsafe extern "thiscall" fn(this: *mut c_void, subframe_idx: u32, payload: *mut u8);

/// Pure-Rust port of `FrameCache__Allocate` (0x4FA950).
///
/// Allocates `entry_size` bytes of payload from the [`FrameCache`] reached
/// via `(*context_ptr).frame_cache`. On success, returns a pointer to the
/// payload start. On contention, evicts the oldest live entry (notifying
/// its owner via `vtable[1]`) and retries.
///
/// # Layout details
///
/// - The header allocation occupies `pad = ((entry_size + 8 + 0xb) & ~3)`
///   bytes total (= `entry_size + 8` rounded up to a 4-byte stride after
///   adding 11 bytes of slack).
/// - The `payload_size` field stored in the entry header is
///   `entry_size + 8` (NOT padded — the original asm computes the padded
///   stride only for advancing `write_head`, never stores it).
/// - On the long-backref `entry_count` cleanup branch where the new head
///   becomes null, the original asm clears `wrap_marker` and `write_head`
///   to 0 — the buffer is now empty and the next alloc starts from the
///   beginning.
///
/// # Calling convention
///
/// The original is a usercall: `EAX = entry_size`, stack args
/// `(context_ptr, owner, frame_idx)`. The Rust port takes these as
/// regular params; callers should pass the same values.
///
/// # Safety
///
/// - `context_ptr` must point at a valid `SpriteCache` whose `frame_cache`
///   field references a `FrameCache` initialized via `FUN_004fa860`
///   (with a non-null `buffer` of at least `entry_size + 0x13` bytes).
/// - `owner` must point at an object whose first field is a vtable
///   pointer with `vtable[1]` matching [`EvictCallback`]. In practice
///   this is `*mut Sprite` or `*mut SpriteBank`, both of whose
///   `on_subframe_evicted` callbacks share the signature.
/// - `entry_size` must be small enough to fit in the buffer
///   (`entry_size + 0x13 <= capacity`); otherwise this loops forever.
pub unsafe fn frame_cache_allocate(
    entry_size: u32,
    context_ptr: *mut SpriteCache,
    owner: *mut c_void,
    frame_idx: u32,
) -> *mut u8 {
    let payload_size = entry_size + 8;
    let pad = (entry_size + 0x13) & !3;

    loop {
        let fc = (*context_ptr).frame_cache;
        let write = (*fc).write_head;
        let wrap = (*fc).wrap_marker;
        let end = write + pad;

        // Decide where (if anywhere) the new entry can go in this pass.
        let placed_offset: Option<u32> = if write < wrap {
            // Case A: write_head sits behind wrap_marker (a previously
            // wrapped buffer). Allocation succeeds iff it doesn't reach
            // wrap_marker.
            if end <= wrap { Some(write) } else { None }
        } else {
            // Case B: write_head ahead of wrap_marker. Allocation
            // succeeds iff it fits before the end of the buffer; if it
            // doesn't, try wrapping to offset 0 — that succeeds iff
            // `pad <= wrap_marker` (head_entry isn't sitting in the
            // [0, pad) range).
            if end <= (*fc).capacity {
                Some(write)
            } else if pad <= wrap {
                Some(0)
            } else {
                None
            }
        };

        if let Some(offset) = placed_offset {
            let entry_addr = (*fc).buffer.add(offset as usize);
            let entry = entry_addr as *mut FrameCacheEntry;

            (*entry).payload_size = payload_size;
            (*entry).next = null_mut();

            if !(*fc).tail_entry.is_null() {
                (*(*fc).tail_entry).next = entry;
            }
            (*fc).tail_entry = entry;
            if (*fc).head_entry.is_null() {
                (*fc).head_entry = entry;
            }
            (*fc).entry_count += 1;
            (*fc).write_head = offset + pad;

            // Note: original asm writes frame_idx and owner AFTER the
            // POPAD/POP sequence — this is functionally equivalent.
            (*entry).frame_idx = frame_idx;
            (*entry).owner = owner;

            return entry_addr.add(0x10);
        }

        // ── Out of room: evict the oldest entry and retry ───────────
        //
        // The original asm reloads `fc` from `*(context_ptr + 4)` here
        // (in case the eviction callback somehow rebinds it). In practice
        // both real callbacks just clear a `decoded_ptr` field, but we
        // mirror the reload exactly.
        let fc = (*context_ptr).frame_cache;
        let head = (*fc).head_entry;
        if head.is_null() {
            // Theoretically unreachable: an empty cache has wrap=write=0
            // and thus must always fit; if it doesn't, `pad > capacity`
            // and the original asm spins forever. We mirror that.
            continue;
        }

        // Notify the owner that its subframe payload is being dropped.
        // Both Sprite and SpriteBank vtable[1] callbacks share the same
        // shape: thiscall(this, frame_idx, payload). The asm pushes
        // `payload` first then `frame_idx` (so `frame_idx` is the first
        // stack arg), with ECX = owner.
        let victim_owner = (*head).owner;
        let victim_frame_idx = (*head).frame_idx;
        let victim_payload = (head as *mut u8).add(0x10);

        // Read vtable[1] manually so we don't need to commit to either
        // SpriteVtable or SpriteBankVtable as the callback's `this` type.
        let vtable = *(victim_owner as *const *const u32);
        let slot1 = *vtable.add(1);
        let callback: EvictCallback = core::mem::transmute(slot1);
        callback(victim_owner, victim_frame_idx, victim_payload);

        // Reload fc and unlink the (just-notified) head entry.
        let fc = (*context_ptr).frame_cache;
        if (*fc).head_entry.is_null() {
            continue;
        }
        let new_head = (*(*fc).head_entry).next;
        (*fc).head_entry = new_head;
        if new_head.is_null() {
            // Last entry just removed — clear the bookkeeping back to
            // the empty state.
            (*fc).entry_count -= 1;
            (*fc).tail_entry = null_mut();
            (*fc).wrap_marker = 0;
            (*fc).write_head = 0;
        } else {
            // wrap_marker tracks where the head entry now sits within
            // the buffer (as a byte offset from `buffer`).
            (*fc).entry_count -= 1;
            (*fc).wrap_marker = (new_head as u32).wrapping_sub((*fc).buffer as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::MaybeUninit;

    /// A minimal owner whose vtable[1] increments a counter when invoked.
    #[repr(C)]
    struct MockOwner {
        vtable: *const MockVtable,
        evict_count: u32,
        last_idx: u32,
    }

    #[repr(C)]
    struct MockVtable {
        slot_0: usize,
        slot_1: unsafe extern "thiscall" fn(*mut c_void, u32, *mut u8),
    }

    unsafe extern "thiscall" fn mock_evict(this: *mut c_void, idx: u32, _payload: *mut u8) {
        let owner = this as *mut MockOwner;
        (*owner).evict_count += 1;
        (*owner).last_idx = idx;
    }

    static MOCK_VTABLE: MockVtable = MockVtable {
        slot_0: 0,
        slot_1: mock_evict,
    };

    /// Build a freshly-zeroed FrameCache with a heap-allocated buffer.
    /// Returns the cache (heap-allocated, must be freed by the caller).
    fn make_cache(capacity: u32) -> (*mut SpriteCache, *mut FrameCache, *mut u8) {
        let buf = unsafe {
            let layout = core::alloc::Layout::from_size_align(capacity as usize, 4).unwrap();
            std::alloc::alloc_zeroed(layout)
        };
        let fc = Box::leak(Box::new(unsafe {
            MaybeUninit::<FrameCache>::zeroed().assume_init()
        })) as *mut FrameCache;
        unsafe {
            (*fc).buffer = buf;
            (*fc).capacity = capacity;
        }
        let sc = Box::leak(Box::new(unsafe {
            MaybeUninit::<SpriteCache>::zeroed().assume_init()
        })) as *mut SpriteCache;
        unsafe {
            (*sc).frame_cache = fc;
        }
        (sc, fc, buf)
    }

    fn free_cache(sc: *mut SpriteCache, fc: *mut FrameCache, buf: *mut u8, capacity: u32) {
        unsafe {
            let layout = core::alloc::Layout::from_size_align(capacity as usize, 4).unwrap();
            std::alloc::dealloc(buf, layout);
            drop(Box::from_raw(fc));
            drop(Box::from_raw(sc));
        }
    }

    /// Single allocation: header + payload, returns payload start.
    #[test]
    fn alloc_single() {
        let (sc, fc, buf) = make_cache(0x1000);
        let mut owner = MockOwner {
            vtable: &MOCK_VTABLE,
            evict_count: 0,
            last_idx: 0,
        };
        unsafe {
            let p = frame_cache_allocate(0x100, sc, &mut owner as *mut _ as *mut c_void, 7);

            // Returned pointer is buffer + 0x10
            assert_eq!(p, buf.add(0x10));

            // Header fields written
            let entry = buf as *mut FrameCacheEntry;
            assert_eq!((*entry).payload_size, 0x100 + 8);
            assert!((*entry).next.is_null());
            assert_eq!((*entry).frame_idx, 7);
            assert_eq!((*entry).owner, &mut owner as *mut _ as *mut c_void);

            // Cache bookkeeping
            assert_eq!((*fc).head_entry, entry);
            assert_eq!((*fc).tail_entry, entry);
            assert_eq!((*fc).entry_count, 1);
            // pad = ((entry_size + 0x13) & ~3) = ((0x100 + 0x13) & ~3) = 0x110
            assert_eq!((*fc).write_head, 0x110);
        }
        free_cache(sc, fc, buf, 0x1000);
    }

    /// Two allocations chain into the LRU list.
    #[test]
    fn alloc_two_chain() {
        let (sc, fc, buf) = make_cache(0x1000);
        let mut owner = MockOwner {
            vtable: &MOCK_VTABLE,
            evict_count: 0,
            last_idx: 0,
        };
        unsafe {
            let owner_ptr = &mut owner as *mut _ as *mut c_void;
            let _p1 = frame_cache_allocate(0x100, sc, owner_ptr, 1);
            let _p2 = frame_cache_allocate(0x100, sc, owner_ptr, 2);

            let e1 = buf as *mut FrameCacheEntry;
            let e2 = buf.add(0x110) as *mut FrameCacheEntry;
            assert_eq!((*e1).next, e2);
            assert!((*e2).next.is_null());
            assert_eq!((*fc).head_entry, e1);
            assert_eq!((*fc).tail_entry, e2);
            assert_eq!((*fc).entry_count, 2);
            assert_eq!((*fc).write_head, 0x220);
        }
        free_cache(sc, fc, buf, 0x1000);
    }

    /// Eviction: fill the buffer, then a third alloc that won't fit
    /// triggers eviction of the oldest entry and re-uses its space at
    /// the start of the buffer (after wrap).
    #[test]
    fn alloc_triggers_eviction_and_wrap() {
        // pad = 0x110 per entry. Capacity 0x300 → two entries fit at
        // [0..0x110, 0x110..0x220]. A third 0x100 alloc would need
        // [0x220..0x330] which exceeds capacity → must evict + wrap.
        let (sc, fc, buf) = make_cache(0x300);
        let mut owner = MockOwner {
            vtable: &MOCK_VTABLE,
            evict_count: 0,
            last_idx: 0,
        };
        unsafe {
            let owner_ptr = &mut owner as *mut _ as *mut c_void;
            let _p1 = frame_cache_allocate(0x100, sc, owner_ptr, 1);
            let _p2 = frame_cache_allocate(0x100, sc, owner_ptr, 2);

            // Sanity: wrap_marker is still 0 (no eviction yet).
            assert_eq!((*fc).wrap_marker, 0);
            assert_eq!((*fc).write_head, 0x220);

            let p3 = frame_cache_allocate(0x100, sc, owner_ptr, 3);

            // Eviction was called once with the first entry's frame_idx.
            assert_eq!(owner.evict_count, 1);
            assert_eq!(owner.last_idx, 1);

            // After evicting entry 1, head_entry advanced to entry 2;
            // wrap_marker = (entry 2 offset) = 0x110.
            assert_eq!((*fc).wrap_marker, 0x110);

            // Third alloc wrapped to offset 0 (where entry 1 used to be);
            // p3 should be buf + 0x10.
            assert_eq!(p3, buf.add(0x10));

            // Bookkeeping: 2 live entries (entry 2, entry 3), entry 3 is tail.
            assert_eq!((*fc).entry_count, 2);
            let e2 = buf.add(0x110) as *mut FrameCacheEntry;
            let e3 = buf as *mut FrameCacheEntry;
            assert_eq!((*fc).head_entry, e2);
            assert_eq!((*fc).tail_entry, e3);
            assert_eq!((*e2).next, e3);
            assert!((*e3).next.is_null());
        }
        free_cache(sc, fc, buf, 0x300);
    }
}
