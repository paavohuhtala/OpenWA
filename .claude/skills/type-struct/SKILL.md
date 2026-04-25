---
name: type-struct
description: Identify a class/struct from raw pointer usage, create a typed Rust struct, and propagate type information to eliminate pointer arithmetic. Works with Ghidra for reverse engineering context.
---

# Type a Struct

Turn raw pointer arithmetic (`base.add(0x1234)`, `wb(0x878095 + idx * 0xD7B, ...)`) into typed Rust struct field access.

## Process

### 1. Identify the struct

Find a pattern: repeated stride arithmetic, consistent base+offset reads/writes, or a Ghidra-visible vtable.

- Grep for the stride/offset pattern across the codebase to find all access sites
- Note the allocation size (from `wa_malloc`, `wa_malloc_zeroed`, or CRT malloc calls)
- Check Ghidra for vtable assignments, constructor calls, and inheritance (xrefs to the constructor show all callers and usage contexts)
- Determine: is this a new struct, or does it belong in an existing one (e.g., fields in GameWorld, GameInfo)?

### 2. Map the field layout

For each offset accessed in the code, record:

| Offset | Size | Type | Name | How used |
| ------ | ---- | ---- | ---- | -------- |

Compute offsets relative to the struct base. Verify with:

- Allocation size = total struct size
- `RET imm16` on the constructor confirms param count
- Ghidra decompilation of the constructor shows field initialization order
- Cross-reference with thirdparty RE sources (wkJellyWorm, WormKit) if available

### 3. Create the Rust struct

- Place in the appropriate module (`engine/`, `frontend/`, `task/`, `audio/`, etc.)
- Use `#[repr(C)]` for ABI compatibility
- Add `const _: () = assert!(core::mem::size_of::<T>() == SIZE);` to verify total size
- Name known fields descriptively; use `_unknown_XXXX: [u8; N]` for gaps
- If the struct has a vtable, create a companion `FooVtable` struct with typed function pointers for known slots and `usize` for unknown slots
- Register the module in the parent `mod.rs` and re-export if appropriate

### 4. Update variable and argument types

- Replace `u32` / `*mut u8` parameters with `*mut NewStruct` in function signatures
- This is ABI-compatible on i686 (all 32-bit pointers) so no calling convention changes needed
- Update both Rust functions AND FFI `extern` function pointer types (stdcall, thiscall, cdecl)
- For naked asm bridges, parameter types can be changed without affecting the assembly

### 5. Replace pointer arithmetic with field access

- `*(base.add(0x1234) as *mut u32) = val` → `(*ptr).field_name = val`
- `wb(CONST + idx * STRIDE, val)` → `array[idx].field = val`
- For vtable calls: `*(vtable as *const u32).add(N)` → `(*(*ptr).vtable).method`
- Use `vcall!` macro for one-liner vtable dispatch where appropriate

### 6. Propagate types

- Update callers to pass typed pointers instead of `u32`
- Remove now-unnecessary `_OFF` address constants from `address.rs` (offsets are now struct fields)
- Rename address constants if the struct name changed (e.g., `MAP_CLASS_*` → `MAP_VIEW_*`)
- Update Ghidra labels to match the new names

### 7. Verify

- `cargo build --release -p openwa-dll` — must compile clean, zero warnings
- Run `/replay-test` (headful + headless) to verify runtime correctness
- Check that `sizeof` assertion passes (struct size matches allocation size)

## Naming conventions

- Struct names: `PascalCase`, matching WA class names where known (e.g., `GameWorld`, `WormEntity`, `MapView`)
- Unknown fields: `_unknown_XXXX` where XXXX is the hex offset
- Vtable structs: `FooVtable` companion to `Foo`
- Address constants: `STRUCT_NAME_FUNCTION` (e.g., `MAP_VIEW_CONSTRUCTOR`)
- Module placement: `engine/` for core game objects, `frontend/` for UI/CWnd-derived, `task/` for BaseEntity hierarchy, `audio/` for sound, `display/` for graphics

## When NOT to create a struct

- If offsets use inconsistent strides (e.g., player data with mixed 0x78/0x3C/0x1E strides) — these are interleaved arrays, not a single struct
- If the "struct" is actually a function's stack frame (local variables)
- If only 1-2 fields are known and the struct is huge — use raw offsets with named constants until more fields are discovered
