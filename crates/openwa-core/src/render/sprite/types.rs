use crate::asset::gfx_dir::GfxDir;
use crate::render::{SpriteCache, display::gfx::DisplayGfx, palette::PaletteContext};

// Re-export sprite types from their own modules to keep this file focused on struct layouts.
pub use super::sprite_id::KnownSpriteId;
pub use super::sprite_op::{SpriteFlags, SpriteOp};
/// Per-frame metadata within a Sprite (0x0C bytes).
///
/// Describes the bounding box and bitmap data offset for one animation frame.
/// Array pointed to by `Sprite::frame_meta_ptr`.
///
/// Source: wkJellyWorm `Sprites.h::SpriteFrame`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteFrame {
    /// 0x00: Offset into bitmap data for this frame's pixels.
    ///
    /// When the parent sprite's `header_flags & 0x4000` is clear, this is a
    /// flat byte offset within `Sprite::bitmap_data_ptr` and the bytes there
    /// are already-decoded pixels.
    ///
    /// When `header_flags & 0x4000` is set, the field is split: the **high
    /// byte** (signed `i8`) is an index into `Sprite::subframe_cache_table`,
    /// and the **low 24 bits** are a pixel offset within the corresponding
    /// decoded subframe payload.
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

/// Per-sprite subframe cache entry (0xC bytes).
///
/// Array stored at `Sprite::subframe_cache_table`, count
/// `Sprite::subframe_cache_count`. Each entry tracks one lazily-decompressed
/// LZSS subframe payload owned by the per-`SpriteCache` `FrameCache`.
///
/// The `decoded_ptr` is null until the corresponding frame is first
/// requested by `Sprite__GetFrameForBlit`, then populated by allocating
/// `decoded_size` bytes from the `FrameCache` and decoding the LZSS stream
/// at `Sprite::bitmap_data_ptr + compressed_offset`. When the FrameCache
/// evicts the payload, `Sprite::vtable[1] (on_subframe_evicted)` is called
/// with the subframe index and clears `decoded_ptr` back to null.
///
/// **Note:** the field order differs from
/// [`SpriteBankSubframeCache`] — Sprite has `decoded_ptr` at `+4` and
/// `decoded_size` at `+8`, while SpriteBank has them swapped. This is
/// verified at 0x4FAE42/0x4FAE4B (Sprite path) and 0x4F984D/0x4F9853
/// (SpriteBank path). Sharing one type would corrupt one of the two paths.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteSubframeCache {
    /// +0x00: Byte offset within `Sprite::bitmap_data_ptr` to the LZSS
    /// compressed source for this subframe.
    pub compressed_offset: u32,
    /// +0x04: Pointer to decompressed pixels within the FrameCache buffer.
    /// Null until the subframe is first requested; cleared back to null on
    /// FrameCache eviction.
    pub decoded_ptr: *mut u8,
    /// +0x08: Decompressed payload size in bytes (passed to
    /// `FrameCache__Allocate` as `entry_size`).
    pub decoded_size: u32,
}

const _: () = assert!(core::mem::size_of::<SpriteSubframeCache>() == 0x0C);
const _: () = assert!(core::mem::offset_of!(SpriteSubframeCache, decoded_ptr) == 0x04);
const _: () = assert!(core::mem::offset_of!(SpriteSubframeCache, decoded_size) == 0x08);

/// Per-`SpriteBank` subframe cache entry (0xC bytes).
///
/// Array stored at `SpriteBank::subframe_cache_table`, count
/// `SpriteBank::subframe_cache_count`. Plays the same role as
/// [`SpriteSubframeCache`] but **with a different field order**:
/// `decoded_size` at `+4` and `decoded_ptr` at `+8`. Verified at
/// 0x4F984D/0x4F9853 in `SpriteBank__GetFrameForBlit` and at the
/// eviction callback `SpriteBank::vtable[1]` (0x4F9580) which clears
/// `*(table + idx * 0xC + 8)` (i.e. `decoded_ptr` at `+8`).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteBankSubframeCache {
    /// +0x00: Byte offset within `SpriteBank::bitmap_data_ptr` to the LZSS
    /// compressed source for this subframe.
    pub compressed_offset: u32,
    /// +0x04: Decompressed payload size in bytes.
    pub decoded_size: u32,
    /// +0x08: Pointer to decompressed pixels within the FrameCache buffer,
    /// or null if not yet cached.
    pub decoded_ptr: *mut u8,
}

const _: () = assert!(core::mem::size_of::<SpriteBankSubframeCache>() == 0x0C);
const _: () = assert!(core::mem::offset_of!(SpriteBankSubframeCache, decoded_size) == 0x04);
const _: () = assert!(core::mem::offset_of!(SpriteBankSubframeCache, decoded_ptr) == 0x08);

/// Per-`SpriteBank` frame bounding-box entry (0xC bytes).
///
/// Array stored at `SpriteBank::bbox_table`. Each entry holds a subframe
/// cache index (used to look up the decompressed pixel data in
/// `SpriteBank::subframe_cache_table`), the per-frame offset within the
/// decoded payload, and the source bounding-box coordinates returned to
/// the caller of `SpriteBank__GetFrameForBlit`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteBankBboxEntry {
    /// +0x00: Subframe cache index — selects an entry in
    /// `SpriteBank::subframe_cache_table`.
    pub subframe_idx: u16,
    /// +0x02: Byte offset within the decoded subframe payload to this
    /// frame's pixel data. Added to `SpriteBankSubframeCache::decoded_ptr`
    /// when computing the surface address.
    pub decoded_offset: u16,
    /// +0x04: Left edge X coordinate.
    pub start_x: u16,
    /// +0x06: Top edge Y coordinate.
    pub start_y: u16,
    /// +0x08: Right edge X coordinate.
    pub end_x: u16,
    /// +0x0A: Bottom edge Y coordinate.
    pub end_y: u16,
}

const _: () = assert!(core::mem::size_of::<SpriteBankBboxEntry>() == 0x0C);

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
    /// 0x18: Frame index rounding mode flag (read by
    /// `Sprite__GetFrameForBlit`). When bit 0 is set, the
    /// `frame_idx = (max_frames * anim_value) >> 16` computation adds
    /// `0x8000` for round-to-nearest, with a special case that wraps
    /// `frame_idx == max_frames` back to 0. When clear, simple truncation.
    pub frame_round_mode: u8,
    /// 0x19: Unknown — paired with `frame_round_mode` byte. Both bytes are
    /// written together as `(id >> 16) as u16` by `load_sprite_by_name`.
    pub _unknown_19: u8,
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
    /// 0x2C: Pointer to subframe cache table — array of
    /// `subframe_cache_count` [`SpriteSubframeCache`] entries. Present when
    /// `header_flags & 0x4000`. Each entry caches one lazily-decompressed
    /// LZSS subframe payload owned by the per-`SpriteCache` `FrameCache`.
    pub subframe_cache_table: *mut SpriteSubframeCache,
    /// 0x30: Number of entries in `subframe_cache_table`.
    pub subframe_cache_count: u16,
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
/// Used by GetSpriteInfo (0x523500) and GetSpriteFrameForBlit (0x5237C0,
/// vtable slot 33) as the fallback path when sprite_ptrs[id] is null.
///
/// Layout was reverse-engineered jointly from `SpriteBank__GetFrameForBlit`
/// (0x4F9710) and `SpriteBank__ParseData` (0x4F9640), which writes the
/// table pointers and counts after loading the gfx-dir image.
#[repr(C)]
pub struct SpriteBank {
    /// 0x00: Vtable pointer (0x664180, 2 slots).
    pub vtable: *const SpriteBankVtable,
    /// 0x04: Pointer to the per-process [`SpriteCache`]. Read by
    /// `SpriteBank__GetFrameForBlit` and forwarded to
    /// `FrameCache__Allocate` to find the shared `FrameCache`.
    pub context_ptr: *mut crate::render::SpriteCache,
    /// 0x08: Base sprite ID — subtracted from the lookup ID to compute
    /// the `index_table` offset.
    pub base_id: i32,
    /// 0x0C: Index table — maps `(id - base_id)` to frame indices in
    /// `frame_table`.
    pub index_table: *const i32,
    /// 0x10: Frame table — array of `frame_count` [`SpriteBankFrame`]
    /// entries (animation metadata: flags / width / height /
    /// base frame idx / scale-or-count).
    pub frame_table: *const SpriteBankFrame,
    /// 0x14: Number of valid entries in `frame_table`.
    pub frame_count: i32,
    /// 0x18: Bounding-box table — array of `bbox_count`
    /// [`SpriteBankBboxEntry`] entries. Indexed by the resolved subframe
    /// index from a `SpriteBankFrame`. Holds the per-frame source
    /// rectangle plus a subframe cache index + decoded-payload offset.
    pub bbox_table: *mut SpriteBankBboxEntry,
    /// 0x1C: Number of valid entries in `bbox_table`.
    pub bbox_count: i32,
    /// 0x20: Subframe cache table — array of `subframe_cache_count`
    /// [`SpriteBankSubframeCache`] entries. Each entry tracks one
    /// lazily-decompressed LZSS subframe payload owned by the per-process
    /// `FrameCache`.
    pub subframe_cache_table: *mut SpriteBankSubframeCache,
    /// 0x24: Number of valid entries in `subframe_cache_table`.
    pub subframe_cache_count: i32,
    /// 0x28: Compressed bitmap data base. The LZSS source for any
    /// subframe is `bitmap_data_ptr + cache_entry.compressed_offset`.
    pub bitmap_data_ptr: *const u8,
    /// 0x2C: Owning allocation for the loaded gfx-dir image. The other
    /// `*_table` pointers above index into this buffer; on destruction
    /// only this is freed.
    pub raw_image_buffer: *mut u8,
    /// 0x30..0x130: 256-byte palette translation lookup table. Entry 0
    /// is the implicit transparent index (always 0); entries
    /// `1..=palette_count` are filled by `PaletteContext::map_color` from
    /// the loaded `.spr` palette. Used by `Sprite_LZSS_Decode` to map
    /// source bytes to display palette indices.
    pub palette_lut: [u8; 0x100],
    /// 0x130: Embedded `DisplayBitGrid` sub-object (0x2C bytes). Returned
    /// to callers of `SpriteBank__GetFrameForBlit` after its data
    /// pointer / dimensions / clip rect have been updated to point at the
    /// just-decompressed subframe.
    pub frame_bitgrid: crate::bitgrid::DisplayBitGrid,
    /// 0x15C..0x17C: Trailing 0x20 bytes — uninitialized by the constructor.
    /// Purpose unknown.
    pub _trailing: [u8; 0x17C - 0x15C],
}

const _: () = assert!(core::mem::size_of::<SpriteBank>() == 0x17C);
const _: () = assert!(core::mem::offset_of!(SpriteBank, frame_table) == 0x10);
const _: () = assert!(core::mem::offset_of!(SpriteBank, bbox_table) == 0x18);
const _: () = assert!(core::mem::offset_of!(SpriteBank, subframe_cache_table) == 0x20);
const _: () = assert!(core::mem::offset_of!(SpriteBank, bitmap_data_ptr) == 0x28);
const _: () = assert!(core::mem::offset_of!(SpriteBank, palette_lut) == 0x30);
const _: () = assert!(core::mem::offset_of!(SpriteBank, frame_bitgrid) == 0x130);

/// Animation metadata entry in a `SpriteBank::frame_table` (0xC bytes).
///
/// Holds the per-animation flags, source dimensions, base frame index, and
/// either a frame count or a scaled-mode interpolation pair. Plays the same
/// role as the combination of `Sprite::flags` + `Sprite::width`/`height` +
/// `Sprite::scale_x`/`scale_y` for a single animation within the bank.
///
/// Layout verified from `SpriteBank__GetFrameForBlit` (0x4F9710), which
/// reads the entry as `ushort[6]` and indexes:
/// - `[0] flags & 1` → "anim_value used as-is" mode
/// - `[0] flags & 2` → ping-pong mode
/// - `[1] width`     → out_w
/// - `[2] height`    → out_h
/// - `[3] base_frame_idx` → added to the resolved sub-frame index
/// - `[4] & 0x8000`  → scaled mode toggle, with the low/high 7-bit nibbles
///   sign-extended via `<< 16 >> 5` to form a Fixed16 interpolation pair
///   (analogous to `Sprite::scale_x`/`scale_y`); when clear, multiplied by
///   `anim_value >> 16` like `Sprite::max_frames`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteBankFrame {
    /// 0x00: Animation flags. Bit 0 = `anim_value` used as-is (no signed
    /// clamp), bit 1 = ping-pong frame iteration.
    pub flags: u16,
    /// 0x02: Frame source width in pixels (returned via `out_w`).
    pub width: u16,
    /// 0x04: Frame source height in pixels (returned via `out_h`).
    pub height: u16,
    /// 0x06: Base frame index — added to the sub-frame index resolved
    /// from `scale_or_count` to produce the final lookup into
    /// `SpriteBank::bbox_table`.
    pub base_frame_idx: u16,
    /// 0x08: Scaled mode toggle and payload (also doubles as the "API
    /// width" reported by `SpriteBank__GetInfo`). When `& 0x8000` is set,
    /// the low and high 7-bit nibbles are sign-extended (`<< 16 >> 5`)
    /// into a Fixed16 `scale_x`/`scale_y` pair, identical to
    /// `Sprite::scale_x`/`scale_y`, AND `GetInfo` returns `out_width = 1`.
    /// When clear, the field is the frame count multiplier
    /// (`(scale_or_count * anim_value) >> 16`) and `GetInfo` returns it as
    /// `out_width` (or `width * 2 - 1` if the ping-pong flag is set).
    pub scale_or_count: u16,
    /// 0x0A: Auxiliary 16-bit data value reported by
    /// `SpriteBank__GetInfo` as `out_data << 8`. Not used by
    /// `SpriteBank__GetFrameForBlit`.
    pub data_value: u16,
}

const _: () = assert!(core::mem::size_of::<SpriteBankFrame>() == 0x0C);
const _: () = assert!(core::mem::offset_of!(SpriteBankFrame, width) == 0x02);
const _: () = assert!(core::mem::offset_of!(SpriteBankFrame, height) == 0x04);
const _: () = assert!(core::mem::offset_of!(SpriteBankFrame, base_frame_idx) == 0x06);
const _: () = assert!(core::mem::offset_of!(SpriteBankFrame, scale_or_count) == 0x08);
const _: () = assert!(core::mem::offset_of!(SpriteBankFrame, data_value) == 0x0A);

/// Sprite vtable (0x66418C, 8 slots).
///
/// Slots 6-7 are CTask common stubs (0x5613D0).
#[openwa_core::vtable(size = 8, va = 0x0066_418C, class = "Sprite")]
pub struct SpriteVtable {
    /// destructor (0x4FAA80)
    pub destructor: fn(this: *mut Sprite, flags: u32),
    /// FrameCache eviction callback (0x4FAAD0). Called by
    /// `FrameCache__Allocate` when the LRU buffer evicts an entry owned by
    /// this `Sprite` — the body clears
    /// `subframe_cache_table[subframe_idx].decoded_ptr` so the next blit
    /// re-decompresses the LZSS stream. Stack args: `(subframe_idx,
    /// payload_addr)`; the `payload_addr` arg is unused but pushed by the
    /// allocator anyway.
    pub on_subframe_evicted: fn(this: *mut Sprite, subframe_idx: u32, payload: *mut u8),
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
    /// FrameCache eviction callback (0x4F9580). Same role as
    /// [`SpriteVtable::on_subframe_evicted`] but clears
    /// `subframe_cache_table[subframe_idx].decoded_ptr` (at offset `+8`
    /// in the bank-side entry, vs `+4` in the sprite-side entry).
    pub on_subframe_evicted: fn(this: *mut SpriteBank, subframe_idx: u32, payload: *mut u8),
}

/// Layer sprite — 0x70-byte object used by `load_sprite_by_layer` (vtable slot 37).
///
/// Also used for the "bitmap sprite" path of `BlitSprite` (slot 19): the
/// per-id pointers in `DisplayGfx::sprite_table` (`+0x3DD4`) point at
/// `LayerSprite` instances. `DisplayGfx::GetBitmapSpriteInfo` reads
/// `flags`, `frame_count`, `cell_width`, `cell_height`, and `frame_array`
/// to resolve a frame index → frame metadata + `CBitmap` pointer.
///
/// Unlike `Sprite` (which has a vtable and is created via ConstructSprite), this
/// is a raw allocation with partial init by the caller. Used for background layers
/// (back.spr, layer.spr) which load DirectDraw surfaces directly from `.dir` files.
#[repr(C)]
pub struct LayerSprite {
    pub _pad_00: u32,
    /// 0x04: Sprite name (null-terminated, max 0x50 bytes including null).
    pub name: [u8; 0x50],
    /// 0x54: GfxDir pointer (set by load_sprite_by_name).
    pub gfx_dir: *mut GfxDir,
    /// 0x58: DisplayGfx pointer (set by caller before load).
    pub display_gfx: *mut DisplayGfx,
    /// 0x5C: PaletteContext pointer (set by load_sprite_by_name).
    pub palette_ctx: *mut PaletteContext,
    /// 0x60: Header field from stream (4 bytes).
    pub field_60: u32,
    /// 0x64: Animation/playback mode flags. Read by
    /// `GetBitmapSpriteInfo` to decide how to interpret the
    /// palette/anim parameter:
    /// - bit 0: low 16 bits used as-is (no clamp)
    /// - bit 1: ping-pong (bounce) frame iteration
    pub flags: u16,
    /// 0x66: Frame count (zeroed, then read from stream).
    pub frame_count: u16,
    /// 0x68: Sprite cell width in pixels (used for centering during blit).
    pub cell_width: i16,
    /// 0x6A: Sprite cell height in pixels (used for centering during blit).
    pub cell_height: i16,
    /// 0x6C: Pointer to LayerSpriteFrame array (count stored at ptr[-4]).
    pub frame_array: *mut LayerSpriteFrame,
}

const _: () = assert!(core::mem::size_of::<LayerSprite>() == 0x70);

/// Standalone `CBitmap` cache entry — 12 bytes, vtable `0x643F64`.
///
/// Allocated by `DisplayGfx::DrawTiledBitmap` (slot 11) as the per-strip
/// cached tile bitmap entry stored in `DisplayGfx.bitmap_vec`. Not the
/// same struct as [`LayerSpriteFrame`] (which is 0x14 bytes and uses the
/// same vtable but with a leading 8-byte bounding box) — but the trailing
/// (vtable, surface, pad) triple matches.
///
/// The vtable pointer is `0x643F64` (constructor at 0x573C30 sets it).
/// The `surface` field is lazily allocated via `RenderContext::alloc_surface`
/// the first time the bitmap is blitted.
#[repr(C)]
pub struct CBitmap {
    /// 0x00: Vtable pointer (= `0x643F64`).
    pub vtable: *const core::ffi::c_void,
    /// 0x04: Backend surface object pointer. Lazily allocated by
    /// `cbitmap_blit_via_wrapper` (`FUN_00403c60`); zero until the first
    /// `BlitBitmapClipped` call for this bitmap.
    pub surface: *mut crate::render::display::context::Surface,
    /// 0x08: Padding/reserved (zeroed by constructor).
    pub _pad: u32,
}

const _: () = assert!(core::mem::size_of::<CBitmap>() == 0xC);

/// Per-frame surface element for LayerSprite (0x14 bytes).
///
/// Holds bounding box coordinates and a DirectDraw surface pointer.
/// Allocated in counted arrays: `malloc(count * 0x14 + 4)`, count at `[-4]`.
///
/// Ghidra name: `CBitmap` (unrelated to `BitGrid`/`DisplayBitGrid`). The
/// trailing 12 bytes (vtable + surface + pad) are isomorphic to
/// [`CBitmap`] above and share the same vtable `0x643F64`, but the
/// container has an 8-byte bounding-box prefix that the standalone form
/// does not.
///
/// Constructor: 0x573C30 (sets vtable, zeroes surface).
/// Destructor: 0x5732E0 (releases surface via `surface->vtable[0](1)`).
#[repr(C)]
pub struct LayerSpriteFrame {
    /// 0x00: Frame start X coordinate.
    pub start_x: i16,
    /// 0x02: Frame start Y coordinate.
    pub start_y: i16,
    /// 0x04: Frame end X coordinate.
    pub end_x: i16,
    /// 0x06: Frame end Y coordinate.
    pub end_y: i16,
    /// 0x08: CBitmap vtable pointer (`0x643F64`, set by constructor).
    pub bitmap_vtable: *const core::ffi::c_void,
    /// 0x0C: Backend surface object pointer (lazy via `alloc_surface`).
    pub surface: *mut crate::render::display::context::Surface,
    /// 0x10: Reserved/padding.
    pub _pad_10: u32,
}

const _: () = assert!(core::mem::size_of::<LayerSpriteFrame>() == 0x14);

impl LayerSpriteFrame {
    /// Returns a pointer to the trailing 12-byte `CBitmap` block
    /// (`bitmap_vtable`/`surface`/`_pad_10`) inside this entry. The
    /// `CBitmap` layout is bit-identical to the trailing 12 bytes of
    /// `LayerSpriteFrame`, so the value can be passed to any function
    /// that takes `*mut CBitmap` (e.g., `blit_bitmap_clipped_native`).
    #[inline]
    pub fn bitmap_ptr(this: *mut Self) -> *mut CBitmap {
        unsafe { (this as *mut u8).add(0x08) as *mut CBitmap }
    }
}

crate::define_addresses! {
    class "CBitmap" {
        /// Vtable shared by [`CBitmap`] (12 bytes) and [`LayerSpriteFrame`]
        /// (0x14 bytes). Set by the CBitmap constructor at 0x573C30.
        vtable CBITMAP_VTABLE_MAYBE = 0x0064_3F64;
    }

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
        /// `Sprite__GetFrameForBlit` — usercall on Sprite* (ESI=sprite).
        /// Looks up an animation frame in a Sprite, lazily decompresses its
        /// surface, and returns the frame metadata. Called by
        /// `DisplayGfx::GetSpriteFrameForBlit` (slot 33) for sprite_ptrs IDs.
        fn/Usercall SPRITE_GET_FRAME_FOR_BLIT = 0x004F_AD30;
        /// `SpriteBank__GetFrameForBlit` — usercall on SpriteBank* (ESI=bank).
        /// Same as above but for SpriteBank-backed sprite IDs (sprite_banks).
        fn/Usercall SPRITE_BANK_GET_FRAME_FOR_BLIT = 0x004F_9710;
    }
}
