use super::base::BaseEntity;
use super::game_task::WorldEntity;
use crate::FieldRegistry;
use crate::game::weapon::WeaponSpawnData;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "MissileEntity" {
        ctor MISSILE_ENTITY_CTOR = 0x00507D10;
        /// OnContact — vtable slot 8. Missile-type dispatched contact-impact handler.
        vmethod MISSILE_ENTITY_ON_CONTACT = 0x00508C90;
    }
}

/// MissileEntity vtable — 20 slots. Extends WorldEntity vtable with missile behavior.
///
/// Vtable at Ghidra 0x664438.
///
/// Slot layout notes:
/// - Slots 0–6 inherit from BaseEntity/WorldEntity (slot 6 `process_frame` is the generic
///   children dispatcher `BaseEntity::vt6_ProcessFrame` at 0x00563000; MissileEntity does
///   not override it).
/// - Slot 7 thunks to `WorldEntity::vt7` (0x004FF720) — 2-body elastic collision
///   resolver. Inherited.
/// - Slot 8 is MissileEntity-specific: [`on_contact`](MissileEntityVtable::on_contact).
#[openwa_game::vtable(size = 20, va = 0x00664438, class = "MissileEntity")]
pub struct MissileEntityVtable {
    /// HandleMessage — processes missile messages.
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message: fn(
        this: *mut MissileEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// OnContact — invoked when this missile contacts another entity (terrain,
    /// worm, object). Dispatches by [`missile_type`](MissileEntity::missile_type)
    /// (Standard/Homing/Sheep/Cluster). Calls
    /// `PlayImpactSound_Maybe` + `WorldEntity::vt8` (base OnContact) + conditionally
    /// `CreateExplosion`, `ImpactSpecialFx_Maybe`, and self.slot14 terminator.
    /// thiscall + 2 stack params (other, self_side_flags), RET 0x8. Returns 1 in EAX.
    #[slot(8)]
    pub on_contact:
        fn(this: *mut MissileEntity, other: *mut BaseEntity, self_side_flags: u32) -> u32,
    /// SetTerminateFlag — writes `flag` to `WorldEntity+0x44`. Generic WorldEntity
    /// subclass terminator shared across task types (inherited slot, not a
    /// MissileEntity override). Thiscall(this, flag), RET 0x4.
    /// Target: `WorldEntity::SetTerminateFlag_Maybe` at 0x004FE060.
    #[slot(14)]
    pub set_terminate_flag: fn(this: *mut MissileEntity, flag: u32),
}

/// Projectile / missile entity task.
///
/// Extends WorldEntity (0xFC bytes). One instance per airborne projectile
/// (rockets, grenades, mortar shells, homing missiles, sheep, etc.).
///
/// Inheritance: BaseEntity → WorldEntity → MissileEntity. class_type = 0x0B (11).
/// Constructor: `MissileEntity__Constructor` (0x507D10, stdcall, 4 params).
/// Vtable: `MissileEntity__vtable` (0x00664438).
///
/// Constructor params:
///   param_1 = this
///   param_2 = parent task pointer (passed to WorldEntity ctor)
///   param_3 = scheme weapon data (94 DWORDs from WGT blob)
///   param_4 = spawn data (11 DWORDs: position, velocity, owner, pellet index)
///
/// Source: Ghidra decompilation of 0x507D10 (constructor) and
///         wkJellyWorm MissileEntity.h (field layout reference).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MissileEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94).
    pub base: WorldEntity<*const MissileEntityVtable>,

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
    /// 0x12C: Object pool slot index (assigned from GameWorld+0x3600 pool).
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
    ///   [0x0F] gravity_pct  — 100 → gravity_factor = 1.0  (→ WorldEntity+0x58 via render_data)
    ///   [0x10] bounce_pct   — 100 → bounce_factor  = 1.0  (→ WorldEntity+0x5C via render_data)
    ///   [0x12] friction_pct — 100 → friction_factor = 1.0 (→ WorldEntity+0x60 via render_data)
    ///   [0x14] = 9000
    ///   [0x1A] missile_type — discriminator (2=Standard, 3=Homing, 4=Sheep, 5=Cluster)
    ///   [0x1B] = 4194304 → Fixed16.16 = 64.0
    ///   [0x1C] render_timer — 1 for bazooka, 30 for grenade (3s fuse @ 10fps)
    pub weapon_data: [u32; 0x5E],

    // ---- 0x2D4–0x37B: render/physics parameters (42 DWORDs) ----
    //
    // This region is a shifted copy of weapon_data:
    //   if spawn_params[8] == 0 (single shot):   [N] = weapon_data[N+3]
    //   if spawn_params[8]  > 0 (cluster pellet): [N] = weapon_data[N+52]
    //
    // The constructor copies 42 DWORDs from the appropriate source range; dynamic
    // physics values update some entries during flight. Each entry listed below is
    // named by its semantic role (deduced from MissileEntity::OnContact + constructor
    // analysis). Untouched entries remain in padding arrays.
    /// 0x2D4 — render_data[0]. Contact-face mask tested against `other->+0x30`
    /// (the face of the contacted entity). If `(1 << other_face) & mask != 0`,
    /// the sheep bailout path fires / the contact is rejected.
    pub contact_face_mask: u32,
    /// 0x2D8..0x2EB — render_data[1..6] (untouched by known code paths).
    pub _render_data_01_05: [u32; 5],
    /// 0x2EC — render_data[6]. Compared against `0x40` before `ImpactSpecialFx`
    /// (fire-particle spawn). Exact role still unclear — possibly a weapon-fx flag
    /// or a cluster-origin marker.
    pub fire_particle_trigger: u32,
    /// 0x2F0..0x32F — render_data[7..0x16] (untouched by known code paths).
    ///
    /// Constructor-known uses in this range:
    ///   [0x0C] gravity_pct  → (value << 16) / 100 → WorldEntity+0x58 (gravity_factor)
    ///   [0x0D] bounce_pct   → (value << 16) / 100 → WorldEntity+0x5C (bounce_factor)
    ///   [0x0F] friction_pct → (value << 16) / 100 → WorldEntity+0x60 (friction_factor)
    ///   [0x11] = 9000 for bazooka (→ also copied to post-render field at 0x37C)
    pub _render_data_07_16: [u32; 16],
    /// 0x330 — render_data[0x17]. Missile type discriminator
    /// (see [`MissileType`]). 2=Standard, 3=Homing, 4=Sheep, 5=Cluster.
    ///
    /// SAFETY: stored as the typed `#[repr(u32)]` enum. The constructor and
    /// scheme-data loaders are only ever observed to write 1..=5; if WA's
    /// memory ever holds a raw value outside this range, reading this field
    /// is UB. Guard by construction via the scheme validator if that ever
    /// becomes a concern.
    pub missile_type: MissileType,
    /// 0x334..0x33F — render_data[0x18..0x1A] (untouched by known code paths).
    ///
    /// Constructor-known uses:
    ///   [0x18] = 4194304 → Fixed16.16 = 64.0 (sprite/render size)
    ///   [0x19] render_timer — 1 for bazooka, 30 for grenade (3s fuse timer)
    pub _render_data_18_1a: [u32; 3],
    /// 0x340 — render_data[0x1B]. Sound ID played on impact (via
    /// `PlayImpactSound_Maybe` at 0x004FF020). Passed as first arg alongside the
    /// half-speed magnitude scaled by 0.4.
    pub impact_sound_id: u32,
    /// 0x344 — render_data[0x1C]. Ricochet-eligible side mask tested against the
    /// incoming `self_side_flags` arg. If `(1 << self_side) & mask != 0` and type is
    /// Standard/Cluster, the ricochet countdown branch fires.
    pub ricochet_side_mask: u32,
    /// 0x348 — render_data[0x1D]. Ricochet-roll threshold (0..100).
    /// On ricochet-eligible contact with counter remaining,
    /// `(AdvanceGameRNG() & 0x3FF) % 100 < ricochet_chance_pct` → mirror X velocity.
    pub ricochet_chance_pct: u32,
    /// 0x34C — render_data[0x1E] (untouched by known code paths).
    pub _render_data_1e: u32,
    /// 0x350 — render_data[0x1F]. Explosion ID (passed as 2nd stack arg to
    /// `CreateExplosion` on Standard/Cluster contact).
    pub explosion_id: u32,
    /// 0x354 — render_data[0x20]. Explosion damage base value (implicit ESI arg
    /// to `FUN_00547CB0` damage-jitter helper). Nonzero gate for `CreateExplosion`.
    pub explosion_damage: u32,
    /// 0x358 — render_data[0x21]. Explosion damage scaling percentage (2nd stack
    /// arg to `FUN_00547CB0`, used as `(ESI * param) / 100` before RNG jitter).
    pub explosion_damage_pct: u32,
    /// 0x35C — render_data[0x22]. Ricochet-remaining counter. Decremented on each
    /// ricochet-eligible contact; when it reaches 0, the missile invokes the
    /// slot-14 terminator (`WorldEntity::SetTerminateFlag`) instead of bouncing.
    pub ricochet_counter: u32,
    /// 0x360..0x37B — render_data[0x23..0x29] (untouched by known code paths).
    /// `[0x29]` (= 0x37C offset equivalent inside post_render) is referenced in
    /// constructor comments as "updates during flight".
    pub _render_data_23_29: [u32; 7],

    // ---- 0x37C–0x41B: post-render physics and state ----
    /// 0x37C — remaining fuse timer (in frames) until detonation / sheep expiry.
    /// Initialised by the constructor from `render_data[0x11]` (= 9000 for bazooka,
    /// 30 for grenade @ 10fps = 3s). Counted down each frame by the physics update;
    /// when it reaches 0 the missile detonates / the sheep self-destructs.
    pub fuse_timer: i32,
    /// 0x380..0x393: Further post-render dynamic state (unknown).
    pub _post_render_state_0: [u8; 0x14],
    /// 0x394 — `param_1[0xE5]` in OnContact. Contact-phase flag. Value `2`
    /// disables the normal contact path (routes to the terminator / sheep bailout
    /// block). Probably set by HandleMessage when the missile has already been
    /// flagged for detonation or disarm.
    pub contact_phase: u32,
    /// 0x398: unused so far by OnContact. One dword of post-render state.
    pub _post_render_state_1: u32,
    /// 0x39C — speed-X stash. Written on the terminator / sheep-bailout exit
    /// path (`[+0x39C] = speed_x`). Consumers (cluster spawn, splatter effects)
    /// read this to access the missile's terminal velocity after it has been
    /// marked for destruction.
    pub terminate_stash_speed_x: Fixed,
    /// 0x3A0 — speed-Y stash (mirror of [`terminate_stash_speed_x`]).
    ///
    /// Earlier analysis guessed this was a launch-speed magnitude; that turned
    /// out to be a constructor-only initialisation that OnContact subsequently
    /// overwrites with current velocity. Observed as `0` for bazooka (which
    /// detonates on contact without reaching the terminator block).
    pub terminate_stash_speed_y: Fixed,
    /// 0x3A4: Unknown
    pub _unknown_3a4: u32,
    /// 0x3A8: Homing mode enabled flag (nonzero = active homing).
    /// param_1[0xEA] in constructor. Set to 1 when missile_type == 3 and conditions
    /// are met (target acquired).
    pub homing_enabled: u32,
    /// 0x3AC — sheep bailout re-arm countdown. Set to `0xA` on first sheep
    /// contact; bail setup skipped until it counts back down to 0 (decrement
    /// performed elsewhere — not in OnContact).
    pub sheep_bailout_counter: u32,
    /// 0x3B0 — sheep bailout lock. If nonzero on sheep contact, the bailout is
    /// rejected and the terminator runs immediately. Probably a one-shot latch.
    pub sheep_bailout_locked: u32,
    /// 0x3B4 — sheep action flag. Zeroed on first sheep bailout arm. Used by
    /// sheep-state logic elsewhere to know the bailout stash is live.
    pub sheep_action_flag: u32,
    /// 0x3B8 — sheep bailout X-position stash (pre-contact pos_x).
    pub sheep_stash_pos_x: Fixed,
    /// 0x3BC — sheep bailout Y-position stash (pre-contact pos_y).
    pub sheep_stash_pos_y: Fixed,
    /// 0x3C0 — sheep bailout X-velocity stash (pre-contact speed_x).
    pub sheep_stash_speed_x: Fixed,
    /// 0x3C4 — sheep bailout Y-velocity stash (pre-contact speed_y).
    pub sheep_stash_speed_y: Fixed,
    /// 0x3C8: Horizontal direction sign (+1 or -1, determines facing/travel dir).
    /// param_1[0xF2] in constructor; also rewritten in the homing contact branch
    /// based on `sign(speed_x)` after a RNG roll passes.
    pub direction: i32,
    /// 0x3CC–0x40B: Unknown trailing state.
    /// Allocation size is 0x40C; constructor zeros bytes 0x00–0x3EB.
    pub _unknown_3cc: [u8; 0x40],
}

const _: () = assert!(core::mem::size_of::<MissileEntity>() == 0x40C);

// Explicit offset sanity checks for fields touched by MissileEntity::OnContact.
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(MissileEntity, contact_face_mask) == 0x2D4);
    assert!(offset_of!(MissileEntity, fire_particle_trigger) == 0x2EC);
    assert!(offset_of!(MissileEntity, missile_type) == 0x330);
    assert!(offset_of!(MissileEntity, impact_sound_id) == 0x340);
    assert!(offset_of!(MissileEntity, ricochet_side_mask) == 0x344);
    assert!(offset_of!(MissileEntity, ricochet_chance_pct) == 0x348);
    assert!(offset_of!(MissileEntity, explosion_id) == 0x350);
    assert!(offset_of!(MissileEntity, explosion_damage) == 0x354);
    assert!(offset_of!(MissileEntity, explosion_damage_pct) == 0x358);
    assert!(offset_of!(MissileEntity, ricochet_counter) == 0x35C);
    assert!(offset_of!(MissileEntity, contact_phase) == 0x394);
    assert!(offset_of!(MissileEntity, terminate_stash_speed_x) == 0x39C);
    assert!(offset_of!(MissileEntity, terminate_stash_speed_y) == 0x3A0);
    assert!(offset_of!(MissileEntity, homing_enabled) == 0x3A8);
    assert!(offset_of!(MissileEntity, sheep_bailout_counter) == 0x3AC);
    assert!(offset_of!(MissileEntity, sheep_bailout_locked) == 0x3B0);
    assert!(offset_of!(MissileEntity, sheep_action_flag) == 0x3B4);
    assert!(offset_of!(MissileEntity, sheep_stash_pos_x) == 0x3B8);
    assert!(offset_of!(MissileEntity, sheep_stash_pos_y) == 0x3BC);
    assert!(offset_of!(MissileEntity, sheep_stash_speed_x) == 0x3C0);
    assert!(offset_of!(MissileEntity, sheep_stash_speed_y) == 0x3C4);
    assert!(offset_of!(MissileEntity, direction) == 0x3C8);
};

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_MissileEntityVtable!(MissileEntity, base.base.vtable);

impl MissileEntity {
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
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissileType {
    /// Never observed in the wild; included as a slot in case the scheme data
    /// ever emits it. No known constructor branch handles value 1.
    Unknown1 = 1,
    /// Standard trajectory projectile (bazooka, mortar, grenade, etc.).
    Standard = 2,
    /// Homing missile — tracks nearest worm.
    Homing = 3,
    /// Sheep / animal projectile — walks on terrain.
    Sheep = 4,
    /// Cluster projectile — spawns sub-pellets on detonation.
    Cluster = 5,
}

impl MissileType {
    /// Map a raw `render_data[0x17]` value to a typed [`MissileType`].
    /// Returns `None` for out-of-range values (0, 6+); callers should treat
    /// such cases as "no typed branch applies".
    pub const fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            1 => Self::Unknown1,
            2 => Self::Standard,
            3 => Self::Homing,
            4 => Self::Sheep,
            5 => Self::Cluster,
            _ => return None,
        })
    }
}

// ── Snapshot impl ──────────────────────────────────────────

impl crate::snapshot::Snapshot for MissileEntity {
    unsafe fn write_snapshot(
        &self,
        w: &mut dyn core::fmt::Write,
        indent: usize,
    ) -> core::fmt::Result {
        unsafe {
            use crate::snapshot::{write_indent, write_raw_region};
            let i = indent;
            let b = &self.base; // WorldEntity

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
            writeln!(
                w,
                "render: type={:?} contact_mask=0x{:08X} fx_trigger=0x{:X} snd={} ricochet(mask=0x{:08X} %={}, ctr={}) explosion(id={}, dmg={}, dmg%={})",
                self.missile_type,
                self.contact_face_mask,
                self.fire_particle_trigger,
                self.impact_sound_id,
                self.ricochet_side_mask,
                self.ricochet_chance_pct,
                self.ricochet_counter,
                self.explosion_id,
                self.explosion_damage,
                self.explosion_damage_pct,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "contact_phase = {} terminate_stash = ({}, {}) direction = {} homing = {}",
                self.contact_phase,
                self.terminate_stash_speed_x,
                self.terminate_stash_speed_y,
                self.direction,
                self.homing_enabled,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "sheep(counter={} locked={} action={} stash_pos=({}, {}) stash_speed=({}, {}))",
                self.sheep_bailout_counter,
                self.sheep_bailout_locked,
                self.sheep_action_flag,
                self.sheep_stash_pos_x,
                self.sheep_stash_pos_y,
                self.sheep_stash_speed_x,
                self.sheep_stash_speed_y,
            )?;

            // Unknown regions
            write_indent(w, i)?;
            writeln!(w, "_unknown_fc ({} bytes):", self._unknown_fc.len())?;
            write_raw_region(w, self._unknown_fc.as_ptr(), self._unknown_fc.len(), i + 1)?;
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
}
