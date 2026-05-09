use super::base::BaseEntity;
use super::game_entity::{SubclassData, WorldEntity};
use crate::FieldRegistry;
use crate::game::weapon::WeaponSpawnData;
use openwa_core::fixed::Fixed;
use openwa_core::vec2::Vec2;

pub mod frame_finish;
pub mod free;
pub mod handle_message;
pub mod render;
pub mod sound;

/// MissileEntity's typed view of [`WorldEntity::subclass_data`]
/// (entity offsets +0x38..+0x84, 0x4C bytes total).
///
/// Touched by [`missile_on_contact`](crate::game::missile_contact::missile_on_contact)
/// and the generic terminator dispatch (slot 14, which writes `terminate_flag`).
#[repr(C)]
pub struct MissileSubclassData {
    /// Entity +0x38: Unknown.
    pub _unknown_38: u32,
    /// Entity +0x3C: Action flag — cleared by the ricochet-exhausted /
    /// terminator-bailout paths in `MissileEntity::OnContact`. Purpose
    /// otherwise opaque.
    pub action_flag: u32,
    /// Entity +0x40: Unknown.
    pub _unknown_40: u32,
    /// Entity +0x44: Terminate flag. Written only by vtable slot 14
    /// (`SetTerminateFlag` at 0x004FE060) — `OnContact` dispatches there
    /// rather than touching the slot directly.
    pub terminate_flag: u32,
    /// Entity +0x48: Sheep state flag. Set to 1 by the sheep pre-bailout
    /// stash branch in `OnContact` (alongside saving stash position /
    /// speed and arming the bailout counter).
    pub sheep_state_flag: u32,
    /// Entity +0x4C..+0x84: Unknown.
    pub _unknown_4c: [u8; 0x38],
}

const _: () = assert!(core::mem::size_of::<MissileSubclassData>() == 0x4C);

unsafe impl SubclassData for MissileSubclassData {}

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
    /// `MissileEntity::Free` (0x00508330) — scalar deleting destructor.
    /// Calls the inlined `dtor1` (0x005086F0) and, when bit 0 of `flags`
    /// is set, frees the heap allocation. Thiscall(this, flags), RET 0x4.
    /// Returns `this` in EAX.
    #[slot(1)]
    pub free: fn(this: *mut MissileEntity, flags: u8) -> *mut MissileEntity,
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
    /// `PlayImpactSound` + `WorldEntity::vt8` (base OnContact) + conditionally
    /// `CreateExplosion`, `ImpactSpecialFx_Maybe`, and self.slot14 terminator.
    /// thiscall + 2 stack params (other, self_side_flags), RET 0x8. Returns 1 in EAX.
    #[slot(8)]
    pub on_contact:
        fn(this: *mut MissileEntity, other: *mut BaseEntity, self_side_flags: u32) -> u32,
    /// SetTerminateFlag — writes `flag` to `WorldEntity+0x44`. Generic WorldEntity
    /// subclass terminator shared across entity types (inherited slot, not a
    /// MissileEntity override). Thiscall(this, flag), RET 0x4.
    /// Target: `WorldEntity::SetTerminateFlag_Maybe` at 0x004FE060.
    #[slot(14)]
    pub set_terminate_flag: fn(this: *mut MissileEntity, flag: u32),
    /// `WorldEntity::vt17` (0x00500090) — generic mass-scaled impulse adder.
    /// Bails when `terminate_flag` (`+0x48`) is non-zero. Otherwise applies
    /// `(dx, dy) / mass` (Fixed16.16) to `speed_x`/`speed_y`, and adds
    /// `mode` to the third axis at `+0x98`. Returns `1` on success / `0` if
    /// the bail fired. Inherited slot; the FrameFinish tick uses it to fold
    /// per-axis wind/sway into the running velocity.
    /// Thiscall(this, dx, dy, mode), RET 0xC.
    #[slot(17)]
    pub apply_impulse: fn(this: *mut MissileEntity, dx: i32, dy: i32, mode: i32) -> u32,
}

/// Projectile / missile entity entity.
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
///   param_2 = parent entity pointer (passed to WorldEntity ctor)
///   param_3 = scheme weapon data (94 DWORDs from WGT blob)
///   param_4 = spawn data (11 DWORDs: position, velocity, owner, pellet index)
///
/// Source: Ghidra decompilation of 0x507D10 (constructor) and
///         wkJellyWorm MissileEntity.h (field layout reference).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MissileEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94).
    /// Subclass-data overlay typed as [`MissileSubclassData`].
    pub base: WorldEntity<*const MissileEntityVtable, MissileSubclassData>,

    // ---- 0xFC–0x12F: missile init fields ----
    /// 0xFC–0xFF: Unknown.
    pub _unknown_fc: [u8; 0x4],
    /// 0x100: Cluster-pellet flag — non-zero when this missile was spawned
    /// as the Nth sub-pellet of a cluster volley (`spawn_params.pellet_index
    /// > 0`), zero for single-shot projectiles. The HandleMessage discriminator
    /// view at the top of the function (`piVar8`) selects between
    /// [`_render_data_07`] (single-shot) and [`_render_data_1a`] (cluster) on
    /// the value of this flag.
    pub is_cluster_pellet: u32,
    /// 0x104–0x10B: Unknown.
    pub _unknown_104: [u8; 8],
    /// 0x10C: Splash-sound one-shot latch read by the FrameFinish tick. Set
    /// to `1` after firing the underwater-stash impact sound (sound id `0x39`
    /// channel `5`) once `|speed_y|` crosses `0x10000`; cleared each frame
    /// the missile is above water (`_field_a4 == 0`).
    pub splash_sound_latched: u32,
    /// 0x110: Underwater-entry one-shot latch. Set to `1` the first frame
    /// the missile crosses `_field_b0 != 0`; the FrameFinish tail uses this
    /// to gate the once-per-life "stop fuse sound + clear detonate response
    /// + arm the underwater bucket mask" cleanup.
    pub underwater_entry_latched: u32,
    /// 0x114: Particle-emit phase accumulator (Fixed 16.16). Each FrameFinish
    /// tick adds `(piVar8[1] << 16) / 0x19` (above-water in render band) or
    /// `(piVar8[3] << 16) / 200` (otherwise) to this slot, and consumes
    /// `Fixed::ONE` per spawned `SpawnEffect` particle or
    /// `GameTask::create_bubble_1` bubble.
    pub effect_emit_phase: Fixed,
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
    /// 0x12C: This missile's slot ID in `GameWorld.entity_activity_queue`
    /// (param_1[0x4B]).
    pub activity_rank_slot: u32,

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
    /// 0x2D4 — render_data[0]. Contact-face mask tested against
    /// `other.contact_face` (the face index of the contacted entity). If
    /// `(1 << other_face) & mask != 0`, the sheep bailout path fires /
    /// the contact is rejected.
    pub contact_face_mask: u32,
    /// 0x2D8..0x2EB — render_data[1..6] (untouched by known code paths).
    pub _render_data_01_05: [u32; 5],
    /// 0x2EC — render_data[6]. Compared against `0x40` before `ImpactSpecialFx`
    /// (fire-particle spawn). Exact role still unclear — possibly a weapon-fx flag
    /// or a cluster-origin marker.
    pub fire_particle_trigger: u32,
    /// 0x2F0 — render_data[0x07]. Animation-rate kind discriminator
    /// for single-shot missiles; HandleMessage's `piVar8[2]` view reads
    /// this slot when [`is_cluster_pellet`](MissileEntity::is_cluster_pellet)
    /// is zero. Observed values 3..=6 select different animation-phase
    /// step formulas (case 2) and gate the speed-driven anim-phase update
    /// (case 5). For cluster pellets the equivalent slot is
    /// [`_render_data_1a`](MissileEntity::_render_data_1a) at 0x33C.
    pub _render_data_07: u32,
    /// 0x2F4..0x307 — render_data[0x08..0x0D] (untouched by known code paths).
    ///
    /// Constructor-known uses in this range:
    ///   [0x0C] gravity_pct  → (value << 16) / 100 → WorldEntity+0x58 (gravity_factor)
    pub _render_data_08_0c: [u32; 5],
    /// 0x308 — render_data[0x0D]. Inbound-explosion gate read by
    /// HandleMessage case 0x1C (Explosion). When non-zero, the explosion
    /// message is forwarded to the parent `WorldEntity::HandleMessage` so
    /// it can apply physics impulse / damage to this missile; when zero,
    /// the explosion is silently dropped.
    pub explosion_response_flag: u32,
    /// 0x30C..0x313 — render_data[0x0E..0x10] (mostly untouched by known
    /// code paths).
    ///
    /// Constructor-known uses in this range:
    ///   [0x0D] bounce_pct   → (value << 16) / 100 → WorldEntity+0x5C (bounce_factor)
    ///   [0x0F] friction_pct → (value << 16) / 100 → WorldEntity+0x60 (friction_factor)
    pub _render_data_0e_0f: [u32; 2],
    /// 0x314 — render_data[0x10]. Fuse-timer threshold below which the
    /// per-missile countdown textbox becomes visible during normal
    /// gameplay. The textbox is hidden when [`fuse_timer`] is `>=` this
    /// value (replay-mode rendering ignores the gate and shows the
    /// textbox for the whole fuse duration).
    ///
    /// [`fuse_timer`]: MissileEntity::fuse_timer
    pub textbox_visible_threshold: i32,
    /// 0x318 — render_data[0x11]. Initial fuse-timer value (= 9000 for
    /// bazooka, copied into [`fuse_timer`] at 0x37C by the constructor).
    /// Render reads it as the divisor for the case-0 animation-phase
    /// ramp `0x10000 - (fuse_timer << 16) / fuse_timer_initial`.
    ///
    /// [`fuse_timer`]: MissileEntity::fuse_timer
    pub fuse_timer_initial: i32,
    /// 0x31C..0x32B — render_data[0x12..0x15] (untouched by known code paths).
    pub _render_data_12_15: [u32; 4],
    /// 0x32C — render_data[0x16]. DetonateWeapon (msg 0x2C) response
    /// mode — controls how this missile reacts to the detonate-key /
    /// scheme broadcast:
    ///
    /// - `0` — ignore (default).
    /// - `1` — invoke vtable[14] terminator. Flag is `2` when
    ///   `weapon_data[0x2D] == 3`, otherwise `1`.
    /// - `2` — randomise the fuse: `fuse_timer = (rng & 0xFFFF) % 500`
    ///   and zero this slot together with [`textbox_visible_threshold`]
    ///   (so the new countdown textbox stays hidden until something else
    ///   re-arms it).
    ///
    /// [`textbox_visible_threshold`]: MissileEntity::textbox_visible_threshold
    pub detonate_response_mode: u32,
    /// 0x330 — render_data[0x17]. Missile type discriminator
    /// (see [`MissileType`]). 2=Standard, 3=Homing, 4=Sheep, 5=Cluster.
    ///
    /// SAFETY: stored as the typed `#[repr(u32)]` enum. The constructor and
    /// scheme-data loaders are only ever observed to write 1..=5; if WA's
    /// memory ever holds a raw value outside this range, reading this field
    /// is UB. Guard by construction via the scheme validator if that ever
    /// becomes a concern.
    pub missile_type: MissileType,
    /// 0x334 — render_data[0x18]. Sprite/render size in Fixed 16.16
    /// (4194304 = 64.0 for bazooka).
    pub sprite_size: Fixed,
    /// 0x338 — render_data[0x19]. Initial render/fuse timer in frames
    /// (1 for bazooka, 30 for grenade @ 10fps = 3s).
    pub render_timer: i32,
    /// 0x33C — render_data[0x1A]. Animation-rate kind discriminator for
    /// cluster-pellet missiles; HandleMessage's `piVar8[2]` view reads
    /// this slot when [`is_cluster_pellet`](MissileEntity::is_cluster_pellet)
    /// is non-zero. Same value-set semantics as
    /// [`_render_data_07`](MissileEntity::_render_data_07) (the single-shot
    /// counterpart at 0x2F0).
    pub _render_data_1a: u32,
    /// 0x340 — render_data[0x1B]. Sound ID played on impact (via
    /// `PlayImpactSound` at 0x004FF020). Passed as first arg alongside the
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
    /// to `GameTask__calc_damage` damage-jitter helper). Nonzero gate for `CreateExplosion`.
    pub explosion_damage: u32,
    /// 0x358 — render_data[0x21]. Explosion damage scaling percentage (2nd stack
    /// arg to `GameTask__calc_damage`, used as `(ESI * param) / 100` before RNG jitter).
    pub explosion_damage_pct: u32,
    /// 0x35C — render_data[0x22]. Ricochet-remaining counter. Decremented on each
    /// ricochet-eligible contact; when it reaches 0, the missile invokes the
    /// slot-14 terminator (`WorldEntity::SetTerminateFlag`) instead of bouncing.
    pub ricochet_counter: u32,
    /// 0x360..0x367 — render_data[0x23..0x24] (untouched by known code paths).
    pub _render_data_23_24: [u32; 2],
    /// 0x368 — render_data[0x25]. Override sprite ID used by `render`
    /// when [`_unknown_3a4`] is non-zero (sub-state where the missile
    /// renders a context-specific sprite instead of its primary).
    /// Replaces the per-pellet primary sprite (`render_data[0x06]`
    /// single-shot / `render_data[0x19]` cluster) just before the inner
    /// animation-rate switch.
    ///
    /// [`_unknown_3a4`]: MissileEntity::_unknown_3a4
    pub alt_sprite_id: u32,
    /// 0x36C — render_data[0x26]. Super-animal walking sprite ID, also
    /// used as the eligibility flag — non-zero gates homing missiles'
    /// transition to / from sheep-style steering mode (HandleMessage
    /// case 0x2C (DetonateWeapon) and the FrameFinish tick's homing
    /// branch only call `Task_Missile::start_super_animal` /
    /// `finish_super_animal` when `missile_type == Homing && this != 0`).
    /// In `render`, the value itself is used as one of two sprite IDs
    /// drawn in alternation with [`super_animal_walk_sprite_alt`] every
    /// 5 frames (toggled by `world.frame_counter / 5 & 1`).
    pub super_animal_walk_sprite: u32,
    /// 0x370 — render_data[0x27]. Companion to
    /// [`super_animal_walk_sprite`](MissileEntity::super_animal_walk_sprite):
    /// the alternate frame in the 5-frame walk-cycle toggle that
    /// `render` performs when the missile is in super-animal control
    /// mode (`missile_type == Homing && contact_phase == 1` and not
    /// underwater).
    pub super_animal_walk_sprite_alt: u32,
    /// 0x374..0x37B — render_data[0x28..0x29] (untouched by known code paths).
    /// `[0x29]` (= 0x37C offset equivalent inside post_render) is referenced
    /// in constructor comments as "updates during flight".
    pub _render_data_28_29: [u32; 2],

    // ---- 0x37C–0x41B: post-render physics and state ----
    /// 0x37C — remaining fuse timer (in frames) until detonation / sheep expiry.
    /// Initialised by the constructor from `render_data[0x11]` (= 9000 for bazooka,
    /// 30 for grenade @ 10fps = 3s). Counted down each frame by the physics update;
    /// when it reaches 0 the missile detonates / the sheep self-destructs.
    pub fuse_timer: i32,
    /// 0x380 — post-fuse termination countdown. The frame `fuse_timer` reaches
    /// `0` and this slot is also `0`, the missile invokes the slot-14
    /// terminator (or `finish_super_animal` for active homing). When `> 0`,
    /// the FrameFinish tick decrements it by `0x14` per frame (clamped at 0)
    /// and emits a one-shot `impact_sound_id` via `PlaySoundLocal` channel
    /// `4` the first frame the countdown is active. Set elsewhere; the tick
    /// only consumes it.
    pub post_fuse_terminate_timer: i32,
    /// 0x384 — post-fuse sound one-shot latch. Cleared elsewhere; the tick
    /// sets it to `1` after firing the post-fuse `impact_sound_id` so the
    /// sound only plays once per countdown.
    pub post_fuse_sound_latched: u32,
    /// 0x388 — `RecordLandingEvent` gate. Reset to `0` after each
    /// `record_landing_event_raw` call in the FrameFinish tail.
    pub _field_388: u32,
    /// 0x38C — Unknown.
    pub _field_38c: u32,
    /// 0x390 — Unknown.
    pub _field_390: u32,
    /// 0x394 — `param_1[0xE5]` in OnContact. Contact-phase flag. Value `1`
    /// indicates the missile is in super-animal control mode (sheep-style
    /// steering active); value `2` disables the normal contact path (routes
    /// to the terminator / sheep bailout block).
    pub contact_phase: u32,
    /// 0x398 — Super-animal torque accumulator (running, unclamped).
    /// On modern schemes (`game_version >= 0x1D`) the per-frame clamped
    /// input from steering messages 0x2D / 0x2E lands in
    /// [`super_animal_torque_input`](MissileEntity::super_animal_torque_input)
    /// at 0x3CC and is added into this slot at the top of the FrameFinish
    /// tick. On old schemes the steering messages write here directly
    /// without clamping.
    pub super_animal_torque_accum: u32,
    /// 0x39C..0x3A4 — terminal-velocity stash. Written on the terminator /
    /// sheep-bailout exit path (`stash = self.speed`). Consumers (cluster
    /// spawn, splatter effects) read this to access the missile's terminal
    /// velocity after it has been marked for destruction.
    ///
    /// Earlier analysis guessed Y was a launch-speed magnitude; that turned
    /// out to be a constructor-only initialisation OnContact subsequently
    /// overwrites with current velocity.
    pub terminate_stash_speed: Vec2,
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
    /// 0x3B8..0x3C0 — sheep bailout position stash (pre-contact pos).
    pub sheep_stash_pos: Vec2,
    /// 0x3C0..0x3C8 — sheep bailout velocity stash (pre-contact speed).
    pub sheep_stash_speed: Vec2,
    /// 0x3C8: Horizontal direction sign (+1 or -1, determines facing/travel dir).
    /// param_1[0xF2] in constructor; also rewritten in the homing contact branch
    /// based on `sign(speed_x)` after a RNG roll passes.
    pub direction: i32,
    /// 0x3CC — Super-animal torque per-frame input (clamped to
    /// `[-0x5B0, +0x5B0]`). Steering messages 0x2D (MoveWeaponLeft) and
    /// 0x2E (MoveWeaponRight) add `±0x5B0` here on modern schemes
    /// (`game_version >= 0x1D`), with re-clamping; the FrameFinish tick
    /// then folds this into
    /// [`super_animal_torque_accum`](MissileEntity::super_animal_torque_accum)
    /// and zeros this slot.
    pub super_animal_torque_input: i32,
    /// 0x3D0..0x3D3: Unknown.
    pub _unknown_3d0: [u8; 0x4],
    /// 0x3D4 — Single-byte flag set to 1 by HandleMessage case 0x2C
    /// (DetonateWeapon) on the post-vtable[14]-with-flag-1 sub-branch when
    /// `weapon_data[0x2D] == 1 && game_version < 0x1F0 && weapon_data[9]
    /// == 0x41`. Readers / exact role unidentified.
    pub _field_3d4: u8,
    /// 0x3D5..0x3D7: Unknown.
    pub _unknown_3d5: [u8; 3],
    /// 0x3D8: Headful-only render sub-object handle, allocated by
    /// `Task_Missile::ConstructPointers` (called from the missile
    /// constructor) only when `world.is_headful != 0`. Mirrors the
    /// "two-child wrapper" shape used by `MineEntity::textbox_handle`:
    /// the wrapper holds two refcounted child pointers at +0xC and +0x10,
    /// each released via vtable slot 3 (`thiscall(this, flag=1)`) by the
    /// destructor before `wa_free`-ing the wrapper itself. The first of
    /// two such handles MissileEntity owns; the second is at
    /// [`render_handle_b`](MissileEntity::render_handle_b).
    pub render_handle_a: *mut u8,
    /// 0x3DC: Companion to [`render_handle_a`](MissileEntity::render_handle_a)
    /// with the same wrapper layout. Allocated and freed in lock-step.
    pub render_handle_b: *mut u8,
    /// 0x3E0 — Dig-sound active handle. Holds the value returned by
    /// `GameTask::sound_start_0` for the missile's dig sound on success;
    /// when that call returns -1 (sound system busy), the slot instead
    /// caches `-sound_id` as a "retry me on the next sound-restore"
    /// sentinel — that's what HandleMessage case 0x7A re-arms via
    /// `Task_Missile::start_dig_sound`.
    pub dig_sound_handle: i32,
    /// 0x3E4 — Fuse-sound active handle. Same shape and re-arm protocol
    /// as [`dig_sound_handle`](MissileEntity::dig_sound_handle).
    pub fuse_sound_handle: i32,
    /// 0x3E8 — Animation-phase accumulator. Updated each frame by the
    /// FrameFinish tick body (case 2) and the lighter UpdateNonCritical
    /// path (case 5) at rates that depend on the missile's discriminator
    /// view ([`_render_data_07`] or [`_render_data_1a`]) and on the
    /// missile's speed. Wraps mod 0x10000.
    ///
    /// [`_render_data_07`]: MissileEntity::_render_data_07
    /// [`_render_data_1a`]: MissileEntity::_render_data_1a
    pub animation_phase: u32,
    /// 0x3EC..0x40B: Unknown trailing state.
    /// Allocation size is 0x40C; constructor zeros bytes 0x00–0x3EB.
    pub _unknown_3ec: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<MissileEntity>() == 0x40C);

// Explicit offset sanity checks for fields touched by MissileEntity::OnContact
// and MissileEntity::HandleMessage.
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(MissileEntity, is_cluster_pellet) == 0x100);
    assert!(offset_of!(MissileEntity, splash_sound_latched) == 0x10C);
    assert!(offset_of!(MissileEntity, underwater_entry_latched) == 0x110);
    assert!(offset_of!(MissileEntity, effect_emit_phase) == 0x114);
    assert!(offset_of!(MissileEntity, _render_data_07) == 0x2F0);
    assert!(offset_of!(MissileEntity, explosion_response_flag) == 0x308);
    assert!(offset_of!(MissileEntity, contact_face_mask) == 0x2D4);
    assert!(offset_of!(MissileEntity, fire_particle_trigger) == 0x2EC);
    assert!(offset_of!(MissileEntity, missile_type) == 0x330);
    assert!(offset_of!(MissileEntity, sprite_size) == 0x334);
    assert!(offset_of!(MissileEntity, render_timer) == 0x338);
    assert!(offset_of!(MissileEntity, _render_data_1a) == 0x33C);
    assert!(offset_of!(MissileEntity, impact_sound_id) == 0x340);
    assert!(offset_of!(MissileEntity, ricochet_side_mask) == 0x344);
    assert!(offset_of!(MissileEntity, ricochet_chance_pct) == 0x348);
    assert!(offset_of!(MissileEntity, explosion_id) == 0x350);
    assert!(offset_of!(MissileEntity, explosion_damage) == 0x354);
    assert!(offset_of!(MissileEntity, explosion_damage_pct) == 0x358);
    assert!(offset_of!(MissileEntity, ricochet_counter) == 0x35C);
    assert!(offset_of!(MissileEntity, detonate_response_mode) == 0x32C);
    assert!(offset_of!(MissileEntity, textbox_visible_threshold) == 0x314);
    assert!(offset_of!(MissileEntity, fuse_timer_initial) == 0x318);
    assert!(offset_of!(MissileEntity, alt_sprite_id) == 0x368);
    assert!(offset_of!(MissileEntity, super_animal_walk_sprite) == 0x36C);
    assert!(offset_of!(MissileEntity, super_animal_walk_sprite_alt) == 0x370);
    assert!(offset_of!(MissileEntity, fuse_timer) == 0x37C);
    assert!(offset_of!(MissileEntity, post_fuse_terminate_timer) == 0x380);
    assert!(offset_of!(MissileEntity, post_fuse_sound_latched) == 0x384);
    assert!(offset_of!(MissileEntity, _field_388) == 0x388);
    assert!(offset_of!(MissileEntity, _field_38c) == 0x38C);
    assert!(offset_of!(MissileEntity, _field_390) == 0x390);
    assert!(offset_of!(MissileEntity, contact_phase) == 0x394);
    assert!(offset_of!(MissileEntity, super_animal_torque_accum) == 0x398);
    assert!(offset_of!(MissileEntity, terminate_stash_speed) == 0x39C);
    assert!(offset_of!(MissileEntity, homing_enabled) == 0x3A8);
    assert!(offset_of!(MissileEntity, sheep_bailout_counter) == 0x3AC);
    assert!(offset_of!(MissileEntity, sheep_bailout_locked) == 0x3B0);
    assert!(offset_of!(MissileEntity, sheep_action_flag) == 0x3B4);
    assert!(offset_of!(MissileEntity, sheep_stash_pos) == 0x3B8);
    assert!(offset_of!(MissileEntity, sheep_stash_speed) == 0x3C0);
    assert!(offset_of!(MissileEntity, direction) == 0x3C8);
    assert!(offset_of!(MissileEntity, super_animal_torque_input) == 0x3CC);
    assert!(offset_of!(MissileEntity, _field_3d4) == 0x3D4);
    assert!(offset_of!(MissileEntity, render_handle_a) == 0x3D8);
    assert!(offset_of!(MissileEntity, render_handle_b) == 0x3DC);
    assert!(offset_of!(MissileEntity, dig_sound_handle) == 0x3E0);
    assert!(offset_of!(MissileEntity, fuse_sound_handle) == 0x3E4);
    assert!(offset_of!(MissileEntity, animation_phase) == 0x3E8);
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
    /// Inert / cleared. Written by `HandleMessage` case 0x2's inner
    /// `Unknown1` sub-branch when the missile is underwater AND its
    /// contact-face mask has bit 0x400000 set — those preconditions
    /// disable any further `handle_homing` step. Subsequent frames
    /// observe `Zero` and run the no-op default arm of the inner switch.
    Zero = 0,
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
            0 => Self::Zero,
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
            writeln!(w, "activity_rank_slot = {}", self.activity_rank_slot)?;

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
                "post_fuse(terminate_timer={} sound_latched={})",
                self.post_fuse_terminate_timer, self.post_fuse_sound_latched,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "contact_phase = {} terminate_stash_speed = ({}, {}) direction = {} homing = {}",
                self.contact_phase,
                self.terminate_stash_speed.x,
                self.terminate_stash_speed.y,
                self.direction,
                self.homing_enabled,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "super_animal: torque_accum=0x{:08X} torque_input={} cluster_pellet={}",
                self.super_animal_torque_accum,
                self.super_animal_torque_input,
                self.is_cluster_pellet,
            )?;
            write_indent(w, i)?;
            writeln!(w, "animation_phase = 0x{:04X}", self.animation_phase)?;
            write_indent(w, i)?;
            writeln!(
                w,
                "sheep(counter={} locked={} action={} stash_pos=({}, {}) stash_speed=({}, {}))",
                self.sheep_bailout_counter,
                self.sheep_bailout_locked,
                self.sheep_action_flag,
                self.sheep_stash_pos.x,
                self.sheep_stash_pos.y,
                self.sheep_stash_speed.x,
                self.sheep_stash_speed.y,
            )?;

            // Unknown regions
            write_indent(w, i)?;
            writeln!(w, "_unknown_fc ({} bytes):", self._unknown_fc.len())?;
            write_raw_region(w, self._unknown_fc.as_ptr(), self._unknown_fc.len(), i + 1)?;
            write_indent(w, i)?;
            writeln!(w, "_unknown_104 ({} bytes):", self._unknown_104.len())?;
            write_raw_region(
                w,
                self._unknown_104.as_ptr(),
                self._unknown_104.len(),
                i + 1,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "splash_sound_latched = {} underwater_entry_latched = {} effect_emit_phase = 0x{:08X}",
                self.splash_sound_latched,
                self.underwater_entry_latched,
                self.effect_emit_phase.to_raw(),
            )?;
            write_indent(w, i)?;
            writeln!(w, "_unknown_3d0 ({} bytes):", self._unknown_3d0.len())?;
            write_raw_region(
                w,
                self._unknown_3d0.as_ptr(),
                self._unknown_3d0.len(),
                i + 1,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "_field_3d4 = {} dig_sound_handle = 0x{:08X} fuse_sound_handle = 0x{:08X}",
                self._field_3d4, self.dig_sound_handle, self.fuse_sound_handle
            )?;
            write_indent(w, i)?;
            writeln!(w, "_unknown_3d5 ({} bytes):", self._unknown_3d5.len())?;
            write_raw_region(
                w,
                self._unknown_3d5.as_ptr(),
                self._unknown_3d5.len(),
                i + 1,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "render_handle_a = {:p} render_handle_b = {:p}",
                self.render_handle_a, self.render_handle_b,
            )?;
            write_indent(w, i)?;
            writeln!(w, "_unknown_3ec ({} bytes):", self._unknown_3ec.len())?;
            write_raw_region(
                w,
                self._unknown_3ec.as_ptr(),
                self._unknown_3ec.len(),
                i + 1,
            )?;

            Ok(())
        }
    }
}
