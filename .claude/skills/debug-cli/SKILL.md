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
- `<addr>` (required): Address expression (see syntax below). Accepts hex (`0x56E220`) or decimal.
- `[len]` (optional): Bytes to read. Default: 256. Max: 1 MB.
- `--format hex` (default): Hex dump with ASCII sidebar + pointer annotations.
- `--format raw`: Binary output, suitable for piping to a file.
- `--port N`: Override default port 19840.

## Address Syntax

All address forms can be used with the `read` command. **Always quote the address argument** to prevent shell interpretation.

| Syntax | Meaning |
|--------|---------|
| `0x669F8C` | Ghidra VA (ASLR-rebased automatically) |
| `abs:0x7FFF0000` | Absolute runtime address (no rebase) |
| `0x669F8C+0x10` | Address + hex offset |
| `0x669F8C+16` | Address + decimal offset |
| `0x669F8C[0x10]` | Bracket notation (same as +0x10) |
| `"0x7A0884->0xA0->0x2C"` | Pointer chain (quote required!) |
| `"0x7A0884->0xA0+0x10->0x0"` | Chain with compound offset in segment |

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

These chains are useful starting points for inspecting game state:

```bash
# DDGame (the main game object):
#   g_GameSession(0x7A0884) -> DDGameWrapper at +0xA0 -> DDGame at +0x488
openwa-debug read "0x7A0884->0xA0->0x488" 0x100

# DDGame sub-field (e.g. weapon_table pointer at DDGame+0x510):
openwa-debug read "0x7A0884->0xA0->0x488->0x510" 4

# Weapon table contents (deref the pointer, read raw entries):
#   Get pointer first, then use abs: with the result
openwa-debug read "0x7A0884->0xA0->0x488->0x510" 4
openwa-debug read "abs:<ptr_from_above>" 0x80B0 --format raw > /tmp/weapon_table.bin
```

**Common mistake:** `0x7A0884->0xA0->0x0` does NOT reach DDGame. It reads DDGameWrapper+0x0 (the vtable). DDGame is at DDGameWrapper+0x488.

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

### Inspect a struct via pointer chain

```bash
# Follow g_GameSession -> DDGameWrapper -> DDGame, read 0x100 bytes
openwa-debug read "0x7A0884->0xA0->0x488" 0x100

# Read team arena state (DDGame+0x4628)
openwa-debug read "0x7A0884->0xA0->0x488->0x4628" 0x200
```

### Dump a large region for offline analysis

```bash
# Dump weapon table (71 x 0x1D0 = 0x80B0 bytes) to file
openwa-debug read "abs:<weapon_table_ptr>" 0x80B0 --format raw > /tmp/weapon_table.bin

# Analyze with Python script
python tools/analyze_weapon_table.py /tmp/weapon_table.bin
```

### Read at an absolute heap address

```bash
# If you know a runtime address from the debug UI or a log
openwa-debug read "abs:0x09AB1234" 64
```

### Inspect a vtable

```bash
# CTask vtable (8 method pointers = 32 bytes)
openwa-debug read 0x669F8C 32
```

### Validate a struct field

Read the struct base, check that the DWORD at the expected offset matches what you expect (vtable address, known constant, pointer kind).

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
- Source: `crates/openwa-debug-cli/` (CLI), `crates/openwa-debug-proto/` (protocol), `crates/openwa-wormkit/src/debug_server.rs` (server)
