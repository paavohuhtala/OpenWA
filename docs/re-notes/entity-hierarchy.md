# Entity Hierarchy - Worms Armageddon 3.8.1

## Overview

WA uses a hierarchical entity/entity system. All game objects inherit from `BaseEntity`,
which forms a tree via parent/children pointers. Entities communicate through a
message passing system (see `EntityMessage` enum).

## Class Hierarchy

```
BaseEntity (base)                    vtable: 0x669F8C    ctor: 0x5625A0
├── WorldEntity                   vtable: 0x6641F8    ctor: 0x4FED50
│   ├── WormEntity                                    ctor: 0x50BFB0
│   ├── OldWarmEntity                                 ctor: 0x51FEB0
│   ├── MissileEntity                                 ctor: 0x507D10
│   ├── ArrowEntity                                   ctor: 0x4FE130
│   ├── MineEntity                                    ctor: 0x506660
│   ├── CanisterEntity                                ctor: 0x501A80
│   ├── CrateEntity                                   ctor: 0x502490
│   ├── OilDrumEntity                                 ctor: 0x504AF0
│   ├── CrossEntity                                   ctor: 0x5045C0
│   └── LandEntity                                    ctor: 0x505440
├── FilterEntity                                      ctor: 0x54F3D0
├── TeamEntity                                        ctor: 0x555BB0
├── WorldRootEntity                                    ctor: 0x55B280
├── AirStrikeEntity                                   ctor: 0x5553C0
├── DirtEntity                                        ctor: 0x54EDC0
├── FlameEntity                                       ctor: 0x54F0F0
├── FireEntity                                        ctor: 0x54F4C0
├── GirderEntity (was FireEntityBall)                   ctor: 0x550890
├── GasEntity                                         ctor: 0x554750
├── SmokeEntity                                       ctor: 0x5551D0
├── CloudEntity                                       ctor: 0x5482E0
├── SeaBubbleEntity                                   ctor: 0x554FE0
├── ScoreBubbleEntity                                 ctor: 0x554CA0
├── CPUEntity                                         ctor: 0x5485D0
└── SpriteAnimEntityation                             ctor: 0x5466C0
```

Note: Not all entities are confirmed to inherit from WorldEntity. The hierarchy
above needs further verification via Ghidra analysis of each constructor.

## BaseEntity vtable (0x669F8C)

| Index | Offset | Address  | Name (from wkJellyWorm) |
| ----- | ------ | -------- | ----------------------- |
| 0     | 0x00   | 0x562710 | vtable0 (init?)         |
| 1     | 0x04   | 0x562620 | Free                    |
| 2     | 0x08   | 0x562F30 | HandleMessage           |
| 3     | 0x0C   | 0x5613D0 | unknown                 |
| 4     | 0x10   | 0x5613D0 | unknown (=vt3)          |
| 5     | 0x14   | 0x562FA0 | unknown                 |
| 6     | 0x18   | 0x563000 | unknown                 |
| 7     | 0x1C   | 0x563210 | ProcessFrame            |

Note: wkJellyWorm says 7 methods but we see 8 entries. The 8th may be
the start of WorldEntity's extended vtable region, or the count was off by one.

## WorldEntity vtable (0x6641F8)

First 3 entries override BaseEntity:

| Index | Offset | Address  | Name                        |
| ----- | ------ | -------- | --------------------------- |
| 0     | 0x00   | 0x4FF1C0 | vtable0 override            |
| 1     | 0x04   | 0x4FEF10 | Free override               |
| 2     | 0x08   | 0x4FF280 | HandleMessage override      |
| 3     | 0x0C   | 0x5613D0 | (inherited from BaseEntity) |
| 4     | 0x10   | 0x5613D0 | (inherited from BaseEntity) |
| 5     | 0x14   | 0x562FA0 | (inherited from BaseEntity) |
| 6     | 0x18   | 0x563000 | (inherited from BaseEntity) |
| 7     | 0x1C   | 0x4FF720 | WorldEntity-specific        |
| 8     | 0x20   | 0x4FFED0 | WorldEntity-specific        |
| 9     | 0x24   | 0x500070 | WorldEntity-specific        |
| 10    | 0x28   | 0x500080 | WorldEntity-specific        |
| 11    | 0x2C   | 0x5000E0 | WorldEntity-specific        |
| 12    | 0x30   | 0x500380 | WorldEntity-specific        |
| 13    | 0x34   | 0x500360 | WorldEntity-specific        |
| 14    | 0x38   | 0x4FE060 | WorldEntity-specific        |
| 15    | 0x3C   | 0x500CC0 | WorldEntity-specific        |
| 16    | 0x40   | 0x592A50 | WorldEntity-specific        |
| 17    | 0x44   | 0x500090 | WorldEntity-specific        |
| 18    | 0x48   | 0x545780 | WorldEntity-specific        |
| 19    | 0x4C   | 0x4AA060 | WorldEntity-specific        |

WorldEntity also has a secondary vtable at object offset 0xE8, pointing to 0x669CF8.

## BaseEntity memory layout (0x30 bytes)

```
Offset  Size  Field
0x00    4     vtable pointer
0x04    4     parent (BaseEntity*)
0x08    4     unknown (set to 0x10 in ctor)
0x0C    4     children.max_size (init 0)
0x10    4     children.unk4 (init 0)
0x14    4     children.size
0x18    4     children.data_ptr (allocated 0x60 bytes, zeroed 0x40)
0x1C    4     children.hash_list
0x20    4     class_type (ClassType enum)
0x24    12    unknown padding
```

## WorldEntity memory layout (0xEC bytes)

Extends BaseEntity at offset 0x00. Key fields discovered from constructor:

```
Offset  Size  Field
0x00    0x30  BaseEntity base
0x20    4     class_type (set to 3 = GameCollisionEntity in ctor)
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

## Physics defaults from WorldEntity constructor

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
