# Frontend Navigation State Machine

## Overview

The WA frontend uses MFC `CDialog` modal dialogs driven by a main navigation loop.
The loop is at `0x4E6440` (`Frontend__MainNavigationLoop`), which is the `CWinApp::Run()`
virtual method override — a 19KB function that handles initialization and screen dispatch.

## Architecture

```
g_CurrentScreen (0x6B3504)
        │
        ▼
┌──────────────────────┐
│ Main Navigation Loop │  (0x4E6440)
│ do {                 │
│   switch(screenId) { │
│     case 10: ...     │──► Create dialog ──► DoModal() ──► result
│     case 11: ...     │                                       │
│     ...              │                                       │
│   }                  │◄──── store result in g_CurrentScreen ─┘
│ } while(true);       │
│                      │
│ case 15: EXIT        │──► cleanup, return
└──────────────────────┘
```

Each iteration:
1. Reads `g_CurrentScreen` (0x6B3504)
2. Dispatches to the matching case → allocates dialog → calls constructor → DoModal
3. `FrontendChangeScreen(screen_id)` inside the dialog calls `EndDialog(screen_id)`
4. DoModal returns the result code → stored back into `g_CurrentScreen`
5. Previous dialog destroyed via `Frontend__PostScreenCleanup` (0x4EB450)
6. Loop continues

## FrontendChangeScreen (0x447A20)

`stdcall(screen_id)` with `ESI` = dialog this pointer (MSVC `__usercall`).

Two modes based on `g_FrontendFrame` (0x6B3908):

- **Normal** (g_FrontendFrame != 0): Disable window → palette transition
  (0x422180) → call vtable[0x15C] twice (args 1, 2) → re-enable window →
  CDialog::EndDialog(screen_id)
- **Init** (g_FrontendFrame == 0): Store screen_id at dialog+0x44, clear flag
  bit 0x10 at dialog+0x3C

## Screen Dispatch Table

### Valid main-loop screens (10–30)

| ID | Hex | Name | Constructor | Notes |
|----|-----|------|-------------|-------|
| 10 | 0x0A | Deathmatch | 0x440F40 | `PreTransitionCleanup` before |
| 11 | 0x0B | LocalMultiplayer | 0x49C420 | `PreTransitionCleanup` before |
| 12 | 0x0C | Training | 0x4E0880 | Palette load before |
| 14 | 0x0E | Missions | 0x499190 | Palette load before |
| 15 | 0x0F | **Exit** | — | Cleanup + return from CWinApp::Run |
| 16 | 0x10 | PostInitMenu | 0x4C91B0 | Calls 0x4E4BD0 after |
| 17 | 0x11 | MainMenuRedirect | — | Sets g_CurrentScreen = 18 |
| 18 | 0x12 | **MainMenu** | 0x4866C0 | Primary menu dialog |
| 21 | 0x15 | SinglePlayer | 0x4D69F0 | Palette load before |
| 22 | 0x16 | CampaignA | 0x4A2B70 | `PreTransitionCleanup` after |
| 23 | 0x17 | CampaignB | 0x4A24D0 | `PreTransitionCleanup` after |
| 24 | 0x18 | AdvancedSettings | 0x4279E0 | Calls 0x4E4BD0 after |
| 25 | 0x19 | IntroMovie | 0x470870 | Hardcodes next=18, sets g_SkipToMainMenu |
| 27 | 0x1B | NetworkHostSetup | 0x4ADCA0 | `PreTransitionCleanup` after |
| 28 | 0x1C | NetworkOnlineSetup | 0x4ACBC0 | `PreTransitionCleanup` after |
| 29 | 0x1D | NetworkDialogA | 0x4BC970 | `PreTransitionCleanup` after |
| 30 | 0x1E | NetworkDialogB | 0x4BBC00 | `PreTransitionCleanup` after |

### Network/lobby screens (1700–1707)

| ID | Hex | Name | Constructor | Notes |
|----|-----|------|-------------|-------|
| 1700 | 0x6A4 | NetworkProviderSelect | 0x4A7990 | Special post-DoModal logic |
| 1701 | 0x6A5 | NetworkSettings | 0x4C23C0 | |
| 1702 | 0x6A6 | LAN | 0x480A80 | |
| 1703 | 0x6A7 | WormNet | 0x472400 | |
| 1704 | 0x6A8 | LobbyHost | 0x4B0160 | Fall-through from setup |
| 1705 | 0x6A9 | LobbyHostContinued | (fall-through) | |
| 1706 | 0x6AA | LobbyClient | (fall-through) | |
| 1707 | 0x6AB | LobbyGameStart | 0x4BDBE0 | |

### Dialog result codes (NOT dispatched by main loop)

These are returned by `EndDialog` from sub-dialogs (button handlers, options screens).
If they reach the main loop, they hit the default case → `g_CurrentScreen = 0x0F` → exit.
By that point, the game has already launched or the action has been taken.

| ID | Hex | Name | Meaning |
|----|-----|------|---------|
| 0 | 0x00 | Cancel | Return to previous / back |
| 1 | 0x01 | Splash | Splash/intro trigger |
| 2 | 0x02 | NetworkOptionA | Network option result |
| 4 | 0x04 | SinglePlayerPrep | SP prep result |
| 5 | 0x05 | SinglePlayerConfirm | SP confirm result |
| 32 | 0x20 | SchemeEditorDone | Return from scheme/weapon editor |
| 35 | 0x23 | StartGameOffline | Start game, no network |
| 36 | 0x24 | StartGameOnline | Start game, network available |
| 37 | 0x25 | MultiplayerLocal | Multiplayer, no network |
| 38 | 0x26 | MultiplayerOnline | Multiplayer, network available |
| 39 | 0x27 | OptionsAccepted | Return after applying options + saving theme |
| 50 | 0x32 | InGameA | In-game state |
| 52 | 0x34 | InGameB | In-game state |
| 58 | 0x3A | InGameC | In-game state |

#### Offline/Online pairs

The "Start Game" and "Multiplayer" paths each check a network availability flag
and branch to an offline/online result code:

- **Start Game** (`this+0x244` flag): 35 (offline) / 36 (online)
- **Multiplayer** (`this+0x140` flag): 37 (offline) / 38 (online)

Both paths use 0 (Cancel) as the "back to main menu" value.

## Key Functions

| Address | Name | Purpose |
|---------|------|---------|
| 0x4E6440 | Frontend__MainNavigationLoop | CWinApp::Run override — main dispatch loop |
| 0x447A20 | FrontendChangeScreen | Ends current dialog via EndDialog(screen_id) |
| 0x447960 | Frontend__DoModalWrapper | Palette transition + custom DoModal |
| 0x40FD60 | CDialog__DoModal_Custom | Custom MFC DoModal |
| 0x40FBE0 | CDialog__CustomMsgPump | Custom message pump (replaces RunModalLoop) |
| 0x40FF90 | FrontendDialog__OnIdle | Per-frame idle: cursor, mouse, paint dispatch |
| 0x40BF60 | FrontendDialog__PaintControlTree | Traverse control tree, paint dirty controls |
| 0x4EB450 | Frontend__PostScreenCleanup | Destroys previous dialog |
| 0x4E4AE0 | Frontend__PreTransitionCleanup | Cleanup before certain transitions |
| 0x422180 | Frontend__PaletteAnimation | Palette transition effect |
| 0x446BA0 | FrontendDialog__Constructor | Base frontend dialog constructor |
| 0x44E850 | Frontend__OnMultiplayerButton | Multiplayer button handler |
| 0x44EC10 | Frontend__OnNetworkButton | Network button handler |
| 0x48DAB0 | Frontend__OnOptionsAccept | Options accept handler |
| 0x4F14A0 | Frontend__OnStartGameButton | Start game button handler |
| 0x4D4920 | Network__IsAvailable | Check network availability |

## Key Globals

| Address | Name | Type | Description |
|---------|------|------|-------------|
| 0x6B3504 | g_CurrentScreen | u32 | Screen ID driving the main dispatch loop |
| 0x6B3908 | g_FrontendFrame | CFrameWnd* | Main frame window |
| 0x6B390C | g_FrontendHwnd | HWND | Main window handle |
| 0x79D6D4 | g_DDDisplayWrapper | void* | Display backend wrapper (valid during entire frontend) |
| 0x7A083D | g_SkipToMainMenu | u8 | Skip splash, go directly to main menu |
| 0x7A083F | g_AutoNetworkFlag | u8 | Auto-network mode flag |
| 0x7C0A20 | g_MainMenuActive | u8 | 0xFF during screen 18 |
| 0x7C03D0 | g_CWinApp | CWinApp* | MFC application singleton |
| 0x7C0D40 | g_NetworkMode | u32 | 0=LAN, nonzero=WormNET |
| 0x7C0D68 | g_NetworkSubtype | u32 | Network sub-type selector |

## Bitmap Font System

WA uses a custom bitmap font renderer, NOT GDI, for all game/frontend text.

### Font files
- `Graphics\Font.bmp` + `Data\Gfx\FontExt\%s2.fex` (extended glyphs)
- Four sizes: `SmlFont2.bmp` (12px), `StdFont2.bmp` (17px), `MedFont2.bmp` (25px), `BigFont2.bmp` (33px)

### Key structures
- Font array at `0x7A0F58`, each font is `0x241C` bytes
- Glyph table: 256 entries at `font+0x14`, each `0x24` bytes
- Character width table at `0x6B2DD9` (256 bytes)
- Registry options: `LargerFonts` (font+0x20), `FrontendFontDynamicAntialiasing` (font+0x70)

### Key functions

| Address | Name | Description |
|---------|------|-------------|
| 0x414680 | Font__LoadFonts | Loads all four font sizes from BMP files |
| 0x4143D0 | Font__RenderGlyphs | Core glyph renderer: iterates chars, blits from sprite sheet |
| 0x427830 | Font__DrawText | Higher-level: measures string, creates surface, renders |
| 0x5236B0 | DDDisplay__DrawTextOnBitmap | thiscall(font_id, bitmap, hAlign, vAlign, msg, a7, a8) |
| 0x4FAF00 | DDDisplay__ConstructTextbox | thiscall(dst, length, fontid) |
| 0x4FB070 | SetTextboxText | stdcall(textbox, msg, textcolor, color1, color2, a6, a7, opacity) |
| 0x542200 | DrawTextboxLocal | Draws textbox at screen position |

### Rendering pipeline
1. Create surface via `g_DDDisplayWrapper->vtable[22]` (CreateSurface factory)
2. `Font__RenderGlyphs` iterates characters, looks up glyph data in font table
3. Each glyph is blitted from the font sprite sheet onto the surface
4. Surface blitted to screen via `Surface__Blit` (0x403BF0 → surface->vtable[11])

### Display backends
The display wrapper at `0x79D6D4` (vtable `0x662EC8`, size 0x1C) delegates to one of 6 backends:
DirectDraw legacy, DirectDraw classic, D3D9 basic, D3D9 shader, OpenGL CPU, OpenGL GPU.
Concrete backend stored at `wrapper+0x18`. Initialized in the main navigation loop
before any dialog is created.
