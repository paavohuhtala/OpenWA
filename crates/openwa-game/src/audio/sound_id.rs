/// Packed sound ID + playback flags, passed as a single u32.
///
/// Layout:
/// - Bits 0-15: sound slot index (1-499 for SFX, higher for speech)
/// - Bit 16: loop flag (play in a loop)
/// - Bit 17: raw volume flag (skip master volume scaling)
///
/// Used throughout the sound system: queue insertion, DSSound playback,
/// and streaming dispatch all operate on this same packed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SoundId(pub u32);

impl SoundId {
    /// Sound slot index (low 16 bits). 0 = invalid/empty.
    #[inline]
    pub fn index(self) -> usize {
        (self.0 & 0xFFFF) as usize
    }

    /// Whether the sound should loop continuously (bit 16).
    #[inline]
    pub fn is_looping(self) -> bool {
        self.0 & 0x10000 != 0
    }

    /// Whether volume should be used directly, skipping master volume scaling (bit 17).
    #[inline]
    pub fn is_raw_volume(self) -> bool {
        self.0 & 0x20000 != 0
    }

    /// Return a copy with the loop flag cleared.
    #[inline]
    pub fn without_loop(self) -> Self {
        Self(self.0 & 0xFFFEFFFF)
    }
}
