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
use crate::task::base::Vtable;

/// Ptr32 alias for raw pointer fields (compiles on 64-bit host).
type Ptr32 = u32;

#[repr(C)]
pub struct DisplayBase<V: Vtable = *const DisplayBaseVtable> {
    // +0x000: vtable pointer
    pub vtable: V,
    // +0x004: sprite cache wrapper (0x28-byte SpriteCacheWrapper)
    pub sprite_cache: Ptr32,
    // +0x008..0x1008: sprite pointer array 0 (0x400 entries), zeroed by ctor.
    // IsSpriteLoaded checks this at offset 0x1008 (which is array 1 below).
    pub sprite_array_0: [u32; 0x400],
    // +0x1008..0x2008: sprite pointer array 1 (0x400 entries), zeroed by ctor.
    pub sprite_array_1: [u32; 0x400],
    // +0x2008..0x3008: sprite pointer array 2 (0x400 entries), zeroed by ctor.
    pub sprite_array_2: [u32; 0x400],
    // +0x3008..0x3018: gap (0x10 bytes, 4 u32s: indices 0xC02..0xC05)
    pub _gap_3008: [u32; 4],
    // +0x3018: field at u32 index 0xC06, zeroed by ctor
    pub _field_3018: u32,
    // +0x301C..0x309C: gap (0x80 bytes, 32 u32s: indices 0xC07..0xC26)
    pub _gap_301c: [u32; 32],
    // +0x309C..0x311C: font object pointers (32 entries), zeroed by ctor.
    // Indexed by font_id (valid range 1-31). Used by GetFontInfo, GetFontMetric, SetFontParam.
    pub font_table: [u32; 32],
    // +0x311C..0x312C: layer context pointers (4 entries), zeroed by ctor.
    // Indexed by layer (valid range 1-3). Returned by set_active_layer (vtable slot 5).
    // Used as palette data input for update_palette (vtable slot 24).
    pub layer_contexts: [u32; 4],
    // +0x312C: index 0xC4B, zeroed by ctor
    pub _field_312c: u32,
    // +0x3130..0x352C: slot_table — 0xFF entries, all initialized to 1 by ctor
    pub slot_table: [u32; 0xFF],
    // +0x352C: index 0xD4B = 0xFFFFFFFF
    pub _field_352c: u32,
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
#[openwa_core::vtable(size = 32, va = 0x0066_45F8, class = "DisplayBase")]
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

/// Sprite buffer control block (0x3C bytes, inner).
/// Allocated by FUN_004fa860. Holds a raw pixel buffer and capacity.
#[repr(C)]
pub struct SpriteBufferCtrl {
    /// Pointer to pixel buffer (0x80020 allocated, 0x80000 used)
    pub buffer: Ptr32,
    /// Buffer capacity (0x80000)
    pub capacity: u32,
    pub _fields_08: [u32; 5],
    pub _pad_1c: [u8; 0x3C - 0x1C],
}

const _: () = assert!(core::mem::size_of::<SpriteBufferCtrl>() == 0x3C);

/// Sprite cache wrapper (0x28 bytes, outer).
/// Constructed by FUN_004fa860 (receives `this` in EDI).
/// Has its own vtable (0x664188) and holds a pointer to [`SpriteBufferCtrl`].
#[repr(C)]
pub struct SpriteCacheWrapper {
    /// Vtable pointer (0x664188 in WA, rebased at runtime)
    pub vtable: Ptr32,
    /// Pointer to the 0x3C-byte buffer control block
    pub buffer_ctrl: Ptr32,
    pub _pad_08: [u8; 0x28 - 0x08],
}

const _: () = assert!(core::mem::size_of::<SpriteCacheWrapper>() == 0x28);

/// Ghidra address of the SpriteCacheWrapper vtable.
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
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::{wa_malloc, WABox};

        // Allocate and zero the entire struct.
        let this = WABox::<Self>::alloc(0x3560, 0x3560).leak();

        // Point to WA's headless vtable in .rdata (identity-checked by WA code).
        (*this).vtable = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *const DisplayBaseVtable;

        // Initialize slot_table: 0xFF entries = 1
        for slot in &mut (*this).slot_table {
            *slot = 1;
        }

        // Sentinel value
        (*this)._field_352c = 0xFFFF_FFFF;

        // Create sprite cache: 0x28 wrapper → 0x3C buffer ctrl → 0x80020 buffer.
        //
        // Original flow (FUN_004fa860):
        //   1. Constructor allocates 0x28 wrapper, passes as EDI to FUN_004fa860
        //   2. FUN_004fa860 sets wrapper[0] = vtable 0x664188
        //   3. Allocates 0x3C buffer ctrl, sets ctrl[0] = buffer, ctrl[4] = capacity
        //   4. Sets wrapper[4] = ctrl pointer
        //   5. Returns wrapper in EAX → stored at this+4
        let wrapper = WABox::<SpriteCacheWrapper>::alloc(0x28, 0x28).leak();
        (*wrapper).vtable = rb(SPRITE_CACHE_VTABLE);

        let ctrl = WABox::<SpriteBufferCtrl>::alloc(0x3C, 0x3C).leak();
        let buf = wa_malloc(0x80020);
        core::ptr::write_bytes(buf, 0, 0x80000);
        (*ctrl).buffer = buf as u32;
        (*ctrl).capacity = 0x80000;

        (*wrapper).buffer_ctrl = ctrl as u32;
        (*this).sprite_cache = wrapper as u32;

        this
    }
}
