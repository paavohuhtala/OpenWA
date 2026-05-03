use super::base::BaseEntity;
use super::game_entity::WorldEntity;
use crate::FieldRegistry;
use crate::game::KnownWeaponId;
use crate::game::weapon::WeaponEntry;
use derive_more::TryFrom;
use openwa_core::fixed::Fixed;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WormState(pub u32);

impl WormState {
    pub fn is(self, state: KnownWormState) -> bool {
        self == WormState(state as u32)
    }

    pub fn is_between(self, range: std::ops::RangeInclusive<KnownWormState>) -> bool {
        let Some(as_known) = KnownWormState::try_from(self.0 as u8).ok() else {
            return false;
        };

        range.contains(&as_known)
    }

    pub fn is_any_of(self, states: &[KnownWormState]) -> bool {
        states.iter().any(|&s| self.is(s))
    }
}

impl From<KnownWormState> for WormState {
    fn from(value: KnownWormState) -> Self {
        WormState(value as u32)
    }
}

/// Worm state machine states.
///
/// WormEntity's `SetState` (vtable slot 14) transitions between these.
/// The state byte lives at WormEntity+0x44 (inside `base.subclass_data`).
/// Also stored in WormEntry.state in the TeamArena.
///
/// States 0x68..=0x8A are the "weapon/action active" range — checked by
/// `(state - 0x68) < 0x23` in HandleMessage. States 0x80+ are dying/dead.
/// Names are best guesses from behavioral observation and disassembly.
///
/// Source: WormEntity::HandleMessage (0x510B40) decompilation + weapon fire dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFrom, PartialOrd, Ord)]
#[try_from(repr)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum KnownWormState {
    /// Transitional state checked by CheckWormState0x64 (0x5228D0).
    /// Appears briefly during turn transitions.
    Transitional = 0x64,
    /// Idle — not this worm's turn. Also used by MailMineMole to re-enter idle.
    Idle = 0x65,
    /// Idle variant — grouped with 0x65/0x67 in HandleMessage switch cases.
    IdleVariant_Maybe = 0x66,
    /// Active turn — this worm is currently being controlled.
    Active = 0x67,
    /// Active variant — observed in WormEntry. Start of weapon/action range.
    ActiveVariant_Maybe = 0x68,
    /// Unknown (within weapon/action range).
    Unknown_0x69 = 0x69,
    /// Unknown (within weapon/action range).
    Unknown_0x6A = 0x6A,
    /// Unknown (within weapon/action range).
    Unknown_0x6B = 0x6B,
    /// Fire Punch — performing fire punch attack (sub34=1).
    FirePunch = 0x6C,
    /// Kamikaze — performing kamikaze attack (sub34=4). Frequently checked in HandleMessage.
    Kamikaze = 0x6D,
    /// Pneumatic Drill — using the pneumatic drill (sub34=8).
    PneumaticDrill = 0x6E,
    /// Air Strike pending — set when air strike fires with `_unknown_208 == 0`.
    AirStrikePending_Maybe = 0x6F,
    /// Unknown — sub34=6 has no replay test log data.
    Unknown_0x70 = 0x70,
    /// Blowtorch — using the blowtorch weapon (sub34=11).
    Blowtorch = 0x71,
    /// Unknown — sub34=18 has no replay test log data.
    Unknown_0x72 = 0x72,
    /// Weapon charging — entered from aiming states (0x7B, 0x7C) before release.
    /// Also set by CheckPendingAction when field +0xBC is nonzero.
    WeaponCharging_Maybe = 0x73,
    /// Teleport cancelled — teleport failed or was denied.
    TeleportCancelled_Maybe = 0x74,
    /// Suicide Bomber — performing suicide bomber attack (sub34=5).
    SuicideBomber = 0x75,
    /// Unknown (within weapon/action range).
    Unknown_0x76 = 0x76,
    /// Weapon selected — entered via SelectCursor (msg 0x24) from idle/active states.
    /// HandleMessage sets param[0xa7]=-1 when already in this state.
    WeaponSelected_Maybe = 0x77,
    /// Weapon aimed — post-select state. Teleport check accepts this.
    /// Also used for Magic Bullet weapon fire.
    WeaponAimed_Maybe = 0x78,
    /// Unknown. Checked in WeaponRelease spawn offset (type 2, Y adjustment when state == 0x79).
    Unknown_0x79 = 0x79,
    /// Unknown.
    Unknown_0x7A = 0x7A,
    /// Aiming with angle — entered for aimed weapons. Sets angle params.
    /// Teleport check accepts this. Transitions to 0x73 on fire.
    AimingAngle_Maybe = 0x7B,
    /// Rope swinging — IsNotOnRope checks `state != 0x7C`.
    /// Transitions to 0x73 on fire.
    RopeSwinging = 0x7C,
    /// Pre-fire variant — MailMineMole version check uses this.
    /// Transitions to 0x7E or 0x65 depending on FUN_004fb580.
    PreFire_Maybe = 0x7D,
    /// Post-fire / special movement — entered from 0x78 and 0x7D
    /// when FUN_004fb580 returns nonzero.
    PostFire_Maybe = 0x7E,
    /// Drowning — worm fell in water.
    Drowning = 0x7F,
    /// Hurt — worm took damage.
    Hurt = 0x80,
    /// Dead variant 1.
    Dead1 = 0x81,
    /// Dying variant 1 — checked alongside 0x83 in HandleMessage.
    Dying1_Maybe = 0x82,
    /// Dying variant 2 — checked alongside 0x82 in HandleMessage.
    Dying2_Maybe = 0x83,
    /// Unknown.
    Unknown_0x84 = 0x84,
    /// Unknown.
    Unknown_0x85 = 0x85,
    /// Dead — set by Surrender (msg 0x29). Frequently checked in HandleMessage.
    Dead = 0x86,
    /// Dead variant 3.
    Dead3 = 0x87,
    /// Unknown — grouped with idle states (0x65/0x66/0x67/0x8B) in HandleMessage.
    Unknown_0x88 = 0x88,
    /// Dying/special animation (from WormEntry state documentation).
    DyingAnimation_Maybe = 0x89,
    /// Unknown. End of weapon/action range.
    Unknown_0x8A = 0x8A,
    /// Unknown state checked in TeamEntity handlers.
    /// Grouped with idle states in HandleMessage switch cases.
    Unknown_0x8B = 0x8B,
}

/// Raw-pointer methods for WormEntity.
///
/// `set_state_raw` and other vtable methods are now auto-generated by the
/// `bind_WormEntityVtable!` macro. Only non-vtable raw helpers remain here.
impl WormEntity {
    /// Set the weapon fire completion flag at +0x3C without creating `&mut self`.
    pub unsafe fn set_fire_complete_raw(this: *mut WormEntity, value: i32) {
        unsafe {
            let base = this as *mut u8;
            core::ptr::write(base.add(0x3C) as *mut i32, value);
        }
    }

    /// Fire subtype 1 at WorldEntity+0x34 (subclass_data[4..8]).
    /// Air strike temporarily swaps this with `_unknown_190` during position update.
    pub unsafe fn fire_subtype_1(this: *const WormEntity) -> i32 {
        unsafe { i32::from_ne_bytes((&(*this).base.subclass_data)[4..8].try_into().unwrap()) }
    }

    /// Set fire subtype 1 without creating `&mut self` (avoids LLVM noalias violations).
    pub unsafe fn set_fire_subtype_1_raw(this: *mut WormEntity, value: i32) {
        unsafe {
            (&mut (*this).base.subclass_data)[4..8].copy_from_slice(&value.to_ne_bytes());
        }
    }

    /// Set the action field at WorldEntity+0x48 (subclass_data[0x18..0x1C]).
    /// Cleared after air strike completion alongside `_unknown_208` / `_unknown_19x`.
    pub unsafe fn set_action_field_raw(this: *mut WormEntity, value: i32) {
        unsafe {
            (&mut (*this).base.subclass_data)[0x18..0x1C].copy_from_slice(&value.to_ne_bytes());
        }
    }
}

crate::define_addresses! {
    class "WormEntity" {
        /// WormEntity vtable
        vtable WORM_ENTITY_VTABLE = 0x006644C8;
        /// WormEntity constructor
        ctor WORM_ENTITY_CONSTRUCTOR = 0x0050BFB0;
        /// `WormEntity::NotifyMoved_Maybe` (was `BroadcastDamageEvent_Maybe`).
        /// Usercall `(ESI = this)`, plain RET. Looks up the SharedData entity
        /// at key `(esi=0, edi=0x14)` and sends it `WormMoved` (msg 0x47);
        /// also commits the cursor-marker pos and bumps a per-worm "first
        /// action of turn" counter. Called from `HandleMessage` Pre-switch A.
        fn/Usercall WORM_ENTITY_NOTIFY_MOVED = 0x0050F730;
        /// `WormEntity::CommitPendingHealth_Maybe`. Usercall `(ESI = this)`,
        /// plain RET, no stack args. Called from `HandleMessage` case 0x44
        /// (ShowDamage); copies pending damage from `_field_188`/`_field_18C`
        /// into the active health-display fields.
        fn/Usercall WORM_ENTITY_COMMIT_PENDING_HEALTH = 0x00510830;
        /// `WormEntity::CancelActiveWeapon_Maybe`. Usercall `(ESI = this)`,
        /// plain RET, no stack args. Called from `HandleMessage` case 0x79
        /// (WeaponClaimControl); ends the worm's active weapon if one is in
        /// flight.
        fn/Usercall WORM_ENTITY_CANCEL_ACTIVE_WEAPON = 0x0050E790;
        /// `WormEntity::ApplyDamage_Maybe`. Usercall
        /// `(ESI = this, [stack] = arg1, arg2)`, RET 0x8. Called from
        /// `HandleMessage` case 0x42 (AdvanceWorm) with `(arg1=1, arg2=1)` —
        /// applies the per-frame damage tick used by Drown/Strangle/etc.
        fn/Usercall WORM_ENTITY_APPLY_DAMAGE = 0x0050F580;
        /// `WormEntity::SelectWeapon_Maybe`. Usercall
        /// `(EDI = this, [stack] = weapon_id, ammo_count)`, RET 0x8. Called
        /// from `HandleMessage` case 0x33 (SelectWeapon).
        fn/Usercall WORM_ENTITY_SELECT_WEAPON = 0x0051AE50;
        /// `WormStartFiring`. Usercall `(EAX = this)`, plain RET, no stack
        /// args. 551 instructions, cyclo 108 — too large to port; bridged
        /// from `HandleMessage` case 0x26 (FireWeapon).
        fn/Usercall WORM_ENTITY_START_FIRING = 0x0051B7F0;
        /// `WormEntity::ClearWeaponState_Maybe`. Usercall `(ESI = this)`,
        /// plain RET, no stack args. Sibling of `CancelActiveWeapon` —
        /// called from `FinishTurn` (msg 0x37) when `shot_data_1 != 0` or
        /// the network flag is set; full teardown including SharedData
        /// notification and weapon-table fields.
        fn/Usercall WORM_ENTITY_CLEAR_WEAPON_STATE = 0x0050E710;
        /// `WormEntity::BroadcastWeaponName_Maybe`. Thiscall
        /// `(ECX = this, [stack] = name_str_ptr, flag)`, RET 0x8. Forwards
        /// to `FUN_005480F0(this, name_str_ptr, *(this+0x10c)+0x11, this+0x2f0)`.
        /// Called from `StartTurn` (msg 0x34) with the resolved
        /// `LocalizedTemplate` token 0x69D and `flag = 1`.
        fn/Thiscall WORM_ENTITY_BROADCAST_WEAPON_NAME = 0x0050D540;
        /// `WormEntity::BroadcastWeaponSettings_Maybe`. Fastcall
        /// `(ECX = this)`, plain RET, no stack args. Called from `StartTurn`
        /// (msg 0x34) only when `selected_weapon != None`. Decodes the active
        /// weapon's `WeaponSpawn` descriptor and broadcasts a settings string
        /// via the same SharedData observer used by weapon-name announcements.
        fn/Fastcall WORM_ENTITY_BROADCAST_WEAPON_SETTINGS = 0x00510600;
        /// `WormEntity::SelectFuse`. Usercall
        /// `(EDX = fuse_value, ESI = this)`, plain RET, no stack args. Called
        /// from `HandleMessage` case 0x2F. Writes
        /// `worm.selected_fuse_value` (+0x2BC) after decoding the active
        /// `WeaponSpawn`, then broadcasts updated settings via
        /// `BroadcastWeaponSettings_Maybe`.
        fn/Usercall WORM_ENTITY_SELECT_FUSE = 0x00510430;
        /// `WormEntity::SelectBounce`. Usercall
        /// `(EAX = bounce_value, ESI = this)`, plain RET, no stack args.
        /// Called from `HandleMessage` case 0x31. Writes
        /// `worm.selected_bounce_flag` (+0x2C0), XOR-toggling the previous
        /// value when the message carries `-1`, then broadcasts updated
        /// settings.
        fn/Usercall WORM_ENTITY_SELECT_BOUNCE = 0x005104D0;
        /// `WormEntity::SelectHerd`. Usercall
        /// `(EAX = herd_value, ESI = this)`, plain RET, no stack args. Called
        /// from `HandleMessage` case 0x30. Writes
        /// `worm.selected_herd_index` (+0x2C4), cycling `% iVar2` when the
        /// message carries `-1` (capped to `selected_weapon_ammo` when
        /// positive and below the cycled value), then broadcasts updated
        /// settings.
        fn/Usercall WORM_ENTITY_SELECT_HERD = 0x00510540;
        /// `WormEntity::CanIdleSound_Maybe`. Usercall `(EAX = this)`, plain
        /// RET, returns `i32` in EAX (nonzero ⇒ idle sound permitted). Called
        /// from case 0x5 (UpdateNonCritical) — gates the idle-sound emission
        /// alongside `stationary_frames > 499`.
        fn/Usercall WORM_ENTITY_CAN_IDLE_SOUND = 0x0050E5E0;
    }

    class "WeaponSpawn" {
        /// `WeaponSpawn::DecodeDescriptor_Maybe`. Usercall
        /// `(EAX = out_eax_ptr, EDX = out_edx_ptr)` + 7 stack args
        /// `(descriptor, out2..out7)`, RET 0x1C. Decodes a weapon's spawn
        /// descriptor at `weapon_table.entries[id].fire_params` (offset
        /// 0x510 inside `GameWorld`, stride 0x1D0) and writes 9 output
        /// flags/ints into the caller's stack slots. Case 0x5
        /// (UpdateNonCritical) only inspects two of those outputs (arg3 +
        /// arg4) — both being zero means "no aim sprite required".
        fn/Usercall WEAPON_SPAWN_DECODE_DESCRIPTOR = 0x00565C10;
    }

    class "TeamArena" {
        /// `SetActiveWorm_Maybe`. Usercall
        /// `(EAX = team_arena_base /* world+0x4628 */, EDX = team_idx,
        /// ESI = activate_value)`, plain RET. Maintains the
        /// "currently-active worm" registry at `world+0x2C30..+0x2C44` and
        /// toggles a per-team-block flag at `team_idx*0x51c - 8`. ESI=0
        /// deactivates; non-zero is stored as the active-worm marker (the
        /// caller in `StartTurn` passes `worm_index` directly, in
        /// `FinishTurn` passes 0).
        fn/Usercall TEAM_ARENA_SET_ACTIVE_WORM = 0x00522500;
    }

    class "GameRuntime" {
        /// `FUN_00547D80` — fastcall `(ECX = entity_owning_world,
        /// EDX = arg)`, plain RET. Used by `WormEntity::FinishTurn` with
        /// `arg = 0xE`. Resets a 14×0x14-byte queue at `world+0x73B0..` and
        /// clears `world+0x739C`; stores `EDX` into `world+0x72E4`. Likely
        /// some pending-event clearing.
        fn/Fastcall WORM_FINISH_TURN_CLEANUP = 0x00547D80;
    }

    class "EntityActivityQueue" {
        /// `EntityActivityQueue::ResetRank` (0x00541790) — usercall
        /// `(EAX = queue, [stack] = slot)`, RET 0x4. Despite WA's "release"
        /// shape, this does NOT free the slot: it zeroes `ages[slot]` and
        /// "ages up" every younger slot, effectively promoting the calling
        /// entity to "newest" in the activity ranking. The genuine free is
        /// at 0x00541860 (`FreeSlotById`, called only from destructors).
        /// The queue is the
        /// [`EntityActivityQueue`](crate::engine::EntityActivityQueue)
        /// embedded in `GameWorld` at offset `0x600`. Negative `slot` =
        /// release-all (bumps every active slot's age, sets `count = 0`).
        fn/Usercall ENTITY_ACTIVITY_QUEUE_RESET_RANK = 0x00541790;
    }
}

/// Virtual method table for WormEntity (vtable at 0x6644C8, 20 slots).
///
/// WormEntity overrides 14 of the 20 inherited BaseEntity/WorldEntity slots;
/// 6 slots pass through unchanged. Slot layout by vtable byte offset:
///
/// ```text
/// Slot  Offset  Name                 Base class
/// ----  ------  -------------------  ----------
///  0    0x00    WriteReplayState     BaseEntity (overridden)
///  1    0x04    Free                 BaseEntity (overridden)
///  2    0x08    HandleMessage        BaseEntity (overridden)
///  3    0x0C    GetEntityData        BaseEntity (overridden)
///  4    0x10    (stub, returns 0)    BaseEntity — inherited
///  5    0x14    ProcessChildren      BaseEntity — inherited
///  6    0x18    ProcessFrame         BaseEntity — inherited
///  7    0x1C    OnContactEntity      WorldEntity (overridden)
///  8    0x20    OnWormPush           WorldEntity (overridden)
///  9    0x24    OnLandBounce         WorldEntity (overridden)
/// 10    0x28    OnLandSlide          WorldEntity? (new)
/// 11    0x2C    OnSink               WorldEntity? (new)
/// 12    0x30    (inherited)          WorldEntity
/// 13    0x34    OnKilled             WorldEntity (overridden)
/// 14    0x38    SetState             WormEntity (new)
/// 15    0x3C    CheckPendingAction   WormEntity (new)
/// 16    0x40    IsNotOnRope          WormEntity (new)
/// 17    0x44    (inherited)          WorldEntity
/// 18    0x48    GetTeamIndex         WormEntity (new)
/// 19    0x4C    (inherited)          WorldEntity
/// ```
#[openwa_game::vtable(size = 20)]
pub struct WormEntityVtable {
    /// WriteReplayState — serializes worm state to a replay stream
    #[slot(0)]
    pub write_replay_state: fn(this: *mut WormEntity, stream: *mut u8),
    /// Free — calls inner destructor, then `_free(this)` if flags & 1
    #[slot(1)]
    pub free: fn(this: *mut WormEntity, flags: u8) -> *mut WormEntity,
    /// HandleMessage — processes all EntityMessages sent to this worm
    #[slot(2)]
    pub handle_message: fn(
        this: *mut WormEntity,
        sender: *mut BaseEntity,
        msg_type: u32,
        size: u32,
        data: *const u8,
    ),
    /// GetEntityData — returns worm data by query code
    #[slot(3)]
    pub get_entity_data: fn(this: *mut WormEntity, query: u32, param: u32, out: *mut u32) -> u32,
    // Slots 4-6: Inherited BaseEntity stubs (auto-filled as usize)
    /// OnContactEntity — handles physical contact with another entity
    #[slot(7)]
    pub on_contact_entity: fn(this: *mut WormEntity, other: *mut WorldEntity, flags: u32) -> u32,
    /// OnWormPush — post-contact worm-worm push impulse
    #[slot(8)]
    pub on_worm_push: fn(this: *mut WormEntity, other: *mut WorldEntity, flags: u32) -> u32,
    /// OnLandBounce — worm lands on terrain; plays thud sound, bounce physics
    #[slot(9)]
    pub on_land_bounce: fn(this: *mut WormEntity),
    /// OnLandSlide — secondary landing callback; sliding/friction physics
    #[slot(10)]
    pub on_land_slide: fn(this: *mut WormEntity),
    /// OnSink — worm sinks in water/acid; transitions to drowning state
    #[slot(11)]
    pub on_sink: fn(this: *mut WormEntity, dx: i32, dy: i32) -> u32,
    // Slot 12: Inherited (auto-filled)
    /// OnKilled — worm death; plays death sound, transitions to dead state
    #[slot(13)]
    pub on_killed: fn(this: *mut WormEntity),
    /// SetState — worm state machine; handles all state transitions
    #[slot(14)]
    pub set_state: fn(this: *mut WormEntity, state: KnownWormState),
    /// CheckPendingAction — if field +0xBC is set, calls SetState(0x73)
    #[slot(15)]
    pub check_pending_action: fn(this: *mut WormEntity),
    /// IsNotOnRope — returns true if worm state != 0x7C (rope-swinging)
    #[slot(16)]
    pub is_not_on_rope: fn(this: *const WormEntity) -> bool,
    // Slot 17: Inherited (auto-filled)
    /// GetTeamIndex — returns worm's team index (field +0xFC)
    #[slot(18)]
    pub get_team_index: fn(this: *const WormEntity) -> u32,
    // Slot 19: Inherited (auto-filled)
}

/// Worm entity entity — the primary playable character in WA.
///
/// Extends WorldEntity (0xFC bytes) with worm identity, physics overrides, and
/// per-worm state. Total size: 0x3FC bytes.
///
/// Constructor: 0x50BFB0 (stdcall, 5 params):
///   this, parent_task, team_index, worm_index, init_data_ptr
///
/// Vtable at 0x6644C8. Class type byte: 0x12.
///
/// # Important fields in the WorldEntity base
/// The worm state field lives at **offset +0x44** (inside `base.subclass_data`).
/// Use [`WormEntity::state`] to read it without pointer arithmetic.
///
/// Source: Ghidra decompilation of 0x50BFB0, vtable analysis of 0x6644C8,
///         wkJellyWorm WormEntity.h
#[derive(FieldRegistry)]
#[repr(C)]
pub struct WormEntity {
    /// 0x00–0xFB: WorldEntity base (position, velocity, sound emitter, etc.)
    pub base: WorldEntity<*const WormEntityVtable>,

    /// 0xFC: Team index (0-based); 3rd constructor param
    pub team_index: u32,
    /// 0x100: Worm index within team (0-based); 4th constructor param
    pub worm_index: u32,
    /// 0x104: This worm currently controls its team's turn.
    /// Set to 1 by `StartTurn` (msg 0x34), cleared to 0 by `FinishTurn` (msg 0x37).
    /// Gates many turn-only operations in `HandleMessage` (weapon select, fire,
    /// movement, etc.). Also checked in `OnContactEntity` and the kill-event
    /// dispatch in `vt_set_state` to take a dying-active-worm path.
    pub turn_active: u32,
    /// 0x108: Worm's turn is paused (e.g. weapon UI overlay open).
    /// Set to 1 by `PauseTurn` (msg 0x35), cleared to 0 by `ResumeTurn` (msg 0x36).
    pub turn_paused: u32,
    /// 0x10C: Unknown
    pub _unknown_10c: u32,
    /// 0x110–0x137: Ten u32s copied from spawn init_data (5th constructor param)
    pub spawn_params: [u32; 10],
    /// 0x138–0x13F: Unknown
    pub _unknown_138: [u8; 8],
    /// 0x140: Set to 1 by `TeamVictory` (msg 0x14). Companion of `_field_14c`;
    /// both flags are written together as a "team has won this round" marker.
    /// Reader TBD.
    pub _field_140: u32,
    /// 0x144: Poison source bitmask — tracks which alliances have poisoned this worm.
    /// PoisonWorm handler (msg 0x51): `poison_source_mask |= alliance_bit`.
    /// Prevents double-poisoning from the same source.
    pub poison_source_mask: u32,
    /// 0x148: Poison damage per turn. 0 = not poisoned.
    /// PoisonWorm handler: `poison_damage += msg_data[0]`.
    /// ApplyPoison handler (msg 0x3E): subtracts this from health each turn.
    pub poison_damage: i32,
    /// 0x14C: Set to 1 by `TeamVictory` (msg 0x14) — see `_field_140`.
    pub _field_14c: u32,
    /// 0x150: Unknown (slot 9 in GetEntityData query 0x7D4 output)
    pub _unknown_150: u32,
    /// 0x154: Took-damage marker. Set to 1 by `WormDamaged` (msg 0x47) when
    /// the team+worm pair matches this worm. Cleared in some SetState transitions.
    pub took_damage_flag: u32,
    /// 0x158: This worm's slot ID in `GameWorld.entity_activity_queue`
    /// (assigned at construction, freed at destruction). Read by
    /// `BehaviorTick`'s water-death path as `ages[slot]` to derive the
    /// stagger delay for the worm's score-bubble.
    pub activity_rank_slot: u32,
    /// 0x15C: Write-only mirror of `game_info+0xD926` (a per-scheme facing
    /// byte). Damage paths (cases 0x1C/0x76, 0x4B, plus a few state-reset
    /// branches inside case 0x24) copy that scheme byte here on every hit;
    /// the reader is not yet identified.
    pub _field_15c: u32,
    /// 0x160: Sticky "already-took-this-event" lockout. Set to `1` near the
    /// top of cases 0x1C/0x76 / 0x3B / 0x3E / 0x4B before applying damage,
    /// blocks re-entry until something else clears it (TBD).
    pub damage_lockout_flag: u32,
    /// 0x164: Frames the worm has stayed stationary (no movement). Resets on movement.
    pub stationary_frames: u32,
    /// 0x168: Snapshot of `world._field_5d8` taken at `StartTurn` (msg 0x34).
    /// `FinishTurn` (msg 0x37) computes a turn-duration delta against this and
    /// accumulates `(now - this + 999) / 1000` into the per-worm
    /// `WormEntry.turn_action_counter_Maybe` field at +0x4098.
    pub turn_start_field_5d8: u32,
    /// 0x16C: Mirror written by `FinishTurn`: `worm.turn_end_field_5d8 =
    /// world._field_5d8`. Read alongside `turn_start_field_5d8` for the delta.
    pub turn_end_field_5d8: u32,
    /// 0x170: Currently selected weapon ID.
    pub selected_weapon: KnownWeaponId,
    /// 0x174: Ammo count for the currently selected weapon.
    /// Snapshotted by `SelectWeapon` (msg 0x33) via `GetAmmo()` and used to
    /// gate firing (when 0, weapon cannot be fired) and to track minimum
    /// ammo seen (`_field_b1`).
    pub selected_weapon_ammo: i32,
    /// 0x178: Display health (animated toward target). Used for health bar interpolation.
    /// Stored as `00 00 XX 00` where XX is health — actual layout is u16 at +0x17A.
    pub display_health_raw: u32,
    /// 0x17C: Target health (matches WormEntry.health). Same byte layout as display_health.
    pub target_health_raw: u32,
    /// 0x180: Cleared by the damage paths (cases 0x1C/0x76, 0x3B, 0x3E, 0x4B)
    /// only when `_field_184` is nonzero. Reader / paired-flag semantics TBD.
    pub _field_180: u32,
    /// 0x184: Read-only gate that controls whether `_field_180` is cleared
    /// on a damage event. Writers TBD.
    pub _field_184: u32,
    /// 0x188: Damage-taken accumulator for the current turn. Cases 0x1C/0x76,
    /// 0x3B, 0x3E, 0x4B add `applied_damage` here either pre- or post-clamp
    /// depending on scheme byte `0xD94A` ("kaboom counter" / "true damage"
    /// scheme toggle).
    pub damage_taken_this_turn: i32,
    /// 0x18C–0x18F: Unknown
    pub _unknown_18c: [u8; 4],
    /// 0x190: Secondary position/mode field. Air strike temporarily swaps this with +0x34.
    pub _unknown_190: i32,
    /// 0x194–0x197: Unknown
    pub _unknown_194: [u8; 4],
    /// 0x198: Cleared after air strike fire.
    pub _unknown_198: i32,
    /// 0x19C: Cleared after air strike fire.
    pub _unknown_19c: i32,
    /// 0x1A0–0x1A7: Unknown
    pub _unknown_1a0: [u8; 0x1A8 - 0x1A0],
    /// 0x1A8: Facing direction copy. -1 = left, +1 = right (same as +0x3DC).
    pub facing_direction_2: i32,
    /// 0x1AC: Inverted facing direction. +1 = left, -1 = right.
    pub facing_direction_inv: i32,
    /// 0x1B0–0x1BB: Unknown
    pub _unknown_1b0: [u8; 0x1BC - 0x1B0],
    /// 0x1BC: Cleared by `StartTurn` (msg 0x34). Semantics TBD; only known
    /// writer is the turn-start initialization.
    pub _field_1bc: u32,
    /// 0x1C0: Worm is in retreat phase (post-fire timer until turn ends).
    /// Set to 1 by `RetreatStarted` (msg 0x3C), cleared to 0 by `RetreatFinished` (msg 0x3D).
    pub retreat_active: u32,
    /// 0x1C4: Thinking (chevrons-over-head) animator state.
    /// 1 = ramping up (set by `ThinkingShow`, msg 0x1A), 2 = ramping down
    /// (set by `ThinkingHide`, msg 0x1B). Cleared to 0 once the value at
    /// `thinking_anim` saturates.
    pub thinking_state: u32,
    /// 0x1C8: Thinking animator value (0..0x10000). Driven by `thinking_state`
    /// in `WormEntity::BehaviorTick`: increments by `0x51E/frame` while ramping.
    pub thinking_anim: u32,
    /// 0x1CC: Snapshot of `pos_x` taken when `thinking_state` transitions 1→2
    /// by `WormEntity__BeginThinkingHide` (called via msgs 0x1B / 0x37).
    /// Used by `WormEntity__DrawCursorMarker_Maybe` to keep the chevrons sprite
    /// anchored at the worm's old position while it fades out.
    pub thinking_anim_pos_x: Fixed,
    /// 0x1D0: Snapshot of `pos_y` (companion to `thinking_anim_pos_x`).
    pub thinking_anim_pos_y: Fixed,
    /// 0x1D4: Edge-triggered "move up" request. Set to 1 by `MoveUp` (msg
    /// 0x20) when `weapons_enabled != 0`. Consumed and cleared by
    /// `WormEntity::DrainInputBuffer` (0x005148E0).
    pub input_msg_move_up: u32,
    /// 0x1D8: Edge-triggered "move down" request. Set to 1 by `MoveDown`
    /// (msg 0x21) when `weapons_enabled != 0`. Same drain as `move_up`.
    pub input_msg_move_down: u32,
    /// 0x1DC: Edge-triggered "move left" request. Set to 1 by `MoveLeft`
    /// (msg 0x1E) unconditionally. Same drain as `move_up`.
    pub input_msg_move_left: u32,
    /// 0x1E0: Edge-triggered "move right" request. Set to 1 by `MoveRight`
    /// (msg 0x1F) unconditionally. Same drain as `move_up`.
    pub input_msg_move_right: u32,
    /// 0x1E4–0x1E7: Unknown
    pub _unknown_1e4: [u8; 4],
    /// 0x1E8: Y-axis impulse magnitude consumed by `Jump` (msg 0x24) in the
    /// `RopeSwinging` (0x7C) state — the case forwards this value as the
    /// `impulse_y` argument to `WorldEntity::add_impulse` (vtable slot 17)
    /// before transitioning to `WeaponCharging` (0x73). Writers TBD.
    pub _field_1e8: i32,
    /// 0x1EC: Movement streak counter. Increases ~once per second while moving
    /// in one direction. Resets to 0 when movement resumes after a stop.
    /// Set to -1 when the worm is blocked (e.g. hits a wall).
    pub movement_streak: i32,
    /// 0x1F0–0x203: Unknown
    pub _unknown_1f0: [u8; 0x204 - 0x1F0],
    /// 0x204: Per-event damage accumulator. Damage paths (0x1C/0x76, 0x4B)
    /// add the message's `damage` field here on each hit. Distinct from
    /// `damage_taken_this_turn` — likely a kill-credit or scoreboard total.
    pub damage_event_accum: i32,
    /// 0x208: Action/mode flag. Checked in air strike (must be 0 to fire).
    /// Cleared after air strike completes.
    pub _unknown_208: i32,
    /// 0x20C–0x217: Unknown
    pub _unknown_20c: [u8; 0x218 - 0x20C],
    /// 0x218: Set to Fixed(1.0) when SpecialImpact (msg 0x4B) is received
    /// with `damage_kind == 1` (drowning). Probably a fade/animation
    /// strength for the drown sprite — reader not yet pinned down.
    pub drown_marker: Fixed,
    /// 0x21C–0x247: Unknown
    pub _unknown_21c: [u8; 0x248 - 0x21C],
    /// 0x248: Landscape scale factor for spawn offset calculation (Fixed 16.16).
    /// Used by WeaponRelease to convert aim params to world-space projectile offsets.
    pub landscape_scale: Fixed,
    /// 0x24C: Aim angle (fixed-point 16.16, range 0..0x10000 = 0..360 degrees).
    pub aim_angle: Fixed,
    /// 0x250: Cleared by `FinishTurn` (msg 0x37) only when
    /// `world.version_flag_4 == 0` (pre-v3.5 schemes). Reader TBD.
    pub _field_250: u32,
    /// 0x254-0x257: Unknown
    pub _unknown_254: [u8; 4],
    /// 0x258: Cleared unconditionally by `FinishTurn` (msg 0x37). Reader TBD.
    pub _field_258: u32,
    /// 0x25C-0x267: Unknown
    pub _unknown_25c: [u8; 0xC],
    /// 0x268: Show aiming cursor flag (nonzero = cursor visible).
    pub show_cursor: u32,
    /// 0x26C–0x27F: Unknown
    pub _unknown_26c: [u8; 0x14],
    /// 0x280: Kill request kind. Set by `KillWorm` (msg 0x40): `1` = plain kill,
    /// `2` = variant (msg 0x41). Consumed by `WormEntity::BehaviorTick` to
    /// fire the kill `SetState(0x82|0x84)`.
    pub kill_request: u32,
    /// 0x284: Shot/aim data 1 (slot 5 in GetEntityData query 0x7D4).
    /// For weapons 0x22/0x24 (Teleport/Freeze), copied from shot_data_2 on first fire.
    /// Sent in HandleMessage 0x49 buffer during WeaponRelease.
    pub shot_data_1: u32,
    /// 0x288: Shot/aim data 2 (slot 6 in GetEntityData query 0x7D4).
    pub shot_data_2: u32,
    /// 0x28C: Per-turn "weapons enabled" flag. Set to 1 by msg
    /// `EnableWeapons` (0x45); cleared by msg `DisableWeapons` (0x46) /
    /// `DeactivateOnIdle`. Required non-zero for `ReleaseWeapon` to act.
    pub weapons_enabled: u32,
    /// 0x290: Fire sync frame counter 1. Compared with fire_sync_frame_2
    /// in WeaponRelease; when equal, weapon slot table is reset.
    pub fire_sync_frame_1: i32,
    /// 0x294: Fire sync frame counter 2.
    pub fire_sync_frame_2: i32,
    /// 0x298: Unknown
    pub _unknown_298: u32,
    /// 0x29C: Jump-request marker. Set to `1` by `Jump` (msg 0x24) when the
    /// worm transitions out of an idle state (0x65/0x66/0x67/0x88/0x8B) into
    /// 0x77 (PreJump). Set to `-1` by `Jump` when already in 0x77 (jump-cancel
    /// re-entry). Cleared by `JumpUp` (msg 0x25) on the same idle→0x77
    /// transition.
    pub _field_29c: i32,
    /// 0x2A0: Jump-release marker. Set to `1` by `JumpUp` (msg 0x25) when
    /// state==0x77 and `_field_29c == 0` (i.e. the JumpUp arrived without a
    /// preceding Jump-cancel). Cleared together with `_field_29c` on the
    /// idle→0x77 transition.
    pub _field_2a0: i32,
    /// 0x2A4–0x2AB: Unknown
    pub _unknown_2a4: [u8; 0x2AC - 0x2A4],
    /// 0x2AC: Snapshot of `world._field_5cc` (the running frame counter)
    /// taken when a damage-grunt sound plays. Cases 0x1C/0x76 and 0x4B
    /// rate-limit the sound to fire at most once per 24 frames.
    pub last_damage_sound_frame: i32,
    /// 0x2B0: Damage-stack count (accumulated by case 0x4B). Cleared at
    /// `TurnStarted` (msg 0x38).
    pub damage_stack_count: u32,
    /// 0x2B4–0x2BB: Unknown
    pub _unknown_2b4: [u8; 0x2BC - 0x2B4],
    /// 0x2BC: Per-worm selected fuse value, written by `SelectFuse_Maybe`
    /// (msg 0x2F). Cycled in `[0, iVar3-1]` where `iVar3` is `5` offline /
    /// `10` online (with `fe_version < 0x1B`). Read at fire time by
    /// `WeaponRelease`, which forwards `(value + 1) * 1000` ms as the
    /// fuse-timer slot of the [`WeaponReleaseContext`]. The range-checked
    /// SelectFuse path also accepts the sentinel `0xFF` when scheme byte
    /// 0xD9B1 > 0x1F and the message carried `-1` — empirically a
    /// "remember last fuse" marker.
    pub selected_fuse_value: i32,
    /// 0x2C0: Per-worm selected bounce flag, written by
    /// `SelectBounce_Maybe` (msg 0x31). Toggled (XOR 1) when the message
    /// carries `-1`. Read at fire time by `WeaponRelease` to set the
    /// bounce-settle delay slot of [`WeaponReleaseContext`] (`0` ⇒ 30
    /// frames, `1` ⇒ 60 frames).
    pub selected_bounce_flag: i32,
    /// 0x2C4: Per-worm selected herd cycle index, written by
    /// `SelectHerd_Maybe` (msg 0x30). Cycled `% iVar2` (5 / 9 / 10
    /// depending on scheme bytes 0xD9D0 / 0xD9B1) when the message carries
    /// `-1`. Capped to `selected_weapon_ammo` if the cap is positive and
    /// less than the cycled value.
    pub selected_herd_index: i32,
    /// 0x2C8: Network/sound condition flag. In type 2 (rope) sound dispatch,
    /// sound plays only when this is 0 OR when it equals 1.
    pub _unknown_2c8: i32,
    /// 0x2CC: Network flag. Nonzero = network mode active.
    /// Used in sound dispatch conditions and HandleMessage 0x49 buffer.
    pub _unknown_2cc: i32,
    /// 0x2D0–0x2DB: Unknown
    pub _unknown_2d0: [u8; 0x2DC - 0x2D0],
    /// 0x2DC: Cliff-fall flag. Cleared at `TurnStarted` (msg 0x38).
    pub cliff_fall_flag: u32,
    /// 0x2E0: Weapon parameter 1. Polymorphic per weapon:
    /// - WeaponRelease: ammo_per_turn (copied to release context)
    /// - Air Strike: fire position X
    /// - Freeze: freeze effect X position
    pub weapon_param_1: i32,
    /// 0x2E4: Weapon parameter 2. Polymorphic per weapon:
    /// - WeaponRelease: ammo_per_slot (copied to release context)
    /// - Air Strike: fire position Y
    /// - Freeze: freeze effect Y position
    pub weapon_param_2: i32,
    /// 0x2E8: Cursor/aim parameter, typed i32. StartTurn (msg 0x34) sets
    /// to `-1` for `game_version < 0x103` (default cursor) and `0` for
    /// later versions; SelectCursor (msg 0x32) overrides from the
    /// incoming message payload at offset 12.
    pub _field_2e8: i32,
    /// 0x2EC: Weapon parameter 3 / launch count.
    /// - Weapons 0x22/0x24: checked == 0 to copy shot_data_2 → shot_data_1
    /// - Freeze: freeze target entity ID
    pub weapon_param_3: i32,
    /// 0x2F0: Worm name, null-terminated (max 17 chars, from spawn init_data+3)
    #[field(kind = "CString")]
    pub worm_name: [u8; 0x11],
    /// 0x301: Country / team name from scheme, null-terminated (max 17 chars)
    #[field(kind = "CString")]
    pub country_name: [u8; 0x11],
    /// 0x312: Health display string, null-terminated ASCII (e.g. "100", "88").
    /// Updated when health changes for the floating health number display.
    #[field(kind = "CString")]
    pub health_text: [u8; 0x09],
    /// 0x31B: Poison damage display string, null-terminated ASCII (e.g. "5", "").
    /// Shown as the green poison damage number above the worm.
    #[field(kind = "CString")]
    pub poison_text: [u8; 0x09],
    /// 0x324–0x32F: Unknown
    pub _unknown_324: [u8; 0x330 - 0x324],
    /// 0x330: Remote-detonation crate triggered. Set to 1 by `DetonateCrate`
    /// (msg 0x62).
    pub detonate_crate_flag: u32,
    /// 0x334: Facing direction copy. -1 = left, +1 = right (same as +0x3DC).
    pub facing_direction_3: i32,
    /// 0x338: Facing-related flag. Cleared at `TurnStarted` (msg 0x38).
    pub facing_flag: u8,
    /// 0x339: Unknown
    pub _unknown_339: u8,
    /// 0x33A: Saved-aim flag. When set at `TurnStarted`, the aim angle is
    /// snapped to the nearest quadrant and the flag is cleared.
    pub saved_aim_flag: u8,
    /// 0x33B–0x33F: Unknown
    pub _unknown_33b: [u8; 5],
    /// 0x340: Poison-tick accumulator. Cleared at `TurnStarted` (msg 0x38).
    pub poison_tick_accum: u32,
    /// 0x344–0x367: Unknown
    pub _unknown_344: [u8; 0x368 - 0x344],
    /// 0x368: Animator / controller object (dispatched via vtable for state animations)
    pub animator: *mut u8,
    /// 0x36C: Active weapon entry pointer. Points to `&WeaponTable.entries[selected_weapon]`.
    /// Contains fire type (+0x30), subtypes (+0x34/+0x38), and completion flag (+0x3C).
    /// Used by WeaponRelease: `MOV EAX, [EDI+0x36C]` before calling FireWeapon.
    pub active_weapon_entry: *mut WeaponEntry,
    /// 0x370–0x377: Unknown (rope anchor, weapon-specific data, etc.)
    pub _unknown_370: [u8; 8],
    /// 0x378–0x397: Aim-fade animation values (8 × Fixed 16.16, default 1.0 = 0x10000).
    /// Reset to 1.0 by `WeaponFinished` (msg 0x49) for Bungee weapons.
    pub aim_fade: [Fixed; 8],
    /// 0x398: Aux ease value (Fixed). Eased toward [`_field_39c`] in
    /// `WormEntity::EaseAuxValue` (case 0x5, UpdateNonCritical) using the
    /// generic 10%-step `linear_ease_with_min_step` primitive (min step =
    /// `0x1999`). When the eased value is non-zero AND `turn_active != 0`,
    /// case 0x5 zeros `aim_fade[5]` and `aim_fade[7]` to suppress the
    /// aim-arrow targets.
    pub _field_398: Fixed,
    /// 0x39C: Target value the [`_field_398`] aux is eased toward.
    pub _field_39c: Fixed,
    /// 0x3A0: "No aim sprite required" flag, set by `HandleMessage` case
    /// 0x5 (UpdateNonCritical) when either `selected_weapon == None` or
    /// `WeaponSpawn::DecodeDescriptor`'s arg3 + arg4 outputs are both 0.
    /// Reader semantics TBD — almost certainly drives the per-frame aim
    /// indicator render path.
    pub _field_3a0: u32,
    /// 0x3A4: Last seen value of `world._field_7640`. Case 0x5
    /// (UpdateNonCritical) early-returns once `turn_active != 0` and the
    /// world value has changed; the new value is stored here and aim_fade
    /// slots [1] / [7] are reset to 1.0.
    pub _field_3a4: u32,
    /// 0x3A8–0x3AF: Unknown
    pub _unknown_3a8: [u8; 0x3B0 - 0x3A8],
    /// 0x3B0: Streaming sound handle. Nonzero when a worm sound effect
    /// (e.g., weapon charge-up) is actively playing. PlayWormSound stores the
    /// new handle here; StopWormSound clears it.
    pub sound_handle: i32,
    /// 0x3B4: Secondary sound handle, used by FUN_00515020 (teleport/weapon sounds).
    /// Same stop/play semantics as `sound_handle` but a separate channel.
    pub sound_handle_2: i32,
    /// 0x3B8–0x3DB: Unknown
    pub _unknown_3b8: [u8; 0x3DC - 0x3B8],
    /// 0x3DC: Facing direction. -1 = facing left, +1 = facing right.
    pub facing_direction: i32,
    /// 0x3E0–0x3E3: Unknown
    pub _unknown_3e0: u32,
    /// 0x3E4: Input: aim up key held (nonzero = adjusting aim upward).
    pub input_aim_up: u32,
    /// 0x3E8: Input: aim down key held (nonzero = adjusting aim downward).
    pub input_aim_down: u32,
    /// 0x3EC: Input: move left key held (nonzero = worm walking left).
    pub input_move_left: u32,
    /// 0x3F0: Input: move right key held (nonzero = worm walking right).
    pub input_move_right: u32,
    /// 0x3F4–0x3F7: Unknown
    pub _unknown_3f4: [u8; 4],
    /// 0x3F8: Drowning damage running total. Case 0x4B's `damage_kind == 1`
    /// branch adds the post-scale damage here on top of the regular health
    /// decrement; reader TBD (probably feeds the kill-attribution system).
    pub drown_damage_accum: i32,
}

const _: () = assert!(core::mem::size_of::<WormEntity>() == 0x3FC);

/// cdecl-callable impl behind the EAX-passing usercall hook for
/// `WormEntity::CanIdleSound_Maybe` (0x0050E5E0). Returns `1` when the
/// worm holds an unpaused turn, has no per-worm action-pending flag set
/// on its [`WormEntry::_field_98`], AND is not currently in motion;
/// returns `0` otherwise. Two callers — `WormEntity::HandleMessage`
/// case 0x5 (UpdateNonCritical) and `WormEntity::BehaviorTick`.
pub unsafe extern "cdecl" fn worm_can_idle_sound_impl(this: *mut WormEntity) -> i32 {
    unsafe {
        if (*this).turn_active == 0 || (*this).turn_paused != 0 {
            return 0;
        }
        let world = (*(this as *const super::base::BaseEntity)).world;
        let arena: *const crate::engine::team_arena::TeamArena = &raw const (*world).team_arena;
        let entry = crate::engine::team_arena::TeamArena::team_worm(
            arena,
            (*this).team_index as usize,
            (*this).worm_index as usize,
        );
        if (*entry)._field_98 != 0 {
            return 0;
        }
        let is_moving = super::game_entity::WorldEntity::is_moving_raw(
            this as *const super::game_entity::WorldEntity,
        );
        if is_moving { 0 } else { 1 }
    }
}

// Generate typed vtable method wrappers: handle_message(), on_contact_entity(), etc.
bind_WormEntityVtable!(WormEntity, base.base.vtable);

impl WormEntity {
    /// Returns the worm's current state code (lives at offset +0x44, inside
    /// `base.subclass_data`). See [`WormState`] for known values.
    pub fn state(&self) -> WormState {
        WormState(u32::from_ne_bytes(
            self.base.subclass_data[0x14..0x18].try_into().unwrap(),
        ))
    }

    pub fn is_in_state(&self, state: KnownWormState) -> bool {
        self.state() == WormState::from(state)
    }

    // Weapon fire dispatch state:
    // - Fire type/subtypes live in the WeaponEntry (via active_weapon_entry at +0x36C)
    // - Completion flag lives in WorldEntity.subclass_data[12] (this object, at +0x3C)

    /// Weapon fire completion flag at WorldEntity+0x3C (subclass_data[12]).
    /// Set to 0 before FireWeapon dispatch, 1 after.
    pub fn fire_complete(&self) -> i32 {
        i32::from_ne_bytes(self.base.subclass_data[12..16].try_into().unwrap())
    }

    /// Set the weapon fire completion flag.
    pub fn set_fire_complete(&mut self, value: i32) {
        self.base.subclass_data[12..16].copy_from_slice(&value.to_ne_bytes());
    }

    // vtable() method is now provided by bind_WormEntityVtable! macro above.

    /// Pure Rust port of `WormEntity::LandingCheck_Maybe` (WA 0x0050D450,
    /// `__usercall(ESI=this)`, plain RET).
    ///
    /// Examines the worm's position and state and records a landing-event
    /// bbox at one of the per-kind slots in [`crate::engine::GameWorld::render_entries`].
    /// The kind id (1, 2, 3, 4, 9, 11) classifies the event:
    ///
    /// | Kind | Branch                                                  |
    /// |------|---------------------------------------------------------|
    /// | 1    | dead/dying worm, GameInfo per-team byte == starting team|
    /// | 11   | dead/dying worm, fast-forward set OR byte mismatch      |
    /// | 9    | state == [`WormState::Unknown_0x85`]                    |
    /// | 2    | worm is moving ([`crate::entity::WorldEntity::is_moving_raw`])  |
    /// | 4    | inside the level scroll bbox (+0x779C / +0x77A0 / +0x77A4) |
    /// | 3    | otherwise                                               |
    ///
    /// Off-screen-above (`y < -0x270F0000` ≈ `y < -9999.0` Fixed) and
    /// underwater-kill (`y >> 16 >= world.water_kill_y`) gate the entire
    /// dispatch — both early-out without recording anything.
    pub unsafe fn landing_check_raw(this: *mut WormEntity) {
        unsafe {
            use crate::engine::world::GameWorld;
            use crate::entity::base::BaseEntity;
            use crate::entity::game_entity::WorldEntity;

            let pos_x = (*this).base.pos_x.0;
            let pos_y = (*this).base.pos_y.0;

            // Off-screen-above sanity gate.
            if pos_y < -0x270F0000_i32 {
                return;
            }

            let world = (*(this as *const BaseEntity)).world;
            // Underwater-kill gate.
            if (pos_y >> 16) >= (*world).water_kill_y {
                return;
            }

            let kind: u32 = if (*this).turn_active != 0 {
                // Active worm dying mid-turn — different event kind than passive
                // worm deaths (e.g. collateral damage). `turn_active` is set by
                // `StartTurn` (msg 0x34) and cleared by `FinishTurn` (msg 0x37).
                if (*world).fast_forward_request != 0 {
                    11
                } else {
                    // Compare WA's per-team byte against `starting_team_index`:
                    //   byte at GameInfo + team_index * 0xBB8 - 0x768
                    // For team_index == 1 this hits `team_records[0].speech_bank_id`
                    // (the alliance group); higher indices step through the per-team
                    // records. The team_index == 0 case would read before the struct.
                    // Faithful to WA's address arithmetic; semantics deserve more RE.
                    let game_info = (*world).game_info as *const u8;
                    let team_index = (*this).team_index as i32;
                    let alliance_byte = *game_info.offset((team_index * 0xBB8 - 0x768) as isize);
                    let starting_team = (*(*world).game_info).starting_team_index as u8;
                    if alliance_byte == starting_team {
                        1
                    } else {
                        11
                    }
                }
            } else if (*this).state().is(KnownWormState::Unknown_0x85) {
                9
            } else if WorldEntity::is_moving_raw(&raw const (*this).base as *const WorldEntity) {
                2
            } else if (*world).level_bound_min_x.0 <= pos_x
                && pos_x <= (*world).level_bound_max_x.0
                && (*world).level_bound_min_y.0 <= pos_y
            {
                4
            } else {
                3
            };

            GameWorld::record_landing_event_raw(world, kind, pos_x, pos_y);
        }
    }
}

// ── Snapshot impl ──────────────────────────────────────────

impl crate::snapshot::Snapshot for WormEntity {
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
            writeln!(w, "angle = {}", b.angle)?;
            write_indent(w, i)?;
            writeln!(w, "team_index = {}", self.team_index)?;
            write_indent(w, i)?;
            writeln!(w, "worm_index = {}", self.worm_index)?;
            write_indent(w, i)?;
            writeln!(w, "activity_rank_slot = {}", self.activity_rank_slot)?;
            write_indent(w, i)?;
            writeln!(w, "selected_weapon = {:?}", self.selected_weapon)?;
            write_indent(w, i)?;
            writeln!(w, "facing = {}", self.facing_direction)?;
            write_indent(w, i)?;
            writeln!(w, "aim_angle = {}", self.aim_angle)?;

            write_indent(w, i)?;
            write!(w, "spawn_params =")?;
            for v in &self.spawn_params {
                write!(w, " {:08X}", v)?;
            }
            writeln!(w)?;

            // Raw dump of unknown regions
            write_indent(w, i)?;
            writeln!(w, "_unknown_1f0 ({} bytes):", self._unknown_1f0.len())?;
            write_raw_region(
                w,
                self._unknown_1f0.as_ptr(),
                self._unknown_1f0.len(),
                i + 1,
            )?;
            write_indent(w, i)?;
            writeln!(w, "_unknown_28c ({} bytes):", 4)?;
            write_raw_region(
                w,
                &self.weapons_enabled as *const u32 as *const u8,
                4,
                i + 1,
            )?;
            write_indent(w, i)?;
            writeln!(w, "_unknown_370 ({} bytes):", self._unknown_370.len())?;
            write_raw_region(
                w,
                self._unknown_370.as_ptr(),
                self._unknown_370.len(),
                i + 1,
            )?;
            writeln!(w, "aim_fade: {:?}", self.aim_fade.map(|f| f.0))?;

            Ok(())
        }
    }
}
