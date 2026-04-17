# openwa-core

Cross-platform, idiomatic Rust code. The portable counterpart to `openwa-game`.

## Charter

Code in this crate **must not**:

- reference WA.exe memory (`rb()`, `va::*`, `registry::*`, the address book)
- depend on the `i686-pc-windows-msvc` target (no thiscall/usercall asm, no 32-bit pointer assumptions)
- call Windows APIs (no `windows` / `windows-sys` dependency)
- require MinHook, DirectSound, DirectDraw, or any WA-specific runtime

If a piece of code needs any of the above, it belongs in `openwa-game`.

## What lives here

Fundamental types, pure math, file-format parsers, and any other code that could, in principle, be used by a tool that never runs WA.exe (e.g. an offline replay analyzer, a scheme-file editor, or a cross-platform port).

Modules migrate here from `openwa-game` one at a time. The acceptance test is simple: `cargo check -p openwa-core --target <non-windows>` must succeed once non-trivial code lives here.

See [../openwa-game/CLAUDE.md](../openwa-game/CLAUDE.md) for the WA.exe-specific side.
