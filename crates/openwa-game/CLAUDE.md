# openwa-game

WA.exe-specific code (`i686-pc-windows-msvc` only). Types, addresses, parsers, ASLR rebasing, typed WA function wrappers, and game logic. The source of truth for all reverse-engineered type layouts, known addresses, and Rust reimplementations of WA functions.

Cross-platform fundamentals live in `openwa-core` — see `../openwa-core/CLAUDE.md` for what's there (`fixed`, `log`, `rng`, `scheme`, `sprite_lzss`, `trig`, `weapon`). When reaching for a numeric type (`Fixed`), weapon enum (`Weapon`, `FireType`, `SpecialFireSubtype`), PRNG, or LZSS, import from `openwa_core::` directly.

See root `CLAUDE.md` for project-wide rules: calling conventions, design conventions, FFI style.

## Address Registry & Pointer Identification

The `registry` module provides a structured, queryable database of known addresses. Three systems work together:

### `define_addresses!` macro

Defines known WA.exe addresses with metadata (kind, calling convention, class name). Generates both `pub const` values and `inventory`-collected `AddrEntry` items for runtime queries. Supports class blocks and standalone entries:

```rust
crate::define_addresses! {
    class "CTaskWorm" {
        vtable CTASK_WORM_VTABLE = 0x0066_44C8;
        ctor CTASK_WORM_CONSTRUCTOR = 0x0050_BFB0;
    }
    fn/Fastcall ADVANCE_GAME_RNG = 0x0053_F320;
    global G_GAME_SESSION = 0x007A_0884;
}
```

**Distributed definitions**: Each module defines its own addresses via `define_addresses!`. `address.rs` re-exports them into `mod va` for backward compatibility (`va::CTASK_WORM_VTABLE` still works). When adding new addresses, place the `define_addresses!` block in the home module, then add a `pub use` re-export in `address.rs`.

### `#[derive(FieldRegistry)]`

Auto-generates a field map for `#[repr(C)]` structs using `offset_of!()`. Fields prefixed `_unknown`/`_pad` are skipped. Applied to all key structs (DDGame, CTask, CTaskWorm, etc.). Enables runtime offset -> field name lookups.

Each field gets a `ValueKind` for typed formatting, auto-inferred from the Rust type:

- `u8/u16/u32/i8/i16/i32` -> scalar variants, `bool` -> `Bool`, `Fixed` -> `Fixed`
- `*mut T` / `*const T` -> `Pointer`, `ClassType` -> `Enum`
- Arrays and unknown types -> `Raw`
- Override with `#[field(kind = "CString")]` for null-terminated string fields, etc.

```rust
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTask { ... }
// Generates: CTask::field_registry() -> &'static StructFields
// Also registers in global inventory for struct_fields_for("CTask")

// String fields use #[field(kind = "...")] override:
#[field(kind = "CString")]
pub worm_name: [u8; 0x11],  // Displays as "Ainsley" instead of raw hex
```

### Query API (`registry::*`)

- `lookup_va(ghidra_va)` -- find address entry by VA (exact or nearest-below)
- `vtable_class_name(ghidra_vtable)` -- vtable address -> class name
- `format_va(ghidra_va)` -- human-readable name string
- `struct_fields_for("DDGame")` / `struct_fields_for_vtable(va)` -- get field map
- `field_at_inherited("CTaskWorm", offset)` -- inheritance-aware field lookup (walks CTaskWorm -> CGameTask -> CTask)
- `identify_pointer(value, delta)` -> `PointerIdentity` -- full pointer identification (static addresses, live objects, vtable-based object detection)
- `register_live_object()` / `identify_live_pointer()` -- track heap objects for field-level pointer resolution
- `vtable_info_for("PaletteVtable")` -- vtable slot metadata (name, index, doc)

### `#[vtable(...)]` attribute macro

Defines typed vtable structs from sparse slot definitions. The macro generates the full `#[repr(C)]` struct with `usize` gap-fillers, registry metadata, a companion `bind_!` macro, and optional address constants.

```rust
#[openwa_game::vtable(size = 38, va = 0x0066_A218, class = "DisplayGfx")]
pub struct DisplayVtable {
    /// set layer color
    #[slot(4)]
    pub set_layer_color: fn(this: *mut DisplayGfx, layer: i32, color: i32),
    /// set active layer, returns layer context ptr
    #[slot(5)]
    pub set_active_layer: fn(this: *mut DisplayGfx, layer: i32) -> *mut u8,
}

// Generate calling wrappers on the class struct (supports nested field paths)
bind_DisplayVtable!(DisplayGfx, base.vtable);
```

Key features:

- **`#[slot(N)]`** for sparse vtables -- gaps auto-filled with `usize`. Optional when all slots are declared sequentially.
- **`fn(...)` shorthand** -- auto-normalized to `unsafe extern "thiscall" fn(...)`.
- **Named parameters** -- `fn(this: *mut T, mode: u32)` flows through to generated wrappers as `fn set_mode(&mut self, mode: u32)`. The `this` param becomes `&mut self` (or `&self` for `*const`).
- **`bind_XxxVtable!`** -- companion macro generates method wrappers on the class struct.
- **`vtable_replace!`** -- type-safe vtable slot patching for `install()` functions. Accepts method names (resolved via `offset_of!`) or slot indices:

```rust
vtable_replace!(DSSoundVtable, va::DS_SOUND_VTABLE, {
    play_sound [originals::PLAY] => my_play_sound,  // save original + replace
    load_wav                     => my_load_wav,     // pure replace
})?;
```
