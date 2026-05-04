# RenderQueue System

## Overview

WA.exe uses a deferred rendering queue. Game logic enqueues draw commands into a per-frame buffer, then `RQ_RenderDrawingQueue` (0x542350) processes them all at once, sorted by priority/layer.

The RenderQueue lives at GameWorld+0x524 (see `world_offsets::RENDER_QUEUE`).

## RenderQueue struct (0x12008 bytes)

| Offset  | Size    | Field                                                          |
| ------- | ------- | -------------------------------------------------------------- |
| 0x00000 | 4       | `buffer_offset` — write position (grows downward from 0x10000) |
| 0x00004 | 0x10000 | `_buffer` — command data storage                               |
| 0x10004 | 4       | `entry_count` — number of enqueued commands                    |
| 0x10008 | 0x2000  | `entry_ptrs[0x800]` — pointers to command entries              |

Max 2048 (0x800) entries per frame. Buffer grows downward; each alloc subtracts from `buffer_offset`.

## Command Types (0–14)

The dequeue function uses a switch on command type (first DWORD of each entry).

### Enqueue functions (standalone, all ported to Rust)

| Type | Function                   | Address  | DDDisplay vtable | Purpose                                                              |
| ---- | -------------------------- | -------- | ---------------- | -------------------------------------------------------------------- |
| 0    | RQ_DrawRect                | 0x541F40 | [0x48]           | Filled rectangle (clipped)                                           |
| 1    | RQ_DrawBitmapGlobal        | 0x542170 | [0x50]           | Bitmap at world position                                             |
| 2    | RQ_DrawTextboxLocal        | 0x542200 | [0x50]           | Text box (optional clipping)                                         |
| 4    | RQ_DrawSpriteGlobal        | 0x541FE0 | [0x4C]           | Sprite at world position                                             |
| 5    | RQ_DrawSpriteLocal         | 0x542060 | [0x4C]           | Sprite at screen position (camera-adjusted)                          |
| 6    | RQ_DrawSpriteOffset        | 0x5420E0 | [0x4C]           | Sprite with optional clipping/rotation                               |
| 8    | RQ_DrawLineStrip           | 0x541DD0 | [0x38]           | Connected line segments (variable-size)                              |
| 9    | RQ_DrawPolygon             | 0x541E50 | [0x34]           | Filled polygon edges (variable-size)                                 |
| 0xB  | RQ_DrawScaled              | 0x541ED0 | [0x40]           | Scaled sprite                                                        |
| 0xD  | RQ_DrawPixel               | 0x541D60 | [0x2C]           | Single pixel                                                         |
| 0xE  | RQ_DrawClippedSprite_Maybe | 0x5422A0 | [0x58]           | Sprite with complex clipping (like type 6 but different vtable slot) |

### Types without standalone enqueue functions

These types exist in the dequeue switch but have no standalone enqueue function in the binary. Their enqueue code is either inlined at call sites or unused in WA 3.8.1.

| Type | DDDisplay vtable | Dequeue behavior                         |
| ---- | ---------------- | ---------------------------------------- |
| 3    | [0x54]           | Coord-clipped, 5 params                  |
| 7    | [0x30]           | Direct command struct pass, 3 params     |
| 10   | [0x3C]           | Camera-adjusted + pixel offset, 6 params |
| 12   | [0x44]           | Like type 0xB but different vtable slot  |

## DDDisplay Drawing Vtable

> **Note:** The "DDDisplay vtable" described in this section is the same vtable as [`DisplayGfxVtable`](../../crates/openwa-game/src/render/display/vtable.rs) at 0x66A218 — there is no separate DDDisplay class. The dequeue function receives `world.display` (= `world+4`, also reachable as `runtime.display` at runtime+0x4D0) and dispatches through its 38-slot vtable. The original method-name mappings below (PutPixel, DrawSprite, etc.) were inferred before the full DisplayGfxVtable was reverse-engineered and have NOT been re-audited against the current slot definitions; some don't obviously line up (e.g., type 0xD "PutPixel" → slot 11 which is now `draw_tiled_bitmap`). Use the table as a starting hint; trust `DisplayGfxVtable` for the canonical signatures.

The dequeue function dispatches draw calls through a DDDisplay vtable. Slot offsets:

| Offset | Method            | Used by types |
| ------ | ----------------- | ------------- |
| 0x2C   | PutPixel          | 0xD           |
| 0x30   | (unknown)         | 7             |
| 0x34   | DrawPolygonEdge   | 9             |
| 0x38   | DrawLineSegment   | 8             |
| 0x3C   | (unknown)         | 10            |
| 0x40   | DrawScaledSprite  | 0xB           |
| 0x44   | (unknown)         | 12            |
| 0x48   | DrawClippedRect   | 0             |
| 0x4C   | DrawSprite        | 4, 5, 6       |
| 0x50   | DrawBitmap        | 1, 2          |
| 0x54   | (unknown)         | 3             |
| 0x58   | DrawClippedSprite | 0xE           |

Verified slot meanings for `RenderEscMenuOverlay` (0x00535000): slot 19 (offset 0x4C) = `blit_sprite` (sprite ID 0x20 + palette 0xa000 for cursor highlight); slot 20 (offset 0x50) = `draw_scaled_sprite` (takes the ESC-menu BitGrid canvas).

## Helper Functions

| Address  | Name                        | Purpose                                                     |
| -------- | --------------------------- | ----------------------------------------------------------- |
| 0x542B10 | RQ_GetCameraOffset_Maybe    | Returns camera/viewport offset for screen-space conversion  |
| 0x542BA0 | RQ_ClipCoordinates          | Main 16.16 fixed-point coordinate clipping                  |
| 0x542C70 | RQ_ClipWithRefOffset_Maybe  | Clipping with reference offset transform (type 6 flag 0x04) |
| 0x542D50 | RQ_TransformWithZoom_Maybe  | Coordinate transformation with zoom/scale                   |
| 0x542E60 | RQ_SmoothInterpolate_Maybe  | Linear interpolation with threshold (smooth scrolling)      |
| 0x542F10 | RQ_UpdateClipBounds_Maybe   | Viewport bounding box constraint updater                    |
| 0x542F70 | RQ_SaturateClipBounds_Maybe | Clipping with overflow saturation                           |

## Render Pipeline

| Address  | Name                   | Role                                                                         |
| -------- | ---------------------- | ---------------------------------------------------------------------------- |
| 0x56E040 | GameRuntime__RenderFrame           | Top-level render frame dispatcher                                                  |
| 0x533DC0 | GameRender                         | Core game render: resets queue, broadcasts msg 3 to entity tree, runs RQ, then 5 tail funcs |
| 0x535000 | GameRuntime__RenderEscMenuOverlay  | Per-frame ESC-menu overlay (was mislabeled `RenderTerrain_Maybe`). Blits panel_a/_b canvases via `display.draw_scaled_sprite` (slot 20) when their respective anim values are non-zero, plus a cursor-highlight sprite via `blit_sprite` (slot 19) on the active state |
| 0x540B00 | MenuPanel__Render                  | Incremental redraw of a panel's canvas (only items whose hover state changed are repainted). Returns `panel.display_a` (DisplayBitGrid*) for the caller to blit |
| 0x534F20 | GameRuntime__RenderWaitingForPeersTextbox | Pre-round network "PLEASE WAIT" textbox (gate: game_state == 1, network play, not all peer teams have joined). Was misnamed `RenderHUD_Maybe`. |
| 0x534E00 | GameRuntime__RenderNetworkEndWaitTextbox  | Post-round network "PLEASE WAIT %d SEC" textbox (gate: game_state in {NETWORK_END_AWAITING_PEERS, NETWORK_END_STARTED}, network play). Was misnamed `RenderTurnStatus_Maybe`. |
| 0x533C80 | PaletteManage_Maybe                | Palette/color management                                                            |
| 0x533A80 | PaletteAnimate_Maybe               | Palette animation/cycling (3 color palettes)                                       |

Note: the actual terrain is rendered via the world entity tree (LandEntity etc.) responding to message 3 broadcast inside `GameRender`, not in any tail function. The five tail funcs are all overlay/HUD layers drawn on top of the queue-rendered scene.

## Higher-Level Drawing Functions

These are not RenderQueue enqueue methods but call them:

| Address  | Name              | Notes                                            |
| -------- | ----------------- | ------------------------------------------------ |
| 0x500720 | WormEntity::DrawAttachedRope | Uses DrawSpriteLocal + DrawPolygon/DrawLineStrip — segmented rope (ninja rope + bungee, while attached) anchored to a `WormEntity` |
| 0x5197D0 | DrawCrosshairLine | Uses DrawPolygon + DrawSpriteLocal               |

## Dequeue Processing (0x542350)

`RQ_RenderDrawingQueue` sorts entries by priority field (DWORD at entry+4) via `qsort`, then iterates in reverse order (highest priority last = drawn on top). Each command is dispatched through the type switch, with coordinate clipping applied before calling the DDDisplay vtable method.

Fixed-point coordinates (16.16) are right-shifted to pixel values (`>> 0x10`) before passing to DDDisplay methods.
