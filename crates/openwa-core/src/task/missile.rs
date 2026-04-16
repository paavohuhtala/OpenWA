use super::base::CTask;
use super::game_task::CGameTask;
use crate::FieldRegistry;
use crate::fixed::Fixed;
use crate::game::weapon::WeaponSpawnData;

crate::define_addresses! {
    class "CTaskMissile" {
        /// CTaskMissile vtable - projectile entity
        vtable CTASK_MISSILE_VTABLE = 0x0066_4438;
        ctor CTASK_MISSILE_CTOR = 0x0050_7D10;
    }
}

/// CTaskMissile vtable — 12 slots. Extends CGameTask vtable with missile behavior.
///
/// Vtable at Ghidra 0x664438.
#[openwa_core::vtable(size = 12, va = 0x0066_4438, class = "CTaskMissile")]
pub struct CTaskMissileVTable {
    /// HandleMessage — processes missile messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message:
        fn(this: *mut CTaskMissile, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
    /// ProcessFrame — per-frame missile update (physics, homing, detonation).
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut CTaskMissile, flags: u32),
}

/// Projectile / missile entity task.
///
/// Extends CGameTask (0xFC bytes). One instance per airborne projectile
/// (rockets, grenades, mortar shells, homing missiles, sheep, etc.).
///
/// Inheritance: CTask → CGameTask → CTaskMissile. class_type = 0x0B (11).
/// Constructor: `CTaskMissile__Constructor` (0x507D10, stdcall, 4 params).
/// Vtable: `CTaskMissile__vtable` (0x00664438).
///
/// Constructor params:
///   param_1 = this
///   param_2 = parent task pointer (passed to CGameTask ctor)
///   param_3 = scheme weapon data (94 DWORDs from WGT blob)
///   param_4 = spawn data (11 DWORDs: position, velocity, owner, pellet index)
///
/// Source: Ghidra decompilation of 0x507D10 (constructor) and
///         wkJellyWorm CTaskMissile.h (field layout reference).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskMissile {
    /// 0x00–0xFB: CGameTask base (pos at 0x84/0x88, speed at 0x90/0x94).
    pub base: CGameTask<*const CTaskMissileVTable>,

    // ---- 0xFC–0x12F: missile init fields ----
    /// 0xFC–0x10F: Unknown missile flags and state
    pub _unknown_fc: [u8; 0x14],
    /// 0x110: Unknown
    pub _unknown_110: u32,
    /// 0x114: Unknown
    pub _unknown_114: u32,
    /// 0x118: Unknown — observed being set from scheme data at construction
    pub _unknown_118: u32,
    /// 0x11C: Unknown
    pub _unknown_11c: u32,
    /// 0x120: Unknown
    pub _unknown_120: u32,
    /// 0x124: Unknown
    pub _unknown_124: u32,
    /// 0x128: Position-derived launch seed. Computed by constructor as:
    /// `((spawn_x + spawn_y) / 256 / 20) + 0x10000`. param_1[0x4A].
    pub launch_seed: u32,
    /// 0x12C: Object pool slot index (assigned from DDGame+0x3600 pool).
    /// param_1[0x4B].
    pub slot_id: u32,

    // ---- 0x130–0x15B: spawn data (11 DWORDs, from param_4) ----
    /// 0x130–0x15B: Spawn parameters copied from constructor param_4.
    /// See `WeaponSpawnData` for field documentation.
    pub spawn_params: WeaponSpawnData,

    // ---- 0x15C–0x2D3: weapon/scheme data (94 DWORDs, from param_3) ----
    /// 0x15C–0x2D3: Weapon/scheme properties (94 DWORDs copied verbatim from param_3).
    ///
    /// The WGT scheme blob is split into two logical halves:
    ///   [0x00..0x34] primary projectile params (→ also mirrored in render_data)
    ///   [0x34..0x5E] cluster sub-pellet params (→ render_data when pellet_index > 0)
    ///
    /// Runtime-observed for bazooka:
    ///   [0x03] = 137342 → Fixed16.16 ≈ 2.10 (some radius/size)
    ///   [0x05] = 100, [0x06] = 50, [0x09] = 48
    ///   [0x0F] gravity_pct  — 100 → gravity_factor = 1.0  (→ CGameTask+0x58 via render_data)
    ///   [0x10] bounce_pct   — 100 → bounce_factor  = 1.0  (→ CGameTask+0x5C via render_data)
    ///   [0x12] friction_pct — 100 → friction_factor = 1.0 (→ CGameTask+0x60 via render_data)
    ///   [0x14] = 9000
    ///   [0x1A] missile_type — discriminator (2=Standard, 3=Homing, 4=Sheep, 5=Cluster)
    ///   [0x1B] = 4194304 → Fixed16.16 = 64.0
    ///   [0x1C] render_timer — 1 for bazooka, 30 for grenade (3s fuse @ 10fps)
    pub weapon_data: [u32; 0x5E],

    // ---- 0x2D4–0x37B: render/physics parameters (42 DWORDs) ----
    /// 0x2D4–0x37B: Per-projectile render and physics parameters (42 DWORDs).
    ///
    /// This is NOT a separate data block — it is a shifted copy of weapon_data:
    ///   if spawn_params[8] == 0 (single shot):   render_data[N] = weapon_data[N+3]
    ///   if spawn_params[8]  > 0 (cluster pellet): render_data[N] = weapon_data[N+52]
    ///
    /// The constructor copies 42 DWORDs from the appropriate source range
    /// into this region (param_1[0xB5..0xDE]). Dynamic physics values update
    /// some entries during flight (e.g. render_data[0x29] @ 0x37C).
    ///
    /// Key indices (relative to render_data, = weapon_data[N+3] for single shots):
    ///   [0x0C] gravity_pct  → (value << 16) / 100 → CGameTask+0x58 (gravity_factor)
    ///   [0x0D] bounce_pct   → (value << 16) / 100 → CGameTask+0x5C (bounce_factor)
    ///   [0x0F] friction_pct → (value << 16) / 100 → CGameTask+0x60 (friction_factor)
    ///   [0x11] = 9000 for bazooka (→ also copied to post-render field at 0x37C)
    ///   [0x17] missile_type — type discriminator (see MissileType)
    ///   [0x18] = 4194304 → Fixed16.16 = 64.0 (sprite/render size)
    ///   [0x19] render_timer — 1 for bazooka, 30 for grenade (3s fuse timer)
    pub render_data: [u32; 0x2A],

    // ---- 0x37C–0x41B: post-render physics and state ----
    /// 0x37C–0x39F: Post-render dynamic state. render_data[0x11] is copied to
    /// [0x37C] by the constructor; physics updates the values each frame.
    pub _unknown_37c: [u8; 0x24],
    /// 0x3A0: Fixed16.16 launch speed magnitude (computed from initial velocity).
    /// Observed as 0 for bazooka — may only be populated for specific types.
    pub launch_speed_raw: Fixed,
    /// 0x3A4: Unknown
    pub _unknown_3a4: u32,
    /// 0x3A8: Homing mode enabled flag (nonzero = active homing).
    /// param_1[0xEA] in constructor. Set to 1 when missile_type == 3 and conditions
    /// are met (target acquired).
    pub homing_enabled: u32,
    /// 0x3AC–0x3BF: Unknown homing state fields
    pub _unknown_3ac: [u8; 0x14],
    /// 0x3C0–0x3C7: Unknown
    pub _unknown_3c0: [u8; 8],
    /// 0x3C8: Horizontal direction sign (+1 or -1, determines facing/travel dir).
    /// param_1[0xF2] in constructor. Set to -1 for homing/sheep if initial_speed_x < 0.
    pub direction: i32,
    /// 0x3CC–0x40B: Unknown trailing state.
    /// Allocation size is 0x40C; constructor zeros bytes 0x00–0x3EB.
    pub _unknown_3cc: [u8; 0x40],
}

const _: () = assert!(core::mem::size_of::<CTaskMissile>() == 0x40C);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_CTaskMissileVTable!(CTaskMissile, base.base.vtable);

impl CTaskMissile {
    /// Missile type from `render_data[0x17]` (= weapon_data[0x1A] for single shots).
    ///
    /// Governs constructor and movement behaviour (homing, sheep walk,
    /// cluster splitting, etc.).
    pub fn missile_type(&self) -> MissileType {
        match self.render_data[0x17] {
            2 => MissileType::Standard,
            3 => MissileType::Homing,
            4 => MissileType::Sheep,
            5 => MissileType::Cluster,
            n => MissileType::Unknown(n),
        }
    }

    /// Spawn X as Fixed16.16.
    pub fn spawn_x(&self) -> Fixed {
        self.spawn_params.spawn_x
    }

    /// Spawn Y as Fixed16.16.
    pub fn spawn_y(&self) -> Fixed {
        self.spawn_params.spawn_y
    }

    /// Aim cursor X at time of fire, Fixed16.16.
    pub fn cursor_x(&self) -> Fixed {
        self.spawn_params.cursor_x
    }

    /// Aim cursor Y at time of fire, Fixed16.16.
    pub fn cursor_y(&self) -> Fixed {
        self.spawn_params.cursor_y
    }
}

/// Missile movement/behaviour type, encoded in `render_data[0x17]`.
///
/// The constructor switches on this value to set up physics, homing,
/// direction, and clustering behaviour. Corresponds to weapon_data[0x1A]
/// for single-shot projectiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissileType {
    /// Standard trajectory projectile (bazooka, mortar, grenade, etc.). Value = 2.
    Standard,
    /// Homing missile — tracks nearest worm. Value = 3.
    Homing,
    /// Sheep / animal projectile — walks on terrain. Value = 4.
    Sheep,
    /// Cluster projectile — spawns sub-pellets on detonation. Value = 5.
    Cluster,
    /// Unknown type code (value 1 never observed; any other unrecognised value).
    Unknown(u32),
}

// ── Snapshot impl ──────────────────────────────────────────

#[cfg(target_arch = "x86")]
impl crate::snapshot::Snapshot for CTaskMissile {
    unsafe fn write_snapshot(
        &self,
        w: &mut dyn core::fmt::Write,
        indent: usize,
    ) -> core::fmt::Result {
        use crate::snapshot::{write_indent, write_raw_region};
        let i = indent;
        let b = &self.base; // CGameTask

        write_indent(w, i)?;
        writeln!(w, "pos = ({}, {})", b.pos_x, b.pos_y)?;
        write_indent(w, i)?;
        writeln!(w, "speed = ({}, {})", b.speed_x, b.speed_y)?;
        write_indent(w, i)?;
        writeln!(w, "launch_seed = 0x{:08X}", self.launch_seed)?;
        write_indent(w, i)?;
        writeln!(w, "slot_id = {}", self.slot_id)?;

        let sp = &self.spawn_params;
        write_indent(w, i)?;
        writeln!(w, "spawn_params:")?;
        write_indent(w, i + 1)?;
        writeln!(
            w,
            "owner={} spawn=({}, {}) speed=({}, {})",
            sp.owner_id, sp.spawn_x, sp.spawn_y, sp.initial_speed_x, sp.initial_speed_y
        )?;
        write_indent(w, i + 1)?;
        writeln!(
            w,
            "cursor=({}, {}) pellet={} fallback=({}, {})",
            sp.cursor_x, sp.cursor_y, sp.pellet_index, sp.fallback_timer, sp.fallback_param
        )?;

        write_indent(w, i)?;
        write!(w, "weapon_data =")?;
        for (j, v) in self.weapon_data.iter().enumerate() {
            if j % 16 == 0 {
                writeln!(w)?;
                write_indent(w, i + 1)?;
            }
            write!(w, " {:08X}", v)?;
        }
        writeln!(w)?;

        write_indent(w, i)?;
        write!(w, "render_data =")?;
        for (j, v) in self.render_data.iter().enumerate() {
            if j % 16 == 0 {
                writeln!(w)?;
                write_indent(w, i + 1)?;
            }
            write!(w, " {:08X}", v)?;
        }
        writeln!(w)?;

        write_indent(w, i)?;
        writeln!(w, "launch_speed_raw = {}", self.launch_speed_raw)?;
        write_indent(w, i)?;
        writeln!(w, "homing_enabled = {}", self.homing_enabled)?;
        write_indent(w, i)?;
        writeln!(w, "direction = {}", self.direction)?;

        // Unknown regions
        write_indent(w, i)?;
        writeln!(w, "_unknown_fc ({} bytes):", self._unknown_fc.len())?;
        write_raw_region(w, self._unknown_fc.as_ptr(), self._unknown_fc.len(), i + 1)?;
        write_indent(w, i)?;
        writeln!(w, "_unknown_37c ({} bytes):", self._unknown_37c.len())?;
        write_raw_region(
            w,
            self._unknown_37c.as_ptr(),
            self._unknown_37c.len(),
            i + 1,
        )?;
        write_indent(w, i)?;
        writeln!(w, "_unknown_3cc ({} bytes):", self._unknown_3cc.len())?;
        write_raw_region(
            w,
            self._unknown_3cc.as_ptr(),
            self._unknown_3cc.len(),
            i + 1,
        )?;

        Ok(())
    }
}
