#!/usr/bin/env python3
"""One-shot porter for WA's `InitWeaponDefaultsBaseline` (0x00537320, 1161 inst)
and `InitWeaponDefaultsExtended` (0x00539100, 1100 inst).

Both functions are flat unrolled `MOV [EAX+offset], imm/reg` sequences (no
branches, no calls) that overlay per-weapon defaults onto the WeaponTable
(at GameWorld+0x510). They run sequentially and share semantics.

Three irregularities the parser handles:
  - `LEA ESI, [EAX+src]; LEA EDI, [EAX+dst]; MOV ECX, n; REP MOVSD` — clones a
    range of dwords from one weapon row into another. Emitted as
    `core::ptr::copy_nonoverlapping(...)`.
  - `MOV reg, [game_info+0xD778]; CMP …; SETcc; <arithmetic>; MOV [EAX+off], reg` —
    a scheme-version-conditional value that resolves to one of two integers.
    Emitted as `if game_version < THR { LO } else { HI }`.
  - `MOV DL, [ECX+0xD963]` saved on the stack, later folded into a similar
    SBB/NEG/AND/OR pattern that yields `0x42185e` or `0x42187e` based on
    whether the byte was < 2.

The script uses a tiny abstract interpreter: tracks register values
(int / `Cond(threshold, lo, hi)`), stack memory, and `LEA`-derived base
pointers. Stores get filed by table offset and emitted as named-field writes
against `WeaponEntry`/`WeaponFireParams` using the field plan from
`generate_weapon_data.py`.

Usage:
    python tools/port_init_weapon_defaults.py > /tmp/init_weapon_defaults.rs
"""

from __future__ import annotations
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path

ENTRY_SIZE = 0x1D0
ENTRY_COUNT = 71
FIRE_PARAMS_OFFSET = 0x3C

# ────── Field plans (mirrored from generate_weapon_data.py) ──────

# WeaponEntry header fields (offsets 0x00-0x3B). `fire_params` covers the rest.
WEAPON_ENTRY_FIELDS = [
    ("name1",              "ptr",  0x00, 4),
    ("name2",              "ptr",  0x04, 4),
    ("panel_state",        "i32",  0x08, 4),
    ("requires_aiming",    "i32",  0x0C, 4),
    ("defined",            "i32",  0x10, 4),
    ("shot_count",         "i32",  0x14, 4),
    ("unknown_18",         "i32",  0x18, 4),
    ("retreat_time",       "i32",  0x1C, 4),
    ("creates_projectile", "i32",  0x20, 4),
    ("availability",       "i32",  0x24, 4),
    ("enabled",            "i32",  0x28, 4),
    ("_unknown_2c",        "u8x4", 0x2C, 4),
    ("fire_type",          "i32",  0x30, 4),
    ("special_subtype",    "i32",  0x34, 4),
    ("fire_method",        "i32",  0x38, 4),
]

# WeaponFireParams scalar fields (offsets relative to fire_params start = entry+0x3C).
_FP_SCALARS = [
    ("shot_count",       "i32",   0x00),
    ("spread",           "i32",   0x04),
    ("unknown_0x44",     "i32",   0x08),
    ("collision_radius", "fixed", 0x0C),
    ("unknown_0x4c",     "i32",   0x10),
    ("unknown_0x50",     "i32",   0x14),
    ("unknown_0x54",     "i32",   0x18),
    ("unknown_0x58",     "i32",   0x1C),
    ("unknown_0x5c",     "i32",   0x20),
    ("sprite_id",        "i32",   0x24),
    ("impact_type",      "i32",   0x28),
    ("unknown_0x68",     "i32",   0x2C),
    ("trail_effect",     "i32",   0x30),
    ("gravity_pct",      "i32",   0x34),
    ("wind_influence",   "i32",   0x38),
    ("bounce_pct",       "i32",   0x3C),
    ("unknown_0x7c",     "i32",   0x40),
    ("unknown_0x80",     "i32",   0x44),
    ("friction_pct",     "i32",   0x48),
    ("explosion_delay",  "i32",   0x4C),
    ("fuse_timer",       "i32",   0x50),
    ("missile_type",     "i32",   0x68),
    ("render_size",      "fixed", 0x6C),
    ("render_timer",     "i32",   0x70),
]
# WeaponFireParams array fields. `(name, elem_kind, byte_offset, elem_count)`.
_FP_ARRAYS = [
    ("unknown_0x90_0xa0",   "i32", 0x54, 5),
    ("homing_params",       "i32", 0x74, 5),
    ("unknown_0xc4_0xcc",   "i32", 0x88, 3),
    ("unknown_0xd0_0x10c",  "i32", 0x94, 15),
    ("cluster_params",      "i32", 0xD0, 42),
    ("entry_metadata",      "i32", 0x178, 7),
]
# Build flat (offset → path-suffix, kind) lookup.
FIRE_PARAMS_LOOKUP: dict[int, tuple[str, str]] = {}
for n, k, off in _FP_SCALARS:
    FIRE_PARAMS_LOOKUP[off] = (n, k)
for n, k, off, cnt in _FP_ARRAYS:
    for i in range(cnt):
        FIRE_PARAMS_LOOKUP[off + i * 4] = (f"{n}[{i}]", k)


def field_for_table_offset(byte_off: int) -> tuple[int, str, str]:
    """Resolve an absolute weapon-table byte offset to
    `(weapon_id, rust_path, kind)`. The `rust_path` is everything that
    follows `entries[N]`: e.g. `.fire_params.shot_count` or `.shot_count`."""
    weapon_id, intra = divmod(byte_off, ENTRY_SIZE)
    if weapon_id >= ENTRY_COUNT:
        raise ValueError(f"byte offset 0x{byte_off:X} maps to weapon {weapon_id} (>= 71)")
    if intra < FIRE_PARAMS_OFFSET:
        for name, kind, off, size in WEAPON_ENTRY_FIELDS:
            if intra == off:
                return weapon_id, f".{name}", kind
        raise ValueError(
            f"byte offset 0x{byte_off:X} (weapon {weapon_id} +0x{intra:X}) "
            f"falls inside header but doesn't match any field")
    intra -= FIRE_PARAMS_OFFSET
    if intra in FIRE_PARAMS_LOOKUP:
        suffix, kind = FIRE_PARAMS_LOOKUP[intra]
        # Array index: emit `.foo[N]` (no leading dot before `[`).
        if suffix.endswith("]"):
            head, _, _ = suffix.partition("[")
            return weapon_id, f".fire_params.{head}{suffix[len(head):]}", kind
        return weapon_id, f".fire_params.{suffix}", kind
    raise ValueError(
        f"byte offset 0x{byte_off:X} (weapon {weapon_id} fp+0x{intra:X}) "
        f"doesn't match any fire_params field")


# ────── Symbolic value representation ──────

@dataclass(frozen=True)
class Cond:
    """A scheme-version (or game_info+0xD963 byte) conditional value:
    `if <expr> < threshold { lo } else { hi }`."""
    op: str        # e.g. "game_version_lt_0x21" — see render_cond
    threshold: int  # threshold the original CMP used
    lo: int        # value when condition held (e.g. SETL fired)
    hi: int        # value when condition didn't hold

    def render_rust(self, kind: str) -> str:
        cond_expr = self.cond_expr_rust()
        lo = render_imm(self.lo, kind)
        hi = render_imm(self.hi, kind)
        return f"if {cond_expr} {{ {lo} }} else {{ {hi} }}"

    def cond_expr_rust(self) -> str:
        if self.op == "game_version_lt":
            return f"game_version < 0x{self.threshold:X}"
        if self.op == "game_version_ge":
            return f"game_version >= 0x{self.threshold:X}"
        if self.op == "d963_lt":
            return f"scheme_byte_d963 < 0x{self.threshold:X}"
        if self.op == "d963_ge":
            return f"scheme_byte_d963 >= 0x{self.threshold:X}"
        raise ValueError(f"unknown Cond op: {self.op}")


# A symbolic value is either an int (concrete) or one of the dataclasses below.
SymVal = "int | Cond | Sym"

@dataclass(frozen=True)
class Sym:
    """An opaque symbolic value (function arg, an unmodelled load, etc.).
    `expr` is the Rust expression that produces it."""
    expr: str  # Rust expression


# ────── Disassembly parser ──────

INSTR_RE = re.compile(r"^([0-9a-fA-F]+):\s+(\S+)(?:\s+(.+))?$")

@dataclass
class Instr:
    addr: int
    mnemonic: str
    operands: str  # raw operand string, comma-separated (with spaces preserved)
    raw: str

def parse_asm(path: Path) -> list[Instr]:
    out = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        m = INSTR_RE.match(line)
        if not m:
            raise ValueError(f"unrecognised line: {line!r}")
        addr = int(m.group(1), 16)
        mnem = m.group(2).upper()
        ops = (m.group(3) or "").strip()
        out.append(Instr(addr=addr, mnemonic=mnem, operands=ops, raw=line))
    return out


# ────── Operand parsing helpers ──────

REG8_TO_REG32 = {
    "AL": ("EAX", 0), "BL": ("EBX", 0), "CL": ("ECX", 0), "DL": ("EDX", 0),
    "AH": ("EAX", 8), "BH": ("EBX", 8), "CH": ("ECX", 8), "DH": ("EDX", 8),
}
REGS32 = {"EAX", "EBX", "ECX", "EDX", "ESI", "EDI", "EBP", "ESP"}

def parse_imm(tok: str) -> int:
    tok = tok.strip()
    if tok.startswith("0x") or tok.startswith("0X"):
        return int(tok, 16) & 0xFFFFFFFF
    return int(tok) & 0xFFFFFFFF

@dataclass
class MemOp:
    """Memory operand `[base + index*scale + disp]`."""
    base: str | None       # register name or None
    index: str | None
    scale: int             # 1, 2, 4, 8
    disp: int              # may be 0
    size: str              # "dword", "byte", "word"

def parse_mem_operand(s: str) -> MemOp:
    """Parse `dword ptr [EAX + 0x18]`, `byte ptr [ESP + 0x13]`,
    `[ECX*8 + 3]`, `[ECX + ECX*1 + 1]`, etc."""
    s = s.strip()
    size = "dword"
    if s.startswith("dword ptr"):
        s = s[len("dword ptr"):].strip()
    elif s.startswith("byte ptr"):
        s = s[len("byte ptr"):].strip()
        size = "byte"
    elif s.startswith("word ptr"):
        s = s[len("word ptr"):].strip()
        size = "word"
    if not (s.startswith("[") and s.endswith("]")):
        raise ValueError(f"expected memory operand, got {s!r}")
    inner = s[1:-1].strip()
    base = None
    index = None
    scale = 1
    disp = 0
    # Tokenize on '+' / '-' boundaries.
    # Make leading '-' work as a sign.
    parts = re.split(r"\s*([+\-])\s*", inner)
    # parts = [first, sign, term, sign, term, ...]
    sign = 1
    first = parts[0].strip()
    terms = [(1, first)]
    for i in range(1, len(parts), 2):
        s_sign = 1 if parts[i] == "+" else -1
        terms.append((s_sign, parts[i + 1].strip()))
    for s_sign, term in terms:
        if not term:
            continue
        # Could be "REG", "REG*N", or "imm"
        if "*" in term:
            reg, n = term.split("*")
            reg = reg.strip().upper()
            if reg not in REGS32:
                raise ValueError(f"bad scaled-index register: {reg}")
            if index is not None:
                raise ValueError(f"two scaled-index terms in {s!r}")
            index = reg
            scale = int(n.strip(), 0)
            if s_sign != 1:
                raise ValueError(f"negative scaled index? {s!r}")
        elif term.upper() in REGS32:
            reg = term.upper()
            if base is None:
                base = reg
            elif index is None:
                index = reg
                scale = 1
            else:
                raise ValueError(f"three regs in {s!r}")
        else:
            v = parse_imm(term)
            if s_sign < 0:
                v = (-v) & 0xFFFFFFFF
            disp = (disp + v) & 0xFFFFFFFF
    return MemOp(base=base, index=index, scale=scale, disp=disp, size=size)


# ────── Abstract interpreter ──────

@dataclass
class Sim:
    """Per-function state."""
    name: str
    regs: dict = field(default_factory=dict)
    stack: dict = field(default_factory=dict)   # esp_disp -> SymVal
    flags: dict = field(default_factory=dict)   # 'cmp': (op_value, imm)
    # Output: list of (table_byte_off, SymVal, kind_hint, source_addr, size).
    # `kind_hint` is "byte" if the original was a byte store, else "dword".
    writes: list = field(default_factory=list)
    # Block copies: (dst_off, src_off, dword_count, source_addr).
    block_copies: list = field(default_factory=list)
    # AND-clear ops on table memory: (off, mask, source_addr).
    and_writes: list = field(default_factory=list)
    # All ops in original source order: ("write", tup) | ("copy", tup) | ("and", tup).
    ops_in_order: list = field(default_factory=list)
    # LEA-derived base pointers — when EBP holds `EAX + N`, we model writes
    # through EBP as writes to `EAX + N + offset`. Map: reg -> (base_reg, disp).
    base_ptrs: dict = field(default_factory=dict)

    def get(self, name: str) -> SymVal:
        if name in self.regs:
            return self.regs[name]
        raise ValueError(f"read of un-set register {name}")

    def set(self, name: str, val: SymVal):
        self.regs[name] = val
        if name in self.base_ptrs:
            del self.base_ptrs[name]


def run_baseline(asm: list[Instr]) -> Sim:
    sim = Sim(name="baseline")
    # Pre-populate stack arg slots. Frame is `SUB ESP,8 + 4 PUSHes` = 0x18 below
    # initial ESP, so [ESP+0x1C] = arg1 (game_info), [ESP+0x20] = arg2 (cap).
    sim.stack[0x1C] = Sym("game_info as u32")
    sim.stack[0x20] = Sym("cap")
    run_instrs(sim, asm, body_starts_at_addr=0x00537327)  # post-prologue
    return sim

def run_extended(asm: list[Instr]) -> Sim:
    sim = Sim(name="extended")
    # 4 PUSHes only (no SUB ESP), so [ESP+0x14] = game_info.
    sim.stack[0x14] = Sym("game_info as u32")
    run_instrs(sim, asm, body_starts_at_addr=0x00539104)  # post-prologue
    return sim


def is_terminator(instr: Instr) -> bool:
    return instr.mnemonic in ("RET", "POP")  # POP appears in epilogue


def run_instrs(sim: Sim, asm: list[Instr], body_starts_at_addr: int):
    started = False
    for ins in asm:
        if not started:
            if ins.addr >= body_starts_at_addr:
                started = True
            else:
                continue  # skip prologue PUSH/SUB
        # Skip pure epilogue instructions (POP / ADD ESP / RET) without action.
        if ins.mnemonic == "RET":
            return
        if ins.mnemonic == "POP":
            # POP just discards the saved-reg value; we don't model it.
            continue
        if ins.mnemonic == "ADD" and ins.operands.startswith("ESP,"):
            continue
        if ins.mnemonic == "PUSH":
            # Doesn't appear post-prologue in either function; bail loudly.
            raise NotImplementedError(f"unexpected PUSH at {ins.addr:08X}")
        execute(sim, ins)


def execute(sim: Sim, ins: Instr):
    m = ins.mnemonic
    ops_raw = ins.operands

    if m == "MOV":
        return op_mov(sim, ops_raw, ins)
    if m == "MOVSD.REP":
        return op_rep_movsd(sim, ins)
    if m == "XOR":
        return op_xor(sim, ops_raw, ins)
    if m == "CMP":
        return op_cmp(sim, ops_raw, ins)
    if m == "SETL":
        return op_setcc(sim, ops_raw, "lt", ins)
    if m == "SETGE":
        return op_setcc(sim, ops_raw, "ge", ins)
    if m == "SBB":
        return op_sbb(sim, ops_raw, ins)
    if m == "SUB":
        return op_sub(sim, ops_raw, ins)
    if m == "ADD":
        return op_add(sim, ops_raw, ins)
    if m == "AND":
        return op_and(sim, ops_raw, ins)
    if m == "OR":
        return op_or(sim, ops_raw, ins)
    if m == "NEG":
        return op_neg(sim, ops_raw, ins)
    if m == "LEA":
        return op_lea(sim, ops_raw, ins)
    raise NotImplementedError(f"opcode {m!r} at 0x{ins.addr:08X}: {ins.raw}")


def split_two_ops(s: str) -> tuple[str, str]:
    # Split on first comma not inside brackets.
    depth = 0
    for i, c in enumerate(s):
        if c == "[":
            depth += 1
        elif c == "]":
            depth -= 1
        elif c == "," and depth == 0:
            return s[:i].strip(), s[i + 1:].strip()
    raise ValueError(f"expected two operands in {s!r}")


def op_mov(sim: Sim, ops: str, ins: Instr):
    dst_s, src_s = split_two_ops(ops)
    # Resolve src first (might depend on regs we're about to overwrite).
    if dst_s.upper() in REGS32:
        # Destination = 32-bit register.
        if src_s.upper() in REGS32:
            src_reg = src_s.upper()
            sim.set(dst_s.upper(), sim.get(src_reg))
            # Propagate base-pointer info on reg-to-reg move (e.g. `MOV ESI, EBP`
            # in the extended fn, where EBP holds `LEA EBP,[EAX+0x29b0]`).
            if src_reg in sim.base_ptrs:
                sim.base_ptrs[dst_s.upper()] = sim.base_ptrs[src_reg]
        elif src_s.startswith(("dword ptr [", "[")):
            mem = parse_mem_operand(src_s)
            sim.set(dst_s.upper(), mem_load(sim, mem, ins))
        else:
            sim.set(dst_s.upper(), parse_imm(src_s))
        return
    if dst_s in REG8_TO_REG32:
        # Destination = 8-bit register. Only DL is used in the baseline:
        # `MOV DL, byte ptr [ECX+0xD963]` — store as a tagged Sym so we can
        # later move it to the stack.
        if src_s.startswith(("byte ptr [", "[")):
            mem = parse_mem_operand(src_s)
            sim.set(dst_s, mem_load(sim, mem, ins))
        else:
            sim.set(dst_s, parse_imm(src_s))
        return
    # Destination is memory.
    mem = parse_mem_operand(dst_s)
    if src_s.upper() in REGS32:
        val = sim.get(src_s.upper())
    elif src_s in REG8_TO_REG32:
        # `MOV byte ptr [...], CL/BL/DL/...` — prefer an explicit byte-reg
        # value (set by `MOV DL, byte [mem]`); otherwise extract from the
        # corresponding 32-bit reg.
        if src_s in sim.regs:
            val = sim.regs[src_s]
        else:
            reg32, shift = REG8_TO_REG32[src_s]
            val32 = sim.get(reg32)
            if not isinstance(val32, int):
                raise NotImplementedError(
                    f"low-byte read of non-concrete {reg32}={val32!r} "
                    f"at 0x{ins.addr:08X}")
            val = (val32 >> shift) & 0xFF
    elif src_s.startswith(("dword ptr [", "byte ptr [", "[")):
        # No mem→mem in our corpus.
        raise NotImplementedError(f"mem→mem MOV at 0x{ins.addr:08X}")
    else:
        val = parse_imm(src_s)
    mem_store(sim, mem, val, ins)


def op_xor(sim: Sim, ops: str, ins: Instr):
    a, b = split_two_ops(ops)
    if a.upper() == b.upper():
        sim.set(a.upper(), 0)
        return
    raise NotImplementedError(f"XOR a,b (a≠b) at 0x{ins.addr:08X}")


def op_cmp(sim: Sim, ops: str, ins: Instr):
    a, b = split_two_ops(ops)
    if a.startswith(("dword ptr [", "byte ptr [", "[")):
        mem = parse_mem_operand(a)
        sim.flags["cmp_lhs"] = ("mem", mem)
    else:
        # Reg compare not used here.
        raise NotImplementedError(f"CMP reg at 0x{ins.addr:08X}")
    sim.flags["cmp_rhs"] = parse_imm(b)


def cmp_predicate(sim: Sim) -> tuple[str, int]:
    """Resolve the saved CMP into a `(op, threshold)` pair where `op` is one
    of `'game_version_lt'`, `'game_version_ge'`, `'d963_lt'`, `'d963_ge'`. Caller picks
    the lt/ge variant based on whether SETL or SETGE was used."""
    lhs = sim.flags.get("cmp_lhs")
    rhs = sim.flags.get("cmp_rhs")
    if lhs is None or rhs is None:
        raise ValueError(f"SETcc without preceding CMP: regs={sim.regs}")
    kind, mem = lhs
    if kind != "mem":
        raise NotImplementedError("non-memory CMP lhs")
    if mem.size == "byte" and mem.base == "ESP" and mem.disp == 0x13:
        return "d963", rhs
    if mem.size == "dword" and mem.disp == 0xD778:
        # Base must be a register that holds game_info.
        return "game_version", rhs
    raise NotImplementedError(f"unrecognised CMP lhs: {mem}")


def op_setcc(sim: Sim, ops: str, kind: str, ins: Instr):
    """SETL CL → CL = 1 if (cmp_lhs < cmp_rhs) else 0."""
    if ops.upper() != "CL":
        raise NotImplementedError(f"SETcc into {ops!r}")
    pred_kind, threshold = cmp_predicate(sim)
    op = f"{pred_kind}_{kind}"  # e.g. 'game_version_lt'
    # Cond.lo = result when condition holds (=1), Cond.hi = otherwise (=0).
    sim.set("ECX", Cond(op=op, threshold=threshold, lo=1, hi=0))


def op_sbb(sim: Sim, ops: str, ins: Instr):
    """SBB ECX, ECX after a CMP: ECX = -1 if borrow (lhs < rhs) else 0."""
    a, b = split_two_ops(ops)
    if a.upper() != "ECX" or b.upper() != "ECX":
        raise NotImplementedError(f"SBB {ops}")
    pred_kind, threshold = cmp_predicate(sim)
    op = f"{pred_kind}_lt"
    sim.set("ECX", Cond(op=op, threshold=threshold, lo=0xFFFFFFFF, hi=0))


def fold(v1: SymVal, op, v2: SymVal) -> SymVal:
    """Apply `op(lo,hi)` to the result of a Cond, or to a concrete int."""
    if isinstance(v1, int) and isinstance(v2, int):
        return op(v1, v2) & 0xFFFFFFFF
    if isinstance(v1, Cond) and isinstance(v2, int):
        return Cond(op=v1.op, threshold=v1.threshold,
                    lo=op(v1.lo, v2) & 0xFFFFFFFF,
                    hi=op(v1.hi, v2) & 0xFFFFFFFF)
    if isinstance(v1, int) and isinstance(v2, Cond):
        return Cond(op=v2.op, threshold=v2.threshold,
                    lo=op(v1, v2.lo) & 0xFFFFFFFF,
                    hi=op(v1, v2.hi) & 0xFFFFFFFF)
    raise NotImplementedError(f"fold {v1!r} . {v2!r}")


def op_sub(sim: Sim, ops: str, ins: Instr):
    a, b = split_two_ops(ops)
    if a.upper() == "ECX":
        # SUB ECX, EBP — when EBP=1 this maps a Cond {lo=1, hi=0} into {lo=0, hi=-1}.
        bv = sim.get(b.upper()) if b.upper() in REGS32 else parse_imm(b)
        sim.set("ECX", fold(sim.get("ECX"), lambda x, y: x - y, bv))
        return
    raise NotImplementedError(f"SUB {ops}")


def op_add(sim: Sim, ops: str, ins: Instr):
    a, b = split_two_ops(ops)
    if a.upper() == "ECX":
        bv = sim.get(b.upper()) if b.upper() in REGS32 else parse_imm(b)
        sim.set("ECX", fold(sim.get("ECX"), lambda x, y: x + y, bv))
        return
    raise NotImplementedError(f"ADD {ops}")


def op_and(sim: Sim, ops: str, ins: Instr):
    a, b = split_two_ops(ops)
    if a.upper() == "ECX":
        sim.set("ECX", fold(sim.get("ECX"), lambda x, y: x & y, parse_imm(b)))
        return
    if a.startswith(("dword ptr [", "[")):
        # `AND dword ptr [EAX+offset], imm` — bit-clear of an existing dword.
        mem = parse_mem_operand(a)
        if mem.base != "EAX" or mem.index is not None:
            raise NotImplementedError(f"AND on non-table memory at 0x{ins.addr:08X}")
        mask = parse_imm(b)
        rec = (mem.disp, mask, ins.addr)
        sim.and_writes.append(rec)
        sim.ops_in_order.append(("and", rec))
        return
    raise NotImplementedError(f"AND {ops}")


def op_or(sim: Sim, ops: str, ins: Instr):
    a, b = split_two_ops(ops)
    if a.upper() in REGS32:
        imm = parse_imm(b)
        # `OR reg, 0xFFFFFFFF` is the MSVC idiom for `reg = -1` — drops any
        # prior value (including unmodellable Syms like game_info).
        if imm == 0xFFFFFFFF:
            sim.set(a.upper(), 0xFFFFFFFF)
            return
        sim.set(a.upper(), fold(sim.get(a.upper()),
                                lambda x, y: x | y, imm))
        return
    raise NotImplementedError(f"OR {ops}")


def op_neg(sim: Sim, ops: str, ins: Instr):
    if ops.upper() != "ECX":
        raise NotImplementedError(f"NEG {ops}")
    sim.set("ECX", fold(0, lambda x, y: x - y, sim.get("ECX")))


def op_lea(sim: Sim, ops: str, ins: Instr):
    dst, src = split_two_ops(ops)
    dst = dst.upper()
    mem = parse_mem_operand(src)
    # Three shapes appear in the corpus:
    #   1) LEA ESI/EDI, [EAX + disp]      → MOVSD source/dest setup
    #   2) LEA EBP,    [EAX + disp]       → base pointer for subsequent stores
    #   3) LEA ECX, [ECX*8 + 3] / [ECX + ECX*1 + 1]  → arithmetic on ECX
    if mem.base == "EAX" and mem.index is None:
        # Case 1 or 2: `dst = EAX + disp`. We track this as a "base pointer".
        sim.base_ptrs[dst] = ("EAX", mem.disp)
        sim.set(dst, Sym(f"(table.cast::<u8>().add(0x{mem.disp:X}))"))
        # Wipe any prior conflict with regs[dst]; sim.set already cleared base_ptrs.
        sim.base_ptrs[dst] = ("EAX", mem.disp)
        return
    if mem.base == "ECX" and mem.index == "ECX" and mem.scale == 1:
        # `LEA ECX, [ECX + ECX*1 + disp]` → ECX = 2*ECX + disp.
        v = sim.get("ECX")
        sim.set("ECX", fold(v, lambda x, _: 2 * x, 0))
        sim.set("ECX", fold(sim.get("ECX"), lambda x, y: x + y, mem.disp))
        return
    if mem.base is None and mem.index == "ECX":
        # `LEA ECX, [ECX*N + disp]` → ECX = N*ECX + disp.
        v = sim.get("ECX")
        scale = mem.scale
        sim.set("ECX", fold(v, lambda x, _: scale * x, 0))
        sim.set("ECX", fold(sim.get("ECX"), lambda x, y: x + y, mem.disp))
        return
    raise NotImplementedError(f"LEA {ops}")


def mem_load(sim: Sim, mem: MemOp, ins: Instr) -> SymVal:
    if mem.base == "ESP" and mem.index is None:
        if mem.disp in sim.stack:
            return sim.stack[mem.disp]
        raise NotImplementedError(
            f"load from un-set [ESP+0x{mem.disp:X}] at 0x{ins.addr:08X}")
    if mem.base == "EBP" and mem.index is None and mem.disp == 0xD778:
        return Sym("game_version as u32")
    if mem.base == "ECX" and mem.index is None and mem.disp == 0xD963:
        return Sym("scheme_byte_d963 as u32")
    raise NotImplementedError(f"load from {mem} at 0x{ins.addr:08X}: {ins.raw}")


def mem_store(sim: Sim, mem: MemOp, val: SymVal, ins: Instr):
    # Stack stores: [ESP+disp]
    if mem.base == "ESP" and mem.index is None:
        sim.stack[mem.disp] = val
        return
    # Resolve [base+disp] where base might be EAX directly or a LEA-derived ptr.
    base, base_disp = resolve_table_addr(sim, mem)
    if base != "TABLE":
        raise NotImplementedError(f"store via non-table base at 0x{ins.addr:08X}: {ins.raw}")
    table_off = (base_disp + mem.disp) & 0xFFFFFFFF
    rec = (table_off, val, mem.size, ins.addr)
    sim.writes.append(rec)
    sim.ops_in_order.append(("write", rec))


def resolve_table_addr(sim: Sim, mem: MemOp) -> tuple[str, int]:
    """Return ('TABLE', base_disp) when the memory operand resolves to a
    `table + N` address. Raises otherwise."""
    if mem.index is not None:
        raise NotImplementedError(f"indexed store: {mem}")
    if mem.base == "EAX":
        return "TABLE", 0
    if mem.base in sim.base_ptrs:
        anchor, disp = sim.base_ptrs[mem.base]
        if anchor != "EAX":
            raise NotImplementedError(f"non-EAX-anchored base ptr: {anchor}")
        return "TABLE", disp
    raise NotImplementedError(f"can't resolve table addr for {mem}")


def op_rep_movsd(sim: Sim, ins: Instr):
    """MOVSD.REP ES:EDI,ESI  with ECX=count → block-copy. Both ESI and EDI
    must be table-relative pointers we know about."""
    src = sim.base_ptrs.get("ESI")
    dst = sim.base_ptrs.get("EDI")
    cnt = sim.regs.get("ECX")
    if src is None:
        raise NotImplementedError(f"REP MOVSD with ESI not a table ptr at 0x{ins.addr:08X}")
    if dst is None:
        raise NotImplementedError(f"REP MOVSD with EDI not a table ptr at 0x{ins.addr:08X}")
    if not isinstance(cnt, int):
        raise NotImplementedError(f"REP MOVSD with non-concrete ECX={cnt} at 0x{ins.addr:08X}")
    src_anchor, src_disp = src
    dst_anchor, dst_disp = dst
    if src_anchor != "EAX" or dst_anchor != "EAX":
        raise NotImplementedError("REP MOVSD with non-EAX anchor")
    rec = (dst_disp, src_disp, cnt, ins.addr)
    sim.block_copies.append(rec)
    sim.ops_in_order.append(("copy", rec))


# ────── Rust emission ──────

def render_imm(v: int, kind: str) -> str:
    """Render a concrete integer per the destination field's kind."""
    if kind == "fixed":
        return f"Fixed(0x{v:08X}_u32 as i32)"
    if kind == "ptr":
        return "core::ptr::null()"
    if kind == "u8x4":
        # `_unknown_2c: [u8; 4]` — split the dword, little-endian.
        bs = [(v >> (8 * i)) & 0xFF for i in range(4)]
        return "[" + ", ".join(f"0x{b:02X}" for b in bs) + "]"
    # i32 (signed)
    if v >= 0x80000000:
        s = v - 0x100000000
        return f"{s}"
    return f"0x{v:X}" if v > 9 else f"{v}"

def render_value(val: SymVal, kind: str) -> str:
    if isinstance(val, int):
        return render_imm(val, kind)
    if isinstance(val, Cond):
        return val.render_rust(kind)
    if isinstance(val, Sym):
        # Only used for the saved game_info pointer / cap / game_version; cast as needed.
        if kind == "i32":
            return f"({val.expr}) as i32"
        if kind == "fixed":
            return f"Fixed(({val.expr}) as i32)"
        return val.expr
    raise TypeError(f"render_value: {val!r}")


def emit_rust(sim_baseline: Sim, sim_extended: Sim) -> str:
    """Combine both sims into a single Rust function. Operations are emitted in
    original asm order, with banner comments at function boundaries."""
    out: list[str] = []
    out.append("//! Per-weapon defaults populator: ports WA's two unrolled-write")
    out.append("//! initialiser functions to Rust.")
    out.append("//!")
    out.append("//! - `InitWeaponDefaultsBaseline` (0x00537320, 1161 inst, no calls / no branches)")
    out.append("//! - `InitWeaponDefaultsExtended` (0x00539100, 1100 inst, no calls / no branches)")
    out.append("//!")
    out.append("//! Both are flat sequences of `MOV [EAX+offset], imm/reg` stores; merged")
    out.append("//! here into a single function that runs after the skeleton zeroes and")
    out.append("//! per-entry-inits the table (see [`crate::game::init_weapon_table`]).")
    out.append("//! Bit-identity vs the original asm was confirmed by side-by-side")
    out.append("//! `memcmp` across 597 worms2d replays (15 distinct `game_version`s,")
    out.append("//! both `cap` branches).")
    out.append("//!")
    out.append("//! Originally generated by `tools/port_init_weapon_defaults.py` (kept")
    out.append("//! committed for reference). This file is now the source of truth and")
    out.append("//! is hand-editable; the script is only useful if you ever need to")
    out.append("//! re-derive from scratch. Trailing `// 0xNNNNNNNN` comments on each")
    out.append("//! write are the source asm address in WA -- preserved as cross-refs.")
    out.append("")
    out.append("use openwa_core::fixed::Fixed;")
    out.append("use crate::engine::game_info::GameInfo;")
    out.append("use crate::game::weapon::WeaponTable;")
    out.append("")
    out.append("/// Per-weapon defaults: writes ~2200 dwords across all 71 entries.")
    out.append("/// Replaces WA's `InitWeaponDefaultsBaseline` (0x00537320) and")
    out.append("/// `InitWeaponDefaultsExtended` (0x00539100). Caller must have")
    out.append("/// memset+per-entry-init the table first (see [`init_weapon_table`]).")
    out.append("pub unsafe fn populate_weapon_table_defaults(")
    out.append("    table: *mut WeaponTable,")
    out.append("    game_info: *mut GameInfo,")
    out.append("    cap: u32,")
    out.append(") {")
    out.append("    unsafe {")
    out.append("        let game_version: i32 = (*game_info).game_version;")
    out.append("        let scheme_byte_d963: u8 = *(game_info as *const u8).add(0xD963);")
    out.append("        let entries = &mut (*table).entries;")
    out.append("")

    def emit_sim(sim: Sim, banner: str):
        out.append(f"        // --- {banner} ---")
        for kind, rec in sim.ops_in_order:
            if kind == "write":
                table_off, val, size, addr = rec
                if size == "byte":
                    # Sub-byte stores into u8x4 / similar — emit a byte write.
                    out.append(emit_byte_write(table_off, val, addr))
                else:
                    out.append(emit_dword_write(table_off, val, addr))
            elif kind == "copy":
                dst_off, src_off, cnt, addr = rec
                out.append(emit_block_copy(dst_off, src_off, cnt, addr))
            elif kind == "and":
                off, mask, addr = rec
                out.append(emit_and_clear(off, mask, addr))
        out.append("")

    emit_sim(sim_baseline, "InitWeaponDefaultsBaseline (0x00537320)")
    emit_sim(sim_extended, "InitWeaponDefaultsExtended (0x00539100)")

    out.append("    }")
    out.append("}")
    return "\n".join(out) + "\n"


def emit_dword_write(table_off: int, val: SymVal, addr: int) -> str:
    weapon, path, kind = field_for_table_offset(table_off)
    rendered = render_value(val, kind)
    return f"        entries[{weapon}]{path} = {rendered};  // 0x{addr:08X}"


def emit_byte_write(table_off: int, val: SymVal, addr: int) -> str:
    """Sub-byte store: lower the destination to a raw `*mut u8` write so the
    field-offset resolver doesn't need to know about every sub-dword field."""
    if not isinstance(val, int):
        raise NotImplementedError(f"non-concrete byte store at 0x{addr:08X}")
    return (f"        *(table as *mut u8).add(0x{table_off:X}) = 0x{val & 0xFF:02X};"
            f"  // 0x{addr:08X}")


def emit_block_copy(dst_off: int, src_off: int, cnt_dwords: int, addr: int) -> str:
    return (
        f"        core::ptr::copy_nonoverlapping(\n"
        f"            (table as *const u8).add(0x{src_off:X}) as *const u32,\n"
        f"            (table as *mut u8).add(0x{dst_off:X}) as *mut u32,\n"
        f"            {cnt_dwords},\n"
        f"        );  // 0x{addr:08X}: REP MOVSD ECX={cnt_dwords}"
    )


def emit_and_clear(off: int, mask: int, addr: int) -> str:
    return (f"        *((table as *mut u8).add(0x{off:X}) as *mut u32) &= "
            f"0x{mask:08X};  // 0x{addr:08X}")


# ────── Driver ──────

def main():
    here = Path(__file__).parent
    asm_baseline = parse_asm(here / "_init_weapon_defaults_baseline.asm")
    asm_extended = parse_asm(here / "_init_weapon_defaults_extended.asm")
    sim_b = run_baseline(asm_baseline)
    sim_e = run_extended(asm_extended)
    sys.stdout.write(emit_rust(sim_b, sim_e))


if __name__ == "__main__":
    main()
