#!/usr/bin/env python3
"""Analyze a raw weapon table dump from the debug CLI.

Usage:
    python tools/analyze_weapon_table.py /tmp/weapon_table.bin

Reads 71 x 0x1D0 byte entries and prints field analysis.
"""

import struct
import sys
from pathlib import Path

ENTRY_SIZE = 0x1D0
ENTRY_COUNT = 71
FIRE_PARAMS_OFFSET = 0x3C

WEAPON_NAMES = [
    "None", "Bazooka", "HomingMissile", "Mortar", "HomingPigeon",
    "SheepLauncher", "Grenade", "ClusterBomb", "BananaBomb", "BattleAxe",
    "Earthquake", "Shotgun", "Handgun", "Uzi", "Minigun",
    "Longbow", "FirePunch", "DragonBall", "Kamikaze", "SuicideBomber",
    "Prod", "Dynamite", "Mine", "Sheep", "SuperSheep",
    "AquaSheep", "MoleBomb", "AirStrike", "NapalmStrike", "MailStrike",
    "MineStrike", "MoleSquadron", "BlowTorch", "PneumaticDrill", "Girder",
    "BaseballBat", "GirderPack", "NinjaRope", "Bungee", "Parachute",
    "Teleport", "ScalesOfJustice", "SuperBanana", "HolyGrenade", "FlameThrower",
    "SalvationArmy", "MbBomb", "PetrolBomb", "Skunk", "MingVase",
    "SheepStrike", "CarpetBomb", "MadCow", "OldWoman", "Donkey",
    "NuclearTest", "Armageddon", "SkipGo", "Surrender", "SelectWorm",
    "Freeze", "MagicBullet", "JetPack", "LowGravity", "FastWalk",
    "LaserSight", "Invisibility", "DamageX2", "CrateSpy", "DoubleTurnTime",
    "CrateShower",
]

FIRE_TYPE_NAMES = {0: "none", 1: "projectile", 2: "rope", 3: "grenade", 4: "special"}

# Known WeaponEntry fields (offset -> name)
KNOWN_FIELDS = {
    0x00: "name1_ptr",
    0x04: "name2_ptr",
    0x08: "panel_state",
    0x0C: "_unknown_0c",
    0x10: "defined",
    0x24: "availability",
    0x28: "enabled",
    0x30: "fire_type",
    0x34: "fire_subtype_34",
    0x38: "fire_subtype_38",
}


def read_entries(data: bytes) -> list[bytes]:
    assert len(data) == ENTRY_SIZE * ENTRY_COUNT, f"Expected {ENTRY_SIZE * ENTRY_COUNT} bytes, got {len(data)}"
    return [data[i * ENTRY_SIZE:(i + 1) * ENTRY_SIZE] for i in range(ENTRY_COUNT)]


def dword_at(entry: bytes, offset: int) -> int:
    return struct.unpack_from("<I", entry, offset)[0]


def sdword_at(entry: bytes, offset: int) -> int:
    return struct.unpack_from("<i", entry, offset)[0]


def print_header_fields(entries: list[bytes]):
    """Print known header fields (0x00-0x3B) for all weapons."""
    print("=" * 80)
    print("WEAPON ENTRY HEADER (0x00-0x3B)")
    print("=" * 80)

    fmt = "{:<4} {:<16} {:>4} {:>4} {:>4} {:>12} {:>4}  {:>4} {:>4} {:>4}"
    print(fmt.format("ID", "Name", "panl", "unk0C", "def", "avail", "enab", "ftyp", "sub34", "sub38"))
    print("-" * 80)

    for i, entry in enumerate(entries):
        name = WEAPON_NAMES[i]
        panel = sdword_at(entry, 0x08)
        unk0c = sdword_at(entry, 0x0C)
        defined = sdword_at(entry, 0x10)
        avail = sdword_at(entry, 0x24)
        enabled = sdword_at(entry, 0x28)
        ftype = sdword_at(entry, 0x30)
        sub34 = sdword_at(entry, 0x34)
        sub38 = sdword_at(entry, 0x38)

        type_name = FIRE_TYPE_NAMES.get(ftype, f"?{ftype}")
        print(f"{i:<4} {name:<16} {panel:>4} {unk0c:>4} {defined:>4} {avail:>12} {enabled:>4}  {type_name:<11} {sub34:>4} {sub38:>4}")


def print_unknown_header_region(entries: list[bytes]):
    """Print the unknown 0x14-0x23 region."""
    print("\n" + "=" * 80)
    print("UNKNOWN HEADER REGION (0x14-0x23)")
    print("=" * 80)

    for i, entry in enumerate(entries):
        vals = [dword_at(entry, off) for off in range(0x14, 0x24, 4)]
        if any(v != 0 for v in vals):
            print(f"{i:<4} {WEAPON_NAMES[i]:<16} " + " ".join(f"{v:08X}" for v in vals))
    print("(only non-zero entries shown)")


def analyze_fire_params(entries: list[bytes]):
    """Analyze fire_params region (0x3C-0x1CF) across all weapons."""
    print("\n" + "=" * 80)
    print("FIRE PARAMS ANALYSIS (0x3C-0x1CF, 0x194 bytes = 101 DWORDs)")
    print("=" * 80)

    num_dwords = (ENTRY_SIZE - FIRE_PARAMS_OFFSET) // 4

    # For each DWORD offset, collect all values
    interesting = []
    all_zero = []

    for di in range(num_dwords):
        byte_off = FIRE_PARAMS_OFFSET + di * 4
        values = [dword_at(entry, byte_off) for entry in entries]
        unique = set(values)
        nonzero = [v for v in values if v != 0]

        if len(unique) == 1 and values[0] == 0:
            all_zero.append(di)
        elif len(unique) <= 3:
            interesting.append((di, byte_off, values, "few_values"))
        else:
            interesting.append((di, byte_off, values, "varied"))

    print(f"\nAll-zero DWORDs ({len(all_zero)} of {num_dwords}): indices {all_zero[:20]}{'...' if len(all_zero) > 20 else ''}")
    print(f"Non-trivial DWORDs: {len(interesting)}")

    # Print interesting fields grouped by pattern
    print("\n--- Fields with few distinct values (likely flags/enums) ---")
    for di, byte_off, values, kind in interesting:
        if kind != "few_values":
            continue
        unique = sorted(set(values))
        # Which weapons have each value?
        val_weapons = {}
        for i, v in enumerate(values):
            val_weapons.setdefault(v, []).append(i)

        print(f"\n  +0x{byte_off:03X} (fire_params[{di - FIRE_PARAMS_OFFSET // 4}]):")
        for v in unique:
            weapons = val_weapons[v]
            names = [WEAPON_NAMES[w] for w in weapons[:8]]
            suffix = f" +{len(weapons)-8} more" if len(weapons) > 8 else ""
            print(f"    {v:>10} ({v:#010x}): {', '.join(names)}{suffix}")

    print("\n--- Fields with many distinct values (likely parameters) ---")
    print(f"{'Offset':<10} {'Min':>10} {'Max':>10} {'Unique':>6}  Sample values (Bazooka, Grenade, Shotgun, NinjaRope)")
    print("-" * 90)

    for di, byte_off, values, kind in interesting:
        if kind != "varied":
            continue
        unique_count = len(set(values))
        min_v = min(values)
        max_v = max(values)
        samples = [values[1], values[6], values[11], values[37]]  # Bazooka, Grenade, Shotgun, NinjaRope

        sample_str = ", ".join(f"{v:>8}" for v in samples)
        print(f"+0x{byte_off:03X}     {min_v:>10} {max_v:>10} {unique_count:>6}  [{sample_str}]")


def print_fire_params_by_type(entries: list[bytes]):
    """Group weapons by fire_type and print fire_params for each group."""
    groups: dict[int, list[int]] = {}
    for i, entry in enumerate(entries):
        ftype = sdword_at(entry, 0x30)
        groups.setdefault(ftype, []).append(i)

    print("\n" + "=" * 80)
    print("FIRE PARAMS BY WEAPON TYPE")
    print("=" * 80)

    for ftype in sorted(groups.keys()):
        if ftype == 0:
            continue
        type_name = FIRE_TYPE_NAMES.get(ftype, f"type_{ftype}")
        weapon_ids = groups[ftype]
        print(f"\n--- Type {ftype} ({type_name}): {len(weapon_ids)} weapons ---")
        print(f"Weapons: {', '.join(WEAPON_NAMES[w] for w in weapon_ids)}")

        # Find fields that vary within this type group
        num_dwords = (ENTRY_SIZE - FIRE_PARAMS_OFFSET) // 4
        for di in range(min(num_dwords, 40)):  # First 40 DWORDs for readability
            byte_off = FIRE_PARAMS_OFFSET + di * 4
            values = {w: dword_at(entries[w], byte_off) for w in weapon_ids}
            unique = set(values.values())
            if len(unique) > 1 or (len(unique) == 1 and list(unique)[0] != 0):
                val_str = ", ".join(f"{WEAPON_NAMES[w][:8]}={values[w]}" for w in weapon_ids[:6])
                print(f"  +0x{byte_off:03X} fp[{di}]: {val_str}")


def main():
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <weapon_table.bin>")
        sys.exit(1)

    data = Path(sys.argv[1]).read_bytes()
    entries = read_entries(data)

    print_header_fields(entries)
    print_unknown_header_region(entries)
    analyze_fire_params(entries)
    print_fire_params_by_type(entries)


if __name__ == "__main__":
    main()
