use core::ops::{Add, Mul, Neg, Sub};
use std::ops::{AddAssign, SubAssign};

/// 16.16 fixed-point number used throughout WA for coordinates and velocities.
///
/// The game uses `0x10000` (65536) to represent `1.0`.
/// For example, a position of `0x30000` means 3.0 world units.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Fixed(pub i32);

impl Fixed {
    pub const FRACTIONAL_BITS: u32 = 16;
    pub const SCALE: i32 = 1 << Self::FRACTIONAL_BITS; // 0x10000 = 65536

    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(Self::SCALE);

    #[inline]
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn to_raw(self) -> i32 {
        self.0
    }

    #[inline]
    pub const fn from_int(n: i32) -> Self {
        Self(n << Self::FRACTIONAL_BITS)
    }

    #[inline]
    pub const fn to_int(self) -> i32 {
        self.0 >> Self::FRACTIONAL_BITS
    }

    #[inline]
    pub fn from_f32(f: f32) -> Self {
        Self((f * Self::SCALE as f32) as i32)
    }

    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
}

impl Add for Fixed {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Fixed {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul for Fixed {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        // Use i64 to avoid overflow during multiplication
        Self(((self.0 as i64 * rhs.0 as i64) >> Self::FRACTIONAL_BITS) as i32)
    }
}

impl Neg for Fixed {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl core::fmt::Debug for Fixed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Fixed({:.4} raw=0x{:08x})", self.to_f32(), self.0)
    }
}

impl core::fmt::Display for Fixed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:.4}", self.to_f32())
    }
}

impl AddAssign for Fixed {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Fixed {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}
