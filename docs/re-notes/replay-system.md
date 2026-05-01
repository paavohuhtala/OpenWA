# Replay System

WA's replay system records and plays back game inputs from `.WAgame` files.

## Command-Line Entry Points

In `Frontend__MainNavigationLoop` (0x4E6440):

- `/play <file>` -- play replay from start
- `/playat <file> [MM:SS.FF]` -- play from specific time offset

Globals set by arg parsing:

- `0x0088AF58` -- replay filename buffer
- `0x0088C77C` -- playback position (frame number)
- `0x0088C778` -- position valid flag (0 or 1)

## GameWorld Replay Offsets

| Offset  | Type        | Description                                                                                                                                                      |
| ------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| +0x7E41 | byte        | Deferred hurry flag. Set to 1 during replay when spacebar triggers a hurry. Consumed by GameFrameEndProcessor which converts it to a local Hurry message (0x17). |
| +0xDB08 | byte        | Replay state flag A. Both A and B must be non-zero for the replay hurry path.                                                                                    |
| +0xDB0A | byte        | Replay state flag B.                                                                                                                                             |
| +0xDB0B | byte        | Replay state flag C (checked by SendGamePacketConditional).                                                                                                      |
| +0xDB1C | ptr         | Replay payload pointer (malloc'd from file data).                                                                                                                |
| +0xDB48 | dword       | Replay active flag (set to 1 by ReplayLoader).                                                                                                                   |
| +0xDB50 | dword       | Replay frame pointer/state.                                                                                                                                      |
| +0xDB54 | dword       | Replay frame pointer/state.                                                                                                                                      |
| +0xDB58 | dword       | Replay frame pointer/state.                                                                                                                                      |
| +0xDB60 | char[0x400] | Input replay file path.                                                                                                                                          |
| +0xDF60 | char[0x400] | Output replay file path (for recording).                                                                                                                         |
| +0xEF60 | dword       | Init counter (set to 0 by loader).                                                                                                                               |

## Spacebar Advance Mechanism (Fully Traced)

When spacebar is pressed during replay playback, this chain fires:

```
Keyboard input
  -> Control task (0x5451F0) translates keyboard action
  -> Sets WorldRoot+0x1F4 = 1 ("hurry requested")
  -> WorldRoot ProcessInput (msg 4)
     -> WorldRoot_HurryHandler (0x55E5F0)
        checks WorldRoot+0x1F4
        Normal game: SendGamePacketWrapped(0x17, 0)
        Replay mode:  GameWorld+0x7E41 = 1 (deferred hurry)
  -> GameFrameEndProcessor (0x531960) end-of-frame
     reads GameWorld+0x7E41
     if set: sends msg 0x17 (Hurry) locally + via SendGamePacketConditional
     clears flag
  -> Turn ends, next turn begins from replay data
```

Packet 0x17 = EntityMessage_Hurry (value 23 from wkJellyWorm enum). In replay mode
this is processed locally, not over the network. The deferred flag (0x7E41) bridges
the gap between "input detected during ProcessInput" and "end-of-frame processing."

## Key Functions

| Address  | Name                      | Description                                                                                                                              |
| -------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 0x462DF0 | ReplayLoader              | Loads .WAgame file. Validates magic 0x4157 ("WA"), stores payload at GameWorld+0xDB1C. param_2: 1=play, 2=getmap, 3=getscheme, 4=repair. |
| 0x4E3490 | ParseReplayPosition       | Parses "MM:SS.FF" -> frame number. Returns -1 on failure.                                                                                |
| 0x531880 | SendGamePacketConditional | Sends packet if network buffer allows.                                                                                                   |
| 0x531960 | GameFrameEndProcessor     | End-of-frame. Reads deferred hurry flag, sends Hurry message.                                                                            |
| 0x531D00 | GameFrameDispatcher       | Main frame loop. Message queue + GameFrameEndProcessor.                                                                                  |
| 0x5451F0 | ControlTask_HandleMessage | Translates keyboard input (msg 0xC) -> game messages.                                                                                    |
| 0x553BD0 | GameMessageRouter         | Routes messages through task handler tree.                                                                                               |
| 0x55DC00 | WorldRoot_HandleMessage   | Message dispatcher. Case 2=FrameFinish, 4=ProcessInput.                                                                                  |
| 0x55E5F0 | WorldRoot_HurryHandler    | Hurry logic. Normal: packet 0x17. Replay: deferred flag. \_\_usercall(ESI).                                                              |
| 0x55FDA0 | TurnManager_ProcessFrame  | Per-frame turn timer, decrements by 0x14 (20ms).                                                                                         |
| 0x5611E0 | WorldRoot_AutoSelectTeams | Iterates teams during ProcessInput, sends packet 0x2B.                                                                                   |

## Replay File Format

- Magic: 4 bytes, value 0x4157 ("WA" in little-endian)
- Version in upper 16 bits of first dword
- Size: 4 bytes (payload length)
- Payload: binary game state + recorded inputs

## Direct Replay Control (Future)

To bypass keyboard simulation entirely, set GameWorld+0x7E41 = 1 from our DLL thread.
The game's existing GameFrameEndProcessor converts it to a local Hurry message every
frame. This is the simplest approach and faithfully replicates what spacebar does.

Prerequisites: replay flags DB08 and DB0A must both be non-zero (true during normal
replay playback).
