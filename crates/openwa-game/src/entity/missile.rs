use super::base::BaseEntity;
use super::game_entity::{SubclassData, WorldEntity};
use crate::FieldRegistry;
use crate::game::EntityMessage;
use crate::game::weapon::WeaponReleaseContext;
use crate::render::textbox::Textbox;
use openwa_core::fixed::Fixed;
use openwa_core::vec2::Vec2;

pub mod frame_finish;
pub mod free;
pub mod handle_message;
pub mod render;
pub mod sound;
pub mod super_animal;

/// Maximum steering torque applied to super sheep / aqua sheep per frame.
pub const MAX_STEERING_TORQUE: Fixed = Fixed(0x5B0);

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
    /// Entity +0x48: Digger state flag (mole bomb / drill burrow,
    /// `MissileType::Digger`). Set to 1 by the digger pre-bailout stash
    /// branch in `OnContact` (alongside saving stash position / speed and
    /// arming the bailout re-arm counter).
    pub digger_state_flag: u32,
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

/// MissileEntity vtable (0x00664438) — 20 slots. Slots 0–7 inherited from
/// BaseEntity / WorldEntity.
#[openwa_game::vtable(size = 20, va = 0x00664438, class = "MissileEntity")]
pub struct MissileEntityVtable {
    /// `MissileEntity::Free` (0x00508330). Calls inlined `dtor1` (0x005086F0)
    /// and frees the heap allocation when bit 0 of `flags` is set.
    #[slot(1)]
    pub free: fn(this: *mut MissileEntity, flags: u8) -> *mut MissileEntity,
    /// `MissileEntity::HandleMessage` (0x0050B400).
    #[slot(2)]
    pub handle_message: fn(
        this: *mut MissileEntity,
        sender: *mut BaseEntity,
        msg_type: EntityMessage,
        size: u32,
        data: *const u8,
    ),
    /// `MissileEntity::OnContact` (0x00508C90). Dispatches by
    /// [`missile_type`](MissileEntity::missile_type).
    #[slot(8)]
    pub on_contact:
        fn(this: *mut MissileEntity, other: *mut BaseEntity, self_side_flags: u32) -> u32,
    /// `WorldEntity::SetTerminateFlag_Maybe` (0x004FE060) — inherited;
    /// writes `flag` to subclass terminate slot.
    #[slot(14)]
    pub set_terminate_flag: fn(this: *mut MissileEntity, flag: u32),
    /// `WorldEntity::vt17` (0x00500090) — inherited mass-scaled impulse
    /// adder; FrameFinish folds wind/sway through this. Bails when
    /// `terminate_flag` is set.
    #[slot(17)]
    pub apply_impulse: fn(this: *mut MissileEntity, dx: i32, dy: i32, mode: i32) -> u32,
}

/// Airborne projectile (rockets, grenades, mortar shells, homing missiles,
/// sheep, mole bombs, etc.). class_type = 0x0B.
///
/// Inheritance: BaseEntity → WorldEntity → MissileEntity. Constructor
/// `MissileEntity::Constructor` (0x00507D10, stdcall, 4 params: this, parent,
/// scheme weapon data, spawn data). Vtable at 0x00664438.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MissileEntity {
    /// 0x00–0xFB: WorldEntity base (pos at 0x84/0x88, speed at 0x90/0x94).
    /// Subclass-data overlay typed as [`MissileSubclassData`].
    pub base: WorldEntity<*const MissileEntityVtable, MissileSubclassData>,

    // ---- 0xFC–0x12F: missile init fields ----
    /// 0xFC–0xFF: Unknown.
    pub _unknown_fc: [u8; 0x4],
    /// 0x100 — Homing-engaged latch. Set by `Task_Missile::apply_direct_homing`
    /// (0x00509EB0) on the first homing pulse (which also snapshots the
    /// pre-homing velocity into `+0x104/+0x108`); cleared by
    /// `Task_Missile::handle_homing` (0x0050ABA0) when the homing burn
    /// timer at `+0x358` drains. HandleMessage and Render use this to
    /// switch between the normal-flight and homing-burn render-data views
    /// (`_render_data_07` vs `_render_data_1a`) and to bias the
    /// spawn-effect Y threshold by `0x80` during the burn.
    pub homing_engaged_latch: u32,
    /// 0x104–0x10B — Pre-homing velocity snapshot (Fixed16.16 vx, vy)
    /// captured by `apply_direct_homing`. `apply_pigeon_homing` iterates
    /// here each frame and mirrors back into `+0x90/+0x94`.
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
    /// 0x118: Crate-pickup counter for `Task_Missile::cluster_crate_sweep`
    /// (0x0050A720). Incremented each time an in-flight missile collects a
    /// crate (Animal / Cluster sub-pellet types only); the fuse timer is
    /// extended by `fuse_timer / count` so each pickup grants a diminishing
    /// "extra time" bonus. Bounded by `game_info[+0xD9AF]` (per-scheme limit
    /// byte; 0 = unlimited). Initialized from scheme data at construction.
    pub crate_pickup_count: u32,
    /// 0x11C: Unknown
    pub _unknown_11c: u32,
    /// 0x120: Trail-effect emit-phase accumulator (Fixed 16.16). Mirrors
    /// [`effect_emit_phase`] but for the secondary "trail" particle stream
    /// gated by `sprite_size & 0x40000000`. Each FrameFinish-driven
    /// `Task_Missile::update_effect` (0x0050B240) tick adds
    /// [`trail_emit_step`] here and consumes `Fixed::ONE` per emitted
    /// particle (anim_kind 0xE0000 via SharedData lookup of SpriteAnimEntity).
    ///
    /// [`effect_emit_phase`]: MissileEntity::effect_emit_phase
    /// [`trail_emit_step`]: MissileEntity::trail_emit_step
    pub trail_emit_phase: Fixed,
    /// 0x124: Trail-effect emit-step (Fixed 16.16). Decays by `0xCCC` per
    /// frame and is clamped to `[0, Fixed::ONE]`; reaching 0 stops trail
    /// emission. Companion to [`trail_emit_phase`].
    ///
    /// [`trail_emit_phase`]: MissileEntity::trail_emit_phase
    pub trail_emit_step: Fixed,
    /// 0x128: Position-derived launch seed. Computed by constructor as:
    /// `((spawn_x + spawn_y) / 256 / 20) + 0x10000`. param_1[0x4A].
    pub launch_seed: Fixed,
    /// 0x12C: This missile's slot ID in `GameWorld.entity_activity_queue`
    /// (param_1[0x4B]).
    pub activity_rank_slot: u32,

    // ---- 0x130–0x15B: spawn data (11 DWORDs, from param_4) ----
    /// 0x130–0x15B: Spawn parameters copied verbatim from constructor
    /// param_4 (a `WeaponReleaseContext`).
    pub spawn_params: WeaponReleaseContext,

    // ---- 0x15C–0x2D3: weapon/scheme data (94 DWORDs, from param_3) ----
    /// 0x15C–0x2D3: Weapon/scheme properties copied verbatim from param_3.
    /// The WGT blob is split: `[0x00..0x34]` primary projectile params,
    /// `[0x34..0x5E]` cluster sub-pellet params. The constructor copies 42
    /// DWORDs of one half into the `_render_data_*` block below
    /// (`spawn_params.pellet_index == 0` → primary; else → sub-pellet).
    pub weapon_data: [u32; 0x5E],

    // ---- 0x2D4–0x37B: render/physics parameters (42 DWORDs copied from
    //      one half of weapon_data; some entries mutated during flight) ----
    /// 0x2D4 — render_data[0]. Contact-face mask tested against
    /// `other.contact_face`. When `(1 << other_face) & mask != 0`, the
    /// digger bailout / contact-rejection path fires.
    pub contact_face_mask: u32,
    /// 0x2D8..0x2EB — render_data[1..6] (untouched by known code paths).
    pub _render_data_01_05: [u32; 5],
    /// 0x2EC — render_data[6]. Compared against `0x40` before `ImpactSpecialFx`
    /// (fire-particle spawn). Exact role still unclear — possibly a weapon-fx flag
    /// or a cluster-origin marker.
    pub fire_particle_trigger: u32,
    /// 0x2F0 — render_data[0x07]. Animation-rate kind discriminator
    /// for normal-flight (non-homing) missiles; HandleMessage's
    /// `piVar8[2]` view reads this slot when
    /// [`homing_engaged_latch`](MissileEntity::homing_engaged_latch)
    /// is zero. Observed values 3..=6 select different animation-phase
    /// step formulas (case 2) and gate the speed-driven anim-phase update
    /// (case 5). When the homing burn engages, the equivalent slot is
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
    /// (see [`MissileType`]). 1=Homing, 2=Standard, 3=Animal, 4=Digger,
    /// 5=Cluster (sub-pellet).
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
    /// homing-burn flight; HandleMessage's `piVar8[2]` view reads
    /// this slot when [`homing_engaged_latch`](MissileEntity::homing_engaged_latch)
    /// is non-zero. Same value-set semantics as
    /// [`_render_data_07`](MissileEntity::_render_data_07) (the
    /// normal-flight counterpart at 0x2F0).
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
    /// 0x350 — render_data[0x1F]. **Polymorphic.**
    /// - `MissileType::Standard` / `Cluster`: Explosion ID (passed as 2nd
    ///   stack arg to `CreateExplosion` on contact).
    /// - `MissileType::Homing`: homing-kind discriminator read by
    ///   `inner_homing_tick` (1 = direct, 2 = direct + pigeon).
    pub explosion_id: u32,
    /// 0x354 — render_data[0x20]. **Polymorphic.**
    /// - `MissileType::Standard` / `Cluster`: Explosion damage base value
    ///   (implicit ESI arg to `GameTask__calc_damage` damage-jitter helper);
    ///   nonzero gate for `CreateExplosion`.
    /// - `MissileType::Homing`: lock-on countdown — counted down by
    ///   `inner_homing_tick` until target acquisition broadcasts
    ///   [`WeaponHomingMessage`].
    ///
    /// [`WeaponHomingMessage`]: crate::game::message::WeaponHomingMessage
    pub explosion_damage: u32,
    /// 0x358 — render_data[0x21]. **Polymorphic.**
    /// - `MissileType::Standard` / `Cluster`: Explosion damage scaling
    ///   percentage (2nd stack arg to `GameTask__calc_damage`, used as
    ///   `(ESI * param) / 100` before RNG jitter).
    /// - `MissileType::Homing`: burn-engagement countdown — frames remaining
    ///   of active `apply_direct_homing` / `apply_pigeon_homing` steering;
    ///   reaching 0 also clears [`homing_engaged_latch`].
    ///
    /// [`homing_engaged_latch`]: MissileEntity::homing_engaged_latch
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
    /// 0x36C — render_data[0x26]. Super-animal walking sprite ID; also the
    /// eligibility flag — non-zero gates the `MissileType::Animal` transition
    /// to/from super-animal steering (HandleMessage case 0x2C and FrameFinish
    /// only call `start_super_animal` / `finish_super_animal` when this is
    /// non-zero). `render` alternates this and
    /// [`super_animal_walk_sprite_alt`] every 5 frames during super-animal
    /// control.
    pub super_animal_walk_sprite: u32,
    /// 0x370 — render_data[0x27]. Companion to
    /// [`super_animal_walk_sprite`](MissileEntity::super_animal_walk_sprite).
    pub super_animal_walk_sprite_alt: u32,
    /// 0x374 — render_data[0x28]. Super-animal start sound — played
    /// one-shot on channel 5 by `start_super_animal` when non-zero.
    pub super_animal_start_sound_id: u32,
    /// 0x378 — render_data[0x29]. Super-animal loop sound — replaces the
    /// fuse sound while jetpack steering is active. `start_super_animal`
    /// stops the fuse sound and starts a fresh streaming sound (storing
    /// the handle in [`fuse_sound_handle`]); `finish_super_animal` stops
    /// it again on exit.
    ///
    /// [`fuse_sound_handle`]: MissileEntity::fuse_sound_handle
    pub super_animal_loop_sound_id: u32,

    // ---- 0x37C–0x41B: post-render physics and state ----
    /// 0x37C — remaining fuse timer in frames. Counted down by FrameFinish;
    /// reaching 0 triggers detonate / fuse-expiry. Initialised from
    /// `render_data[0x11]`.
    pub fuse_timer: i32,
    /// 0x380 — post-fuse termination countdown. While `> 0`, FrameFinish
    /// decrements by `0x14` per frame and emits a one-shot
    /// `impact_sound_id`. When `fuse_timer` and this both hit 0, the missile
    /// invokes slot-14 (or `finish_super_animal` for active super-animal).
    pub post_fuse_terminate_timer: i32,
    /// 0x384 — one-shot latch for the post-fuse `impact_sound_id` emit.
    pub post_fuse_sound_latched: u32,
    /// 0x388 — `RecordLandingEvent` gate; reset to 0 after each call.
    pub _field_388: u32,
    /// 0x38C — Unknown.
    pub _field_38c: u32,
    /// 0x390 — Unknown.
    pub _field_390: u32,
    /// 0x394 — Contact-phase flag. `1` = super-animal control active;
    /// `2` = disable normal contact path (route to terminator / digger
    /// bailout block).
    pub contact_phase: u32,
    /// 0x398 — Super-animal torque accumulator (running, unclamped). 16.16
    /// fixed-point angle: low 16 bits are the sub-pixel fraction consumed by
    /// the sprite's frame table (slot 33 `anim_value`), high 16 bits the
    /// integer rotation count. Modern schemes (`game_version >= 0x1D`) feed
    /// this from
    /// [`super_animal_torque_input`](MissileEntity::super_animal_torque_input)
    /// at the top of FrameFinish; old schemes have steering messages
    /// (0x2D/0x2E) write here directly without clamping.
    pub super_animal_torque_accum: Fixed,
    /// 0x39C..0x3A4 — Terminal-velocity stash. Written on the
    /// terminator/digger-bailout exit (`stash = self.speed`); read by
    /// downstream consumers (cluster spawn, splatter) after the missile is
    /// marked for destruction.
    pub terminate_stash_speed: Vec2,
    /// 0x3A4: Unknown
    pub _unknown_3a4: u32,
    /// 0x3A8 — Super-animal target-lock flag. Set by the constructor when
    /// `+0x32C != 0 && spawn_data[0] != 0` (broadcasts msg 0x52 to find/lock
    /// a target). Despite its previous `homing_enabled` name, this is **not**
    /// the classic homing-missile flag — homing-missile steering is governed
    /// by `+0x350/+0x354/+0x358` in `Task_Missile::handle_homing` and is
    /// independent of this field. Observed at runtime: SHEEP
    /// (`MissileType::Animal`) reports 1 here; HOMING MISSILE reports 0.
    pub super_animal_target_locked: u32,
    /// 0x3AC — Digger bailout re-arm countdown (mole bomb / drill burrow).
    /// Armed to `0xA` on first digger contact; decrement is performed
    /// elsewhere, not in OnContact.
    pub digger_bailout_counter: u32,
    /// 0x3B0 — Digger bailout lock. If non-zero on digger contact, the
    /// bailout is rejected and the terminator runs immediately.
    pub digger_bailout_locked: u32,
    /// 0x3B4 — Digger action flag. Zeroed on first digger bailout arm.
    pub digger_action_flag: u32,
    /// 0x3B8..0x3C0 — Digger pre-contact position stash.
    pub digger_stash_pos: Vec2,
    /// 0x3C0..0x3C8 — Digger pre-contact velocity stash.
    pub digger_stash_speed: Vec2,
    /// 0x3C8 — Horizontal direction sign (`+1` or `-1`). Constructor-init
    /// then rewritten in OnContact's animal branch from `sign(speed_x)`
    /// after a passing RNG roll.
    pub direction: i32,
    /// 0x3CC — Per-frame super-animal torque input, clamped to
    /// `[-MAX_STEERING_TORQUE, +MAX_STEERING_TORQUE]` by steering messages 0x2D / 0x2E on modern
    /// schemes (`game_version >= 0x1D`). Folded into
    /// [`super_animal_torque_accum`](MissileEntity::super_animal_torque_accum)
    /// and zeroed at the top of FrameFinish.
    pub super_animal_torque_input: Fixed,
    /// 0x3D0..0x3D3: Unknown.
    pub _unknown_3d0: [u8; 0x4],
    /// 0x3D4 — Set to 1 by HandleMessage case 0x2C on a specific sub-branch
    /// (`weapon_data[0x2D] == 1 && game_version < 0x1F0 &&
    /// weapon_data[9] == 0x41`). Readers / exact role unidentified.
    pub _field_3d4: u8,
    /// 0x3D5..0x3D7: Unknown.
    pub _unknown_3d5: [u8; 3],
    /// 0x3D8 — Headful-only [`Textbox`] handle, allocated by
    /// `Task_Missile::ConstructPointers` via `DisplayGfx::ConstructTextbox`
    /// when `world.is_headful != 0`. Used by missile renderers (e.g.
    /// homing-pigeon target labels) to draw text overlays.
    pub render_handle_a: *mut Textbox,
    /// 0x3DC — Companion to [`render_handle_a`](MissileEntity::render_handle_a),
    /// allocated and freed in lock-step.
    pub render_handle_b: *mut Textbox,
    /// 0x3E0 — Dig-sound active handle. Live id on success;
    /// `-sound_id` retry sentinel when `GameTask::sound_start_0` returned
    /// `-1`. HandleMessage case 0x7A re-arms via
    /// `Task_Missile::start_dig_sound`.
    pub dig_sound_handle: i32,
    /// 0x3E4 — Fuse-sound active handle (same protocol as
    /// [`dig_sound_handle`](MissileEntity::dig_sound_handle)).
    pub fuse_sound_handle: i32,
    /// 0x3E8 — Animation-phase accumulator. Wraps mod 0x10000. Advanced by
    /// FrameFinish (case 2) and UpdateNonCritical (case 5) at rates that
    /// depend on [`_render_data_07`] / [`_render_data_1a`] and speed.
    ///
    /// [`_render_data_07`]: MissileEntity::_render_data_07
    /// [`_render_data_1a`]: MissileEntity::_render_data_1a
    pub animation_phase: Fixed,
    /// 0x3EC..0x40B: Unknown trailing state.
    /// Allocation size is 0x40C; constructor zeros bytes 0x00–0x3EB.
    pub _unknown_3ec: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<MissileEntity>() == 0x40C);

// Explicit offset sanity checks for fields touched by MissileEntity::OnContact
// and MissileEntity::HandleMessage.
const _: () = {
    use core::mem::offset_of;
    assert!(offset_of!(MissileEntity, homing_engaged_latch) == 0x100);
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
    assert!(offset_of!(MissileEntity, super_animal_target_locked) == 0x3A8);
    assert!(offset_of!(MissileEntity, digger_bailout_counter) == 0x3AC);
    assert!(offset_of!(MissileEntity, digger_bailout_locked) == 0x3B0);
    assert!(offset_of!(MissileEntity, digger_action_flag) == 0x3B4);
    assert!(offset_of!(MissileEntity, digger_stash_pos) == 0x3B8);
    assert!(offset_of!(MissileEntity, digger_stash_speed) == 0x3C0);
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

/// Missile movement/behaviour type, encoded at `render_data[0x17]`
/// (= `weapon_data[0x1A]` for single-shot projectiles). Discriminator for
/// the constructor's setup switch and the FrameFinish per-tick dispatch.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissileType {
    /// Inert / cleared. Written by `HandleMessage` case 0x2's inner
    /// `Homing` sub-branch when the missile is underwater AND its
    /// contact-face mask has bit 0x400000 set — those preconditions
    /// disable any further `handle_homing` step. Subsequent frames
    /// observe `Zero` and run the no-op default arm of the inner switch.
    Zero = 0,
    /// Homing missile / homing pigeon. Per-tick handler at 0x0050ABA0
    /// (`Task_Missile::handle_homing`) advances the homing burn — the
    /// `homing_engaged_latch` (+0x100) and the burn timer (+0x358) gate
    /// when `apply_direct_homing` / `apply_pigeon_homing` actually steers.
    Homing = 1,
    /// Standard trajectory projectile (bazooka, mortar, grenade, cluster
    /// grenade, air-strike, etc.). Pure ballistic; no per-tick handler.
    Standard = 2,
    /// Animal / walking-creature projectile — sheep, super-sheep, old
    /// woman, etc. Per-tick handler at 0x0050A7E0
    /// (`Task_Missile::handle_animal`) switches on `contact_phase`:
    /// =1 → super-sheep jetpack, else → walking on terrain. Constructor
    /// case 3 zeros velocity at spawn (animal starts stationary);
    /// `OnContact` calls the super-sheep target reaffirmation
    /// (`HomingTargetCheck`) and zeros velocity if no target remains.
    Animal = 3,
    /// Digger / burrow projectile — mole bomb, pneumatic drill. Per-tick
    /// handler at 0x0050A430 (`Task_Missile::handle_digger`, stdcall) does
    /// the dig-into-terrain motion gated by the
    /// `digger_bailout_counter` re-arm cooldown.
    Digger = 4,
    /// Cluster sub-pellet — used by missiles spawned via
    /// `Task_Missile::create_clusters` (0x005096A0) when the parent
    /// detonates. Constructor and `OnContact` share the value-2 (`Standard`)
    /// arms; the only differences are the per-tick handler at 0x0050A720
    /// (a crate-collection sweep) and a faster cumulative-damage decay
    /// (`/0x32` instead of `/0x64`). Short-lived: only present between
    /// cluster-burst and pellet impact.
    Cluster = 5,
}

impl MissileType {
    /// Map a raw `render_data[0x17]` value to a typed [`MissileType`].
    /// Returns `None` for out-of-range values (6+); callers should treat
    /// such cases as "no typed branch applies".
    pub const fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            0 => Self::Zero,
            1 => Self::Homing,
            2 => Self::Standard,
            3 => Self::Animal,
            4 => Self::Digger,
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
            writeln!(w, "pos = ({}, {})", b.pos.x, b.pos.y)?;
            write_indent(w, i)?;
            writeln!(w, "speed = ({}, {})", b.speed_x, b.speed_y)?;
            write_indent(w, i)?;
            writeln!(
                w,
                "launch_seed = 0x{:08X}",
                self.launch_seed.to_raw() as u32
            )?;
            write_indent(w, i)?;
            writeln!(w, "activity_rank_slot = {}", self.activity_rank_slot)?;

            let sp = &self.spawn_params;
            write_indent(w, i)?;
            writeln!(w, "spawn_params:")?;
            write_indent(w, i + 1)?;
            writeln!(
                w,
                "owner=({}, {}) spawn=({}, {}) offset=({}, {})",
                sp.owner_id,
                sp.owner_worm_id,
                sp.spawn_x,
                sp.spawn_y,
                sp.spawn_offset_x,
                sp.spawn_offset_y
            )?;
            write_indent(w, i + 1)?;
            writeln!(
                w,
                "cursor=({}, {}) pellet={} delay={} network_delay={}",
                sp.cursor_x, sp.cursor_y, sp.pellet_index, sp.delay, sp.network_delay
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
                "contact_phase = {} terminate_stash_speed = ({}, {}) direction = {} super_animal_target_locked = {}",
                self.contact_phase,
                self.terminate_stash_speed.x,
                self.terminate_stash_speed.y,
                self.direction,
                self.super_animal_target_locked,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "super_animal: torque_accum=0x{:08X} torque_input={} homing_engaged={}",
                self.super_animal_torque_accum.to_raw() as u32,
                self.super_animal_torque_input,
                self.homing_engaged_latch,
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "animation_phase = 0x{:04X}",
                self.animation_phase.to_raw() as u32
            )?;
            write_indent(w, i)?;
            writeln!(
                w,
                "digger(counter={} locked={} action={} stash_pos=({}, {}) stash_speed=({}, {}))",
                self.digger_bailout_counter,
                self.digger_bailout_locked,
                self.digger_action_flag,
                self.digger_stash_pos.x,
                self.digger_stash_pos.y,
                self.digger_stash_speed.x,
                self.digger_stash_speed.y,
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
