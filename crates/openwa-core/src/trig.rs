//! Fixed-point sine/cosine tables and interpolated lookup.
//!
//! The two 1025-entry `[i32; 1025]` tables are byte-for-byte copies of the
//! sine and cosine tables in WA.exe's `.rdata` (at `G_SIN_TABLE` / `G_COS_TABLE`).
//! Values are 16.16 fixed-point. Each table covers one full period in 1024
//! equal steps (2π / 1024 radians per step); the 1025th entry is a sentinel
//! that matches the 0th and enables interpolation at the wraparound.
//!
//! The lookup angle is a 16-bit quantity where the full range `0..=0xFFFF`
//! maps to one full period. The upper 10 bits select the table entry and
//! the lower 6 bits are used for linear interpolation between adjacent
//! entries.
//!
//! `openwa-game::trig::validate_against_wa_exe` asserts at DLL startup that
//! the embedded tables still match the live WA.exe tables byte-for-byte.

use crate::fixed::Fixed;

/// Size of each table. 1024 + 1 sentinel for wraparound interpolation.
pub const TABLE_LEN: usize = 1025;

/// Decode a little-endian `[u8; 4100]` blob into `[i32; 1025]` at const time.
const fn decode_table(bytes: &[u8; TABLE_LEN * 4]) -> [i32; TABLE_LEN] {
    let mut out = [0i32; TABLE_LEN];
    let mut i = 0;
    while i < TABLE_LEN {
        let b0 = bytes[i * 4] as u32;
        let b1 = bytes[i * 4 + 1] as u32;
        let b2 = bytes[i * 4 + 2] as u32;
        let b3 = bytes[i * 4 + 3] as u32;
        out[i] = (b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) as i32;
        i += 1;
    }
    out
}

/// Byte-for-byte copy of WA.exe's sine table at `G_SIN_TABLE` (0x006A_1860).
pub const SIN_TABLE: [i32; TABLE_LEN] = decode_table(include_bytes!("trig_tables/sin_table.bin"));

/// Byte-for-byte copy of WA.exe's cosine table at `G_COS_TABLE` (0x006A_1C60).
pub const COS_TABLE: [i32; TABLE_LEN] = decode_table(include_bytes!("trig_tables/cos_table.bin"));

/// Interpolated lookup from a 1025-entry fixed-point trig table.
///
/// Pure helper over an arbitrary table pointer. `angle` is a 16-bit angle
/// where `0..=0xFFFF` maps to one full period.
#[inline]
pub fn trig_lookup_table(table: &[i32; TABLE_LEN], angle: u32) -> Fixed {
    let index = ((angle as i32) >> 6) as usize & 0x3FF;
    let frac = Fixed::from_raw(((angle & 0x3F) << 10) as i32);
    let base = Fixed::from_raw(table[index]);
    let next = Fixed::from_raw(table[index + 1]);
    (next - base).mul_raw(frac) + base
}

/// Sine lookup using the embedded table. Safe, no WA.exe dependency.
#[inline]
pub fn sin(angle: u32) -> Fixed {
    trig_lookup_table(&SIN_TABLE, angle)
}

/// Cosine lookup using the embedded table. Safe, no WA.exe dependency.
#[inline]
pub fn cos(angle: u32) -> Fixed {
    trig_lookup_table(&COS_TABLE, angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_zero_is_zero() {
        assert_eq!(SIN_TABLE[0], 0);
        assert_eq!(sin(0), Fixed::from_raw(0));
    }

    #[test]
    fn cos_zero_is_one() {
        assert_eq!(COS_TABLE[0], 0x0001_0000);
        assert_eq!(cos(0), Fixed::ONE);
    }

    #[test]
    fn sin_quarter_period_is_one() {
        // 1024 entries per period, so quarter-period = entry 256.
        assert_eq!(SIN_TABLE[256], 0x0001_0000);
    }

    #[test]
    fn cos_quarter_period_is_zero() {
        assert_eq!(COS_TABLE[256], 0);
    }

    #[test]
    fn tables_wrap_at_sentinel() {
        // 1025th entry is the wraparound sentinel (equals entry 0).
        assert_eq!(SIN_TABLE[1024], SIN_TABLE[0]);
        assert_eq!(COS_TABLE[1024], COS_TABLE[0]);
    }
}
