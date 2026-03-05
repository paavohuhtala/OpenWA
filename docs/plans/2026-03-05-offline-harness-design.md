# Offline Test Harness Design

**Goal:** Load WA.exe into a 32-bit test process via `cargo test`, enabling offline validation of addresses, vtable contents, and eventually calling WA.exe functions — without running the game.

**Architecture:** A new `openwa-harness` crate that uses `LoadLibraryExA` with `DONT_RESOLVE_DLL_REFERENCES` to map WA.exe into process memory. This gives us a relocated PE image with readable `.text`, `.rdata`, and `.data` sections. Tests run via `cargo test --target i686-pc-windows-msvc -p openwa-harness`.

## How PE Loading Works

```
LoadLibraryExA("WA.exe", NULL, DONT_RESOLVE_DLL_REFERENCES)
  → Windows maps PE sections into memory
  → Applies base relocations (WA.exe has .reloc section)
  → Does NOT resolve imports (IAT entries are unpatched)
  → Does NOT call entry point
  → Returns HMODULE = base address of mapped image
```

Delta computation: `loaded_base - IMAGE_BASE (0x400000)`.
Any Ghidra address `addr` maps to `loaded_base + (addr - 0x400000)`.

If `LoadLibraryEx` refuses to load an EXE, fallback: manual PE loader (read file, parse headers, VirtualAlloc, copy sections, apply relocations).

## Test Layers

### Layer 1: Read-only validation (initial scope)

Load WA.exe, read memory at known offsets:
- Vtable entries in `.rdata` match our address constants
- Function prologues at known `.text` offsets match expected bytes
- String data at known locations matches expected values
- Cross-reference consistency (vtable method pointers land in `.text`)

This replaces/supplements the runtime validator's static checks but runs via `cargo test` with no game needed.

### Layer 2: Function calls (stretch goal)

Call simple WA.exe functions from the test process:
- Requires resolving at minimum CRT imports (malloc, free, memset, memcpy)
- IAT patching: find import entries, overwrite with pointers to our own implementations
- Start with `WA_MallocMemset` (0x53E910) — uses EDI register for size, calls malloc+memset
- Validate return values and memory contents

### Layer 3: Complex function execution (future)

- Broader IAT patching for Windows API stubs
- Mock subsystems (DirectDraw, DirectSound) to prevent crashes
- Test constructors, initialization sequences

## Crate Structure

```
crates/openwa-harness/
  Cargo.toml          # 32-bit test crate, depends on openwa-types, windows-sys
  src/
    lib.rs            # WaImage loader struct, rebase helpers
    tests/            # (or use #[cfg(test)] modules)
      pe_loading.rs   # Layer 1: read-only checks
```

## Key Type: WaImage

```rust
pub struct WaImage {
    base: *const u8,     // HMODULE from LoadLibraryEx
    delta: i32,          // base - 0x400000
}

impl WaImage {
    /// Load WA.exe from given path
    pub fn load(exe_path: &str) -> Result<Self, ...>;

    /// Convert Ghidra address to pointer in loaded image
    pub fn ptr(&self, ghidra_addr: u32) -> *const u8;

    /// Read u32 at a Ghidra address
    pub fn read_u32(&self, ghidra_addr: u32) -> u32;
}

impl Drop for WaImage {
    fn drop(&mut self) { FreeLibrary(self.base); }
}
```

## WA.exe Path

The harness needs to find WA.exe. Options in priority order:
1. `WAEXE_PATH` environment variable
2. Known Steam path: `I:\games\SteamLibrary\steamapps\common\Worms Armageddon\WA.exe`
3. Skip tests with clear message if not found

## Dependencies

- `windows-sys` crate for `LoadLibraryExA`, `FreeLibrary` (links to kernel32)
- `openwa-types` for address constants and struct definitions
- No other external dependencies needed for Layer 1

## Risks

- `LoadLibraryEx` might refuse to load an EXE (unlikely but possible) → fallback to manual PE mapping
- WA.exe might not have a `.reloc` section → would need to load at exact base address 0x400000 (but we confirmed it has one: 188KB .reloc section)
- Function calls (Layer 2+) may crash if they touch unresolved imports → start read-only, add IAT patching incrementally
