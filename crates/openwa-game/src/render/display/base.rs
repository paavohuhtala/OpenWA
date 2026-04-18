/// DisplayBase — base class of the display subsystem hierarchy.
///
/// Constructor: DisplayBase__Constructor (0x522DB0), stdcall(this) → DisplayBase*.
/// Vtable (primary): 0x6645F8 (set by constructor, has _purecall slots).
/// Vtable (headless overlay): 0x66A0F8 (fills in stub slots for headless mode).
/// Size: 0x3560 bytes.
///
/// Inheritance:
/// ```text
/// DisplayBase (this)       ← vtable 0x6645F8 / 0x66A0F8
///   └─ DisplayGfx (derived)  ← vtable 0x66A218
/// ```
///
/// In headless mode (`GameInfo.headless_mode != 0`), only the base is constructed
/// with the headless vtable overlay. In normal mode, `DisplayGfx` (derived) is
/// constructed instead. The session's `display` field holds a polymorphic pointer
/// to whichever variant.
///
/// ## Vtable identity constraint
///
/// WA code checks vtable pointer addresses (likely RTTI/dynamic_cast). A custom
/// vtable at a different address crashes even with identical content. We must
/// point to WA's vtable in .rdata and patch individual slots there in-place
/// (via VirtualProtect in `display.rs`).
use crate::{
    render::{
        display::font::Font,
        palette::PaletteContext,
        sprite::{Sprite, SpriteBank},
    },
    task::base::Vtable,
    wa_alloc::{wa_malloc_struct_zeroed, wa_malloc_zeroed},
};

#[repr(C)]
pub struct DisplayBase<V: Vtable = *const DisplayBaseVtable> {
    // +0x000: vtable pointer
    pub vtable: V,
    // +0x004: sprite cache wrapper (0x28-byte SpriteCacheWrapper)
    pub sprite_cache: *mut SpriteCache,
    // +0x008..0x1008: per-slot layer ID (0x400 entries), zeroed by ctor.
    // Stores which layer (1-3) each sprite slot belongs to.
    pub sprite_layers: [u32; 0x400],
    // +0x1008..0x2008: per-slot Sprite* pointers (0x400 entries), zeroed by ctor.
    // Points to Sprite objects (0x70 bytes, vtable 0x66418C). Checked by IsSpriteLoaded/GetSpriteInfo.
    pub sprite_ptrs: [*mut Sprite; 0x400],
    // +0x2008..0x3008: per-slot SpriteBank* pointers (0x400 entries), zeroed by ctor.
    // Points to SpriteBank objects (0x17C bytes). Fallback path in
    // GetSpriteInfo and GetSpriteFrameForBlit (vtable slot 33).
    pub sprite_banks: [*mut SpriteBank; 0x400],
    // +0x3008..0x3018: gap (0x10 bytes, 4 u32s: indices 0xC02..0xC05)
    pub _gap_3008: [u32; 4],
    // +0x3018: field at u32 index 0xC06, zeroed by ctor
    pub _field_3018: u32,
    // +0x301C..0x309C: per-font layer index (0x80 bytes, 32 u32s).
    // Records which `layer_contexts[1..=3]` palette context owns each
    // font's color slots. Written by `load_font` (slot 34) — the WA API
    // calls this parameter "mode", but it's the same value space as the
    // layer index used by `set_layer_color` / `set_active_layer` /
    // `update_palette` / `set_layer_visibility`. Read back by
    // `load_font_extension` (slot 35) to recover the owning palette
    // context. Valid values are 1, 2, or 3; only 1 is observed in the
    // shipping game (all 28 fonts go on layer 1).
    pub font_layers: [u32; 32],
    // +0x309C..0x311C: font object pointers (32 entries), zeroed by ctor.
    // Indexed by font_id (valid range 1-31). Used by GetFontInfo, GetFontMetric, SetFontParam.
    pub font_table: [*mut Font; 32],
    // +0x311C..0x312C: layer context pointers (4 entries), zeroed by ctor.
    // Indexed by `Layer` (valid range 1-3); index 0 unused. Returned by
    // set_active_layer (vtable slot 5). Used as palette data input for
    // update_palette (vtable slot 24). `null_mut()` = no context allocated.
    pub layer_contexts: [*mut PaletteContext; 4],
    // +0x312C: Palette slot table guard. Always 0 — prevents palette_slot_alloc
    // from allocating starting at index 0. Part of the contiguous scan area:
    // [slot_table_guard(0)] [slot_table(1s)...] [slot_table_sentinel(-1)].
    pub slot_table_guard: u32,
    // +0x3130..0x352C: Palette slot table — 0xFF entries, all initialized to 1 by ctor.
    // Value 1 = available, 0 = in-use. Scanned by palette_slot_alloc (set_layer_color).
    pub slot_table: [u32; 0xFF],
    // +0x352C: Palette slot table sentinel. Initialized to -1 (0xFFFFFFFF).
    // Terminates the palette_slot_alloc scan with failure if no free slots found.
    pub slot_table_sentinel: u32,
    // +0x3530..0x3540: layer visibility flags (4 entries), zeroed by ctor.
    // Indexed by layer. Cleared to 0 by set_layer_visibility when visible < 0.
    pub layer_visibility: [u32; 4],
    // +0x3540: display initialized flag (set to 1 by DisplayGfx__Init)
    pub display_initialized: u32,
    // +0x3544: unknown
    pub _unknown_3544: u32,
    // +0x3548: display width in pixels (set by DisplayGfx__Init)
    pub display_width: u32,
    // +0x354C: display height in pixels
    pub display_height: u32,
    // +0x3550: clip rect x1 (left, init 0)
    pub clip_x1: i32,
    // +0x3554: clip rect y1 (top, init 0)
    pub clip_y1: i32,
    // +0x3558: clip rect x2 (right, init = width)
    pub clip_x2: i32,
    // +0x355C: clip rect y2 (bottom, init = height)
    pub clip_y2: i32,
}

const _: () = assert!(core::mem::size_of::<DisplayBase>() == 0x3560);

/// Vtable layout for DisplayBase (32 slots based on headless vtable at 0x66A0F8).
///
/// In headless mode, most slots are no-op stubs. The primary vtable (0x6645F8)
/// has _purecall for the drawing slots; the headless overlay replaces them.
#[openwa_game::vtable(size = 32, va = 0x0066_45F8, class = "DisplayBase")]
pub struct DisplayBaseVtable {
    /// destructor
    pub destructor: fn(this: *mut DisplayBase, flags: u8) -> *mut DisplayBase,
    pub slot_01: fn(this: *mut DisplayBase),
    pub slot_02: fn(this: *mut DisplayBase),
    pub slot_03: fn(this: *mut DisplayBase),
    pub slot_04: fn(this: *mut DisplayBase),
    pub slot_05: fn(this: *mut DisplayBase),
    pub slot_06: fn(this: *mut DisplayBase),
    pub slot_07: fn(this: *mut DisplayBase),
    pub slot_08: fn(this: *mut DisplayBase),
    pub slot_09: fn(this: *mut DisplayBase),
    pub slot_10: fn(this: *mut DisplayBase),
    pub slot_11: fn(this: *mut DisplayBase),
    pub slot_12: fn(this: *mut DisplayBase),
    pub slot_13: fn(this: *mut DisplayBase),
    pub slot_14: fn(this: *mut DisplayBase),
    pub slot_15: fn(this: *mut DisplayBase),
    pub slot_16: fn(this: *mut DisplayBase),
    pub slot_17: fn(this: *mut DisplayBase),
    pub slot_18: fn(this: *mut DisplayBase),
    pub slot_19: fn(this: *mut DisplayBase),
    pub slot_20: fn(this: *mut DisplayBase),
    pub slot_21: fn(this: *mut DisplayBase),
    pub slot_22: fn(this: *mut DisplayBase),
    pub slot_23: fn(this: *mut DisplayBase),
    pub slot_24: fn(this: *mut DisplayBase),
    pub slot_25: fn(this: *mut DisplayBase),
    pub slot_26: fn(this: *mut DisplayBase),
    pub slot_27: fn(this: *mut DisplayBase),
    pub slot_28: fn(this: *mut DisplayBase),
    pub slot_29: fn(this: *mut DisplayBase),
    pub slot_30: fn(this: *mut DisplayBase),
    pub slot_31: fn(this: *mut DisplayBase),
}

// ── Sprite cache sub-objects ──────────────────────────────────────────────

/// FrameCache — LRU ring-buffer allocator for decompressed sprite subframe
/// pixels. Allocated as 0x3C bytes by FUN_004fa860; only the first 0x1C are
/// initialized and used by the LRU bookkeeping. The remaining 0x20 bytes are
/// unknown / unused by `FrameCache__Allocate` (0x4FA950).
///
/// Lives at `SpriteCache + 0x4` (one shared cache per `SpriteCache`, NOT per
/// `Sprite`/`SpriteBank`). Both `Sprite__GetFrameForBlit` and
/// `SpriteBank__GetFrameForBlit` allocate decompressed frame surfaces here.
///
/// Each allocation produces a `FrameCacheEntry` (16-byte header + payload)
/// in the ring buffer, owned by the sprite/bank that requested it. When the
/// buffer fills, the oldest entry is evicted and its owner is notified via
/// `owner.vtable[1](owner, frame_idx, payload)` so the owner can clear its
/// per-subframe `decoded_ptr`.
#[repr(C)]
pub struct FrameCache {
    /// +0x00: Pointer to the pixel buffer (0x80020 allocated, 0x80000 used).
    pub buffer: *mut u8,
    /// +0x04: Buffer capacity in bytes (0x80000).
    pub capacity: u32,
    /// +0x08: Current allocation cursor (offset within `buffer`).
    pub write_head: u32,
    /// +0x0C: Read head / wrap point (offset within `buffer`).
    pub wrap_marker: u32,
    /// +0x10: Most recently allocated entry (LRU tail).
    pub tail_entry: *mut FrameCacheEntry,
    /// +0x14: Oldest entry (LRU head, evicted first).
    pub head_entry: *mut FrameCacheEntry,
    /// +0x18: Number of live entries.
    pub entry_count: u32,
    /// +0x1C..0x3C: Trailing 0x20 bytes — allocated but never written by
    /// the constructor. Purpose unknown.
    pub _trailing: [u8; 0x3C - 0x1C],
}

const _: () = assert!(core::mem::size_of::<FrameCache>() == 0x3C);
const _: () = assert!(core::mem::offset_of!(FrameCache, write_head) == 0x08);
const _: () = assert!(core::mem::offset_of!(FrameCache, tail_entry) == 0x10);
const _: () = assert!(core::mem::offset_of!(FrameCache, entry_count) == 0x18);

/// FrameCache entry header (16 bytes, followed inline by `payload_size - 8`
/// bytes of payload).
///
/// Allocated by `FrameCache__Allocate` (0x4FA950) inside the FrameCache ring
/// buffer. The header lays out as:
///
/// - `+0x00 payload_size` is `entry_size + 8` (NOT padded — the function
///   stores the unpadded "data length" here; the actual ring-buffer stride is
///   `((entry_size + 8 + 0xb) & ~3)` and is recomputed each pass).
/// - `+0x04 next` links into the singly-linked LRU chain (head → ... → tail).
/// - `+0x08 frame_idx` and `+0x0c owner` are the third/second stack args from
///   the caller — note the swap relative to the caller's argument order.
///
/// On eviction the cache calls `owner.vtable[1](owner, frame_idx, payload)`
/// so the owner can clear its `decoded_ptr` for that subframe.
#[repr(C)]
pub struct FrameCacheEntry {
    /// +0x00: Stored size (= caller's `entry_size + 8`).
    pub payload_size: u32,
    /// +0x04: Next entry in the LRU list, or null at the tail.
    pub next: *mut FrameCacheEntry,
    /// +0x08: Caller's third stack arg — the subframe cache index used by
    /// the eviction callback to identify which subframe entry to clear.
    pub frame_idx: u32,
    /// +0x0c: Caller's second stack arg — the owner pointer (`*mut Sprite`
    /// or `*mut SpriteBank`). The eviction callback dispatches through
    /// `(*owner).vtable[1]`.
    pub owner: *mut core::ffi::c_void,
    // Followed inline by `payload_size - 8` bytes of payload.
}

const _: () = assert!(core::mem::size_of::<FrameCacheEntry>() == 0x10);
const _: () = assert!(core::mem::offset_of!(FrameCacheEntry, frame_idx) == 0x08);
const _: () = assert!(core::mem::offset_of!(FrameCacheEntry, owner) == 0x0C);

/// Sprite cache (0x28 bytes).
///
/// Constructed by FUN_004fa860 (receives `this` in EDI). Has its own vtable
/// (0x664188, 1 slot) and holds a pointer to the per-process [`FrameCache`].
#[repr(C)]
pub struct SpriteCache {
    /// Vtable pointer (0x664188 in WA, 1 slot)
    pub vtable: *const SpriteCacheVtable,
    /// +0x04: Pointer to the shared `FrameCache`. Read by
    /// `FrameCache__Allocate` (`*(context_ptr + 4)`).
    pub frame_cache: *mut FrameCache,
    pub _pad_08: [u8; 0x28 - 0x08],
}

const _: () = assert!(core::mem::size_of::<SpriteCache>() == 0x28);

/// SpriteCache vtable (0x664188, 1 slot).
///
/// Single-slot vtable for the sprite cache object. Slot 0 is a destructor-like
/// function (0x4FA910).
#[repr(C)]
pub struct SpriteCacheVtable {
    /// Slot 0: destructor / release (0x4FA910)
    pub destructor: unsafe extern "thiscall" fn(this: *mut SpriteCache, flags: u32),
}

/// Ghidra address of the SpriteCache vtable.
const SPRITE_CACHE_VTABLE: u32 = 0x0066_4188;

// ── Construction ──────────────────────────────────────────────────────────

impl DisplayBase {
    /// Construct a DisplayBase for headless mode, entirely in Rust.
    ///
    /// Allocates the struct and sprite cache sub-objects on WA's heap,
    /// initializes all fields to match the original constructor (0x522DB0),
    /// and points to WA's headless vtable in .rdata.
    ///
    /// The vtable must point to WA's .rdata copy (not our own) because WA
    /// checks vtable pointer identity. Individual slots in WA's vtable can
    /// be patched in-place via `display.rs`.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process (uses wa_malloc).
    pub unsafe fn new_headless() -> *mut Self {
        unsafe {
            use crate::address::va;
            use crate::rebase::rb;
            let this = wa_malloc_struct_zeroed::<Self>();

            // Point to WA's headless vtable in .rdata (identity-checked by WA code).
            (*this).vtable = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *const DisplayBaseVtable;

            // Initialize slot_table: 0xFF entries = 1
            for slot in &mut (*this).slot_table {
                *slot = 1;
            }

            // Sentinel value — terminates palette_slot_alloc scan
            (*this).slot_table_sentinel = 0xFFFF_FFFF;

            // Create sprite cache: 0x28 SpriteCache → 0x3C FrameCache → 0x80020 buffer.
            //
            // Original flow (FUN_004fa860):
            //   1. Constructor allocates 0x28 SpriteCache, passes as EDI to FUN_004fa860
            //   2. FUN_004fa860 sets sc[0] = vtable 0x664188
            //   3. Allocates 0x3C FrameCache, sets fc.buffer + fc.capacity
            //   4. Sets sc.frame_cache = fc
            //   5. Returns sc in EAX → stored at this+4
            let sprite_cache = wa_malloc_struct_zeroed::<SpriteCache>();
            (*sprite_cache).vtable = rb(SPRITE_CACHE_VTABLE) as *const SpriteCacheVtable;

            let frame_cache = wa_malloc_struct_zeroed::<FrameCache>();
            // Original allocates 0x80020 but capacity is 0x80000 — extra 0x20 is guard margin.
            (*frame_cache).buffer = wa_malloc_zeroed(0x80020);
            (*frame_cache).capacity = 0x80000;

            (*sprite_cache).frame_cache = frame_cache;
            (*this).sprite_cache = sprite_cache;

            this
        }
    }
}
