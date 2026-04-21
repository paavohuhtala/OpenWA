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
