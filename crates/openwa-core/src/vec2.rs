use crate::fixed::Fixed;
use bytemuck::{Pod, Zeroable};
use core::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// Pair of [`Fixed`] values laid out as two adjacent 32-bit ints.
///
/// `#[repr(C)]` over two `Fixed` (each `#[repr(transparent)] i32`) gives the
/// same layout as `[i32; 2]` — 8 bytes, 4-byte aligned, no padding. That
/// matches every adjacent `pos_x`/`pos_y`, `speed_x`/`speed_y`, etc. field
/// pair in WA's structs, so a `*mut Vec2` reinterpret-cast over the first
/// of the two is sound on `i686-pc-windows-msvc`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Zeroable, Pod)]
#[repr(C)]
pub struct Vec2 {
    pub x: Fixed,
    pub y: Fixed,
}

impl Vec2 {
    pub const ZERO: Self = Self {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    };

    #[inline]
    pub const fn new(x: Fixed, y: Fixed) -> Self {
        Self { x, y }
    }

    #[inline]
    pub const fn from_raw(x: i32, y: i32) -> Self {
        Self {
            x: Fixed::from_raw(x),
            y: Fixed::from_raw(y),
        }
    }

    #[inline]
    pub const fn wrapping_add(self, rhs: Self) -> Self {
        Self {
            x: self.x.wrapping_add(rhs.x),
            y: self.y.wrapping_add(rhs.y),
        }
    }

    #[inline]
    pub const fn wrapping_sub(self, rhs: Self) -> Self {
        Self {
            x: self.x.wrapping_sub(rhs.x),
            y: self.y.wrapping_sub(rhs.y),
        }
    }

    /// Component-wise [`Fixed::mul_raw`]: `((self * rhs) >> 16)` per axis.
    #[inline]
    pub fn mul_raw(self, rhs: Fixed) -> Self {
        Self {
            x: self.x.mul_raw(rhs),
            y: self.y.mul_raw(rhs),
        }
    }

    /// Snap each axis to the nearest of `{-1, 0, +1}` with a half-unit
    /// deadzone, then scale diagonals by `3/5` so the result has roughly
    /// unit magnitude.
    ///
    /// Per axis:
    /// - `v <= -0x8001` (strictly below `-0.5`) → `Fixed(-1)`
    /// - `-0x8000..=0x8000` (closed deadzone around 0) → `Fixed(0)`
    /// - `v >= 0x8001` (strictly above `+0.5`) → `Fixed(+1)`
    ///
    /// If both axes end up non-zero (a diagonal), each is multiplied by
    /// `3/5` with C truncation toward zero, yielding `Fixed(±0.6)` per axis
    /// — close enough to `1/√2 ≈ 0.707` for WA's 8-way movement snap.
    ///
    /// Mirrors WA.exe's `Vec2__SignClampAndScale` (0x00518B80).
    pub fn snap_to_8way(self) -> Self {
        fn snap(v: Fixed) -> Fixed {
            if v.0 < -0x8000 {
                Fixed(-Fixed::SCALE)
            } else if v.0 <= 0x8000 {
                Fixed::ZERO
            } else {
                Fixed::ONE
            }
        }
        let x = snap(self.x);
        let y = snap(self.y);
        if x.0 != 0 && y.0 != 0 {
            Self {
                x: Fixed(x.0 * 3 / 5),
                y: Fixed(y.0 * 3 / 5),
            }
        } else {
            Self { x, y }
        }
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Neg for Vec2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}

/// Component-wise scalar multiply: `Vec2 * i32` scales each raw axis by `n`.
impl Mul<i32> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: i32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Mul<Fixed> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Fixed) -> Self {
        Self {
            x: self.x.mul_raw(rhs),
            y: self.y.mul_raw(rhs),
        }
    }
}

impl core::fmt::Debug for Vec2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Vec2({}, {})", self.x, self.y)
    }
}

impl core::fmt::Display for Vec2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_two_adjacent_i32s() {
        assert_eq!(core::mem::size_of::<Vec2>(), 8);
        assert_eq!(core::mem::align_of::<Vec2>(), 4);
        assert_eq!(core::mem::offset_of!(Vec2, x), 0);
        assert_eq!(core::mem::offset_of!(Vec2, y), 4);
    }

    #[test]
    fn reinterpret_two_adjacent_fixed_as_vec2() {
        #[repr(C)]
        struct Pair {
            a: Fixed,
            b: Fixed,
        }
        let mut p = Pair {
            a: Fixed::from_int(3),
            b: Fixed::from_int(5),
        };
        let v = unsafe { &mut *((&raw mut p.a) as *mut Vec2) };
        assert_eq!(v.x, Fixed::from_int(3));
        assert_eq!(v.y, Fixed::from_int(5));
        v.x = Fixed::from_int(7);
        assert_eq!(p.a, Fixed::from_int(7));
    }

    #[test]
    fn snap_to_8way_matches_wa_semantics() {
        let f = Fixed::from_raw;

        // Deadzone is closed at ±0x8000, opens at ±0x8001.
        assert_eq!(Vec2::new(f(0x8000), f(0)).snap_to_8way(), Vec2::ZERO);
        assert_eq!(Vec2::new(f(-0x8000), f(0)).snap_to_8way(), Vec2::ZERO);
        assert_eq!(
            Vec2::new(f(0x8001), f(0)).snap_to_8way(),
            Vec2::new(Fixed::ONE, Fixed::ZERO)
        );
        assert_eq!(
            Vec2::new(f(-0x8001), f(0)).snap_to_8way(),
            Vec2::new(-Fixed::ONE, Fixed::ZERO)
        );

        // Cardinal directions stay at ±1.
        assert_eq!(
            Vec2::new(f(0x10000), f(0)).snap_to_8way(),
            Vec2::new(Fixed::ONE, Fixed::ZERO)
        );

        // Diagonals scale by 3/5 → ±0x9999 raw on each axis.
        assert_eq!(
            Vec2::new(f(0x10000), f(0x10000)).snap_to_8way(),
            Vec2::from_raw(0x9999, 0x9999)
        );
        assert_eq!(
            Vec2::new(f(-0x10000), f(0x10000)).snap_to_8way(),
            Vec2::from_raw(-0x9999, 0x9999)
        );
    }

    #[test]
    fn arithmetic_is_componentwise() {
        let a = Vec2::from_raw(0x10000, 0x20000);
        let b = Vec2::from_raw(0x40000, 0x80000);
        assert_eq!((a + b), Vec2::from_raw(0x50000, 0xA0000));
        assert_eq!((b - a), Vec2::from_raw(0x30000, 0x60000));
        assert_eq!((a * 4), Vec2::from_raw(0x40000, 0x80000));
        assert_eq!(
            a.mul_raw(Fixed::from_int(2)),
            Vec2::from_raw(0x20000, 0x40000)
        );
    }
}
