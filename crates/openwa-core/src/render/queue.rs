use super::message::{RenderMessage, TypedRenderCmd, COMMAND_TYPE_TYPED};
use super::sprite::sprite_op::SpriteOp;

/// Render command entry (0x18 bytes).
///
/// Enqueued by DrawSpriteGlobal (type 4), DrawSpriteLocal (type 5),
/// and other drawing functions.
#[repr(C)]
pub struct DrawSpriteCmd {
    /// Command type: 4 = sprite global (world-space), 5 = sprite local (screen-space)
    pub command_type: u32,
    pub layer: u32,
    /// X position, upper 16 bits used (Fixed16 format)
    pub x_pos: u32,
    /// Y position, upper 16 bits used (Fixed16 format)
    pub y_pos: u32,
    pub sprite: SpriteOp,
    /// Palette context — passed to `blit_sprite` as the last arg.
    /// Semantics vary by producer (palette pointer, animation index, etc.).
    pub palette: u32,
}

const _: () = assert!(core::mem::size_of::<DrawSpriteCmd>() == 0x18);

/// Render queue — manages a downward-growing buffer of DrawSpriteCmd entries.
///
/// Passed as `this` (ECX) to DrawSpriteGlobal/Local. The buffer area sits
/// between offset 0x04 and the entry metadata at 0x10000+. Entries are
/// allocated from the end of the buffer downward (offset decrements by 0x18).
///
/// Max 0x800 (2048) entries per frame.
#[repr(C)]
pub struct RenderQueue {
    /// 0x00000: Buffer write offset (i32, decrements by 0x18 per entry)
    pub buffer_offset: i32,
    /// 0x00004 - 0x10003: Buffer area for DrawSpriteCmd entries
    pub _buffer: [u8; 0x10000],
    /// 0x10004: Number of enqueued entries (max 0x800)
    pub entry_count: u32,
    /// 0x10008: Pointer array — entry_ptrs[N] points to the N-th command
    pub entry_ptrs: [*mut u8; 0x800],
}

impl RenderQueue {
    /// Allocate a command of type `T` from the downward-growing buffer.
    ///
    /// Returns `None` if the queue is full (>= 0x800 entries or buffer exhausted).
    /// The returned reference is registered in `entry_ptrs` and `entry_count` is incremented.
    pub unsafe fn alloc<T>(&mut self) -> Option<&mut T> {
        if self.entry_count >= 0x800 {
            return None;
        }
        let new_offset = self.buffer_offset - core::mem::size_of::<T>() as i32;
        if new_offset < 0 {
            return None;
        }
        self.buffer_offset = new_offset;

        let entry = &mut *(self._buffer.as_mut_ptr().add(new_offset as usize) as *mut T);
        self.entry_ptrs[self.entry_count as usize] = entry as *mut T as *mut u8;
        self.entry_count += 1;
        Some(entry)
    }

    /// Allocate `size` bytes from the downward-growing buffer.
    ///
    /// Like `alloc<T>()` but for variable-size entries (e.g. DrawLineStrip, DrawPolygon).
    /// Returns `None` if the queue is full or buffer exhausted.
    pub unsafe fn alloc_raw(&mut self, size: usize) -> Option<*mut u8> {
        if self.entry_count >= 0x800 {
            return None;
        }
        let new_offset = self.buffer_offset - size as i32;
        if new_offset < 0 {
            return None;
        }
        self.buffer_offset = new_offset;

        let ptr = self._buffer.as_mut_ptr().add(new_offset as usize);
        self.entry_ptrs[self.entry_count as usize] = ptr;
        self.entry_count += 1;
        Some(ptr)
    }
}

/// Type 0xD — tile-bitmap draw command (0x18 bytes).
///
/// Enqueued by `RQ_EnqueueTiledBitmap` (0x541D60, formerly mis-labelled
/// `RQ_DrawPixel`). The only known producer is `CTaskLand::RenderLandscape`,
/// which uses it to draw cached landscape texture tiles.
///
/// Inside `RenderDrawingQueue`, case 0xD dispatches to
/// `DisplayGfx::draw_tiled_bitmap` (vtable slot 11, 0x56B8C0):
/// ```text
/// RQ_ClipCoordinates(0, x_fixed16, y_fixed16, &out_x, &out_y, ...);
/// vtable[slot 11](
///     out_x >> 16 (or 0 if `flags & 1`),  // X in pixels
///     out_y >> 16,                          // Y in pixels
///     source_descriptor                     // sprite source struct *
/// );
/// ```
///
/// The slot 11 method itself runs three phases on each call: lazily allocate
/// 0x400-row tile surfaces from `source_descriptor->total_height` (cached in
/// `DisplayGfx + 0x3580`), populate them from the source, then clipped-blit
/// the tiles to the display.
///
/// Field names below are corrected. Wire format is byte-identical to what
/// the original `RQ_EnqueueTiledBitmap` writes.
#[repr(C)]
pub struct DrawTiledBitmapCmd {
    /// 0x00: Command type — always 0xD.
    pub command_type: u32,
    /// 0x04: Render layer — hardcoded 0x1B_0000 by the enqueue function.
    pub layer: u32,
    /// 0x08: Source X coordinate (Fixed16). Hardcoded 0xFF00_0000 (= -256.0)
    /// — the tile sheet always renders from off-screen-left, with the
    /// dispatcher's clipping pass producing the on-screen X.
    pub x_fixed16: u32,
    /// 0x0C: Source Y coordinate (Fixed16). Caller-supplied; goes through
    /// `RQ_ClipCoordinates` and the high 16 bits become the destination Y.
    pub y_fixed16: u32,
    /// 0x10: Pointer to a sprite source descriptor — read by the slot 11
    /// method as `p4` with fields at `+0x08` (bpp, 8 or 0x40), `+0x10`
    /// (source row stride), `+0x14`, and `+0x18` (total source height).
    pub source_descriptor: u32,
    /// 0x14: Flag byte. Bit 0 forces the destination X to 0 (ignoring the
    /// clipped result), used for "always start at left edge" rendering.
    pub flags: u8,
    pub _pad: [u8; 3],
}

const _: () = assert!(core::mem::size_of::<DrawTiledBitmapCmd>() == 0x18);

/// Type 0xB — draw crosshair (0x1C = 28 bytes).
///
/// Dispatched by RenderDrawingQueue case 0xB → DisplayGfx::draw_crosshair (slot 16).
/// The enqueue function at 0x541ED0 writes fields in non-sequential order:
/// `[1]=layer, [4]=x, [5]=y, [2]=color_fg, [3]=color_bg, [6]=0`.
#[repr(C)]
pub struct DrawCrosshairCmd {
    pub command_type: u32, // 0xB
    pub layer: u32,
    pub color_fg: u32,
    pub color_bg: u32,
    pub x_pos: u32,
    pub y_pos: u32,
    pub _reserved: u32, // hardcoded 0 (clip ref)
}

const _: () = assert!(core::mem::size_of::<DrawCrosshairCmd>() == 0x1C);

/// Type 0 — filled rectangle (0x20 = 32 bytes).
#[repr(C)]
pub struct DrawRectCmd {
    pub command_type: u32, // 0
    pub layer: u32,
    pub color: u32,
    pub x1: u32,     // & 0xFFFF0000
    pub y1: u32,     // & 0xFFFF0000
    pub x2: u32,     // & 0xFFFF0000
    pub y2: u32,     // & 0xFFFF0000
    pub y_clip: u32, // EDX & 0xFFFF0000
}

const _: () = assert!(core::mem::size_of::<DrawRectCmd>() == 0x20);

/// Type 6 — sprite with offset/scaling (0x24 = 36 bytes).
#[repr(C)]
pub struct DrawSpriteOffsetCmd {
    pub command_type: u32, // 6
    pub layer: u32,
    pub sprite_id: u32,
    pub x_pos: u32,     // & 0xFFFF0000
    pub y_pos: u32,     // & 0xFFFF0000
    pub _reserved: u32, // hardcoded 0
    pub y_clip: u32,    // EDX & 0xFFFF0000
    pub param_7: u32,
    pub param_8: u32,
}

const _: () = assert!(core::mem::size_of::<DrawSpriteOffsetCmd>() == 0x24);

/// Type 1 — bitmap global (0x28 = 40 bytes).
#[repr(C)]
pub struct DrawBitmapGlobalCmd {
    pub command_type: u32, // 1
    pub layer: u32,
    pub x_pos: u32, // & 0xFFFF0000
    pub y_pos: u32, // EDX & 0xFFFF0000
    pub bitmap_ptr: u32,
    pub _reserved: u32, // hardcoded 0
    pub param_6: u32,
    pub param_7: u32,
    pub param_8: u32,
    pub param_9: u32,
}

const _: () = assert!(core::mem::size_of::<DrawBitmapGlobalCmd>() == 0x28);

/// Type 2 — textbox local (0x34 = 52 bytes).
#[repr(C)]
pub struct DrawTextboxLocalCmd {
    pub command_type: u32, // 2
    pub layer: u32,
    pub _reserved_2: u32, // hardcoded 0
    pub x_pos: u32,       // & 0xFFFF0000
    pub y_pos: u32,       // EDX & 0xFFFF0000
    pub _reserved_5: u32, // hardcoded 0
    pub _reserved_6: u32, // hardcoded 0
    pub text_ptr: u32,
    pub _reserved_8: u32, // hardcoded 0
    pub _reserved_9: u32, // hardcoded 0
    pub param_6: u32,
    pub param_7: u32,
    pub param_8: u32,
}

const _: () = assert!(core::mem::size_of::<DrawTextboxLocalCmd>() == 0x34);

/// Type 8 — line strip header (0x10 bytes, followed by count × 0xC vertex data).
///
/// Total allocation: count × 0xC + 0x1C.
#[repr(C)]
pub struct DrawLineStripHeader {
    pub command_type: u32, // 8
    pub layer: u32,        // hardcoded 0xE_0000
    pub count: u32,        // vertex count (from EDI)
    pub param_1: u32,      // stack param
}

const _: () = assert!(core::mem::size_of::<DrawLineStripHeader>() == 0x10);

/// Type 9 — polygon header (0x14 bytes, followed by count × 0xC vertex data).
///
/// Total allocation: count × 0xC + 0x20.
#[repr(C)]
pub struct DrawPolygonHeader {
    pub command_type: u32, // 9
    pub layer: u32,        // hardcoded 0xE_0000
    pub count: u32,        // vertex count (from ESI)
    pub param_1: u32,      // stack param 1
    pub param_2: u32,      // stack param 2
}

const _: () = assert!(core::mem::size_of::<DrawPolygonHeader>() == 0x14);

impl RenderQueue {
    /// Enqueue a typed render message. Constant-time, zero allocation —
    /// the message is stored inline in the existing per-frame buffer.
    ///
    /// Returns `false` if the per-frame queue is full.
    ///
    /// # Safety
    ///
    /// The caller must ensure `self` points to a valid, live `RenderQueue`.
    pub unsafe fn push_typed(&mut self, layer: u32, message: RenderMessage) -> bool {
        match self.alloc::<TypedRenderCmd>() {
            Some(slot) => {
                *slot = TypedRenderCmd {
                    command_type: COMMAND_TYPE_TYPED,
                    layer,
                    message,
                };
                true
            }
            None => false,
        }
    }
}

/// Render command type constants.
pub mod command_type {
    pub const DRAW_RECT: u32 = 0;
    pub const DRAW_BITMAP_GLOBAL: u32 = 1;
    pub const DRAW_TEXTBOX_LOCAL: u32 = 2;
    pub const DRAW_SPRITE_GLOBAL: u32 = 4;
    pub const DRAW_SPRITE_LOCAL: u32 = 5;
    pub const DRAW_SPRITE_OFFSET: u32 = 6;
    pub const DRAW_LINE_STRIP: u32 = 8;
    pub const DRAW_POLYGON: u32 = 9;
    pub const DRAW_CROSSHAIR: u32 = 0xB;
    pub const DRAW_OUTLINED_PIXEL: u32 = 0xC;
    /// Tile-cached bitmap draw — dispatches to `DisplayGfx::draw_tiled_bitmap`
    /// (vtable slot 11). Formerly mis-labelled `DRAW_PIXEL`.
    pub const DRAW_TILED_BITMAP: u32 = 0xD;
}
