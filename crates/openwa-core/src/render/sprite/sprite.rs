use crate::render::SpriteCache;

// Re-export SpriteId from its own module to keep this file focused on struct layouts.
pub use super::sprite_id::SpriteId;

// (SpriteId enum moved to sprite_id.rs)
/// Per-frame metadata within a Sprite (0x0C bytes).
///
/// Describes the bounding box and bitmap data offset for one animation frame.
/// Array pointed to by `Sprite::frame_meta_ptr`.
///
/// Source: wkJellyWorm `Sprites.h::SpriteFrame`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteFrame {
    /// 0x00: Offset into bitmap data for this frame's pixels
    pub bitmap_offset: u32,
    /// 0x04: Left edge X coordinate
    pub start_x: u16,
    /// 0x06: Top edge Y coordinate
    pub start_y: u16,
    /// 0x08: Right edge X coordinate
    pub end_x: u16,
    /// 0x0A: Bottom edge Y coordinate
    pub end_y: u16,
}

const _: () = assert!(core::mem::size_of::<SpriteFrame>() == 0x0C);

/// Sprite object (0x70 bytes, vtable 0x66418C).
///
/// Represents a loaded sprite with animation frames. Created by ConstructSprite
/// (0x4FAA30), populated by ProcessSprite (0x4FAB80) from `.spr` file data.
///
/// Contains frame metadata, palette data, and bitmap pixel data for all frames.
/// Managed by DisplayBase's 1024-slot sprite table (`sprite_ptrs`).
///
/// Source: wkJellyWorm `Sprites.h`, Ghidra decompilation of ConstructSprite + ProcessSprite.
#[repr(C)]
pub struct Sprite {
    /// 0x00: Vtable pointer (0x66418C, 8 slots)
    pub vtable: *const SpriteVtable,
    /// 0x04: Context/parent pointer (ECX from ConstructSprite)
    pub context_ptr: *mut SpriteCache,
    /// 0x08: Unknown
    pub _unknown_08: u16,
    /// 0x0A: Animation frames per second
    pub fps: u16,
    /// 0x0C: Sprite width in pixels
    pub width: u16,
    /// 0x0E: Sprite height in pixels
    pub height: u16,
    /// 0x10: Sprite flags
    pub flags: u16,
    /// 0x12: Frame count (may be overwritten by ProcessSprite)
    pub frame_count: u16,
    /// 0x14: Header flags from .spr file (raw+8)
    pub header_flags: u16,
    /// 0x16: Maximum frame count
    pub max_frames: u16,
    /// 0x18: Unknown
    pub _unknown_18: u16,
    /// 0x1A: Unknown
    pub _unknown_1a: u16,
    /// 0x1C: Scale X (set when negative frame count in .spr)
    pub scale_x: u32,
    /// 0x20: Scale Y (set when negative frame count in .spr)
    pub scale_y: u32,
    /// 0x24: Is-scaled flag (1 if scaling active, 0 otherwise)
    pub is_scaled: u32,
    /// 0x28: Pointer to SpriteFrame array (frame_count entries)
    pub frame_meta_ptr: *mut SpriteFrame,
    /// 0x2C: Secondary frame table pointer (present when header_flags & 0x4000)
    pub secondary_frame_ptr: *mut SpriteFrame,
    /// 0x30: Secondary frame count
    pub secondary_frame_count: u16,
    /// 0x32: Padding
    pub _pad_32: u16,
    /// 0x34: Embedded DisplayBitGrid sub-object (0x2C bytes).
    /// ConstructSprite sets vtable=0x664144, external_buffer=1, cells_per_unit=8.
    /// Populated further by ProcessSprite with pixel data pointers.
    pub bitgrid: crate::bitgrid::DisplayBitGrid,
    /// 0x60: Pointer to raw frame header data
    pub raw_frame_header_ptr: *mut u8,
    /// 0x64: Pointer to bitmap pixel data
    pub bitmap_data_ptr: *mut u8,
    /// 0x68: Pointer to palette data
    pub palette_data_ptr: *mut u8,
    /// 0x6C: Unknown
    pub _unknown_6c: u32,
}

const _: () = assert!(core::mem::size_of::<Sprite>() == 0x70);
const _: () = assert!(core::mem::offset_of!(Sprite, bitgrid) == 0x34);
const _: () = assert!(core::mem::offset_of!(Sprite, raw_frame_header_ptr) == 0x60);

/// SpriteBank — indexed sprite container (0x17C bytes).
///
/// Alternative to Sprite for storing sprite data in DisplayBase's 1024-slot table.
/// While Sprite (sprite_ptrs, 0x1008) stores individual sprites loaded from `.spr` files,
/// SpriteBank (sprite_banks, 0x2008) stores collections of sprite frames accessed via
/// an index table that maps sprite IDs to frame indices.
///
/// Created by LoadSpriteEx (0x523310): allocated 0x17C bytes, first 0x15C zeroed,
/// then constructed and initialized via FUN_004f95a0.
///
/// Used by GetSpriteInfo (0x523500) and LoadSpriteComplex (0x5237C0) as the
/// fallback path when sprite_ptrs[id] is null.
#[repr(C)]
pub struct SpriteBank {
    /// 0x00: Vtable pointer (0x664180, 3 slots)
    pub vtable: *const SpriteBankVtable,
    /// 0x04: Unknown (context pointer?)
    pub _unknown_04: u32,
    /// 0x08: Base sprite ID — subtracted from the lookup ID to compute the index table offset.
    pub base_id: i32,
    /// 0x0C: Index table pointer — maps `(id - base_id)` to frame indices in the frame table.
    pub index_table: *const i32,
    /// 0x10: Frame table pointer — array of SpriteFrame entries (0xC bytes each).
    pub frame_table: *const SpriteBankFrame,
    /// 0x14: Number of valid entries in the frame table (bounds check for lookups).
    pub frame_count: i32,
    /// 0x18 - 0x17B: Remaining fields (unknown)
    pub _unknown_18: [u8; 0x17C - 0x18],
}

const _: () = assert!(core::mem::size_of::<SpriteBank>() == 0x17C);

/// Frame entry in a SpriteBank's frame table (0xC bytes).
///
/// Structurally identical to SpriteFrame but fields have different semantics
/// when accessed by SpriteBank__GetInfo (FUN_004f98c0):
/// - `flags` byte 0: bit 0 = transparency flag, bit 1 = double-width flag
/// - `width` at +0x08: frame width (bit 15 = single-width override)
/// - `data_value` at +0x0A: data reference (shifted left 8 for output)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteBankFrame {
    /// 0x00: Flags byte. Bit 0 = has transparency, bit 1 = double-width.
    pub flags: u8,
    /// 0x01-0x07: Padding / other fields
    pub _pad_01: [u8; 7],
    /// 0x08: Frame width in pixels. Bit 15 set = force width to 1.
    pub width: u16,
    /// 0x0A: Data reference value (shifted left 8 when returned by GetSpriteInfo).
    pub data_value: u16,
}

const _: () = assert!(core::mem::size_of::<SpriteBankFrame>() == 0x0C);

/// Sprite vtable (0x66418C, 8 slots).
///
/// Slots 6-7 are CTask common stubs (0x5613D0).
#[openwa_core::vtable(size = 8, va = 0x0066_418C, class = "Sprite")]
pub struct SpriteVtable {
    /// destructor (0x4FAA80)
    pub destructor: fn(this: *mut Sprite, flags: u32),
    /// unknown (0x4FAAD0)
    pub slot_1: fn(this: *mut Sprite),
    /// unknown (0x4FB5C0)
    pub slot_2: fn(this: *mut Sprite),
    /// unknown (0x4FE550)
    pub slot_3: fn(this: *mut Sprite),
    /// unknown (0x4FE2F0)
    pub slot_4: fn(this: *mut Sprite),
    /// unknown (0x4FE9C0)
    pub slot_5: fn(this: *mut Sprite),
    /// CTask common stub (0x5613D0)
    pub slot_6: fn(this: *mut Sprite),
    /// CTask common stub (0x5613D0)
    pub slot_7: fn(this: *mut Sprite),
}

/// SpriteBank vtable (0x664180, 2 slots).
#[openwa_core::vtable(size = 2, va = 0x0066_4180, class = "SpriteBank")]
pub struct SpriteBankVtable {
    /// destructor (0x4F94E0)
    pub destructor: fn(this: *mut SpriteBank, flags: u32),
    /// init/load (0x4F9580)
    pub slot_1: fn(this: *mut SpriteBank),
}

crate::define_addresses! {
    class "Sprite" {
        /// ConstructSprite — usercall EAX=sprite_ptr, ECX=context_ptr
        ctor/Usercall CONSTRUCT_SPRITE = 0x004F_AA30;
        /// Sprite destructor — thiscall, vtable slot 0
        fn/Thiscall DESTROY_SPRITE = 0x004F_AA80;
        /// LoadSpriteFromVfs
        fn/Usercall LOAD_SPRITE_FROM_VFS = 0x004F_AAF0;
        /// ProcessSprite — parses .spr binary format
        fn/Usercall PROCESS_SPRITE = 0x004F_AB80;
        /// Sprite__GetInfo — usercall EAX=Sprite*, ESI=out_data, ECX=out_width, stack=out_flags
        fn/Usercall SPRITE_GET_INFO = 0x004F_AEC0;
    }

    class "SpriteBank" {
        /// SpriteBank__GetInfo — usercall EAX=layer, ECX=bank*, ESI=out_width, 2 stack params
        fn/Usercall SPRITE_BANK_GET_INFO = 0x004F_98C0;
        /// SpriteBank__Init — usercall, initializes from VFS resource
        fn/Usercall SPRITE_BANK_INIT = 0x004F_95A0;
    }

    class "DisplayGfx_Sprite" {
        /// Load sprite from VFS by name — usercall(EDI=sprite, ECX=gfx) + stack(id, name), RET 0x8
        fn/Usercall LOAD_SPRITE_BY_NAME = 0x0057_33B0;
        /// Free sprite object (with sub-object cleanup) — usercall(EDI=sprite), plain RET
        fn/Usercall FREE_SPRITE_OBJECT = 0x0056_A2F0;
        /// Sprite-path dispatch for LoadSpriteComplex — usercall on Sprite*
        fn/Usercall SPRITE_COMPLEX_SPRITE_PATH = 0x004F_AD30;
        /// SpriteBank-path dispatch for LoadSpriteComplex — usercall on SpriteBank*
        fn/Usercall SPRITE_COMPLEX_BANK_PATH = 0x004F_9710;
    }
}
