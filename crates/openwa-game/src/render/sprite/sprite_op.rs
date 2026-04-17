use core::fmt;

use super::sprite_id::KnownSpriteId;

/// Packed sprite index + transform flags, passed as a single u32.
///
/// Layout (from `DisplayGfx::blit_sprite` at 0x56B080):
/// - Bits 0-15:  sprite index (0 = none, 1-696 for known sprites)
/// - Bits 16-28: transform/rendering flags (see [`SpriteFlags`])
///
/// Used throughout the render pipeline: enqueue functions write it,
/// the render queue stores it, and `blit_sprite` unpacks it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SpriteOp(pub u32);

impl SpriteOp {
    /// No sprite (index 0).
    pub const NONE: Self = Self(0);

    /// Sprite index (low 16 bits). 0 = no sprite.
    #[inline]
    pub const fn index(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// Transform/rendering flag bits (high 16 bits).
    #[inline]
    pub fn flags(self) -> SpriteFlags {
        SpriteFlags::from_bits_truncate(self.0 & 0xFFFF_0000)
    }

    /// Construct from sprite index only (no flags).
    #[inline]
    pub const fn from_index(index: u16) -> Self {
        Self(index as u32)
    }

    /// Construct from index + flags.
    #[inline]
    pub const fn new(index: u16, flags: SpriteFlags) -> Self {
        Self(index as u32 | flags.bits())
    }
}

impl fmt::Debug for SpriteOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let idx = self.index();
        let flags = self.flags();
        if flags.is_empty() {
            write!(f, "SpriteOp({idx})")
        } else {
            write!(f, "SpriteOp({idx}, {flags:?})")
        }
    }
}

impl From<KnownSpriteId> for SpriteOp {
    #[inline]
    fn from(known: KnownSpriteId) -> Self {
        Self(known as u32)
    }
}

bitflags::bitflags! {
    /// Transform/rendering flags for sprite blit operations (high 16 bits
    /// of [`SpriteOp`]).
    ///
    /// Documented from `DisplayGfx::blit_sprite` at 0x56B080.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct SpriteFlags: u32 {
        /// Tiled rendering mode.
        const TILED           = 0x0001_0000;
        /// Additional orientation bit.
        const ORIENTATION     = 0x0002_0000;
        /// Extra horizontal mirror.
        const MIRROR_X        = 0x0004_0000;
        /// Extra vertical mirror.
        const MIRROR_Y        = 0x0008_0000;
        /// Stippled palette adjustment.
        const STIPPLED_PAL    = 0x0010_0000;
        /// Additive blend mode.
        const ADDITIVE        = 0x0020_0000;
        /// Shadow clear mode.
        const SHADOW_CLEAR    = 0x0040_0000;
        /// Invert palette values.
        const INVERT_PALETTE  = 0x0080_0000;
        /// Palette ×4 adjustment.
        const PALETTE_X4      = 0x0100_0000;
        /// Palette transform (color cycling).
        const PALETTE_XFORM   = 0x0200_0000;
        /// Color blend mode.
        const COLOR_BLEND     = 0x0400_0000;
        /// Stippled rendering mode 0.
        const STIPPLED_0      = 0x0800_0000;
        /// Stippled rendering mode 1.
        const STIPPLED_1      = 0x1000_0000;
    }
}
