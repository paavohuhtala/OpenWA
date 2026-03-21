use super::base::CTask;
use super::game_task::CGameTask;

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
#[repr(C)]
pub struct CTaskWormVTable {
    /// [0] 0x0050CAA0: WriteReplayState — serializes worm state to a replay stream;
    ///   writes entity type byte 0x12, then packs field groups based on current state
    pub write_replay_state: unsafe extern "thiscall" fn(*mut CTaskWorm, *mut u8),
    /// [1] 0x0050C7E0: Free — calls inner destructor, then `_free(this)` if flags & 1
    pub free: unsafe extern "thiscall" fn(*mut CTaskWorm, u8) -> *mut CTaskWorm,
    /// [2] 0x00510B40: HandleMessage — processes all TaskMessages sent to this worm
    pub handle_message:
        unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CTask, u32, u32, *const u8),
    /// [3] 0x00516780: GetEntityData — returns worm data by query code:
    ///   0x7D0=(pos_x, pos_y), 0x7D1/0x7D2=collision test, 0x7D4=full state dump
    pub get_entity_data: unsafe extern "thiscall" fn(*mut CTaskWorm, u32, u32, *mut u32) -> u32,
    /// [4]-[6]: Inherited CTask stubs (not overridden by CTaskWorm)
    pub _inherited_4_to_6: [*const (); 3],
    /// [7] 0x0050D5D0: OnContactEntity — handles physical contact with another entity;
    ///   worm-on-dying-worm → apply crush damage; projectile close-pass → SetState(0x80)
    pub on_contact_entity: unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CGameTask, u32) -> u32,
    /// [8] 0x0050D9A0: OnWormPush — post-contact worm-worm push impulse;
    ///   deduplicates via a recent-collision table, then adjusts pos_x or pos_y
    pub on_worm_push: unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CGameTask, u32) -> u32,
    /// [9] 0x0050D810: OnLandBounce — worm lands on terrain; plays thud sound, bounce physics
    pub on_land_bounce: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [10] 0x0050D820: OnLandSlide — secondary landing callback; sliding/friction physics
    pub on_land_slide: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [11] 0x0050D570: OnSink — worm sinks in water/acid; applies (dx, dy) displacement,
    ///   transitions to drowning state (0x7F) unless already flying/rope/dead
    pub on_sink: unsafe extern "thiscall" fn(*mut CTaskWorm, i32, i32) -> u32,
    /// [12]: Inherited (vtable30 from CGameTask — not overridden)
    pub _inherited_12: *const (),
    /// [13] 0x0050D3B0: OnKilled — worm death; plays death sound (0x3A),
    ///   transitions to dead state (0x81 or 0x89) based on game round count
    pub on_killed: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [14] 0x0050E850: SetState — worm state machine; handles all state transitions
    ///   (0x65=idle, 0x67=active turn, 0x7F=drowning, 0x80=hurt, 0x81/0x86=dead, …)
    pub set_state: unsafe extern "thiscall" fn(*mut CTaskWorm, u32),
    /// [15] 0x00516900: CheckPendingAction — if field +0xBC is set, calls SetState(0x73)
    pub check_pending_action: unsafe extern "thiscall" fn(*mut CTaskWorm),
    /// [16] 0x00516920: IsNotOnRope — returns true if worm state != 0x7C (rope-swinging)
    pub is_not_on_rope: unsafe extern "thiscall" fn(*const CTaskWorm) -> bool,
    /// [17]: Inherited (vtable44 from CGameTask — not overridden)
    pub _inherited_17: *const (),
    /// [18] 0x005168F0: GetTeamIndex — returns worm's team index (field +0xFC)
    pub get_team_index: unsafe extern "thiscall" fn(*const CTaskWorm) -> u32,
    /// [19]: Inherited (vtable4C from CGameTask — not overridden)
    pub _inherited_19: *const (),
}

const _: () = assert!(core::mem::size_of::<CTaskWormVTable>() == 20 * 4);

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
#[repr(C)]
pub struct CTaskWorm {
    /// 0x00–0xFB: CGameTask base (position, velocity, sound emitter, etc.)
    pub base: CGameTask,

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
    pub worm_name: [u8; 0x11],
    /// 0x301: Country / team name from scheme, null-terminated (max 17 chars)
    pub country_name: [u8; 0x11],
    /// 0x312–0x333: Unknown (rope string, state history, etc.)
    pub _unknown_312: [u8; 0x334 - 0x312],
    /// 0x334: Facing direction copy. -1 = left, +1 = right (same as +0x3DC).
    pub facing_direction_3: i32,
    /// 0x338–0x367: Unknown
    pub _unknown_338: [u8; 0x368 - 0x338],
    /// 0x368: Animator / controller object (dispatched via vtable for state animations)
    pub animator: *mut u8,
    /// 0x36C: Self-pointer (points to this CTaskWorm). Used by WeaponRelease
    /// to load EAX before calling FireWeapon: `MOV EAX, [EDI+0x36C]`.
    pub weapon_self_ptr: *mut CTaskWorm,
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

impl CTaskWorm {
    /// Returns the worm's current state code (lives at offset +0x44, inside
    /// `base.subclass_data`).
    ///
    /// Known states: `0x65`=idle, `0x67`=active turn, `0x7F`=drowning,
    /// `0x80`=hurt, `0x81`/`0x86`=dead, `0x87`=dead variant, `0x8B`=unknown.
    pub fn state(&self) -> u32 {
        unsafe { *((self as *const CTaskWorm as *const u8).add(0x44) as *const u32) }
    }

    // ── Weapon fire state accessors (offsets +0x30..+0x3C in subclass_data) ──

    /// Weapon fire type (1=projectile, 2=rope, 3=grenade, 4=special).
    /// Offset +0x30 in CGameTask.subclass_data.
    pub fn weapon_fire_type(&self) -> i32 {
        unsafe { *(self.base.subclass_data.as_ptr().add(0) as *const i32) }
    }

    /// Weapon fire subtype for types 3 and 4. Offset +0x34.
    pub fn weapon_fire_subtype_34(&self) -> i32 {
        unsafe { *(self.base.subclass_data.as_ptr().add(4) as *const i32) }
    }

    /// Weapon fire subtype for types 1 and 2. Offset +0x38.
    pub fn weapon_fire_subtype_38(&self) -> i32 {
        unsafe { *(self.base.subclass_data.as_ptr().add(8) as *const i32) }
    }

    /// Weapon fire completion flag. Offset +0x3C.
    /// Set to 0 before dispatch, 1 after.
    pub fn weapon_fire_complete(&self) -> i32 {
        unsafe { *(self.base.subclass_data.as_ptr().add(12) as *const i32) }
    }

    /// Mutable pointer to weapon_fire_complete for setting the flag.
    pub fn weapon_fire_complete_mut(&mut self) -> &mut i32 {
        unsafe { &mut *(self.base.subclass_data.as_mut_ptr().add(12) as *mut i32) }
    }

    /// Address of weapon_fire_complete field (passed as params base to fire handlers).
    pub fn weapon_params_ptr(&self) -> u32 {
        self as *const _ as u32 + 0x3C
    }

    /// Address of weapon_fire_subtype_34 field (params for GrenadeMortar).
    pub fn weapon_params_34_ptr(&self) -> u32 {
        self as *const _ as u32 + 0x34
    }

    /// Address of weapon_fire_subtype_38 field (params for type-4 specials).
    pub fn weapon_params_38_ptr(&self) -> u32 {
        self as *const _ as u32 + 0x38
    }

    /// Returns a reference to the vtable.
    ///
    /// # Safety
    /// The vtable pointer at offset 0 must be the genuine CTaskWorm vtable
    /// (0x6644C8 in Ghidra, rebased at runtime).
    pub unsafe fn vtable(&self) -> &'static CTaskWormVTable {
        &*(self.base.base.vtable as *const CTaskWormVTable)
    }
}
