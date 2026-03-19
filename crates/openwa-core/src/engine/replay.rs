/// Replay file parsing: ReplayStream cursor, ParseReplayPosition, and types.
///
/// Ports the WA.exe stream helper functions (0x461340..0x461690) as methods on
/// `ReplayStream`, and `ParseReplayPosition` (0x4E3490) as a standalone function.

/// Error codes returned by ReplayLoader (0x462DF0).
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    FileNotFound = -1,       // 0xFFFFFFFF
    InvalidFormat = -2,      // 0xFFFFFFFE
    VersionTooNew = -3,      // 0xFFFFFFFD
    MallocFailure = -4,      // 0xFFFFFFFC
    MapLoadFailure = -5,     // 0xFFFFFFFB
    ArtClassLimit = -6,      // 0xFFFFFFFA
    RepairWithTeams = -7,    // 0xFFFFFFF9
    // -8 is unused (gap in error codes)
    NoScheme = -9,           // 0xFFFFFFF7
    SchemeSaveFailure = -10, // 0xFFFFFFF6
}

/// Replay file magic: "WA" in little-endian.
pub const REPLAY_MAGIC: u16 = 0x4157;
/// Maximum supported replay version.
pub const REPLAY_MAX_VERSION: u16 = 0x13;
/// XOR key for game ID integrity check.
pub const REPLAY_XOR_KEY: u32 = 0xEF5B_5C49;

/// Cursor over a replay payload byte buffer.
///
/// Mirrors the 3-DWORD stream context used by WA's stream helper functions:
/// `[data_ptr, total_size, cursor_offset]`.
pub struct ReplayStream<'a> {
    data: &'a [u8],
    cursor: usize,
}

impl<'a> ReplayStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.cursor)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Advance cursor by `n` bytes, returning the raw slice. Public for memcpy to globals.
    pub fn advance_raw(&mut self, n: usize) -> Result<&'a [u8], ReplayError> {
        self.advance(n)
    }

    /// Advance cursor by `n` bytes, returning the slice. Fails if not enough data.
    fn advance(&mut self, n: usize) -> Result<&'a [u8], ReplayError> {
        let end = self.cursor + n;
        if end > self.data.len() {
            return Err(ReplayError::InvalidFormat);
        }
        let slice = &self.data[self.cursor..end];
        self.cursor = end;
        Ok(slice)
    }

    /// Read a single byte. Port of parts of Replay__ReadByteValidated (0x4614D0).
    pub fn read_u8(&mut self) -> Result<u8, ReplayError> {
        let slice = self.advance(1)?;
        Ok(slice[0])
    }

    /// Read a single byte, validating it is in [min, max].
    /// Port of Replay__ReadByteValidated (0x4614D0).
    pub fn read_u8_validated(&mut self, min: u8, max: u8) -> Result<u8, ReplayError> {
        let val = self.read_u8()?;
        if val < min || val > max {
            return Err(ReplayError::InvalidFormat);
        }
        Ok(val)
    }

    /// Read a little-endian u16. Part of Replay__ReadU16Validated (0x4615B0).
    pub fn read_u16(&mut self) -> Result<u16, ReplayError> {
        let slice = self.advance(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    /// Read a little-endian u16, validating range.
    /// Port of Replay__ReadU16Validated (0x4615B0).
    pub fn read_u16_validated(&mut self, min: u16, max: u16) -> Result<u16, ReplayError> {
        let val = self.read_u16()?;
        if val < min || val > max {
            return Err(ReplayError::InvalidFormat);
        }
        Ok(val)
    }

    /// Read a little-endian u32.
    pub fn read_u32(&mut self) -> Result<u32, ReplayError> {
        let slice = self.advance(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    /// Read a little-endian i32.
    pub fn read_i32(&mut self) -> Result<i32, ReplayError> {
        Ok(self.read_u32()? as i32)
    }

    /// Read `n` bytes into a destination buffer. Fails if not enough data.
    pub fn read_into(&mut self, dest: &mut [u8]) -> Result<(), ReplayError> {
        let slice = self.advance(dest.len())?;
        dest.copy_from_slice(slice);
        Ok(())
    }

    /// Read a length-prefixed string into buffer.
    /// Port of Replay__ReadPrefixedString (0x461340).
    ///
    /// Reads 1 byte (length), then `length` bytes of string data, null-terminates.
    pub fn read_prefixed_string(&mut self, dest: &mut [u8]) -> Result<usize, ReplayError> {
        let len = self.read_u8()? as usize;
        if len > dest.len().saturating_sub(1) {
            return Err(ReplayError::InvalidFormat);
        }
        let slice = self.advance(len)?;
        dest[..len].copy_from_slice(slice);
        dest[len] = 0; // null-terminate
        Ok(len)
    }

    /// Read a worm name: either 0x11 fixed bytes or length-prefixed.
    /// Port of Replay__ReadWormName (0x461620).
    pub fn read_worm_name(
        &mut self,
        dest: &mut [u8; 0x11],
        use_fixed: bool,
    ) -> Result<(), ReplayError> {
        if use_fixed {
            let slice = self.advance(0x11)?;
            dest.copy_from_slice(slice);
        } else {
            dest.fill(0);
            self.read_prefixed_string(dest)?;
        }
        Ok(())
    }

    /// Skip `n` bytes.
    pub fn skip(&mut self, n: usize) -> Result<(), ReplayError> {
        self.advance(n)?;
        Ok(())
    }
}

/// Validate team type byte. Port of Replay__ValidateTeamType (0x461690).
///
/// Returns true if the team type value is in a valid range:
/// - Non-negative: type < 13
/// - Negative: -100 or absolute value <= 100 (SETLE in disassembly)
pub fn validate_team_type(team_type: i8) -> bool {
    if team_type >= 0 {
        team_type < 13
    } else {
        team_type == -100 || -(team_type as i32) <= 100
    }
}

/// Parse a replay position time string to frame count (50fps).
///
/// Format: `[MM:]SS[.FF]` where:
/// - MM = minutes (multiplied by 60)
/// - SS = seconds (max 59 after a colon)
/// - FF = fractional (multiplied by 50 for frames, divided by power of 10)
/// - Maximum 2 colons allowed
///
/// Returns frame count, or -1 on parse error.
/// Port of WA.exe ParseReplayPosition (0x4E3490).
pub fn parse_replay_position(input: &[u8]) -> i32 {
    let mut colon_count: i32 = 0;
    let mut accumulated: i32 = 0;
    let mut current: i32 = 0;
    let mut digit_count: i32 = 0;
    let mut frac_divisor: i32 = 0; // 0 = integer part, >0 = fractional
    let mut i = 0;

    loop {
        if i >= input.len() {
            return -1;
        }

        // Process digit runs
        while i < input.len() && input[i] >= b'0' && input[i] <= b'9' {
            let max_digits = if colon_count < 1 { 4 } else { 2 };
            if digit_count >= max_digits {
                return -1;
            }
            if frac_divisor == 0 {
                current = current * 10 + (input[i] - b'0') as i32;
                digit_count += 1;
            } else {
                let frac_value = ((input[i] - b'0') as i32 * 50) / frac_divisor;
                frac_divisor *= 10;
                accumulated += frac_value;
            }
            i += 1;
        }

        // Check delimiter
        if i >= input.len() {
            return -1;
        }
        let delim = input[i];

        if delim != b':' && delim != b'.' && delim != 0 {
            return -1;
        }

        if frac_divisor == 0 {
            // Integer part complete
            if colon_count > 0 && current > 59 {
                return -1;
            }
            accumulated = current + accumulated * 60;
            current = 0;
            digit_count = 0;

            if delim == b':' {
                if colon_count >= 2 {
                    return -1;
                }
                colon_count += 1;
            } else {
                // '.' or '\0' — convert seconds to frames
                accumulated *= 50;
            }
        } else {
            // Fractional part — ':' and '.' are invalid after '.'
            if delim == b':' || delim == b'.' {
                return -1;
            }
        }

        if delim == 0 {
            return accumulated;
        }

        if delim == b'.' {
            frac_divisor = 10;
        }

        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ReplayStream tests ---

    #[test]
    fn test_read_u8() {
        let data = [0x42, 0xFF];
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u8().unwrap(), 0x42);
        assert_eq!(s.read_u8().unwrap(), 0xFF);
        assert!(s.read_u8().is_err());
    }

    #[test]
    fn test_read_u8_validated() {
        let data = [5, 0, 20];
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u8_validated(0, 10).unwrap(), 5);
        assert_eq!(s.read_u8_validated(0, 10).unwrap(), 0);
        assert!(s.read_u8_validated(0, 10).is_err()); // 20 > 10
    }

    #[test]
    fn test_read_u16_le() {
        let data = [0x57, 0x41]; // "WA" = 0x4157
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u16().unwrap(), 0x4157);
    }

    #[test]
    fn test_read_u32_le() {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut s = ReplayStream::new(&data);
        assert_eq!(s.read_u32().unwrap(), 0x12345678);
    }

    #[test]
    fn test_read_prefixed_string() {
        let data = [3, b'f', b'o', b'o', 0, b'x']; // len=3, "foo"
        let mut s = ReplayStream::new(&data);
        let mut buf = [0u8; 16];
        let len = s.read_prefixed_string(&mut buf).unwrap();
        assert_eq!(len, 3);
        assert_eq!(&buf[..4], b"foo\0");
        assert_eq!(s.cursor(), 4);
    }

    #[test]
    fn test_read_prefixed_string_overflow() {
        let data = [10, b'a', b'b']; // claims len=10 but only 2 bytes
        let mut s = ReplayStream::new(&data);
        let mut buf = [0u8; 4];
        assert!(s.read_prefixed_string(&mut buf).is_err());
    }

    #[test]
    fn test_read_worm_name_fixed() {
        let mut data = [0u8; 0x11];
        data[0] = b'W';
        data[1] = b'o';
        data[2] = b'r';
        data[3] = b'm';
        let mut s = ReplayStream::new(&data);
        let mut name = [0u8; 0x11];
        s.read_worm_name(&mut name, true).unwrap();
        assert_eq!(name[0], b'W');
        assert_eq!(s.cursor(), 0x11);
    }

    #[test]
    fn test_read_worm_name_prefixed() {
        let data = [4, b'W', b'o', b'r', b'm'];
        let mut s = ReplayStream::new(&data);
        let mut name = [0u8; 0x11];
        s.read_worm_name(&mut name, false).unwrap();
        assert_eq!(&name[..5], b"Worm\0");
    }

    #[test]
    fn test_read_into() {
        let data = [1, 2, 3, 4, 5];
        let mut s = ReplayStream::new(&data);
        let mut buf = [0u8; 3];
        s.read_into(&mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3]);
        assert_eq!(s.cursor(), 3);
    }

    #[test]
    fn test_skip() {
        let data = [0u8; 10];
        let mut s = ReplayStream::new(&data);
        s.skip(5).unwrap();
        assert_eq!(s.cursor(), 5);
        assert_eq!(s.remaining(), 5);
        assert!(s.skip(6).is_err());
    }

    // --- validate_team_type tests ---

    #[test]
    fn test_validate_team_type() {
        assert!(validate_team_type(0));
        assert!(validate_team_type(12));
        assert!(!validate_team_type(13));
        assert!(validate_team_type(-1));
        assert!(validate_team_type(-99));
        assert!(validate_team_type(-100));
        assert!(!validate_team_type(-101));
    }

    // --- ParseReplayPosition tests ---

    #[test]
    fn test_parse_replay_position_seconds() {
        assert_eq!(parse_replay_position(b"30\0"), 30 * 50);
    }

    #[test]
    fn test_parse_replay_position_minutes_seconds() {
        assert_eq!(parse_replay_position(b"1:30\0"), (60 + 30) * 50);
    }

    #[test]
    fn test_parse_replay_position_with_frames() {
        assert_eq!(parse_replay_position(b"1:30.5\0"), (60 + 30) * 50 + 25);
    }

    #[test]
    fn test_parse_replay_position_zero() {
        assert_eq!(parse_replay_position(b"0\0"), 0);
    }

    #[test]
    fn test_parse_replay_position_invalid() {
        assert_eq!(parse_replay_position(b"abc\0"), -1);
    }

    #[test]
    fn test_parse_replay_position_seconds_over_59() {
        assert_eq!(parse_replay_position(b"1:60\0"), -1);
    }

    #[test]
    fn test_parse_replay_position_no_null() {
        // No null terminator — should return -1
        assert_eq!(parse_replay_position(b"30"), -1);
    }
}
