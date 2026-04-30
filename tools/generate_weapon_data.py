#!/usr/bin/env python3
"""Generate Rust source for `VANILLA_WEAPON_DATA` from a `weapon_table.bin` dump.

The output mirrors the field layout of `WeaponEntry` and `WeaponFireParams` in
`crates/openwa-game/src/game/weapon.rs`. If those structs change, update the
`WEAPON_ENTRY_FIELDS` / `FIRE_PARAMS_FIELDS` plans below and re-run.

Pointer fields (`name1`, `name2`) are emitted as `core::ptr::null()`; their
runtime VAs depend on WA.exe's string-table allocation and don't belong in a
static fixture.

Usage:
    python tools/generate_weapon_data.py <weapon_table.bin> <output.rs>
"""

import struct
import sys
from pathlib import Path

ENTRY_SIZE = 0x1D0
ENTRY_COUNT = 71
FIRE_PARAMS_OFFSET = 0x3C
FIRE_PARAMS_SIZE = ENTRY_SIZE - FIRE_PARAMS_OFFSET  # 0x194

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
assert len(WEAPON_NAMES) == ENTRY_COUNT

# Field plan: (name, kind, byte_offset, byte_size).
#   kind ∈ {"ptr_c_char", "i32", "fixed", "u8xN", "i32xN", "fire_params"}
WEAPON_ENTRY_FIELDS = [
    ("name1",              "ptr_c_char",  0x00, 4),
    ("name2",              "ptr_c_char",  0x04, 4),
    ("panel_state",        "i32",         0x08, 4),
    ("requires_aiming",    "i32",         0x0C, 4),
    ("defined",            "i32",         0x10, 4),
    ("shot_count",         "i32",         0x14, 4),
    ("_unknown_18",        "i32",         0x18, 4),
    ("retreat_time",       "i32",         0x1C, 4),
    ("creates_projectile", "i32",         0x20, 4),
    ("availability",       "i32",         0x24, 4),
    ("enabled",            "i32",         0x28, 4),
    ("_unknown_2c",        "u8x4",        0x2C, 4),
    ("fire_type",          "i32",         0x30, 4),
    ("special_subtype",    "i32",         0x34, 4),
    ("fire_method",        "i32",         0x38, 4),
    ("fire_params",        "fire_params", 0x3C, FIRE_PARAMS_SIZE),
]

# WeaponFireParams field offsets are relative to the FireParams start.
FIRE_PARAMS_FIELDS = [
    ("shot_count",       "i32",    0x00,   4),
    ("spread",           "i32",    0x04,   4),
    ("_fp_02",           "i32",    0x08,   4),
    ("collision_radius", "fixed",  0x0C,   4),
    ("_fp_04",           "i32",    0x10,   4),
    ("_fp_05",           "i32",    0x14,   4),
    ("_fp_06",           "i32",    0x18,   4),
    ("_fp_07",           "i32",    0x1C,   4),
    ("_fp_08",           "i32",    0x20,   4),
    ("sprite_id",        "i32",    0x24,   4),
    ("impact_type",      "i32",    0x28,   4),
    ("_fp_11",           "i32",    0x2C,   4),
    ("trail_effect",     "i32",    0x30,   4),
    ("gravity_pct",      "i32",    0x34,   4),
    ("wind_influence",   "i32",    0x38,   4),
    ("bounce_pct",       "i32",    0x3C,   4),
    ("_fp_16",           "i32",    0x40,   4),
    ("_fp_17",           "i32",    0x44,   4),
    ("friction_pct",     "i32",    0x48,   4),
    ("explosion_delay",  "i32",    0x4C,   4),
    ("fuse_timer",       "i32",    0x50,   4),
    ("_fp_21_25",        "i32x5",  0x54,   20),
    ("missile_type",     "i32",    0x68,   4),
    ("render_size",      "fixed",  0x6C,   4),
    ("render_timer",     "i32",    0x70,   4),
    ("homing_params",    "i32x5",  0x74,   20),
    ("_fp_34_36",        "i32x3",  0x88,   12),
    ("_fp_37_51",        "i32x15", 0x94,   60),
    ("cluster_params",   "i32x42", 0xD0,   168),
    ("entry_metadata",   "i32x7",  0x178,  28),
]


def _verify_fields(fields, total_size, label):
    """Walk the field plan and assert no gaps / overlaps."""
    pos = 0
    for name, _kind, off, sz in fields:
        if off != pos:
            raise AssertionError(
                f"{label}: {name} expected at {pos:#x}, declared at {off:#x}"
            )
        pos += sz
    if pos != total_size:
        raise AssertionError(
            f"{label}: total size {pos:#x}, expected {total_size:#x}"
        )


_verify_fields(WEAPON_ENTRY_FIELDS, ENTRY_SIZE, "WeaponEntry")
_verify_fields(FIRE_PARAMS_FIELDS, FIRE_PARAMS_SIZE, "WeaponFireParams")


def render_i32(v: int) -> str:
    """i32 → decimal for small values, hex for flag-shaped large values."""
    if v == 0:
        return "0"
    # Negative values: keep as decimal so the sign is obvious.
    if v < 0:
        # Re-canonicalize: -1 reads better than `-0x1`.
        if v == -1 or -1024 <= v:
            return str(v)
        return str(v)
    # Positive: hex once it gets large enough to be a likely flag/bitmask.
    if v >= 0x10000:
        return f"0x{v:X}"
    return str(v)


def render_array(values, item_renderer):
    """Render a fixed array, condensing all-equal patterns to `[K; N]`."""
    if all(v == values[0] for v in values):
        return f"[{item_renderer(values[0])}; {len(values)}]"
    return "[" + ", ".join(item_renderer(v) for v in values) + "]"


def render_byte(b: int) -> str:
    return "0" if b == 0 else f"0x{b:02X}"


def render_field_value(kind: str, raw: bytes) -> str:
    """Render the right-hand-side of `field_name: <value>`."""
    if kind == "i32":
        (v,) = struct.unpack_from("<i", raw, 0)
        return render_i32(v)
    if kind == "fixed":
        (v,) = struct.unpack_from("<i", raw, 0)
        return f"Fixed({render_i32(v)})"
    if kind.startswith("u8x"):
        n = int(kind[3:])
        bs = list(struct.unpack_from(f"<{n}B", raw, 0))
        return render_array(bs, render_byte)
    if kind.startswith("i32x"):
        n = int(kind[4:])
        vals = list(struct.unpack_from(f"<{n}i", raw, 0))
        return render_array(vals, render_i32)
    raise ValueError(f"unhandled kind {kind!r}")


def render_fire_params(raw: bytes) -> list[str]:
    """Render the WeaponFireParams body as indented lines (no surrounding braces)."""
    lines = []
    for fname, kind, off, sz in FIRE_PARAMS_FIELDS:
        slot = raw[off : off + sz]
        rhs = render_field_value(kind, slot)
        lines.append(f"                {fname}: {rhs},")
    return lines


def render_entry(idx: int, entry_bytes: bytes) -> list[str]:
    """Render a single `WeaponEntry { ... },` block."""
    name = WEAPON_NAMES[idx]
    out = [f"        // [{idx:>2}] {name}", "        WeaponEntry {"]

    for fname, kind, off, sz in WEAPON_ENTRY_FIELDS:
        slot = entry_bytes[off : off + sz]
        if kind == "ptr_c_char":
            out.append(f"            {fname}: core::ptr::null(),")
        elif kind == "fire_params":
            out.append("            fire_params: WeaponFireParams {")
            out.extend(render_fire_params(slot))
            out.append("            },")
        else:
            out.append(f"            {fname}: {render_field_value(kind, slot)},")

    out.append("        },")
    return out


def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <weapon_table.bin> <output.rs>", file=sys.stderr)
        sys.exit(1)

    in_path = Path(sys.argv[1])
    out_path = Path(sys.argv[2])

    data = in_path.read_bytes()
    expected = ENTRY_SIZE * ENTRY_COUNT
    if len(data) != expected:
        raise SystemExit(
            f"Expected {expected} bytes ({ENTRY_COUNT} × 0x{ENTRY_SIZE:X}), got {len(data)}"
        )

    out: list[str] = []
    out.append("//! GENERATED — do not edit by hand.")
    out.append("//! Regenerate with:")
    out.append("//!")
    out.append("//!     python tools/generate_weapon_data.py <weapon_table.bin> \\")
    out.append("//!         crates/openwa-game/src/game/weapon_data.rs")
    out.append("//!")
    out.append("//! Vanilla WA.exe weapon-table snapshot, captured from a live game via the")
    out.append("//! debug CLI. Mirrors what `InitWeaponTable` (0x0053CAB0) populates at")
    out.append("//! `GameWorld+0x510` in stock builds. Custom schemes / mods can mutate this")
    out.append("//! at runtime; this fixture pins the vanilla baseline.")
    out.append("//!")
    out.append("//! Pointer fields (`name1`, `name2`) are nulled here — their values are")
    out.append("//! runtime VAs in WA.exe's string table (filled by `InitWeaponNameStrings`).")
    out.append("//! The fixture is intended for fire-dispatch metadata, not strings.")
    out.append("")
    out.append("#![allow(clippy::approx_constant, clippy::needless_update)]")
    out.append("")
    out.append("use std::sync::LazyLock;")
    out.append("")
    out.append("use openwa_core::fixed::Fixed;")
    out.append("")
    out.append("use crate::game::weapon::{WeaponEntry, WeaponFireParams};")
    out.append("")
    out.append("pub static VANILLA_WEAPON_DATA: LazyLock<[WeaponEntry; 71]> = LazyLock::new(|| {")
    out.append("    [")

    for i in range(ENTRY_COUNT):
        slice_ = data[i * ENTRY_SIZE : (i + 1) * ENTRY_SIZE]
        out.extend(render_entry(i, slice_))

    out.append("    ]")
    out.append("});")
    out.append("")  # trailing newline

    out_path.write_text("\n".join(out), encoding="utf-8")
    print(f"Wrote {out_path} — {ENTRY_COUNT} entries, {len(data)} input bytes.")


if __name__ == "__main__":
    main()
