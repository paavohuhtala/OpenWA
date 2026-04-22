//! LZSS-style decompressor with palette LUT remapping for sprite subframes.
//!
//! Pure-Rust port of `LZSS_Decode` (0x5B29E0). Used by
//! [`Sprite__GetFrameForBlit`](super::sprite) and `SpriteBank__GetFrameForBlit`
//! to lazily decompress sprite subframe pixel data into a `FrameCache`
//! payload.
//!
//! ## Format
//!
//! The decoder consumes a stream of source bytes from `src`. Each iteration
//! reads one byte and dispatches:
//!
//! - **Literal** (`src[0] & 0x80 == 0`): emit `lut[src[0]]` to `dst`,
//!   advance `src` by 1, advance `dst` by 1.
//!
//! - **Control** (`src[0] & 0x80 != 0`): the control word is `src[0..2]`,
//!   interpreted as `(src[0] << 8) | src[1]`.
//!     - `distance = control & 0x7FF` (11-bit back-reference offset).
//!     - `nibble = (src[0] >> 3) & 0xF`.
//!
//!     Three sub-forms:
//!
//!     1. **Short back-ref** (`nibble != 0`): copy `nibble + 2` bytes from
//!        `dst - distance - 1` to `dst`. Range 3..=17. Advance `src` by 2.
//!     2. **Long back-ref** (`nibble == 0`, `distance != 0`): read a third
//!        byte at `src[2]`. Copy `src[2] + 18` bytes from `dst - distance`
//!        (note the asymmetry — short refs use `-distance - 1`, long refs
//!        use `-distance`). Range 18..=273. Advance `src` by 3.
//!     3. **Terminator** (`nibble == 0`, `distance == 0`): exit the loop.
//!
//! **Encoding asymmetry**: the encoded `distance` field for a short
//! back-ref is `real_distance - 1` (the asm's `DEC ECX` between the
//! `JZ` and the read makes up the difference), but for a long back-ref
//! it equals `real_distance` directly. The `JZ extended` jumps over
//! the `DEC ECX` instruction. This was verified from the disassembly
//! at 0x5B2A13/0x5B2A15 — there is no encoder-side fix-up that would
//! normalize the two paths.
//!
//! ## LUT
//!
//! `lut` is a 256-byte palette translation table. For sprites it lives at
//! `Sprite::palette_data_ptr` (`+0x68`); for sprite banks it's the inline
//! `SpriteBank::palette_lut` (`+0x30`). Entry 0 is always `0` (transparent
//! index); entries `1..=palette_count` are mapped runtime palette indices.
//! See `feedback_layer_palette.md` for the off-by-one rationale.
//!
//! ## Buffer requirements
//!
//! - `src` must contain at least 3 bytes per control sequence (the original
//!   reads `src[2]` unconditionally on the long-backref path).
//! - `dst` must have already-written bytes at offsets `[-distance - 1, -1]`
//!   for any short back-ref, or `[-distance, -1]` for any long back-ref.
//!   Caller responsibility — the decoder does no bounds checking.
//! - `lut` must point at a 256-byte buffer.
//!
//! The original WA function wraps its body in `PUSHAD`/`POPAD` so callers
//! see no register clobber. The Rust port has no such concern.

/// Decode an LZSS-compressed sprite subframe into `dst`, remapping each
/// literal byte through `lut` first.
///
/// # Safety
///
/// - `dst` must point at a writable buffer large enough to hold the
///   decompressed payload (caller-known via `SpriteSubframeCache::decoded_size`
///   or `SpriteBankSubframeCache::decoded_size`).
/// - `src` must contain a well-formed LZSS stream terminated by a
///   zero-length zero-distance control word.
/// - `lut` must point at a 256-byte palette translation table.
/// - Back-references at `dst - distance - 1` (short) or `dst - distance`
///   (long) must lie within the already-decoded portion of `dst`.
pub unsafe fn lzss_decode(dst: *mut u8, src: *const u8, lut: *const u8) {
    unsafe {
        let (src_len, dst_len) = scan_stream_lengths(src);
        let dst_slice = core::slice::from_raw_parts_mut(dst, dst_len);
        let src_slice = core::slice::from_raw_parts(src, src_len);
        let lut_ref = &*(lut as *const [u8; 256]);
        lzss_decode_slice(dst_slice, src_slice, lut_ref);
    }
}

/// Safe slice-based wrapper around [`lzss_decode`].
///
/// Caller sizes `dst` to the expected decompressed payload length. `src`
/// must hold a well-formed LZSS stream (the decoder reads until it sees
/// the terminator control word); the slice length is an upper bound only.
/// `lut` is a 256-byte palette translation table.
///
/// Back-reference bounds are still the caller's responsibility (see the
/// `lzss_decode` safety section) — this wrapper does not add
/// per-byte checks, it only localizes the raw-pointer call.
pub fn lzss_decode_slice(dst: &mut [u8], src: &[u8], lut: &[u8; 256]) {
    let mut src_pos: usize = 0;
    let mut dst_pos: usize = 0;

    loop {
        let b: u8 = loop {
            let b = src
                .get(src_pos)
                .copied()
                .expect("lzss_decode_slice: source stream ended before control byte");
            if b & 0x80 != 0 {
                break b;
            }

            let out = dst
                .get_mut(dst_pos)
                .expect("lzss_decode_slice: output overflow in literal run");
            *out = lut[b as usize];
            src_pos += 1;
            dst_pos += 1;
        };

        let b1 = *src
            .get(src_pos + 1)
            .expect("lzss_decode_slice: truncated control word");
        let distance = (((b as u32) << 8) | (b1 as u32)) & 0x7FF;
        let nibble = ((b as u32) >> 3) & 0xF;

        if nibble != 0 {
            let copy_offset = (distance as usize) + 1;
            let total = (nibble as usize) + 2;
            for _ in 0..total {
                assert!(
                    copy_offset <= dst_pos,
                    "lzss_decode_slice: short back-ref before output start"
                );
                let v = dst[dst_pos - copy_offset];
                let out = dst
                    .get_mut(dst_pos)
                    .expect("lzss_decode_slice: output overflow in short back-ref");
                *out = v;
                dst_pos += 1;
            }
            src_pos += 2;
        } else {
            if distance == 0 {
                return;
            }
            let len = (*src
                .get(src_pos + 2)
                .expect("lzss_decode_slice: truncated long back-ref")
                as usize)
                + 18;
            let copy_offset = distance as usize;
            for _ in 0..len {
                assert!(
                    copy_offset <= dst_pos,
                    "lzss_decode_slice: long back-ref before output start"
                );
                let v = dst[dst_pos - copy_offset];
                let out = dst
                    .get_mut(dst_pos)
                    .expect("lzss_decode_slice: output overflow in long back-ref");
                *out = v;
                dst_pos += 1;
            }
            src_pos += 3;
        }
    }
}

/// Scan the stream to compute the exact number of source bytes consumed and
/// destination bytes produced, stopping at the terminator control word.
///
/// # Safety
///
/// `src` must point at a valid, readable LZSS stream terminated by a
/// zero-length zero-distance control word.
unsafe fn scan_stream_lengths(mut src: *const u8) -> (usize, usize) {
    unsafe {
        let mut src_len: usize = 0;
        let mut dst_len: usize = 0;

        loop {
            let b: u8 = loop {
                let b = *src;
                if b & 0x80 != 0 {
                    break b;
                }
                dst_len += 1;
                src = src.add(1);
                src_len += 1;
            };

            let distance = (((b as u32) << 8) | (*src.add(1) as u32)) & 0x7FF;
            let nibble = ((b as u32) >> 3) & 0xF;

            if nibble != 0 {
                dst_len += (nibble as usize) + 2;
                src = src.add(2);
                src_len += 2;
            } else if distance == 0 {
                src_len += 2;
                return (src_len, dst_len);
            } else {
                dst_len += (*src.add(2) as usize) + 18;
                src = src.add(3);
                src_len += 3;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{lzss_decode, lzss_decode_slice};

    /// Identity LUT — `lut[i] == i`.
    fn id_lut() -> [u8; 256] {
        let mut l = [0u8; 256];
        for (i, value) in l.iter_mut().enumerate() {
            *value = i as u8;
        }
        l
    }

    /// Safe slice wrapper exercises the same code path.
    #[test]
    fn slice_wrapper_literals() {
        let lut = id_lut();
        let src: Vec<u8> = vec![0x01, 0x02, 0x03, 0x80, 0x00];
        let mut dst = vec![0u8; 3];
        lzss_decode_slice(&mut dst, &src, &lut);
        assert_eq!(dst, vec![0x01, 0x02, 0x03]);
    }

    /// Pure-literal stream of bytes 1..16, terminated.
    #[test]
    fn literals_only() {
        let lut = id_lut();
        // 16 literals + terminator (0x80, 0x00 → nibble=0, distance=0).
        let mut src = vec![];
        for i in 1..=16u8 {
            src.push(i);
        }
        src.push(0x80);
        src.push(0x00);

        let mut dst = vec![0u8; 16];
        unsafe {
            lzss_decode(dst.as_mut_ptr(), src.as_ptr(), lut.as_ptr());
        }
        let expected: Vec<u8> = (1..=16u8).collect();
        assert_eq!(dst, expected);
    }

    /// Literals routed through a non-identity LUT.
    #[test]
    fn literals_with_lut() {
        let mut lut = id_lut();
        lut[0x05] = 0xAA;
        lut[0x10] = 0xBB;

        let src: Vec<u8> = vec![0x05, 0x10, 0x80, 0x00];
        let mut dst = vec![0u8; 2];
        unsafe {
            lzss_decode(dst.as_mut_ptr(), src.as_ptr(), lut.as_ptr());
        }
        assert_eq!(dst, vec![0xAA, 0xBB]);
    }

    /// Short back-ref: emit "ABC", then copy 3 bytes from `dst - 3`.
    ///
    /// **Encoding asymmetry**: short refs encode `real_distance - 1` (the
    /// asm's `DEC ECX` adds the +1), while long refs encode the real
    /// distance directly. So to back up 3 bytes, we encode `2` in the
    /// distance bits.
    #[test]
    fn short_backref_basic() {
        let lut = id_lut();
        // nibble = 1 (length = 3), encoded distance = 2 (real = 3).
        // src[0] = 0x80 | (1 << 3) | ((2 >> 8) & 0x7) = 0x88
        // src[1] = 2 & 0xFF = 0x02
        let src: Vec<u8> = vec![b'A', b'B', b'C', 0x88, 0x02, 0x80, 0x00];
        let mut dst = vec![0u8; 6];
        unsafe {
            lzss_decode(dst.as_mut_ptr(), src.as_ptr(), lut.as_ptr());
        }
        assert_eq!(&dst, b"ABCABC");
    }

    /// Short back-ref length boundary: nibble = 15 → copies 17 bytes.
    /// Encode `real_distance - 1 = 16` to back up 17 bytes.
    #[test]
    fn short_backref_max_length() {
        let lut = id_lut();
        // Pre-fill 17 unique bytes, then back-ref:
        //   nibble = 15 (length 17), encoded distance = 16 (real = 17).
        let mut src: Vec<u8> = (1..=17u8).collect();
        // src[0] = 0x80 | (15 << 3) | ((16 >> 8) & 0x7) = 0xF8
        // src[1] = 16 & 0xFF = 0x10
        src.push(0xF8);
        src.push(0x10);
        src.push(0x80);
        src.push(0x00);

        let mut dst = vec![0u8; 34];
        unsafe {
            lzss_decode(dst.as_mut_ptr(), src.as_ptr(), lut.as_ptr());
        }
        let expected: Vec<u8> = (1..=17u8).chain(1..=17u8).collect();
        assert_eq!(dst, expected);
    }

    /// Long back-ref: nibble = 0, distance != 0, length = src[2] + 18.
    /// Note long-form uses `-distance` (NOT `-distance - 1`).
    #[test]
    fn long_backref_basic() {
        let lut = id_lut();
        // Pre-fill 20 bytes 1..20, then long back-ref:
        //   nibble=0, distance=20 (so `-distance` reads dst[-20]=1, etc.)
        //   src[2] = 0 → length = 18.
        let mut src: Vec<u8> = (1..=20u8).collect();
        // src[0] = 0x80 | (0 << 3) | ((20 >> 8) & 0x7) = 0x80
        // src[1] = 20 & 0xFF = 0x14
        // src[2] = 0 → length 18
        src.push(0x80);
        src.push(0x14);
        src.push(0x00);
        src.push(0x80);
        src.push(0x00);

        let mut dst = vec![0u8; 38];
        unsafe {
            lzss_decode(dst.as_mut_ptr(), src.as_ptr(), lut.as_ptr());
        }
        let expected: Vec<u8> = (1..=20u8).chain(1..=18u8).collect();
        assert_eq!(dst, expected);
    }

    /// Self-overlapping short back-ref (RLE-style): emit one byte, then
    /// short-copy with distance = 1 (which becomes `-distance - 1 = -2`,
    /// a stride-2 read). This isn't classic RLE — verify against the
    /// asm semantics, not against expectation.
    ///
    /// With dst = [X, _, _, ...] and copy_offset = 2:
    ///   write 1: dst[1] = dst[1-2]  → reads dst[-1] (out of buffer!)
    ///
    /// So distance = 1 short-copies are unsafe. Test distance = 0 with the
    /// nibble path, which `-1` → reads dst[-1]. Also unsafe.
    /// Skip this test — both edge cases reach outside `dst` and only work
    /// with valid pre-loaded backing.
    #[test]
    fn self_overlap_distance_zero_short() {
        // Use distance = 0, nibble = 1 → copy_offset = 1, length = 3.
        // Each iteration: dst[i] = dst[i - 1].
        // With dst = [42, _, _, _]: yields [42, 42, 42, 42].
        let lut = id_lut();
        let src: Vec<u8> = vec![
            42, // literal
            // distance=0, nibble=1
            // src[0] = 0x80 | (1<<3) | 0 = 0x88
            // src[1] = 0x00
            0x88, 0x00, // terminator
            0x80, 0x00,
        ];
        let mut dst = vec![0u8; 4];
        unsafe {
            lzss_decode(dst.as_mut_ptr(), src.as_ptr(), lut.as_ptr());
        }
        assert_eq!(dst, vec![42, 42, 42, 42]);
    }
}
