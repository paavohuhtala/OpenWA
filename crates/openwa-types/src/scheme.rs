/// Scheme file (.wsc) parser for Worms Armageddon.
///
/// .wsc files store game settings (turn time, wind, per-weapon ammo/delay, etc.).
/// Located in `User\Schemes\*.wsc`.
///
/// Binary format:
///   Bytes 0-3: Magic "SCHM" (0x5343484D)
///   Byte 4:    Version byte
///   Bytes 5+:  Payload (size depends on version)
///
/// Version → payload size:
///   0x01 → 0xD8 bytes (216)
///   0x02 → 0x124 bytes (292)
///   other → version_byte as usize + 0x124 bytes
///
/// Source: Ghidra decompilation of Scheme__Load (0x4D4CD0),
///         Scheme__ParseFile (0x4D44F0), hex analysis of bundled .wsc files.

/// Magic header bytes for .wsc files.
pub const SCHEME_MAGIC: [u8; 4] = *b"SCHM";

/// Size of the file header (magic + version byte).
pub const SCHEME_HEADER_SIZE: usize = 5;

/// Payload size for version 1 schemes.
pub const SCHEME_PAYLOAD_V1: usize = 0xD8;

/// Payload size for version 2 schemes.
pub const SCHEME_PAYLOAD_V2: usize = 0x124;

/// Scheme file version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemeVersion {
    /// Version 1: 0xD8 byte payload (total file: 221 bytes)
    V1,
    /// Version 2: 0x124 byte payload (total file: 297 bytes)
    V2,
    /// Extended: payload = version_byte + 0x124 bytes
    Extended(u8),
}

impl SchemeVersion {
    /// Raw version byte as stored in the file.
    pub fn to_byte(self) -> u8 {
        match self {
            SchemeVersion::V1 => 1,
            SchemeVersion::V2 => 2,
            SchemeVersion::Extended(v) => v,
        }
    }

    /// Parse version from the raw byte.
    pub fn from_byte(b: u8) -> Self {
        match b {
            1 => SchemeVersion::V1,
            2 => SchemeVersion::V2,
            v => SchemeVersion::Extended(v),
        }
    }

    /// Expected payload size for this version.
    pub fn payload_size(self) -> usize {
        match self {
            SchemeVersion::V1 => SCHEME_PAYLOAD_V1,
            SchemeVersion::V2 => SCHEME_PAYLOAD_V2,
            SchemeVersion::Extended(v) => v as usize + SCHEME_PAYLOAD_V2,
        }
    }
}

/// Errors from parsing a .wsc scheme file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemeError {
    /// File is too short to contain the header.
    TooShort { len: usize },
    /// Magic bytes don't match "SCHM".
    BadMagic([u8; 4]),
    /// Payload size doesn't match what the version byte expects.
    PayloadMismatch { expected: usize, got: usize },
}

impl core::fmt::Display for SchemeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SchemeError::TooShort { len } => {
                write!(f, "file too short ({len} bytes, need at least {SCHEME_HEADER_SIZE})")
            }
            SchemeError::BadMagic(m) => {
                write!(f, "bad magic: {:02X} {:02X} {:02X} {:02X} (expected SCHM)", m[0], m[1], m[2], m[3])
            }
            SchemeError::PayloadMismatch { expected, got } => {
                write!(f, "payload size mismatch: expected {expected}, got {got}")
            }
        }
    }
}

/// A parsed .wsc scheme file.
///
/// The payload bytes are initially opaque. Typed field accessors will be
/// added incrementally as we map individual byte offsets through RE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeFile {
    pub version: SchemeVersion,
    /// Raw payload bytes (game options + per-weapon settings).
    /// Length matches `version.payload_size()`.
    pub payload: Vec<u8>,
}

impl SchemeFile {
    /// Parse a scheme file from raw bytes (entire file contents).
    pub fn from_bytes(data: &[u8]) -> Result<Self, SchemeError> {
        if data.len() < SCHEME_HEADER_SIZE {
            return Err(SchemeError::TooShort { len: data.len() });
        }

        let magic: [u8; 4] = data[0..4].try_into().unwrap();
        if magic != SCHEME_MAGIC {
            return Err(SchemeError::BadMagic(magic));
        }

        let version = SchemeVersion::from_byte(data[4]);
        let expected_payload = version.payload_size();
        let actual_payload = data.len() - SCHEME_HEADER_SIZE;

        if actual_payload != expected_payload {
            return Err(SchemeError::PayloadMismatch {
                expected: expected_payload,
                got: actual_payload,
            });
        }

        Ok(SchemeFile {
            version,
            payload: data[SCHEME_HEADER_SIZE..].to_vec(),
        })
    }

    /// Serialize back to .wsc format.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(SCHEME_HEADER_SIZE + self.payload.len());
        buf.extend_from_slice(&SCHEME_MAGIC);
        buf.push(self.version.to_byte());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Total file size (header + payload).
    pub fn file_size(&self) -> usize {
        SCHEME_HEADER_SIZE + self.payload.len()
    }
}

impl SchemeFile {
    /// Load a scheme file from disk.
    pub fn from_file(path: &std::path::Path) -> Result<Self, SchemeFileError> {
        let data = std::fs::read(path).map_err(SchemeFileError::Io)?;
        Self::from_bytes(&data).map_err(SchemeFileError::Parse)
    }

    /// Save a scheme file to disk.
    pub fn to_file(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        std::fs::write(path, self.to_bytes())
    }
}

/// Error type for file-based scheme operations.
#[derive(Debug)]
pub enum SchemeFileError {
    Io(std::io::Error),
    Parse(SchemeError),
}

impl core::fmt::Display for SchemeFileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SchemeFileError::Io(e) => write!(f, "I/O error: {e}"),
            SchemeFileError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_roundtrip() {
        for v in [SchemeVersion::V1, SchemeVersion::V2, SchemeVersion::Extended(5)] {
            assert_eq!(SchemeVersion::from_byte(v.to_byte()), v);
        }
    }

    #[test]
    fn payload_sizes() {
        assert_eq!(SchemeVersion::V1.payload_size(), 0xD8);
        assert_eq!(SchemeVersion::V2.payload_size(), 0x124);
        assert_eq!(SchemeVersion::Extended(3).payload_size(), 3 + 0x124);
    }

    #[test]
    fn parse_v1_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x01);
        data.extend_from_slice(&[0xAA; SCHEME_PAYLOAD_V1]);
        assert_eq!(data.len(), 221);

        let scheme = SchemeFile::from_bytes(&data).unwrap();
        assert_eq!(scheme.version, SchemeVersion::V1);
        assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V1);
        assert!(scheme.payload.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn parse_v2_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x02);
        data.extend_from_slice(&[0xBB; SCHEME_PAYLOAD_V2]);
        assert_eq!(data.len(), 297);

        let scheme = SchemeFile::from_bytes(&data).unwrap();
        assert_eq!(scheme.version, SchemeVersion::V2);
        assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V2);
    }

    #[test]
    fn roundtrip_synthetic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x01);
        data.extend_from_slice(&[0x42; SCHEME_PAYLOAD_V1]);

        let scheme = SchemeFile::from_bytes(&data).unwrap();
        assert_eq!(scheme.to_bytes(), data);
    }

    #[test]
    fn error_too_short() {
        assert_eq!(
            SchemeFile::from_bytes(b"SCH"),
            Err(SchemeError::TooShort { len: 3 })
        );
    }

    #[test]
    fn error_bad_magic() {
        let mut data = vec![b'N', b'O', b'P', b'E', 0x01];
        data.extend_from_slice(&[0; SCHEME_PAYLOAD_V1]);
        assert!(matches!(
            SchemeFile::from_bytes(&data),
            Err(SchemeError::BadMagic([b'N', b'O', b'P', b'E']))
        ));
    }

    #[test]
    fn error_payload_mismatch() {
        // v1 header but only 10 bytes of payload
        let mut data = Vec::new();
        data.extend_from_slice(b"SCHM");
        data.push(0x01);
        data.extend_from_slice(&[0; 10]);

        assert!(matches!(
            SchemeFile::from_bytes(&data),
            Err(SchemeError::PayloadMismatch { expected: 0xD8, got: 10 })
        ));
    }
}
