//! Display layer index newtype.

use core::num::NonZeroU8;

/// Display layer index — valid values are 1, 2, 3.
///
/// Used as the index for `DisplayBase::layer_contexts`,
/// `DisplayBase::layer_visibility`, and as the value space stored in
/// `DisplayBase::font_layers`. Surfaces in `DisplayGfx` vtable slots:
///
/// - slot 4 `set_layer_color`
/// - slot 5 `set_active_layer`
/// - slot 23 `set_layer_visibility`
/// - slot 34 `load_font` (the WA "mode" parameter — same value space)
///
/// Stored as `NonZeroU8` so `Option<Layer>` is one byte and round-trips
/// through 0 = "no layer" sentinels in adjacent code.
///
/// The vtable signatures themselves still take `i32`/`u32` because they
/// match WA's binary contract; conversion happens at the function entry
/// via [`Layer::try_from_i32`] / [`Layer::try_from_u32`].
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
#[repr(transparent)]
pub struct Layer(NonZeroU8);

impl Layer {
    /// Layer 1 — backdrop (gradient/landscape).
    pub const ONE: Layer = match NonZeroU8::new(1) {
        Some(n) => Layer(n),
        None => unreachable!(),
    };
    /// Layer 2 — water/foreground sprites.
    pub const TWO: Layer = match NonZeroU8::new(2) {
        Some(n) => Layer(n),
        None => unreachable!(),
    };
    /// Layer 3 — main game layer.
    pub const THREE: Layer = match NonZeroU8::new(3) {
        Some(n) => Layer(n),
        None => unreachable!(),
    };

    /// Construct from a raw `u8`. Returns `None` outside `1..=3`.
    pub const fn new(value: u8) -> Option<Self> {
        match value {
            1..=3 => match NonZeroU8::new(value) {
                Some(n) => Some(Layer(n)),
                None => None,
            },
            _ => None,
        }
    }

    /// Construct from a signed integer (vtable param). `None` outside `1..=3`.
    #[inline]
    pub const fn try_from_i32(value: i32) -> Option<Self> {
        if value >= 1 && value <= 3 {
            Self::new(value as u8)
        } else {
            None
        }
    }

    /// Construct from an unsigned integer (vtable param). `None` outside `1..=3`.
    #[inline]
    pub const fn try_from_u32(value: u32) -> Option<Self> {
        if value >= 1 && value <= 3 {
            Self::new(value as u8)
        } else {
            None
        }
    }

    /// Index into `[T; 4]` arrays keyed by layer (slot 0 unused).
    #[inline]
    pub const fn idx(self) -> usize {
        self.0.get() as usize
    }

    /// Raw value as `u32`.
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0.get() as u32
    }

    /// Raw value as `i32`.
    #[inline]
    pub const fn as_i32(self) -> i32 {
        self.0.get() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range() {
        assert!(Layer::try_from_i32(0).is_none());
        assert!(Layer::try_from_i32(-1).is_none());
        assert!(Layer::try_from_i32(4).is_none());
        assert!(Layer::try_from_u32(0).is_none());
        assert!(Layer::try_from_u32(4).is_none());
    }

    #[test]
    fn accepts_in_range() {
        assert_eq!(Layer::try_from_i32(1), Some(Layer::ONE));
        assert_eq!(Layer::try_from_i32(2), Some(Layer::TWO));
        assert_eq!(Layer::try_from_i32(3), Some(Layer::THREE));
    }

    #[test]
    fn idx_round_trip() {
        assert_eq!(Layer::ONE.idx(), 1);
        assert_eq!(Layer::TWO.idx(), 2);
        assert_eq!(Layer::THREE.idx(), 3);
    }

    #[test]
    fn option_layer_is_one_byte() {
        assert_eq!(core::mem::size_of::<Option<Layer>>(), 1);
    }
}
