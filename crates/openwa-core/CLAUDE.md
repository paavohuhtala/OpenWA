# openwa-core

Cross-platform, idiomatic Rust code. The portable counterpart to `openwa-game`.

## Charter

Code in this crate **must not**:

- reference WA.exe memory (`rb()`, `va::*`, `registry::*`, the address book)
- depend on the `i686-pc-windows-msvc` target (no thiscall/usercall asm, no 32-bit pointer assumptions)
- call Windows APIs (no `windows` / `windows-sys` dependency)
- require MinHook, DirectSound, DirectDraw, or any WA-specific runtime

If a piece of code needs any of the above, it belongs in `openwa-game`.

## Currently hosted

- **`dir`** — `.dir` sprite-archive file-format parser. `dir_decode(&[u8]) -> DirArchive<'_>` returns a flat entry list (name, offset, size). Also exports `dir_name_hash` — WA's 10-bit bucket hash. The in-memory `GfxDir` runtime container (cache slots, `FILE*`, vtable I/O) stays in `openwa-game::asset::gfx_dir`.
- **`fixed`** — `Fixed(i32)` 16.16 newtype with arithmetic impls; the fundamental numeric type for coordinates and velocities across the project. Also `Fixed64(i64)` — same 16 fractional bits, 48 integer bits, for accumulators that would overflow `Fixed` (replay-clock counters on `GameWorld`).
- **`img`** — `.img` image decoder. Two variants: tagged (`img_decode`, `IMG\x1A` magic + flags word, 1bpp/8bpp, LZSS or raw, optional palette) and headerless (`img_decode_headerless`, fixed 8bpp+palette layout). Returns `DecodedImg` with stride-aligned owned pixels; caller supplies an `FnMut(u32) -> u8` palette-mapping callback.
- **`log`** — file-logging helper (`log_line`). Writes to `OpenWA.log` or the path in `OPENWA_LOG_PATH`.
- **`pal`** — Microsoft RIFF PAL decoder for standalone `.pal` files. `pal_decode(&[u8]) -> DecodedPal` returns `Vec<PalEntry { r, g, b, flags }>`. Ignores trailing `offl`/`tran`/`unde` sub-chunks. WA palettes are typically sparse — a single file populates only one sub-range of a shared 256-entry palette.
- **`rng`** — WA's LCG PRNG (`wa_lcg(state) = state * 0x19660D + 0x3C6EF35F`).
- **`scheme`** — `.wsc` scheme file parser. Reads Worms Armageddon game settings files; has an integration test suite against real WA fixtures.
- **`sprite_lzss`** — `sprite_lzss_decode`, a port of WA.exe's LZSS decompressor (0x5B29E0). Pure byte-level code operating on raw pointers.
- **`trig`** — fixed-point sin/cos tables and interpolated lookup. The 1025-entry tables are byte-for-byte copies of WA.exe's `.rdata` at `G_SIN_TABLE` / `G_COS_TABLE`, embedded via `include_bytes!` + a const-fn decoder. `sin(angle)` / `cos(angle)` are the common-case helpers; `trig_lookup_table(&table, angle)` is the primitive for callers with a non-embedded table. `openwa-game::trig::validate_against_wa_exe` runs on DLL load and asserts the embedded tables still match the live binary byte-for-byte.
- **`weapon`** — Weapon ID space (0..70) plus `FireType` / `FireMethod` / `SpecialFireSubtype` dispatch enums, all with `TryFrom<u32/i32>` impls. The layout structs these enums describe (`WeaponEntry`, `WeaponFireParams`, `WeaponTable`, `WeaponSpawnData`) stay in openwa-game because their `repr(C)` layouts are 32-bit-pointer-dependent.

## What stays in openwa-game

Anything that touches WA.exe memory, pointers whose size is 32-bit-dependent (`WeaponEntry` has `*const c_char` fields), hardcoded Ghidra addresses, Windows APIs, MinHook/DirectX, or depends on `rebase`/`registry`/`mem`. That's most of the codebase — see `../openwa-game/CLAUDE.md`.

## Acceptance test for new modules

`cargo check -p openwa-core --target <non-windows>` must succeed. If the code relies on `windows-sys`, pointer-size assumptions, or anything from `openwa-game`, it belongs on the other side of the split.
