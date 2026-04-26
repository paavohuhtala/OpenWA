---
name: debug-cli
description: Read memory from a live WA.exe process via the debug CLI. Inspect structs, vtables, pointer chains, and raw data at Ghidra or absolute addresses. Requires the game to be running in debug mode (start-debug.ps1).
---

# Debug CLI

Read memory from a live WA.exe process. Useful for validating struct layouts, investigating unknown fields, following pointer chains, and comparing runtime state against Ghidra analysis.

## Prerequisites

The game must be running in debug mode. If the CLI fails to connect, ask the user to:

1. Run `powershell -ExecutionPolicy Bypass -File start-debug.ps1` (builds everything and launches WA.exe with debug server + debug UI enabled)
2. Set up a game scenario as needed (e.g. "start a match with 2 teams of 4 worms each, CPU vs CPU")

The debug server listens on `127.0.0.1:19840` (TCP). It runs inside the injected DLL as a background thread.

**Important:** The server runs inside the DLL. If you change protocol types (Request/Response enums in `openwa-debug-proto`), the game must be restarted to load the new DLL. CLI-only changes (parsing, display) don't require a restart.

## Commands

### ping

Check if the debug server is running.

```bash
target/i686-pc-windows-msvc/release/openwa-debug ping
```

Returns `pong` if connected. Use this to verify the server is up before issuing read commands.

### help

List available server-side commands with usage info.

```bash
target/i686-pc-windows-msvc/release/openwa-debug help
```

### read

Read memory at an address. ASLR rebasing is handled automatically for Ghidra addresses.

```bash
target/i686-pc-windows-msvc/release/openwa-debug read <addr> [len] [--format hex|raw]
```

**Arguments:**

- `<addr>` (required): Address expression (see syntax below). Accepts hex (`0x56E220`), decimal, or symbolic names.
- `[len]` (optional): Bytes to read. Default: 256. Max: 1 MB.
- `--format hex` (default): Hex dump with ASCII sidebar + pointer annotations.
- `--format raw`: Binary output, suitable for piping to a file.
- `--port N`: Override default port 19840.

### inspect

Typed struct inspection — reads all named fields from a FieldRegistry and formats them by ValueKind.

```bash
openwa-debug inspect <class_name> <addr>
```

**Examples:**

```bash
openwa-debug inspect GameWorld world                        # All 84 GameWorld fields
openwa-debug inspect WormEntity "abs:0x1DB439F0"           # Worm at known address
openwa-debug inspect WorldEntity "world->task_land"        # Follow pointer, show base class
openwa-debug inspect GameRuntime "gamesession->runtime"  # Multi-step chain
```

**Output format:**

```
GameWorld at ghidra:0x00XXXXXX (runtime:0xYYYYYYYY)
  +0x0000  keyboard             [ 4]  0x040ACA40 (Keyboard*)
  +0x0084  pos_x                [ 4]  388.4320 (0x01846E96)
  +0x02F0  worm_name            [17]  "Ainsley"
  +0x054C  task_land            [ 4]  0x1DB27938 (LandEntity*)
```

Fields are formatted by their `ValueKind`: pointers resolved to class names, Fixed as float + raw hex, CString as quoted strings, scalars as decimal.

### objects

List all tracked live objects (registered via `register_live_object` in the DLL).

```bash
openwa-debug objects
```

**Output:**

```
Tracked objects (3):
  GameSession          runtime:0x155FDD38  size:0x120  fields:27
  GameRuntime        runtime:0x15580048  size:0x6F00  fields:15
  GameWorld               runtime:0x1C3F8E40  size:0x98D8  fields:84
```

Object names work as symbolic addresses: `world`, `gamesession`, `runtime` (case-insensitive).

### suspend / resume / step / frame / break

Frame-level debugging commands. Pause the game at exact frame boundaries for memory inspection.

```bash
openwa-debug suspend           # Pause at next frame
openwa-debug resume            # Unpause
openwa-debug step              # Advance 1 frame, then pause
openwa-debug step 10           # Advance 10 frames, then pause
openwa-debug frame             # Show current frame + pause state
openwa-debug break 1350        # Set breakpoint at frame 1350
openwa-debug break clear       # Clear breakpoint
```

**Env var:** `OPENWA_BREAK_FRAME=1350` sets a breakpoint at DLL init time. Useful for headless replays — the game auto-pauses at the target frame so you can inspect state via `read`.

**How it works:** The game thread cooperatively blocks on a Windows event at each frame boundary (in the TurnManager_ProcessFrame hook). The debug server controls the event from its TCP thread. No thread suspension APIs — just cooperative blocking.

## Address Syntax

All address forms can be used with `read`, `inspect`, and any command that takes an address. **Always quote chain addresses** to prevent shell interpretation.

| Syntax                                   | Meaning                                             |
| ---------------------------------------- | --------------------------------------------------- |
| `0x669F8C`                               | Ghidra VA (ASLR-rebased automatically)              |
| `abs:0x7FFF0000`                         | Absolute runtime address (no rebase)                |
| `0x669F8C+0x10`                          | Address + hex offset                                |
| `0x669F8C+16`                            | Address + decimal offset                            |
| `0x669F8C[0x10]`                         | Bracket notation (same as +0x10)                    |
| `"0x7A0884->0xA0->0x2C"`                 | Pointer chain (quote required!)                     |
| `"0x7A0884->0xA0+0x10->0x0"`             | Chain with compound offset in segment               |
| `world`                                 | Named alias (resolved via server, case-insensitive) |
| `world+frame_counter`                   | Named alias + field offset (no deref)               |
| `"world->task_land"`                    | Field-name chain: offset to field, then deref       |
| `"gamesession->runtime->display"` | Multi-step field chain                              |

### Symbolic Names

Named live objects (`world`, `gamesession`, `runtime`) resolve to runtime addresses via the server. Field names in `+offset` or `->chain` segments resolve via FieldRegistry lookups, including BaseEntity inheritance chains.

Field-name chain semantics differ from hex chains: `world->task_land` means "add task_land's offset (0x54C) to GameWorld base, then deref" (offset-then-deref). Hex chains like `0x7A0884->0xA0` mean "deref 0x7A0884, then add 0xA0" (deref-then-offset). This matches the natural user intent in each case.

### Pointer Chains

**You MUST quote chain addresses** — `>` is a shell redirect character:

```bash
openwa-debug read "0x7A0884->0xA0->0x488" 64   # correct
openwa-debug read 0x7A0884->0xA0->0x488 64      # WRONG — shell eats >
```

The CLI detects truncated addresses (ending with `-`) and warns about missing quotes.

Chain syntax `addr->offset1->offset2->...` walks a pointer chain server-side:

1. Read DWORD at `addr` to get pointer P1
2. Compute P1 + offset1, read DWORD to get pointer P2
3. Compute P2 + offset2 — this is the final address
4. Display memory at the final address

Each `->N` means "dereference current address, then add N." The last step produces the final address (no deref).

**Compound offsets work in every segment** — both the start address and chain segments support `+offset` and `[offset]`:

```bash
openwa-debug read "0x7A0884->0xA0->0x488+0x10"   # deref, add 0x498
openwa-debug read "0x7A0884->0xA0->0x488->0x10"   # deref, add 0x488, deref again, add 0x10
```

Note: `->0x488+0x10` is ONE step (deref then add 0x498), while `->0x488->0x10` is TWO steps (deref+0x488, deref+0x10). Choose based on whether you need an extra dereference.

The output shows each step of the chain walk before the hex dump, so you can verify intermediate pointers.

## Key Address Chains

With symbolic addresses, most chains are now human-readable:

```bash
# Typed GameWorld inspection (all 84 named fields):
openwa-debug inspect GameWorld world

# Typed worm inspection at a known address:
openwa-debug inspect WormEntity "abs:0x1DB439F0"

# Follow a pointer field and inspect the target:
openwa-debug inspect WorldEntity "world->task_land"

# Read raw memory using symbolic names:
openwa-debug read world 0x100
openwa-debug read "world+frame_counter" 4

# Old hex chains still work:
openwa-debug read "0x7A0884->0xA0->0x488" 0x100
```

**Common mistake:** `0x7A0884->0xA0->0x0` does NOT reach GameWorld. It reads GameRuntime+0x0 (the vtable). GameWorld is at GameRuntime+0x488. With symbolic names, just use `world`.

## Output

**Hex format (default):**

1. Header line showing Ghidra address and runtime (ASLR-rebased) address
2. Hex dump with 16 bytes per line and ASCII sidebar
3. Pointer annotations: for each DWORD that looks like a pointer, shows offset, raw value, Ghidra value, classification, and detail

**Chain output** additionally shows a trace of each deref step before the hex dump.

**Pointer classifications:**

- `VTABLE` — points to .rdata section (likely a vtable). Detail shows `vt[0]` (first virtual method address)
- `CODE` — points to .text section (function pointer or return address)
- `DATA` — points to .data/.bss section (global variable)
- `OBJECT` — heap pointer whose first DWORD is a vtable. Detail shows vtable + vt[0] addresses
- `HEAP` — readable heap pointer without a vtable. Detail shows dereferenced value

## Common Patterns

### Typed struct inspection (preferred)

```bash
# Full GameWorld with all named fields, typed formatting
openwa-debug inspect GameWorld world

# Worm at a known address — shows name, team, position, inputs
openwa-debug inspect WormEntity "abs:0x1DFB4B10"

# Follow pointer and inspect base class for position/speed
openwa-debug inspect WorldEntity "world->task_land"

# GameRuntime from GameSession
openwa-debug inspect GameRuntime "gamesession->runtime"
```

### Raw memory reads

```bash
# Read specific field by name
openwa-debug read "world+frame_counter" 4

# Read team arena state
openwa-debug read "world+team_arena" 0x200

# Old-style hex chain still works
openwa-debug read "0x7A0884->0xA0->0x488" 0x100
```

### Dump a large region for offline analysis

```bash
# Dump weapon table to file
openwa-debug read "abs:<weapon_table_ptr>" 0x80B0 --format raw > /tmp/weapon_table.bin
```

### Read at an absolute heap address

```bash
# If you know a runtime address from the debug UI or a log
openwa-debug read "abs:0x09AB1234" 64
```

### Inspect a vtable

```bash
# BaseEntity vtable (8 method pointers = 32 bytes)
openwa-debug read 0x669F8C 32
```

### Validate a struct field

Use `inspect` to see all named fields with formatted values, or `read` with a field name for raw bytes.

## Limitations

- Cannot write memory, only read.
- Single-client: one connection at a time.
- 10-second read timeout per connection.
- Chain walks stop on NULL pointers with an error message.
- Some game data (e.g. WeaponTable) is only initialized after a match starts. If you get NULL pointers, ensure you're in-game, not at the main menu.

## Notes

- The debug server is enabled by `OPENWA_DEBUG_SERVER=1` env var (set automatically by `start-debug.ps1`)
- The debug UI (egui overlay with entity census, struct inspector, cheats) is enabled by `OPENWA_DEBUG_UI=1` + the `debug-ui` cargo feature
- Both are independent — you can use the CLI without the UI and vice versa
- Protocol: MessagePack over TCP with 4-byte LE length-prefix framing
- The `openwa-debug` binary is built to `target/i686-pc-windows-msvc/release/openwa-debug.exe`
- Source: `crates/openwa-debug-cli/` (CLI), `crates/openwa-debug-proto/` (protocol), `crates/openwa-dll/src/debug_server.rs` (server)
