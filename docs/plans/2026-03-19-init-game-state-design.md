# DDGame__InitGameState Porting Plan

## Overview

Port `DDGame__InitGameState` (0x526500, 1226 decompiled lines) using the same
incremental strategy as the DDGame constructor: port sub-functions individually
as hooks, test each group, then eventually combine into a full Rust replacement.

InitGameState is called once after the DDGame constructor. It creates all runtime
game objects: HUD panel, weapon table, team arena, turn game task, camera state,
scoring, alliance data, and more.

## Structure

InitGameState is actually two merged functions:
- Wrapper (0x526500-0x52657B): SEH setup, copies 2 scheme bytes, calls FUN_541620
- Falls through into CTaskGameState__vmethod_7 (0x52657C, ~2000 instructions)

`param_1` = DDGameWrapper. `param_1+0x488` = DDGame. `DDGame+0x24` = GameInfo/scheme.

## Phase 1: Port usercall functions to pure Rust

All 7 usercall functions are small (<80 lines), mostly pure field writes, and
have zero or minimal dependencies. Porting them eliminates naked asm bridges.

### Group A — Zero-dependency functions (port first, test)

| Function | Address | Lines | What it does |
|---|---|---|---|
| FUN_541620 | 0x541620 | ~15 | Sprite/gfx table init (fastcall ECX,EDX, loop + 3 fields) |
| FUN_541060 | 0x541060 | ~15 | Buffer alloc constructor (EAX=size, ESI=output, calls wa_malloc) |

### Group B — Team/scoring init (port, test)

| Function | Address | Lines | What it does |
|---|---|---|---|
| InitTeamScoring | 0x528510 | ~65 | Pure field writes to CGameTask scoring arrays, zero calls |
| InitAllianceData | 0x5262D0 | ~80 | Alliance bitmask computation, pure field writes + bitmath |

### Group C — Turn state + weapon availability (port, test)

| Function | Address | Lines | What it does |
|---|---|---|---|
| InitTurnState | 0x528690 | ~80 | DDGame turn fields + camera center. Calls FUN_524700 (bridge) |
| FUN_53FFC0 | 0x53FFC0 | ~50 | Weapon availability switch (port FUN_565960 ~20 lines too) |
| FUN_528480 | 0x528480 | ~20 | Post-init: scheme flag + 1 vtable dispatch |

### Bridge (defer porting)

| Function | Address | Lines | Why |
|---|---|---|---|
| FUN_524700 | 0x524700 | ~600 | Scheme-version feature flags. Pure computation but massive. |

## Phase 2: Hook stdcall constructors individually

All verified via RET instruction. Hook each, test in isolation.

### Group D — Core game objects

| Function | Address | RET | Params | Creates |
|---|---|---|---|---|
| HudPanel__Constructor | 0x524070 | 0x4 | 1 | DDGame+0x534 |
| InitWeaponTable | 0x53CAB0 | 0x4 | 1 | Weapon table setup |
| InitTeamsFromSetup | 0x5220B0 | 0x8 | 2 | DDGame+0x4628 team arena |

### Group E — Task objects

| Function | Address | RET | Params | Creates |
|---|---|---|---|---|
| TeamManager__Constructor | 0x563D40 | 0x8 | 2 | DDGame+0x530 |
| CTaskTurnGame__Constructor | 0x55B280 | 0x8 | 2 | Turn game task |
| CTaskGameState__Constructor | 0x532330 | 0x8 | 2 | Game state task |

### Group F — Display objects

| Function | Address | RET | Params | Creates |
|---|---|---|---|---|
| DisplayGfx__Constructor | 0x563FC0 | 0x14 | 5 | Full display gfx |
| DDDisplay__ConstructTextbox | 0x4FAF00 | 0xC | 3 | Textbox |
| FUN_567770 | 0x567770 | 0x4 | 1 | Final conditional display ctor |

### Group G — Buffer/stream objects

| Function | Address | RET | Params | Creates |
|---|---|---|---|---|
| FUN_545FD0 | 0x545FD0 | 0xC | 3 | Buffer object (called twice) |
| FUN_4FB490 | 0x4FB490 | 0x4 | 1 | GameStateStream sub-init |

## Testing Strategy

- Run headful + headless replay tests after each group (A, B, C, D, E, F, G)
- Headless MUST pass (deterministic output match) before proceeding
- Headful validates no crashes, correct rendering, game completion
- Name all functions in Ghidra as they're analyzed

## Naming Convention

Functions ported to Rust: `init_sprite_table`, `init_team_scoring`, etc.
Functions bridged via FFI: `wa_init_feature_flags` (for FUN_524700)
Ghidra labels: `CGameTask__InitTeamScoring`, `CGameTask__InitAllianceData`, etc.
