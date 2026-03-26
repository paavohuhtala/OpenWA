use super::base::CTask;
use super::game_task::CGameTask;
use crate::game::weapon::WeaponEntry;
use crate::FieldRegistry;

/// Worm state machine states.
///
/// CTaskWorm's `SetState` (vtable slot 14) transitions between these.
/// The state byte lives at CTaskWorm+0x44 (inside `base.subclass_data`).
/// Also stored in WormEntry.state in the TeamArenaState.
///
/// States 0x68..=0x8A are the "weapon/action active" range — checked by
/// `(state - 0x68) < 0x23` in HandleMessage. States 0x80+ are dying/dead.
/// Names are best guesses from behavioral observation and disassembly.
///
/// Source: CTaskWorm::HandleMessage (0x510B40) decompilation + weapon fire dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum WormState {
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
    // 0x69–0x6B: unknown (within weapon/action range)
    /// Blowtorch — using the blowtorch weapon.
    Blowtorch = 0x6C,
    /// Baseball Bat — swinging the baseball bat. Frequently checked in HandleMessage.
    BaseballBat = 0x6D,
    /// Kamikaze — performing kamikaze attack.
    Kamikaze = 0x6E,
    // 0x6F: unknown
    /// Dragon Ball — performing dragon ball attack.
    DragonBall = 0x70,
    /// Scales of Justice — using scales of justice.
    ScalesOfJustice = 0x71,
    /// Suicide Bomber — performing suicide bomber attack.
    SuicideBomber = 0x72,
    /// Weapon charging — entered from aiming states (0x7B, 0x7C) before release.
    /// Also set by CheckPendingAction when field +0xBC is nonzero.
    WeaponCharging_Maybe = 0x73,
    /// Teleport cancelled — teleport failed or was denied.
    TeleportCancelled_Maybe = 0x74,
    /// Fire Punch — performing fire punch attack.
    FirePunch = 0x75,
    // 0x76: unknown
    /// Weapon selected — entered via SelectCursor (msg 0x24) from idle/active states.
    /// HandleMessage sets param[0xa7]=-1 when already in this state.
    WeaponSelected_Maybe = 0x77,
    /// Weapon aimed — post-select state. Teleport check accepts this.
    /// Also used for Magic Bullet weapon fire.
    WeaponAimed_Maybe = 0x78,
    // 0x79–0x7A: unknown
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
    // 0x84–0x85: unknown
    /// Dead — set by Surrender (msg 0x29). Frequently checked in HandleMessage.
    Dead = 0x86,
    /// Dead variant 3.
    Dead3 = 0x87,
    /// Unknown — grouped with idle states (0x65/0x66/0x67/0x8B) in HandleMessage.
    Unknown_0x88 = 0x88,
    /// Dying/special animation (from WormEntry state documentation).
    DyingAnimation_Maybe = 0x89,
    // 0x8A: end of weapon/action range
    /// Unknown state checked in CTaskTeam handlers.
    /// Grouped with idle states in HandleMessage switch cases.
    Unknown_0x8B = 0x8B,
}

crate::define_addresses! {
    class "CTaskWorm" {
        /// CTaskWorm vtable
        vtable CTASK_WORM_VTABLE = 0x0066_44C8;
        /// CTaskWorm constructor
        ctor CTASK_WORM_CONSTRUCTOR = 0x0050_BFB0;
    }
}

/// Virtual method table for CTaskWorm (vtable at 0x6644C8, 20 slots).
///
/// CTaskWorm overrides 14 of the 20 inherited CTask/CGameTask slots;
/// 6 slots pass through unchanged. Slot layout by vtable byte offset:
///
/// ```text
/// 0x00 WriteReplayState  0x04 Free             0x08 HandleMessage
/// 0x0C GetEntityData     0x10-0x18 inherited   0x1C OnContactEntity
/// 0x20 OnWormPush        0x24 OnLandBounce     0x28 OnLandSlide
/// 0x2C OnSink            0x30 inherited        0x34 OnKilled
/// 0x38 SetState          0x3C CheckPendingAction 0x40 IsNotOnRope
/// 0x44 inherited         0x48 GetTeamIndex     0x4C inherited
/// ```
#[openwa_core::vtable(size = 20)]
pub struct CTaskWormVTable {
    /// WriteReplayState — serializes worm state to a replay stream
    #[slot(0)]
    pub write_replay_state: fn(this: *mut CTaskWorm, stream: *mut u8),
    /// Free — calls inner destructor, then `_free(this)` if flags & 1
    #[slot(1)]
    pub free: fn(this: *mut CTaskWorm, flags: u8) -> *mut CTaskWorm,
    /// HandleMessage — processes all TaskMessages sent to this worm
    #[slot(2)]
    pub handle_message: fn(this: *mut CTaskWorm, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
    /// GetEntityData — returns worm data by query code
    #[slot(3)]
    pub get_entity_data: fn(this: *mut CTaskWorm, query: u32, param: u32, out: *mut u32) -> u32,
    // Slots 4-6: Inherited CTask stubs (auto-filled as usize)
    /// OnContactEntity — handles physical contact with another entity
    #[slot(7)]
    pub on_contact_entity: fn(this: *mut CTaskWorm, other: *mut CGameTask, flags: u32) -> u32,
    /// OnWormPush — post-contact worm-worm push impulse
    #[slot(8)]
    pub on_worm_push: fn(this: *mut CTaskWorm, other: *mut CGameTask, flags: u32) -> u32,
    /// OnLandBounce — worm lands on terrain; plays thud sound, bounce physics
    #[slot(9)]
    pub on_land_bounce: fn(this: *mut CTaskWorm),
    /// OnLandSlide — secondary landing callback; sliding/friction physics
    #[slot(10)]
    pub on_land_slide: fn(this: *mut CTaskWorm),
    /// OnSink — worm sinks in water/acid; transitions to drowning state
    #[slot(11)]
    pub on_sink: fn(this: *mut CTaskWorm, dx: i32, dy: i32) -> u32,
    // Slot 12: Inherited (auto-filled)
    /// OnKilled — worm death; plays death sound, transitions to dead state
    #[slot(13)]
    pub on_killed: fn(this: *mut CTaskWorm),
    /// SetState — worm state machine; handles all state transitions
    #[slot(14)]
    pub set_state: fn(this: *mut CTaskWorm, state: WormState),
    /// CheckPendingAction — if field +0xBC is set, calls SetState(0x73)
    #[slot(15)]
    pub check_pending_action: fn(this: *mut CTaskWorm),
    /// IsNotOnRope — returns true if worm state != 0x7C (rope-swinging)
    #[slot(16)]
    pub is_not_on_rope: fn(this: *const CTaskWorm) -> bool,
    // Slot 17: Inherited (auto-filled)
    /// GetTeamIndex — returns worm's team index (field +0xFC)
    #[slot(18)]
    pub get_team_index: fn(this: *const CTaskWorm) -> u32,
    // Slot 19: Inherited (auto-filled)
}

/// Worm entity task — the primary playable character in WA.
///
/// Extends CGameTask (0xFC bytes) with worm identity, physics overrides, and
/// per-worm state. Total size: 0x3FC bytes.
///
/// Constructor: 0x50BFB0 (stdcall, 5 params):
///   this, parent_task, team_index, worm_index, init_data_ptr
///
/// Vtable at 0x6644C8. Class type byte: 0x12.
///
/// # Important fields in the CGameTask base
/// The worm state field lives at **offset +0x44** (inside `base.subclass_data`).
/// Use [`CTaskWorm::state`] to read it without pointer arithmetic.
///
/// Source: Ghidra decompilation of 0x50BFB0, vtable analysis of 0x6644C8,
///         wkJellyWorm CTaskWorm.h
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskWorm {
    /// 0x00–0xFB: CGameTask base (position, velocity, sound emitter, etc.)
    pub base: CGameTask<*const CTaskWormVTable>,

    /// 0xFC: Team index (0-based); 3rd constructor param
    pub team_index: u32,
    /// 0x100: Worm index within team (0-based); 4th constructor param
    pub worm_index: u32,
    /// 0x104: Unknown flag (checked in OnContactEntity)
    pub _unknown_104: u32,
    /// 0x108–0x10F: Unknown
    pub _unknown_108: [u8; 8],
    /// 0x110–0x137: Ten u32s copied from spawn init_data (5th constructor param)
    pub spawn_params: [u32; 10],
    /// 0x138–0x14F: Unknown
    pub _unknown_138: [u8; 0x18],
    /// 0x150: Unknown (slot 9 in GetEntityData query 0x7D4 output)
    pub _unknown_150: u32,
    /// 0x154: Unknown (rope-related; cleared in some SetState transitions)
    pub _unknown_154: u32,
    /// 0x158: Worm pool slot index in DDGame (assigned from pool at construction)
    pub slot_id: u32,
    /// 0x15C–0x163: Unknown
    pub _unknown_15c: [u8; 0x164 - 0x15C],
    /// 0x164: Frames the worm has stayed stationary (no movement). Resets on movement.
    pub stationary_frames: u32,
    /// 0x168–0x16F: Unknown
    pub _unknown_168: [u8; 0x170 - 0x168],
    /// 0x170: Currently selected weapon ID.
    pub selected_weapon: u32,
    /// 0x174–0x1A7: Unknown
    pub _unknown_174: [u8; 0x1A8 - 0x174],
    /// 0x1A8: Facing direction copy. -1 = left, +1 = right (same as +0x3DC).
    pub facing_direction_2: i32,
    /// 0x1AC: Inverted facing direction. +1 = left, -1 = right.
    pub facing_direction_inv: i32,
    /// 0x1B0–0x1EB: Unknown
    pub _unknown_1b0: [u8; 0x1EC - 0x1B0],
    /// 0x1EC: Movement streak counter. Increases ~once per second while moving
    /// in one direction. Resets to 0 when movement resumes after a stop.
    /// Set to -1 when the worm is blocked (e.g. hits a wall).
    pub movement_streak: i32,
    /// 0x1F0–0x247: Unknown
    pub _unknown_1f0: [u8; 0x248 - 0x1F0],
    /// 0x248: Unknown
    pub _unknown_248: u32,
    /// 0x24C: Aim angle (fixed-point 16.16, range 0..0x10000 = 0..360 degrees).
    pub aim_angle: u32,
    /// 0x250–0x267: Unknown
    pub _unknown_250: [u8; 0x268 - 0x250],
    /// 0x268: Show aiming cursor flag (nonzero = cursor visible).
    pub show_cursor: u32,
    /// 0x26C–0x283: Unknown
    pub _unknown_26c: [u8; 0x284 - 0x26C],
    /// 0x284–0x28B: Aiming data (slots 5–6 in GetEntityData query 0x7D4)
    pub _unknown_284: [u32; 2],
    /// 0x28C–0x2EF: Unknown
    pub _unknown_28c: [u8; 0x64],
    /// 0x2F0: Worm name, null-terminated (max 17 chars, from spawn init_data+3)
    #[field(kind = "CString")]
    pub worm_name: [u8; 0x11],
    /// 0x301: Country / team name from scheme, null-terminated (max 17 chars)
    #[field(kind = "CString")]
    pub country_name: [u8; 0x11],
    /// 0x312–0x333: Unknown (rope string, state history, etc.)
    pub _unknown_312: [u8; 0x334 - 0x312],
    /// 0x334: Facing direction copy. -1 = left, +1 = right (same as +0x3DC).
    pub facing_direction_3: i32,
    /// 0x338–0x367: Unknown
    pub _unknown_338: [u8; 0x368 - 0x338],
    /// 0x368: Animator / controller object (dispatched via vtable for state animations)
    pub animator: *mut u8,
    /// 0x36C: Active weapon entry pointer. Points to `&WeaponTable.entries[selected_weapon]`.
    /// Contains fire type (+0x30), subtypes (+0x34/+0x38), and completion flag (+0x3C).
    /// Used by WeaponRelease: `MOV EAX, [EDI+0x36C]` before calling FireWeapon.
    pub active_weapon_entry: *mut WeaponEntry,
    /// 0x370–0x3DB: Unknown (rope anchor, weapon-specific data, etc.)
    pub _unknown_370: [u8; 0x3DC - 0x370],
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
    /// 0x3F4–0x3FB: Unknown
    pub _unknown_3f4: [u8; 0x3FC - 0x3F4],
}

const _: () = assert!(core::mem::size_of::<CTaskWorm>() == 0x3FC);

// Generate typed vtable method wrappers: handle_message(), on_contact_entity(), etc.
bind_CTaskWormVTable!(CTaskWorm, base.base.vtable);

impl CTaskWorm {
    /// Returns the worm's current state code (lives at offset +0x44, inside
    /// `base.subclass_data`). See [`WormState`] for known values.
    pub fn state(&self) -> u32 {
        unsafe { *((self as *const CTaskWorm as *const u8).add(0x44) as *const u32) }
    }

    // Weapon fire dispatch state:
    // - Fire type/subtypes live in the WeaponEntry (via active_weapon_entry at +0x36C)
    // - Completion flag lives in CGameTask.subclass_data[12] (this object, at +0x3C)

    /// Weapon fire completion flag at CGameTask+0x3C (subclass_data[12]).
    /// Set to 0 before FireWeapon dispatch, 1 after.
    pub fn fire_complete(&self) -> i32 {
        i32::from_ne_bytes(self.base.subclass_data[12..16].try_into().unwrap())
    }

    /// Set the weapon fire completion flag.
    pub fn set_fire_complete(&mut self, value: i32) {
        self.base.subclass_data[12..16].copy_from_slice(&value.to_ne_bytes());
    }

    // vtable() method is now provided by bind_CTaskWormVTable! macro above.
}

// ── Snapshot impl ──────────────────────────────────────────

#[cfg(target_arch = "x86")]
impl crate::snapshot::Snapshot for CTaskWorm {
    unsafe fn write_snapshot(&self, w: &mut dyn core::fmt::Write, indent: usize) -> core::fmt::Result {
        use crate::snapshot::{write_indent, write_raw_region};
        let i = indent;
        let b = &self.base; // CGameTask

        write_indent(w, i)?; writeln!(w, "pos = ({}, {})", b.pos_x, b.pos_y)?;
        write_indent(w, i)?; writeln!(w, "speed = ({}, {})", b.speed_x, b.speed_y)?;
        write_indent(w, i)?; writeln!(w, "angle = {}", b.angle)?;
        write_indent(w, i)?; writeln!(w, "team_index = {}", self.team_index)?;
        write_indent(w, i)?; writeln!(w, "worm_index = {}", self.worm_index)?;
        write_indent(w, i)?; writeln!(w, "slot_id = {}", self.slot_id)?;
        write_indent(w, i)?; writeln!(w, "selected_weapon = {}", self.selected_weapon)?;
        write_indent(w, i)?; writeln!(w, "facing = {}", self.facing_direction)?;
        write_indent(w, i)?; writeln!(w, "aim_angle = 0x{:08X}", self.aim_angle)?;

        write_indent(w, i)?; write!(w, "spawn_params =")?;
        for v in &self.spawn_params { write!(w, " {:08X}", v)?; }
        writeln!(w)?;

        // Raw dump of unknown regions
        write_indent(w, i)?; writeln!(w, "_unknown_1f0 ({} bytes):", self._unknown_1f0.len())?;
        write_raw_region(w, self._unknown_1f0.as_ptr(), self._unknown_1f0.len(), i + 1)?;
        write_indent(w, i)?; writeln!(w, "_unknown_28c ({} bytes):", self._unknown_28c.len())?;
        write_raw_region(w, self._unknown_28c.as_ptr(), self._unknown_28c.len(), i + 1)?;
        write_indent(w, i)?; writeln!(w, "_unknown_370 ({} bytes):", self._unknown_370.len())?;
        write_raw_region(w, self._unknown_370.as_ptr(), self._unknown_370.len(), i + 1)?;

        Ok(())
    }
}
