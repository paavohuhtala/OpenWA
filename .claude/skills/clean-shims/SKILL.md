---
name: clean-shims
description: Type-propagation and comment cleanup pass for openwa-dll hook shim files. Replaces u32 params with typed pointers/Fixed/newtypes, removes redundant comments, and applies field shorthand. Use on files in crates/openwa-dll/src/replacements/.
user-invocable: true
---

# Clean Shims

Apply type propagation, naming cleanup, and comment trimming to hook shim files in `crates/openwa-dll/src/replacements/`.

Argument: a file path or module name (e.g., `render/render_queue.rs`, `weapon.rs`). If omitted, ask the user which file to clean.

## Type propagation rules

For each `impl_fn` function, match parameter types against the types used at their destination — struct fields, function call arguments, or local variable bindings:

1. **`this`/`self` as `u32`** → typed pointer (`*mut T` / `*const T`). Use the actual struct type the body casts to. Remove the `let q = &mut *(this as *mut T)` rebinding pattern — use `(*ptr).method()` directly.

2. **Coordinates/values as `u32` that are actually `Fixed`** → change to `Fixed`. Signs that a value is fixed-point:
   - Body wraps it: `Fixed(x as i32)`, `Fixed(x as i32).floor()`
   - Arithmetic with `0x10000` (1.0 in 16.16)
   - Passed to a function that takes `Fixed`
   
   After retyping, simplify: `Fixed(x as i32).floor()` → `x.floor()`, `Fixed(x as i32)` → `x`.

3. **Pointers as `u32`** → typed `*mut T` / `*const T`. Check what the body casts them to. If the pointee struct exists in openwa-game (for WA memory-layout structs) or openwa-core (rare — portable types), use it. If not, consider creating a stub `#[repr(C)]` struct in openwa-game.

4. **Newtype wrapping** → if the body wraps a `u32` in a newtype (`SpriteOp(x)`, `SoundId(x)`, `Weapon(x)`, etc.), change the param to that type and use it directly.

5. **Generic param names** (`param_1`, `param_N`) → match to the name at the destination. If assigned to struct field `color`, name it `color`. If passed to a function parameter `src_w`, name it `src_w`.

6. **Field shorthand** → when a parameter name matches a struct field name exactly, use shorthand: `field,` not `field: field,`.

7. **Propagate to callees** — if the shim calls a core function that also takes `u32` for a now-typed parameter, update that function's signature too. Remove entry-point casts (e.g., `let entity = &*(entity_ptr as *const T)` → `let entity = &*entity`).

8. **Propagate to callers** — if the shim calls WA bridge functions or core helpers with casts (`x as u32`, `x as *mut _`), check whether those callees can also be retyped.

## Comment cleanup rules

Each hook group should have at most a single-line comment:

```
// GhidraFunctionName (0xADDRESS)
```

Remove:
- Calling convention details (already encoded in `usercall_trampoline!` / `extern` decl)
- Algorithm descriptions (belong in the core implementation, not the shim)
- Dispatch path descriptions ("dispatched by X case Y into vtable slot N")
- RE history notes ("formerly mis-labelled", "earlier passes")
- Section separator bars (`// ----`)

Keep (in rare cases where useful):
- Brief clarification when the function name is ambiguous or the shim does something non-obvious

If a comment says something was misnamed, verify whether the old name still exists anywhere in the codebase (Ghidra labels, docs, address constants). Fix the stale name first, then remove the comment.

## Naming consistency

- Pointer-to-self parameters: use the struct's conventional short name (`queue`, `entity`, `display`, etc.)
- Coordinate pairs: `x`/`y`, or `x_pos`/`y_pos` if there's ambiguity with other x/y fields
- Clip reference: `y_clip`

## Verification

After all changes:

1. `cargo check --release` — must pass with no errors
2. Review that no `u32` parameter is cast to a pointer or `Fixed` in the function body — those should be typed at the signature level
3. Review that no `let x = &mut *(param as *mut T)` patterns remain for parameters that could be typed directly
