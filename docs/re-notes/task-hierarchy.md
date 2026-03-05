# Task Hierarchy - Worms Armageddon 3.8.1

## Overview

WA uses a hierarchical task/entity system. All game objects inherit from `CTask`,
which forms a tree via parent/children pointers. Tasks communicate through a
message passing system (see `TaskMessage` enum).

## Class Hierarchy

```
CTask (base)                    vtable: 0x669F8C    ctor: 0x5625A0
├── CGameTask                   vtable: 0x6641F8    ctor: 0x4FED50
│   ├── CTaskWorm                                    ctor: 0x50BFB0
│   ├── CTaskOldWorm                                 ctor: 0x51FEB0
│   ├── CTaskMissile                                 ctor: 0x507D10
│   ├── CTaskArrow                                   ctor: 0x4FE130
│   ├── CTaskMine                                    ctor: 0x506660
│   ├── CTaskCanister                                ctor: 0x501A80
│   ├── CTaskCrate                                   ctor: 0x502490
│   ├── CTaskOilDrum                                 ctor: 0x504AF0
│   ├── CTaskCross                                   ctor: 0x5045C0
│   └── CTaskLand                                    ctor: 0x505440
├── CTaskFilter                                      ctor: 0x54F3D0
├── CTaskTeam                                        ctor: 0x555BB0
├── CTaskTurnGame                                    ctor: 0x55B280
├── CTaskAirStrike                                   ctor: 0x5553C0
├── CTaskDirt                                        ctor: 0x54EDC0
├── CTaskFlame                                       ctor: 0x54F0F0
├── CTaskFire                                        ctor: 0x54F4C0
├── CTaskFireBall                                    ctor: 0x550890
├── CTaskGas                                         ctor: 0x554750
├── CTaskSmoke                                       ctor: 0x5551D0
├── CTaskCloud                                       ctor: 0x5482E0
├── CTaskSeaBubble                                   ctor: 0x554FE0
├── CTaskScoreBubble                                 ctor: 0x554CA0
├── CTaskCPU                                         ctor: 0x5485D0
└── CTaskSpriteAnimation                             ctor: 0x5466C0
```

Note: Not all tasks are confirmed to inherit from CGameTask. The hierarchy
above needs further verification via Ghidra analysis of each constructor.

## CTask vtable (0x669F8C)

| Index | Offset | Address    | Name (from wkJellyWorm) |
|-------|--------|------------|-------------------------|
| 0     | 0x00   | 0x562710   | vtable0 (init?)         |
| 1     | 0x04   | 0x562620   | Free                    |
| 2     | 0x08   | 0x562F30   | HandleMessage           |
| 3     | 0x0C   | 0x5613D0   | unknown                 |
| 4     | 0x10   | 0x5613D0   | unknown (=vt3)          |
| 5     | 0x14   | 0x562FA0   | unknown                 |
| 6     | 0x18   | 0x563000   | unknown                 |
| 7     | 0x1C   | 0x563210   | ProcessFrame            |

Note: wkJellyWorm says 7 methods but we see 8 entries. The 8th may be
the start of CGameTask's extended vtable region, or the count was off by one.

## CGameTask vtable (0x6641F8)

First 3 entries override CTask:

| Index | Offset | Address    | Name                    |
|-------|--------|------------|-------------------------|
| 0     | 0x00   | 0x4FF1C0   | vtable0 override        |
| 1     | 0x04   | 0x4FEF10   | Free override           |
| 2     | 0x08   | 0x4FF280   | HandleMessage override  |
| 3     | 0x0C   | 0x5613D0   | (inherited from CTask)  |
| 4     | 0x10   | 0x5613D0   | (inherited from CTask)  |
| 5     | 0x14   | 0x562FA0   | (inherited from CTask)  |
| 6     | 0x18   | 0x563000   | (inherited from CTask)  |
| 7     | 0x1C   | 0x4FF720   | CGameTask-specific      |
| 8     | 0x20   | 0x4FFED0   | CGameTask-specific      |
| 9     | 0x24   | 0x500070   | CGameTask-specific      |
| 10    | 0x28   | 0x500080   | CGameTask-specific      |
| 11    | 0x2C   | 0x5000E0   | CGameTask-specific      |
| 12    | 0x30   | 0x500380   | CGameTask-specific      |
| 13    | 0x34   | 0x500360   | CGameTask-specific      |
| 14    | 0x38   | 0x4FE060   | CGameTask-specific      |
| 15    | 0x3C   | 0x500CC0   | CGameTask-specific      |
| 16    | 0x40   | 0x592A50   | CGameTask-specific      |
| 17    | 0x44   | 0x500090   | CGameTask-specific      |
| 18    | 0x48   | 0x545780   | CGameTask-specific      |
| 19    | 0x4C   | 0x4AA060   | CGameTask-specific      |

CGameTask also has a secondary vtable at object offset 0xE8, pointing to 0x669CF8.

## CTask memory layout (0x30 bytes)

```
Offset  Size  Field
0x00    4     vtable pointer
0x04    4     parent (CTask*)
0x08    4     unknown (set to 0x10 in ctor)
0x0C    4     children.max_size (init 0)
0x10    4     children.unk4 (init 0)
0x14    4     children.size
0x18    4     children.data_ptr (allocated 0x60 bytes, zeroed 0x40)
0x1C    4     children.hash_list
0x20    4     class_type (ClassType enum)
0x24    12    unknown padding
```

## CGameTask memory layout (0xEC bytes)

Extends CTask at offset 0x00. Key fields discovered from constructor:

```
Offset  Size  Field
0x00    0x30  CTask base
0x20    4     class_type (set to 3 = GameCollisionTask in ctor)
0x2C    4     param_3 from ctor
0x30    4     param_3 (weapon id?)
0x38    4     param_4
0x3C    4     unknown
...
0x4C    4     = 0x10000 (Fixed 1.0)
0x50    4     = 0x100000 (Fixed 16.0)
0x54    4     = 0xCCCC (Fixed ~0.8)
0x58    4     = 0x10000 (Fixed 1.0)
0x5C    4     = 0
0x60    4     = 0x10000 (Fixed 1.0)
0x64    4     = 0x20000 (Fixed 2.0)
0x68    4     = 0xF333 (Fixed ~0.95)
...
0x84    4     pos_x (Fixed)
0x88    4     pos_y (Fixed)
0x8C    4     = 0x10000 (Fixed 1.0, scale?)
0x90    4     speed_x (Fixed)
0x94    4     speed_y (Fixed)
...
0xCC    4     = 0x10000 (Fixed 1.0)
0xD8    1     byte flag (init 0)
0xD9    1     byte flag (init 0)
0xE0    4     unknown (init 0)
0xE4    4     unknown (init 0)
0xDC    4     unknown (init 0)
0xE8    4     secondary vtable pointer -> 0x669CF8
```

## Physics defaults from CGameTask constructor

Several fixed-point constants are set in the constructor, likely physics defaults:
- `0x10000` = 1.0 (appears at many offsets — scale factors, gravity?)
- `0x100000` = 16.0 (mass? terminal velocity?)
- `0xCCCC` ≈ 0.8 (friction? damping?)
- `0x20000` = 2.0
- `0xF333` ≈ 0.95 (air resistance?)
- `0x3333` ≈ 0.2
- `0x4CCC` ≈ 0.3
- `0xFEB8` ≈ 0.995
- `0x9999` ≈ 0.6
- `0x3AE1` ≈ 0.23
- `0xFC000000` = -67108864 (large negative, possibly a boundary?)
