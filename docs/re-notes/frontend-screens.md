# Frontend Screen System

## Architecture

The frontend uses **MFC CDialog modal dialogs**, not the CTask class hierarchy.
Screen transitions work through a centralized dispatcher:

```
FrontendFrame (global at 0x6B3908, constructed at 0x4ECCA0)
  └── CDialog modal loop (CDialog__DoModal_Custom at 0x40FD60)
       ├── Button handler calls FrontendChangeScreen(screen_id)
       ├── FrontendChangeScreen calls CDialog::EndDialog(screen_id)
       ├── DoModal returns screen_id to caller
       └── Caller decides next dialog based on return value
```

## FrontendChangeScreen (0x447A20)

Two modes based on `g_FrontendFrame` (0x6B3908):

- **Normal** (g_FrontendFrame != 0): Disable window → palette transition
  (0x422180) → call vtable method at +0x15C twice → re-enable window →
  CDialog::EndDialog(screen_id)
- **Init** (g_FrontendFrame == 0): Store screen_id at ESI+0x44, clear flag bit

## Screen IDs

| ID | Hex | Name | Context |
|----|-----|------|---------|
| 0 | 0x00 | Cancel | Return to previous / close dialog |
| 10 | 0x0A | MainMenu | Main menu screen |
| 11 | 0x0B | LocalMultiplayer | Local multiplayer setup |
| 15 | 0x0F | Minimized | Window minimized (IsIconic check) |
| 16 | 0x10 | InitialLoad | Initial game loading screen |
| 21 | 0x15 | SinglePlayer | Single player / Deathmatch |
| 26 | 0x1A | NetworkSubScreen | Network options sub-screen |
| 35 | 0x23 | StartGameA | Start game (path A) |
| 36 | 0x24 | StartGameB | Start game (path B) |
| 37 | 0x25 | NetworkSetupA | Network setup (path A) |
| 38 | 0x26 | NetworkSetupB | Network setup (path B) |
| 39 | 0x27 | OptionsAccept | Options saved (writes current.thm) |
| 1700 | 0x6A4 | NetworkSelection | Network provider selection |
| 1702 | 0x6A6 | Lan | LAN game |
| 1703 | 0x6A7 | WormNet | WormNET online lobby |
| 1704 | 0x6A8 | LobbyHost | Hosting a game |
| 1706 | 0x6AA | LobbyClient | Joining a game |
| 1707 | 0x6AB | LobbyGameStart | Lobby → game start |

Note: Some callers pass dynamic screen IDs. There may be additional IDs not
discovered through static analysis.

## Key Globals

| Address | Name | Type | Description |
|---------|------|------|-------------|
| 0x6B3908 | g_FrontendFrame | CWnd* | Main application frame window |
| 0x6B390C | g_FrontendHwnd | HWND | Main window handle |
| 0x7C0D40 | g_NetworkMode | int | 0=LAN, nonzero=WormNET |
| 0x7C0D68 | g_NetworkSubtype | int | Network sub-type selector |

## Key Functions

| Address | Name | Purpose |
|---------|------|---------|
| 0x40FD60 | CDialog__DoModal_Custom | Custom DoModal returning screen ID |
| 0x422180 | Frontend__PaletteAnimation | Palette transition effect |
| 0x429830 | Frontend__OnInitialLoad | Initial game load handler |
| 0x441D80 | Frontend__LaunchSinglePlayer | Single player launch |
| 0x446BA0 | FrontendDialog__Constructor | Dialog constructor |
| 0x447A20 | FrontendChangeScreen | Central screen transition dispatcher |
| 0x447AA0 | Frontend__LoadTransitionPalette | Load transition palette |
| 0x44E850 | Frontend__OnMultiplayerButton | Multiplayer button handler |
| 0x44EC10 | Frontend__OnNetworkButton | Network button handler |
| 0x486A10 | Frontend__OnMinimize | Window minimize handler |
| 0x48DAB0 | Frontend__OnOptionsAccept | Options accept handler |
| 0x493CB0 | Lobby__DisplayMessage | Display lobby chat message |
| 0x4AA990 | Lobby__SendGreentext | Send colored lobby text |
| 0x4B7E20 | Lobby__PrintUsedVersion | Print version to lobby |
| 0x4BAE40 | Lobby__OnDisconnect | Handle lobby disconnect |
| 0x4BAEC0 | Lobby__OnGameEnd | Handle game end in lobby |
| 0x4BD400 | Lobby__OnMessage | Handle lobby message |
| 0x4CD9A0 | LobbyDialog__Constructor | Lobby dialog constructor |
| 0x4D4920 | Network__IsAvailable | Check network availability |
| 0x4E4AE0 | Frontend__PreTransitionCleanup | Pre-transition cleanup |
| 0x4ECCA0 | FrontendFrame__Constructor | Frame window constructor |
| 0x4F14A0 | Frontend__OnStartGameButton | Start game button handler |
