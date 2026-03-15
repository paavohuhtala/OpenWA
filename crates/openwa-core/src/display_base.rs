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

/// Ptr32 alias for raw pointer fields (compiles on 64-bit host).
type Ptr32 = u32;

#[repr(C)]
pub struct DisplayBase {
    // +0x000: vtable pointer
    pub vtable: *const DisplayBaseVtable,
    // +0x004: sprite collection sub-object (0x3C control block + 0x80000 buffer)
    pub sprite_collection: Ptr32,
    // +0x008..0x3008: three contiguous 0x400-entry u32 arrays, all zeroed by ctor
    pub _buf_008: [u32; 0xC00],
    // +0x3008..0x3018: gap (0x10 bytes, 4 u32s: indices 0xC02..0xC05)
    pub _gap_3008: [u32; 4],
    // +0x3018: field at u32 index 0xC06, zeroed by ctor
    pub _field_3018: u32,
    // +0x301C..0x309C: gap (0x80 bytes, 32 u32s: indices 0xC07..0xC26)
    pub _gap_301c: [u32; 32],
    // +0x309C..0x311C: indices 0xC27..0xC46 (32 entries), zeroed by ctor
    pub _fields_309c: [u32; 32],
    // +0x311C..0x312C: indices 0xC47..0xC4A (4 entries), zeroed by ctor
    pub _fields_311c: [u32; 4],
    // +0x312C: index 0xC4B, zeroed by ctor
    pub _field_312c: u32,
    // +0x3130..0x352C: slot_table — 0xFF entries, all initialized to 1 by ctor
    pub slot_table: [u32; 0xFF],
    // +0x352C: index 0xD4B = 0xFFFFFFFF
    pub _field_352c: u32,
    // +0x3530..0x3540: indices 0xD4C..0xD4F (4 entries), zeroed by ctor
    pub _fields_3530: [u32; 4],
    // +0x3540..0x3560: remaining padding to fill 0x3560 total
    pub _pad_3540: [u8; 0x3560 - 0x3540],
}

const _: () = assert!(core::mem::size_of::<DisplayBase>() == 0x3560);

/// Vtable layout for DisplayBase (32 slots based on headless vtable at 0x66A0F8).
///
/// In headless mode, most slots are no-op stubs. The primary vtable (0x6645F8)
/// has _purecall for the drawing slots; the headless overlay replaces them.
#[repr(C)]
pub struct DisplayBaseVtable {
    /// Slot 0: destructor — thiscall(this, flags)
    pub destructor: unsafe extern "thiscall" fn(*mut DisplayBase, u8) -> *mut DisplayBase,
    /// Slot 1: thiscall(this)
    pub slot_01: unsafe extern "thiscall" fn(*mut DisplayBase),
    /// Slot 2-3: thiscall(this) — identical in headless (0x4AA060)
    pub slot_02: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_03: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_04: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_05: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_06: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_07: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_08: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_09: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_10: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_11: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_12: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_13: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_14: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_15: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_16: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_17: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_18: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_19: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_20: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_21: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_22: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_23: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_24: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_25: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_26: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_27: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_28: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_29: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_30: unsafe extern "thiscall" fn(*mut DisplayBase),
    pub slot_31: unsafe extern "thiscall" fn(*mut DisplayBase),
}

const _: () = assert!(core::mem::size_of::<DisplayBaseVtable>() == 32 * 4);

// ── No-op stub functions for headless vtable ──────────────────────────────

unsafe extern "thiscall" fn noop_thiscall(_this: *mut DisplayBase) {}

unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
    // Free sprite collection sub-object if present.
    let sc = (*this).sprite_collection;
    if sc != 0 {
        let sc_ptr = sc as *mut SpriteCollection;
        // Free the buffer, then the control block.
        let buf = (*sc_ptr).buffer;
        if buf != 0 {
            wa_free(buf as *mut u8);
        }
        wa_free(sc_ptr as *mut u8);
    }
    // If bit 0 of flags is set, free `this` too (scalar delete).
    if flags & 1 != 0 {
        wa_free(this as *mut u8);
    }
    this
}

/// Static headless vtable — all slots are no-ops except the destructor.
///
/// In headless mode there's no rendering, so every drawing method is a no-op.
/// The "shared real implementation" slots (4-10, 27, 29-31) that exist in the
/// original vtable are also no-ops here since they're never called in headless.
static HEADLESS_VTABLE: DisplayBaseVtable = DisplayBaseVtable {
    destructor: headless_destructor,
    slot_01: noop_thiscall,
    slot_02: noop_thiscall,
    slot_03: noop_thiscall,
    slot_04: noop_thiscall,
    slot_05: noop_thiscall,
    slot_06: noop_thiscall,
    slot_07: noop_thiscall,
    slot_08: noop_thiscall,
    slot_09: noop_thiscall,
    slot_10: noop_thiscall,
    slot_11: noop_thiscall,
    slot_12: noop_thiscall,
    slot_13: noop_thiscall,
    slot_14: noop_thiscall,
    slot_15: noop_thiscall,
    slot_16: noop_thiscall,
    slot_17: noop_thiscall,
    slot_18: noop_thiscall,
    slot_19: noop_thiscall,
    slot_20: noop_thiscall,
    slot_21: noop_thiscall,
    slot_22: noop_thiscall,
    slot_23: noop_thiscall,
    slot_24: noop_thiscall,
    slot_25: noop_thiscall,
    slot_26: noop_thiscall,
    slot_27: noop_thiscall,
    slot_28: noop_thiscall,
    slot_29: noop_thiscall,
    slot_30: noop_thiscall,
    slot_31: noop_thiscall,
};

// ── Sprite collection sub-object ──────────────────────────────────────────

/// Sprite collection control block (0x3C bytes).
/// Created by FUN_004fa860 in the original constructor.
#[repr(C)]
pub struct SpriteCollection {
    /// Pointer to 0x80000-byte buffer
    pub buffer: Ptr32,
    /// Buffer capacity
    pub capacity: u32,
    pub _fields_08: [u32; 5],
    pub _pad_1c: [u8; 0x3C - 0x1C],
}

const _: () = assert!(core::mem::size_of::<SpriteCollection>() == 0x3C);

use crate::wa_alloc::wa_free;

// ── Construction ──────────────────────────────────────────────────────────

impl DisplayBase {
    /// Construct a DisplayBase for headless mode, entirely in Rust.
    ///
    /// Replaces the WA native constructor (0x522DB0) + headless vtable overlay.
    /// Allocates the struct and sprite collection sub-object on WA's heap.
    ///
    /// # Safety
    /// Must be called from within the WA.exe process (uses wa_malloc).
    pub unsafe fn new_headless() -> *mut Self {
        use crate::wa_alloc::WABox;

        let this = WABox::<Self>::alloc(0x3560, 0x3560).leak();

        // Set vtable to our Rust headless vtable.
        (*this).vtable = &HEADLESS_VTABLE;

        // Initialize slot_table: 0xFF entries = 1
        for slot in &mut (*this).slot_table {
            *slot = 1;
        }

        // _field_352c = 0xFFFFFFFF (sentinel)
        (*this)._field_352c = 0xFFFF_FFFF;

        // Create sprite collection sub-object.
        let sc = WABox::<SpriteCollection>::alloc(0x3C, 0x3C).leak();
        let buf = crate::wa_alloc::wa_malloc(0x80020);
        core::ptr::write_bytes(buf, 0, 0x80000);
        (*sc).buffer = buf as u32;
        (*sc).capacity = 0x80000;

        (*this).sprite_collection = sc as u32;
        // NOTE: The original FUN_004fa860 also sets a secondary vtable (0x664188)
        // on the parent object via implicit EDI. This is not yet replicated here,
        // which is why new_headless() doesn't work yet — see TODO in hardware_init.rs.

        this
    }

    /// Allocate and construct a DisplayBase using WA's native constructor (FFI).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process.
    pub unsafe fn construct() -> *mut Self {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::WABox;
        let this = WABox::<Self>::alloc(0x3560, 0x3560).leak();
        let ctor: unsafe extern "stdcall" fn(*mut Self) -> *mut Self =
            core::mem::transmute(rb(va::DISPLAY_BASE_CTOR) as usize);
        ctor(this);
        (*this).vtable = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *const DisplayBaseVtable;
        this
    }
}
