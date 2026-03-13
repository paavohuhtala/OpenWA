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
    pub on_contact_entity:
        unsafe extern "thiscall" fn(*mut CTaskWorm, *mut CGameTask, u32) -> u32,
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
/// The worm state field lives at **offset +0x44** (inside `base._unknown_30`).
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
    /// 0x15C–0x247: Unknown
    pub _unknown_15c: [u8; 0xEC],
    /// 0x248–0x25B: Aiming / weapon data (slots 4–8 in GetEntityData query 0x7D4)
    pub _unknown_248: [u32; 5],
    /// 0x25C–0x283: Unknown
    pub _unknown_25c: [u8; 0x28],
    /// 0x284–0x28B: Aiming data (slots 5–6 in GetEntityData query 0x7D4)
    pub _unknown_284: [u32; 2],
    /// 0x28C–0x2EF: Unknown
    pub _unknown_28c: [u8; 0x64],
    /// 0x2F0: Worm name, null-terminated (max 17 chars, from spawn init_data+3)
    pub worm_name: [u8; 0x11],
    /// 0x301: Country / team name from scheme, null-terminated (max 17 chars)
    pub country_name: [u8; 0x11],
    /// 0x312–0x367: Unknown (rope string, state history, etc.)
    pub _unknown_312: [u8; 0x56],
    /// 0x368: Animator / controller object (dispatched via vtable for state animations)
    pub animator: *mut u8,
    /// 0x36C–0x3FB: Unknown (rope anchor, weapon-specific data, etc.)
    pub _unknown_36c: [u8; 0x90],
}

const _: () = assert!(core::mem::size_of::<CTaskWorm>() == 0x3FC);

impl CTaskWorm {
    /// Returns the worm's current state code (lives at offset +0x44, inside the
    /// CGameTask base's `_unknown_30` padding region).
    ///
    /// Known states: `0x65`=idle, `0x67`=active turn, `0x7F`=drowning,
    /// `0x80`=hurt, `0x81`/`0x86`=dead, `0x87`=dead variant, `0x8B`=unknown.
    pub fn state(&self) -> u32 {
        // SAFETY: offset 0x44 is within CGameTask._unknown_30 (0x30..0x84).
        // Aligned to 4 bytes; repr(C) guarantees no reordering.
        unsafe { *((self as *const CTaskWorm as *const u8).add(0x44) as *const u32) }
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
