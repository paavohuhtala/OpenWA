//! Portable decoder for WA's `.dir` sprite-archive file format.
//!
//! A `.dir` file bundles many named resources (sprites, images, sounds) into
//! one blob with a 1024-bucket hash table for O(1) lookup by name. Resource
//! data is stored at the front of the file; the hash table and its linked-list
//! entry nodes are appended at the end.
//!
//! This module decodes the file format only. The WA runtime's in-memory
//! [`GfxDir`](../../../openwa-game/src/asset/gfx_dir.rs) container (cache
//! slots, `FILE*`, vtable I/O) lives in `openwa-game`.
//!
//! ## File layout
//!
//! | Offset                    | Size                    | Field                 |
//! |---------------------------|-------------------------|------------------------|
//! | `0`                       | 4                       | magic `"DIR\x1A"`      |
//! | `4`                       | 4                       | `total_file_size` u32  |
//! | `8`                       | 4                       | `data_size` u32 — equals `hash_start - 4` |
//! | `12`                      | `hash_start - 12`       | resource data blobs (variable) |
//! | `hash_start = data_size+4`| `1024 * 4`              | bucket array: 1024 encoded-offset `u32`s (0 = empty) |
//! | after buckets             | to EOF                  | entry nodes, reachable via bucket linked lists |
//!
//! Each bucket stores a **relative encoded offset** `o` into the hash region.
//! Decoded position within `raw` is `hash_start + o - 4`, i.e. the encoded
//! origin sits 4 bytes before the bucket array. Encoded offset 0 means the
//! bucket is empty / the linked list terminates.
//!
//! Each entry node is:
//!
//! | Offset | Size | Field                                       |
//! |--------|------|----------------------------------------------|
//! | 0      | 4    | `next` — encoded offset to next node (0 = end) |
//! | 4      | 4    | `value` — absolute byte offset into `raw` where the resource data begins |
//! | 8      | 4    | `data_size` — size of the resource in bytes |
//! | 12     | ...  | null-terminated lowercase ASCII name         |
//!
//! Source: Ghidra decompilation of `GfxDir::LoadDir` (0x5663E0) and
//! `GfxDir::FindEntry` (0x566520); live cross-check against
//! `DATA/Custom/Art/Level.dir`.

/// DIR file magic: `"DIR\x1A"` as little-endian `u32`.
pub const DIR_MAGIC: u32 = u32::from_le_bytes(*b"DIR\x1A");

/// Header size: `magic (4) + total_file_size (4) + data_size (4)`.
pub const DIR_HEADER_SIZE: usize = 12;

/// Number of hash buckets. Fixed by the on-disk format.
pub const DIR_BUCKET_COUNT: usize = 1024;

/// Fixed minimum size of the entry-node portion of a node (sans name).
const NODE_FIXED_SIZE: usize = 12;

/// One entry in a `.dir` archive. Borrows the name directly from the source
/// buffer — no allocation.
#[derive(Debug, Clone, Copy)]
pub struct DirEntry<'a> {
    pub name: &'a str,
    /// Absolute byte offset of the resource within the `.dir` file.
    pub data_offset: u32,
    /// Size of the resource in bytes.
    pub data_size: u32,
}

impl<'a> DirEntry<'a> {
    /// Slice the resource bytes out of the source buffer the archive was
    /// parsed from. Returns `None` if the entry's range is out of bounds
    /// (shouldn't happen for a buffer that successfully passed
    /// [`dir_decode`]).
    pub fn data<'r>(&self, raw: &'r [u8]) -> Option<&'r [u8]> {
        let start = self.data_offset as usize;
        let end = start.checked_add(self.data_size as usize)?;
        raw.get(start..end)
    }
}

/// A fully parsed `.dir` archive's entry list.
#[derive(Debug, Clone)]
pub struct DirArchive<'a> {
    pub entries: Vec<DirEntry<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirDecodeError {
    /// File is smaller than the 12-byte header.
    TooShort,
    /// First four bytes are not `"DIR\x1A"`.
    BadMagic,
    /// `data_size` / `total_file_size` don't fit the provided buffer or are
    /// internally inconsistent (e.g. hash region smaller than 1024 buckets).
    BadSizes,
    /// An encoded bucket/next offset points outside the hash region.
    BadOffset,
    /// An entry node's name didn't terminate before the end of the file.
    NameNotTerminated,
    /// An entry node's name is not valid UTF-8.
    NameNotUtf8,
}

/// Parse a `.dir` archive. Returns all entries in bucket-traversal order
/// (callers that want alphabetical should sort).
pub fn dir_decode(raw: &[u8]) -> Result<DirArchive<'_>, DirDecodeError> {
    use byteorder::{LittleEndian, ReadBytesExt};
    use std::io::Cursor;

    if raw.len() < DIR_HEADER_SIZE {
        return Err(DirDecodeError::TooShort);
    }

    let mut cur = Cursor::new(raw);
    let short = |_| DirDecodeError::TooShort;

    let magic = cur.read_u32::<LittleEndian>().map_err(short)?;
    if magic != DIR_MAGIC {
        return Err(DirDecodeError::BadMagic);
    }
    let total_file_size = cur.read_u32::<LittleEndian>().map_err(short)? as usize;
    let data_size = cur.read_u32::<LittleEndian>().map_err(short)? as usize;

    // `total_file_size` is usually exact; tolerate raw being larger (e.g. a
    // file slurped with trailing padding) but not smaller.
    if total_file_size > raw.len() {
        return Err(DirDecodeError::BadSizes);
    }
    let file_end = total_file_size;

    // Hash region starts at `data_size + 4` and runs to the end of the file.
    // It must hold at least the 1024-slot bucket array.
    let hash_start = data_size.checked_add(4).ok_or(DirDecodeError::BadSizes)?;
    if hash_start > file_end {
        return Err(DirDecodeError::BadSizes);
    }
    let hash_region_len = file_end - hash_start;
    if hash_region_len < DIR_BUCKET_COUNT * 4 {
        return Err(DirDecodeError::BadSizes);
    }

    // Decode one encoded offset `o` relative to the hash region. Returns the
    // absolute offset in `raw`, or `None` if `o == 0` (empty / terminator).
    let resolve = |o: u32| -> Result<Option<u64>, DirDecodeError> {
        if o == 0 {
            return Ok(None);
        }
        // `o` is relative to a 4-byte-earlier origin (see module docs).
        let byte_off = (o as usize)
            .checked_sub(4)
            .ok_or(DirDecodeError::BadOffset)?;
        if byte_off + NODE_FIXED_SIZE > hash_region_len {
            return Err(DirDecodeError::BadOffset);
        }
        Ok(Some((hash_start + byte_off) as u64))
    };

    let mut entries = Vec::new();
    for bucket in 0..DIR_BUCKET_COUNT {
        cur.set_position((hash_start + bucket * 4) as u64);
        let encoded = cur.read_u32::<LittleEndian>().map_err(short)?;

        let mut node = resolve(encoded)?;
        while let Some(abs) = node {
            cur.set_position(abs);
            let next_raw = cur.read_u32::<LittleEndian>().map_err(short)?;
            let value = cur.read_u32::<LittleEndian>().map_err(short)?;
            let entry_size = cur.read_u32::<LittleEndian>().map_err(short)?;

            // Read null-terminated name from the current cursor position
            // (abs + NODE_FIXED_SIZE), bounded by the end of the file.
            let name_start = cur.position() as usize;
            let name_end = raw[name_start..file_end]
                .iter()
                .position(|&b| b == 0)
                .map(|p| name_start + p)
                .ok_or(DirDecodeError::NameNotTerminated)?;
            let name = core::str::from_utf8(&raw[name_start..name_end])
                .map_err(|_| DirDecodeError::NameNotUtf8)?;

            // Sanity-check the resource range against the buffer.
            let data_end = (value as usize)
                .checked_add(entry_size as usize)
                .ok_or(DirDecodeError::BadSizes)?;
            if data_end > raw.len() {
                return Err(DirDecodeError::BadSizes);
            }

            entries.push(DirEntry {
                name,
                data_offset: value,
                data_size: entry_size,
            });

            node = resolve(next_raw)?;
        }
    }

    Ok(DirArchive { entries })
}

/// 10-bit hash function used by `.dir` archives to index the bucket array.
///
/// Port of `gfx_dir_hash` (WA `FUN_566390`). Names must be lowercased ASCII
/// by the caller — the on-disk entries are already stored lowercased, and
/// the WA code lowercases lookup keys before hashing. Useful if a caller
/// wants O(1) lookup; the `dir_decode` path above is linear and doesn't
/// need it.
pub fn dir_name_hash(name: &[u8]) -> u32 {
    let mut hash: u32 = 0;
    for &b in name {
        if b == 0 {
            break;
        }
        let bit9 = (hash >> 9) & 1;
        hash = ((hash << 1) | bit9) & 0x3FF;
        hash = hash.wrapping_add(b as u32) & 0x3FF;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal synthetic `.dir` with one resource + one entry so
    /// tests don't depend on a shipped fixture.
    fn synthetic_archive() -> Vec<u8> {
        // Resource: 4 bytes of payload starting at offset 12.
        let payload: &[u8] = b"PAYL";
        let resource_end = DIR_HEADER_SIZE + payload.len(); // = 16

        // Single node, placed at start of post-bucket area.
        let name = b"foo.spr";
        let node_bytes = {
            let mut v = Vec::new();
            v.extend_from_slice(&0u32.to_le_bytes()); // next = 0
            v.extend_from_slice(&(DIR_HEADER_SIZE as u32).to_le_bytes()); // value
            v.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // size
            v.extend_from_slice(name);
            v.push(0); // null terminator
            v
        };

        // Hash region layout: 1024 bucket slots, then the node.
        let bucket_area = DIR_BUCKET_COUNT * 4;
        let hash_region_len = bucket_area + node_bytes.len();

        let hash_start = resource_end;
        let data_size = hash_start - 4; // by the format's definition
        let total_file_size = hash_start + hash_region_len;

        let mut out = Vec::with_capacity(total_file_size);
        out.extend_from_slice(&DIR_MAGIC.to_le_bytes());
        out.extend_from_slice(&(total_file_size as u32).to_le_bytes());
        out.extend_from_slice(&(data_size as u32).to_le_bytes());
        out.extend_from_slice(payload);

        // Compute hash for "foo.spr" and place its node-offset in that bucket.
        let bucket = dir_name_hash(name) as usize;
        // Encoded offset of the node: node is at byte 0 of the post-bucket
        // area, i.e. byte `bucket_area` within the hash region. Encoded = +4.
        let encoded_offset = (bucket_area + 4) as u32;

        let mut hash_region = vec![0u8; hash_region_len];
        hash_region[bucket * 4..bucket * 4 + 4].copy_from_slice(&encoded_offset.to_le_bytes());
        hash_region[bucket_area..bucket_area + node_bytes.len()].copy_from_slice(&node_bytes);

        out.extend_from_slice(&hash_region);
        assert_eq!(out.len(), total_file_size);
        out
    }

    #[test]
    fn decodes_synthetic_archive() {
        let raw = synthetic_archive();
        let archive = dir_decode(&raw).expect("synthetic archive must decode");
        assert_eq!(archive.entries.len(), 1);
        let e = &archive.entries[0];
        assert_eq!(e.name, "foo.spr");
        assert_eq!(e.data_offset, DIR_HEADER_SIZE as u32);
        assert_eq!(e.data_size, 4);
        assert_eq!(e.data(&raw), Some(&b"PAYL"[..]));
    }

    #[test]
    fn rejects_short_input() {
        assert!(matches!(dir_decode(&[]), Err(DirDecodeError::TooShort)));
        assert!(matches!(dir_decode(&[0; 8]), Err(DirDecodeError::TooShort)));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut raw = synthetic_archive();
        raw[0] = b'X';
        assert!(matches!(dir_decode(&raw), Err(DirDecodeError::BadMagic)));
    }

    #[test]
    fn hash_matches_wa() {
        // "back.spr" in DATA/Custom/Art/Level.dir lives at bucket 244.
        assert_eq!(dir_name_hash(b"back.spr"), 244);
    }
}
