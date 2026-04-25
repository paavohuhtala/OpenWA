use bytemuck::{Pod, Zeroable};
use core::ops::{Add, Div, Mul, Neg, Sub};
use std::ops::{AddAssign, SubAssign};

/// 16.16 fixed-point number used throughout WA for coordinates and velocities.
///
/// The game uses `0x10000` (65536) to represent `1.0`.
/// For example, a position of `0x30000` means 3.0 world units.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Zeroable, Pod)]
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

    #[inline]
    pub fn max(self, other: Self) -> Self {
        if self > other { self } else { other }
    }

    #[inline]
    pub fn min(self, other: Self) -> Self {
        if self < other { self } else { other }
    }

    #[inline]
    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// Floor to integer boundary: clears the fractional bits.
    /// `Fixed(0x38000).floor()` = `Fixed(0x30000)`.
    #[inline]
    pub const fn floor(self) -> Self {
        Self(self.0 & !0xFFFF)
    }

    /// Pixel center: floor + 0.5. Used by line rasterizers to snap to pixel centers.
    #[inline]
    pub const fn pixel_center(self) -> Self {
        Self((self.0 & !0xFFFF) + 0x8000)
    }

    /// Round to nearest integer (half-up): `(self + 0.5).to_int()`.
    #[inline]
    pub const fn round_to_int(self) -> i32 {
        (self.0 + 0x8000) >> Self::FRACTIONAL_BITS
    }

    /// Fixed-point division for line clipping: `(self << 16) / rhs`.
    ///
    /// This is NOT the same as `Fixed / Fixed` (which is `(self << 16) / rhs`
    /// treating both as Fixed). This divides two Fixed values and returns a
    /// Fixed-point ratio suitable for interpolation.
    #[inline]
    pub fn div_raw(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return Self::ZERO;
        }
        Self((((self.0 as i64) << 16) / rhs.0 as i64) as i32)
    }

    /// Fixed-point multiply returning Fixed: `(self * rhs) >> 16`.
    #[inline]
    pub fn mul_raw(self, rhs: Self) -> Self {
        Self(((self.0 as i64 * rhs.0 as i64) >> 16) as i32)
    }

    /// Half a pixel (0.5 in Fixed).
    pub const HALF: Self = Self(0x8000);

    #[inline]
    pub const fn wrapping_add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }

    #[inline]
    pub const fn wrapping_sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
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

/// Scale a Fixed value by a plain integer (no shift — just `raw * n`).
impl Mul<i32> for Fixed {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: i32) -> Self {
        Self(self.0 * rhs)
    }
}

impl Div for Fixed {
    type Output = Self;
    /// Fixed-point division: (self << 16) / rhs, using i64 intermediate.
    #[inline]
    fn div(self, rhs: Self) -> Self {
        Self((((self.0 as i64) << Self::FRACTIONAL_BITS) / rhs.0 as i64) as i32)
    }
}

/// Divide a Fixed value by a plain integer (no shift — just `raw / n`).
impl Div<i32> for Fixed {
    type Output = Self;
    #[inline]
    fn div(self, rhs: i32) -> Self {
        Self(self.0 / rhs)
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

/// Wide sibling of [`Fixed`] — same 16 fractional bits, but 48 integer bits.
///
/// Used for accumulators that need to grow past `Fixed`'s ±32k integer
/// range without losing Fixed precision: `DDGame::replay_frame_accum`
/// adds `Fixed::ONE` every replay tick and would saturate the 32-bit
/// form within ~18 minutes at 50 fps.
///
/// Byte-compatible with two adjacent `u32`s on `i686-pc-windows-msvc`
/// (u64 aligns to 8 there, matching MSVC's C ABI).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Fixed64(pub i64);

impl Fixed64 {
    pub const FRACTIONAL_BITS: u32 = Fixed::FRACTIONAL_BITS;
    pub const SCALE: i64 = 1 << Self::FRACTIONAL_BITS;

    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(Self::SCALE);

    #[inline]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn to_raw(self) -> i64 {
        self.0
    }

    #[inline]
    pub const fn from_fixed(f: Fixed) -> Self {
        Self(f.0 as i64)
    }

    /// Low-32-bit projection: returns the accumulator value as [`Fixed`]
    /// with wraparound. Matches the original code's `accum as i32`
    /// narrowing pattern (used in speed-ratio subtractions).
    #[inline]
    pub const fn to_fixed_wrapping(self) -> Fixed {
        Fixed(self.0 as i32)
    }

    #[inline]
    pub const fn wrapping_add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }

    #[inline]
    pub const fn wrapping_sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Add for Fixed64 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Fixed64 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl AddAssign for Fixed64 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Fixed64 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl core::fmt::Debug for Fixed64 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Fixed64({:.4} raw=0x{:016x})",
            self.0 as f64 / Self::SCALE as f64,
            self.0
        )
    }
}
