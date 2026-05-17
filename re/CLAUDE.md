# `re/` — reverse-engineering metadata catalog

The TOML files under `re/` are the **canonical source of truth** for every reverse-engineered fact about WA.exe: function names, prototypes, calling conventions, custom storage, plate/inline comments, named globals, labels, and struct/union/enum/typedef layouts. They are round-tripped with Ghidra by the `openwa-re` CLI (crate `openwa-re-data`).

**Modify the database by editing TOML files here, then pushing to Ghidra. Never use Ghidra MCP write tools.** Ghidra MCP is for read-only inspection (decompile, xrefs, disassembly, struct layout queries). See the root [CLAUDE.md](../CLAUDE.md#re-database-retoml-and-ghidra-mcp) for the MCP allowed/forbidden split.

## File layout

- One TOML per class / cohesive unit (e.g. [BaseEntity.toml](BaseEntity.toml), [GameRuntime.toml](GameRuntime.toml), [AirStrikeEntity.toml](AirStrikeEntity.toml)). Adding a new file is free — placement is purely organisational, everything merges into one in-memory catalog keyed by VA.
- Subfolders are allowed; the loader walks `re/**/*.toml` recursively. Group related shards under a subdirectory when the top level gets noisy.
- [types.toml](types.toml) — `struct` / `union` / `enum` / `typedef` definitions, plus the `external_types` list of Win32/MFC/CRT names that the validator should treat as known.
- [ASM.toml](ASM.toml) — hand-rolled assembly stubs and other catch-all entries.

### Naming convention: `ClassName__member`

Function and method names use a **double-underscore** separator between the class/module prefix and the member name: `BaseEntity__HandleMessage`, `GameRuntime__StepFrame`, `WormEntity__vt3_Render`. The `__` is the Ghidra-friendly stand-in for `::` (which Ghidra treats as a namespace separator and would split into a real `BaseEntity` namespace). Vtable slots get a `vtN_` prefix on the member name (`vt2_HandleMessage`, `vt6_ProcessFrame`); the global storing the vtable itself is `ClassName__vtable`. Free functions with no owning class take a module prefix in the same shape (e.g. `ASM__memcpy_inline`).

## Schema cheatsheet

Top-level keys per file (all optional, any mix allowed): `[[function]]`, `[[global]]`, `[[label]]`, `[[struct]]`, `[[union]]`, `[[enum]]`, `[[typedef]]`, `[[function_def]]`, `external_types = [...]`. The Rust definitions live in [crates/openwa-re-data/src/model.rs](../crates/openwa-re-data/src/model.rs) with `deny_unknown_fields` — typos fail validation.

### Function

```toml
[[function]]
va = 0x004FE070                              # absolute Ghidra VA (image base 0x400000)
name = "WorldEntity__TryMovePosition"
calling_convention = "__thiscall"            # __stdcall | __cdecl | __thiscall | __fastcall
custom_storage = true                        # set when any param needs an explicit storage =
no_return = false                            # omit when false
plate_comment = "thiscall: ECX = this. 3 stack params, RET 0xC."

  [function.signature]
  returns = "void"

  [[function.param]]
  name = "this"
  type = "WorldEntity *"
  storage = "ECX"                            # required when custom_storage = true

  [[function.param]]
  name = "dx"
  type = "Fixed"
  storage = "stack:0x4"
```

- `calling_convention` is **required whenever you list params** — without it the importer falls back to `__cdecl` and silently scrambles storage.
- `__usercall` is IDA terminology, not a Ghidra convention. Model it as the closest base convention (typically `__stdcall`) + `custom_storage = true` + explicit per-param `storage`.
- `storage` syntax: `"ECX"`, `"EAX"`, `"EDX:EAX"` (multi-register split, low:high), `"stack:0x4"` (size derived from type), `"stack:0x8:4"` (explicit byte size).
- Stack params are listed by their stack offset; the `this` register param can appear in any order — convention is to list it last when using ECX.
- `RET imm16 / 4` = number of stack params the function cleans. Always cross-check against the disassembly.

Optional per-function: `[[function.local]]` (named stack locals — `name`, `type`, `stack_offset`) and `[[function.comment]]` (inline comments — `va`, `kind = "plate" | "eol" | "pre" | "post" | "repeatable" | "decompiler"`, `text`).

### Global / Label

```toml
[[global]]
va = 0x007A0884
name = "G_GAME_SESSION"
type = "GameSession *"                       # optional — name alone is still useful
comment = "Active session pointer, NULL between matches."

[[label]]
va = 0x004ED13E
name = "FrontendIdleProc_HookSite"
```

A label and a global cannot share a VA — same-VA `label ↔ global` conversions are handled as paired remove/add by the importer.

### Struct / Union / Enum / Typedef

```toml
[[struct]]
name = "GameInfoTeamRecord"
namespace = "/OpenWA"                        # omit for root namespace "/"
size = 0xBB8
plate_comment = "Per-team match record. Stride 0xBB8."

  [[struct.field]]
  offset = 0x0
  name = "owner_player_slot"                 # omit name for true padding
  type = "char"
  size = 0x1                                 # optional; Ghidra recomputes from type
  comment = "0 = local; nonzero disables input dispatch."

[[enum]]
name = "Weapon"
size = 4
variant = { Bazooka = 0, HomingMissile = 1, Mortar = 2 }

[[typedef]]
name = "WormIdx"
target = "byte"
```

- `type` strings are passed verbatim to Ghidra: `int`, `BaseEntity *`, `char *[7]`, `byte[160]`, `undefined4`, `_struct_19`, etc. Pointers and arrays are textual derivations of a base type.
- Validation walks every `type` against the catalog (`struct` / `union` / `enum` / `typedef` names + `external_types`); unknown names are warnings, not errors.
- Field `name` can be omitted for true padding bytes (Ghidra's XML omits the attribute on those); don't synthesise `field_NN` placeholders.

### Hex formatting

VAs and offsets: single unbroken hex literal (`0x004FE070`, `0xBB8`). No `_` separators inside an address. See root CLAUDE.md "Hex address formatting".

## The `openwa-re` CLI

Run from anywhere inside the repo — it locates the workspace root and `re/` automatically. Source at [crates/openwa-re-data](../crates/openwa-re-data/).

```text
cargo run -p openwa-re-data --bin openwa-re -- <subcommand>
```

| Subcommand                     | Purpose                                                                                                                                                                                                                                              |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `validate`                     | Parse all `re/**/*.toml`; report schema errors + unknown-type warnings. Run before every commit that touches `re/`.                                                                                                                                  |
| `export [<dir>]`               | Read `re/`, emit `<dir>/wa_import.json` for `OpenWAImport.java`. Validation-gated — refuses to emit on errors. Overlays `<dir>/wa_export_extras.json` if present. Dir defaults to the scratch dir from `.openwa/setup.toml`.                         |
| `import [<dir>]`               | Read `<dir>/wa_export.xml` (+ `wa_export_extras.json`) and apply incremental diffs onto `re/`. Validation-gated; staleness-guarded (refuses if any `re/*.toml` is newer than the last `wa_import.json`, since Ghidra wouldn't have those edits yet). |
| `import --bootstrap [--force]` | One-shot full reseed: shards Ghidra XML into fresh `re/*.toml`. Refuses to overwrite a non-empty `re/` without `--force`. Used only when starting from a clean slate.                                                                                |
| `diff [<dir>]`                 | Print the same diff `import` would apply, without writing.                                                                                                                                                                                           |
| `setup`                        | Interactive wizard — writes `.openwa/setup.toml` (game dir, Ghidra install, project, scratch dir) and installs the Ghidra-side scripts into `~/ghidra_scripts/`.                                                                                     |
| `install-scripts`              | Reinstall `OpenWAExport.java` / `OpenWAImport.java` into `~/ghidra_scripts/` after a repo update.                                                                                                                                                    |

The scratch dir (default `<repo>/.openwa/scratch`, gitignored) holds three fixed filenames shared with Ghidra:

- `wa_export.xml` — written by `OpenWAExport.java`, read by `openwa-re import`.
- `wa_export_extras.json` — sidecar written by `OpenWAExport.java` carrying `calling_convention` / `no_return` / `custom_storage` (Ghidra's XML DTD cannot represent these). Load-bearing — without it, custom-storage functions lose their per-param register/stack assignments. Auto-paired on import and export.
- `wa_import.json` — written by `openwa-re export`, read by `OpenWAImport.java`.

## Round-trip workflow

**Default direction — TOML → Ghidra:**

1. Edit `re/*.toml`.
2. `cargo run -p openwa-re-data --bin openwa-re -- validate`. Fix any errors.
3. `cargo run -p openwa-re-data --bin openwa-re -- export`.
4. In Ghidra: `Tools → OpenWA → Import catalog`, or run the script over MCP — `mcp__ghidra-mcp__run_ghidra_script` with `script_name = "OpenWAImport.java"`. This is the one MCP write path that's allowed: the script applies a TOML-derived manifest, so Ghidra is just catching up to the canonical state.
5. Subsequent MCP reads now reflect the new names/types.

You can drive the full validate → export → Ghidra-import loop from the agent without user help — run `validate` and `export` via Bash, then trigger `OpenWAImport.java` via MCP.

**Reverse direction — Ghidra → TOML** (only when somebody made changes in Ghidra itself, e.g. through the headful UI):

1. In Ghidra: `Tools → OpenWA → Export catalog`, or `mcp__ghidra-mcp__run_ghidra_script` with `script_name = "OpenWAExport.java"`. Produces `wa_export.xml` + `wa_export_extras.json` in the scratch dir.
2. `cargo run -p openwa-re-data --bin openwa-re -- import`. Diffs against `re/` and applies incrementally to the relevant TOML shards.
3. Commit the resulting TOML changes.

The Ghidra-side scripts live at [ghidra_scripts/OpenWAExport.java](../ghidra_scripts/OpenWAExport.java) / [OpenWAImport.java](../ghidra_scripts/OpenWAImport.java) and are copied into `~/ghidra_scripts/` by `openwa-re setup` / `install-scripts`.

## Common edits

- **Rename a function / global / label**: change the `name` field. Don't touch `va`.
- **Add a missing prototype**: add `calling_convention`, `[function.signature]`, and `[[function.param]]` blocks. Set `custom_storage = true` if any param needs an explicit register or non-default stack slot, and add a `storage = "..."` to every param.
- **Split an `_unknown_XX` padding field**: edit the `[[struct.field]]` block — add a name, narrow the `type` / `size`, add new field entries for the remaining bytes. Keep `offset` monotonic; the validator catches overlap and out-of-bounds.
- **Drop a `_Maybe` suffix**: when you're unsure of a function/global/struct's purpose, name it with a `_Maybe` suffix (a guess beats a `FUN_xxxxxxxx` placeholder). Remove the suffix by editing the `name` once you've confirmed the purpose.
- **Document a tricky function**: add or expand `plate_comment` (full-block comment above the listing) or per-instruction `[[function.comment]]` entries. Do not document the calling convention / custom storage as text; just fix the metadata fields.

After any edit, `openwa-re validate` is cheap (~1s on the full catalog) — run it before you commit.

## Out of scope for `openwa-re import` (edit directly in TOML)

The incremental `import` path covers function field updates, label create/rename/delete, and global create/rename/retype/delete. Struct / union / enum / typedef / function_def changes are **not** absorbed from Ghidra automatically — make those edits in the TOML directly. New function creations and function deletions are reported but not applied; verify and add/remove the entry by hand.
