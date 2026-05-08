//! Pure-Rust port of `MineEntity::Constructor` (0x00506660, `__stdcall`,
//! RET 0x18).
//!
//! Args: `(this, parent, fire_params, release_ctx, level_gen_flag,
//! dud_lock)`. Worm-placed mines (Mine / MineStrike) reach this with
//! `(level_gen_flag = 0, dud_lock = 1)`. Pre-placed level-generation mines
//! pass `(level_gen_flag = 1, dud_lock = 0)` so they drop to the terrain
//! and roll for dud at fuse end. Allocation + zero-init of the first
//! `0x19C` bytes is done by [`fire_mine`](crate::game::weapon_fire::fire_mine);
//! the constructor proper does the rest.
//!
//! Subsystem callees still bridged to WA:
//!  * `WorldEntity::Constructor` (0x004FED50) — large MFC-decorated init
//!    that this slice doesn't touch.
//!  * `EntityActivityQueue::ResetRank` (0x00541790) — usercall(EAX=queue,
//!    [stack]=slot). Reused via `super::handle_message::bridge_reset_rank`.
//!  * `WorldEntity::CheckMoveCollision` (0x004FB9D0) — stdcall, 6 args.
//!  * `GameCollisionTask::gradient` (0x00500230) — usercall(EAX=this,
//!    [stack]=x,y,kind,*out_grad), RET 0x10.
//!  * `Math::fixa1tan16` (0x00575840) — usercall(EAX=arg).
//!  * `FramePostProcessHookVec::push_back_one` (0x00507C40) —
//!    usercall(ESI=vec, [stack]=&value). Used to grow the projectile-play
//!    log slot.
//!  * `DisplayGfx::ConstructTextbox` (0x004FAF00, stdcall) — used by the
//!    Rust port of `MineEntity::ConstructPointers` to allocate the
//!    countdown textbox sub-object.
//!  * [`spawn_effect`](crate::game::weapon_release::spawn_effect) is the
//!    Rust port of WA's `SpawnEffect` (0x00547C30); used by
//!    `MineEntity::InsertIntoMineList` to spit out a smoke puff when the
//!    LRU mine gets evicted to make room.

use core::sync::atomic::{AtomicU32, Ordering};

use super::handle_message::bridge_reset_rank;
use super::{MineEntity, MineEntityVtable};
use crate::engine::EntityActivityQueue;
use crate::engine::game_info::GameInfo;
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::class_type::ClassType;
use crate::game::weapon::WeaponFireParams;
use crate::game::weapon_fire::WeaponReleaseContext;
use crate::rebase::rb;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "MineEntity" {
        /// `MineEntity::ConstructPointers` (0x00506D20) —
        /// `__usercall(EDI = this)`, plain RET. Ported pure-Rust as
        /// [`construct_pointers`]; address kept for registry lookups.
        fn/Usercall MINE_CONSTRUCT_POINTERS = 0x00506D20;
        /// `MineEntity::InsertIntoMineList` (0x00506B70) —
        /// `__usercall(EDI = this)`, plain RET. Ported pure-Rust as
        /// [`insert_into_mine_list`]; address kept for registry lookups.
        fn/Usercall MINE_INSERT_INTO_LIST = 0x00506B70;
    }
    /// `FramePostProcessHookVec::push_back_one` (0x00507C40, was
    /// `FUN_00507c40`) — `__usercall(ESI = vec, [stack] = &value)`, RET
    /// 0x4. Appends a single dword to the std::vector at `ESI` (begin/end
    /// at +0x4/+0x8, capacity_end at +0xC); on grow, delegates to
    /// `FramePostProcessHookVec::InsertOne_Maybe`.
    fn/Usercall FRAME_POST_PROCESS_HOOK_VEC_PUSH_BACK_ONE = 0x00507C40;
    /// `Math::fixa1tan16` (0x00575840) — `__usercall(EAX = gradient)`,
    /// plain RET. Returns Fixed16 atan of the gradient (slope ratio).
    fn/Usercall MATH_FIXA1TAN16 = 0x00575840;
    /// `GameCollisionTask::gradient` (0x00500230) —
    /// `__usercall(EAX = this, [stack] = x, y, kind, *out_grad)`, RET 0x10.
    /// Probes terrain slope at `(x, y)`; returns 1 + writes `*out_grad`
    /// when the entity is grounded, 0 otherwise.
    fn/Usercall GAME_COLLISION_TASK_GRADIENT = 0x00500230;
}

// Saved bridge addresses, populated by [`init_addrs`].
//
// `WorldEntity::Constructor` (0x004FED50) — `__stdcall(this, parent,
// class_type, flag)`, RET 0x10. Large MFC-style initializer that this
// slice doesn't touch.
static WORLD_ENTITY_CTOR_ADDR: AtomicU32 = AtomicU32::new(0);
// `WorldEntity::CheckMoveCollision` (0x004FB9D0) — `__stdcall`, 6 args,
// RET 0x18. Same helper used by [`WorldEntity::try_move_position_raw`];
// the ctor's drop loop calls it directly to probe without committing.
static CHECK_MOVE_COLLISION_ADDR: AtomicU32 = AtomicU32::new(0);
static FRAME_POST_PROCESS_HOOK_VEC_PUSH_BACK_ONE_ADDR: AtomicU32 = AtomicU32::new(0);
static MATH_FIXA1TAN16_ADDR: AtomicU32 = AtomicU32::new(0);
static GAME_COLLISION_TASK_GRADIENT_ADDR: AtomicU32 = AtomicU32::new(0);
static CONSTRUCT_TEXTBOX_ADDR: AtomicU32 = AtomicU32::new(0);

pub unsafe fn init_addrs() {
    WORLD_ENTITY_CTOR_ADDR.store(rb(0x004FED50), Ordering::Relaxed);
    CHECK_MOVE_COLLISION_ADDR.store(
        rb(crate::entity::game_entity::CHECK_MOVE_COLLISION),
        Ordering::Relaxed,
    );
    FRAME_POST_PROCESS_HOOK_VEC_PUSH_BACK_ONE_ADDR.store(
        rb(FRAME_POST_PROCESS_HOOK_VEC_PUSH_BACK_ONE),
        Ordering::Relaxed,
    );
    MATH_FIXA1TAN16_ADDR.store(rb(MATH_FIXA1TAN16), Ordering::Relaxed);
    GAME_COLLISION_TASK_GRADIENT_ADDR.store(rb(GAME_COLLISION_TASK_GRADIENT), Ordering::Relaxed);
    CONSTRUCT_TEXTBOX_ADDR.store(rb(crate::address::va::CONSTRUCT_TEXTBOX), Ordering::Relaxed);
}

/// `WorldEntity::Constructor` (0x004FED50) — `__stdcall(this, parent,
/// class_type, flag)`, RET 0x10.
#[inline]
unsafe fn world_entity_ctor(
    this: *mut WorldEntity<*const MineEntityVtable>,
    parent: *mut BaseEntity,
    class_type: u32,
    flag: u32,
) {
    type Fn = unsafe extern "stdcall" fn(
        *mut WorldEntity<*const MineEntityVtable>,
        *mut BaseEntity,
        u32,
        u32,
    );
    unsafe {
        let f: Fn = core::mem::transmute(WORLD_ENTITY_CTOR_ADDR.load(Ordering::Relaxed) as usize);
        f(this, parent, class_type, flag);
    }
}

/// Direct call into [`CHECK_MOVE_COLLISION`](crate::entity::game_entity::CHECK_MOVE_COLLISION)
/// matching the ctor's drop-loop signature (6 args, stdcall, RET 0x18).
#[inline]
unsafe fn check_move_collision(
    game_state_stream: *mut u8,
    this: *mut MineEntity,
    x: i32,
    y: i32,
    field_dc: u32,
    flags: u32,
) -> u32 {
    type Fn = unsafe extern "stdcall" fn(*mut u8, *mut MineEntity, i32, i32, u32, u32) -> u32;
    unsafe {
        let f: Fn =
            core::mem::transmute(CHECK_MOVE_COLLISION_ADDR.load(Ordering::Relaxed) as usize);
        f(game_state_stream, this, x, y, field_dc, flags)
    }
}

/// `GameCollisionTask::gradient` (0x00500230) — `__usercall(EAX = this,
/// [stack] = x, y, kind, *out_grad)`, RET 0x10. Returns 1 (and writes
/// `*out_grad`) when the entity sits on terrain steep enough to estimate
/// a slope; returns 0 otherwise.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_gradient(
    _this: *mut MineEntity,
    _x: i32,
    _y: i32,
    _kind: i32,
    _out_grad: *mut i32,
) -> u32 {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",   // this
        "push dword ptr [esp+20]",      // out_grad
        "push dword ptr [esp+20]",      // kind
        "push dword ptr [esp+20]",      // y
        "push dword ptr [esp+20]",      // x
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 20",
        addr = sym GAME_COLLISION_TASK_GRADIENT_ADDR,
    );
}

/// `Math::fixa1tan16` (0x00575840) — `__usercall(EAX = gradient)`, plain
/// RET. Returns the Fixed16 atan of the supplied gradient.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_fixa1tan16(_gradient: i32) -> i32 {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 4",
        addr = sym MATH_FIXA1TAN16_ADDR,
    );
}

/// `FramePostProcessHookVec::push_back_one` (0x00507C40) —
/// `__usercall(ESI = vec, [stack] = &value)`, RET 0x4. ESI is callee-saved,
/// so the trampoline saves it across the call.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_vec_push_back_one(_vec: *mut u8, _value_ptr: *const u32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",   // vec
        "push dword ptr [esp+12]",      // &value
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop esi",
        "ret 8",
        addr = sym FRAME_POST_PROCESS_HOOK_VEC_PUSH_BACK_ONE_ADDR,
    );
}

/// `DisplayGfx::ConstructTextbox` (0x004FAF00) — `__stdcall(this, anchor,
/// kind)`, RET 0xC. Returns the textbox handle in EAX.
#[inline]
unsafe fn construct_textbox(this: *mut u8, anchor: i32, kind: i32) -> *mut u8 {
    type Fn = unsafe extern "stdcall" fn(*mut u8, i32, i32) -> *mut u8;
    unsafe {
        let f: Fn = core::mem::transmute(CONSTRUCT_TEXTBOX_ADDR.load(Ordering::Relaxed) as usize);
        f(this, anchor, kind)
    }
}

/// Pure-Rust port of `MineEntity::ConstructPointers` (0x00506D20). When
/// running headful, allocates a 0x158-byte buffer for the per-mine
/// countdown textbox and stores the resulting handle in
/// [`MineEntity::textbox_handle`]. Headless: no-op.
pub unsafe fn construct_pointers(this: *mut MineEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        if (*world).is_headful == 0 {
            return;
        }
        let buf = crate::wa_alloc::wa_malloc(0x158);
        if buf.is_null() {
            (*this).textbox_handle = core::ptr::null_mut();
            return;
        }
        // WA only zeroes the first 0x138 bytes; the trailing 0x20 bytes
        // are scratch the textbox impl initialises lazily.
        core::ptr::write_bytes(buf, 0, 0x138);
        let game_info = (*world).game_info;
        let f380 = *((game_info as *const u8).add(0xF380) as *const u32);
        let kind = if f380 != 0 { 2 } else { 1 };
        (*this).textbox_handle = construct_textbox(buf, 4, kind);
    }
}

/// Pure-Rust port of `MineEntity::InsertIntoMineList` (0x00506B70).
/// Records `this` in `world.mine_list[]`: takes the first free slot if
/// any, else evicts the oldest (smallest [`inserted_frame`]) mine — that
/// mine is freed via vtable slot 1 and a smoke puff is spawned at its
/// position.
///
/// [`inserted_frame`]: MineEntity::inserted_frame
pub unsafe fn insert_into_mine_list(this: *mut MineEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let game_info = (*world).game_info;
        let capacity = (*game_info).mine_list_capacity;
        let table = (*world).mine_list;

        // First pass: place into the first empty slot, if any.
        for i in 0..capacity {
            let slot_ptr = table.add(i as usize);
            if (*slot_ptr).is_null() {
                *slot_ptr = this;
                (*this).inserted_frame = (*world).frame_counter;
                (*this).mine_list_slot = i;
                return;
            }
        }

        // Full: find the LRU mine (smallest `inserted_frame`).
        let mut best_idx: i32 = -1;
        let mut best_age: i32 = 0;
        for i in 0..capacity {
            let victim_age = (**table.add(i as usize)).inserted_frame;
            if best_idx < 0 || victim_age < best_age {
                best_idx = i as i32;
                best_age = victim_age;
            }
        }
        let victim = *table.add(best_idx as usize);

        // Spit out a smoke puff at the victim's position, tinted by the
        // placer team's font palette. Team ids are 1-based here; level-gen
        // mines (team 0) can't reach this branch in practice because the
        // registry hands out empty slots first.
        let team_record = GameInfo::team_record_1based(game_info, (*victim).placer_team_index);
        let state_flag = ((*team_record).font_palette_idx as u32).wrapping_add(8);
        crate::game::weapon_release::spawn_effect(
            victim as *mut crate::entity::WormEntity,
            0,
            (*victim).base.pos_x,
            (*victim).base.pos_y,
            0,
            0,
            0,
            state_flag,
            Fixed::ONE,
            Fixed(0xCCC),
        );

        MineEntity::free_raw(victim, 1);

        *table.add(best_idx as usize) = this;
        (*this).mine_list_slot = best_idx as u32;
        (*this).inserted_frame = (*world).frame_counter;
    }
}

/// Pure-Rust port of `MineEntity::Constructor` (0x00506660, `__stdcall`,
/// RET 0x18). Caller (`fire_mine` etc.) has already allocated `0x1BC`
/// bytes and zeroed the first `0x19C`. Returns `this` (matching WA).
pub unsafe fn mine_constructor(
    this: *mut MineEntity,
    parent: *mut BaseEntity,
    fire_params: *const WeaponFireParams,
    release_ctx: *const WeaponReleaseContext,
    level_gen_flag: u32,
    dud_lock: u32,
) -> *mut MineEntity {
    unsafe {
        // Parent ctor + class_type + vtable.
        world_entity_ctor(&raw mut (*this).base, parent, 10, 2);
        let world: *mut GameWorld = (*(this as *const BaseEntity)).world;
        (*(this as *mut BaseEntity)).class_type = ClassType::Mine;
        (*this).base.base.vtable = rb(super::MINE_ENTITY_VTABLE) as *const MineEntityVtable;

        // Block-copy 8 dwords of WeaponFireParams to mine + 0x170.
        core::ptr::copy_nonoverlapping(
            fire_params as *const u32,
            (this as *mut u8).add(0x170) as *mut u32,
            8,
        );
        // Block-copy 11 dwords of WeaponReleaseContext to mine + 0x144.
        core::ptr::copy_nonoverlapping(
            release_ctx as *const u32,
            (this as *mut u8).add(0x144) as *mut u32,
            11,
        );

        let queue: *mut EntityActivityQueue = &raw mut (*world).entity_activity_queue;
        let slot = EntityActivityQueue::acquire(queue);
        (*this).activity_rank_slot = slot as u32;
        bridge_reset_rank(queue, slot);

        // Field initialization from fire_params + zeros.
        // Re-read via dword index (mirrors WA's param_3[N] decomp). Using
        // the public WeaponFireParams field would also work, but indexing
        // keeps the per-store mapping obvious.
        let fp = fire_params as *const i32;
        (*this).triggered_flag = 0;
        (*this)._field_108 = 0;
        (*this).trigger_armed_flag = 1;
        (*this).damage = *fp.add(6);
        (*this).trigger_class_mask = *fp.add(2) as u32;
        (*this).arm_delay = *fp.add(1);
        let fuse_seed = *fp.add(3);
        (*this).fuse_timer = fuse_seed;
        (*this).trigger_range = *fp.add(0) as u32;
        (*this).bubble_phase = Fixed(0);
        (*this).splash_played = 0;
        (*this)._field_134 = 0;
        (*this)._field_10c = 0;
        (*this).fled = 0;
        (*this).beep_tier_index = fuse_seed.wrapping_div(250);
        (*this).is_not_dud = dud_lock;

        // Anim phase seed. The world-side u32 read at +0x5cc has not been
        // typed yet; transmute via raw byte offset to avoid coining a new
        // field name in this slice.
        let world_5cc = *((world as *const u8).add(0x5CC) as *const u32);
        // subclass_data[4..8] = 2 — read by `try_move_position_raw` and
        // `gradient` below; the bitwise mask further down overwrites this
        // with `0x421846 | …` after both calls have used the seed value.
        write_subclass_dword(this, 0x4, 2);
        (*this)._field_190 = (world_5cc % 10).wrapping_mul(0x199A);

        // Initial placement: probe `(spawn_x, spawn_y)`; commit on accept.
        let rc = release_ctx as *const u32;
        let spawn_x = *rc.add(2) as i32;
        let spawn_y = *rc.add(3) as i32;
        WorldEntity::try_move_position_raw(this as *mut WorldEntity, spawn_x, spawn_y);

        (*this).base.speed_y = Fixed(*rc.add(5) as i32);
        (*this).base.speed_x = Fixed(*rc.add(4) as i32);

        // game_info byte at +0x7e40 selects two flag bits in the
        // collision-flag word at mine + 0x34. Replicate the SBB bitwise
        // sequence verbatim.
        let scheme_byte: u8 = *((world as *const u8).add(0x7E40));
        let bit_20 = if scheme_byte >= 2 { 0x20u32 } else { 0 };
        let bit_10 = if scheme_byte >= 8 { 0x10u32 } else { 0 };
        write_subclass_dword(this, 0x4, 0x00421846 | bit_20 | bit_10);

        // Derived seed at subclass_data[0x24..0x28]: signed-div-by-20 of
        // `(spawn_x + spawn_y) >> 8` truncated to 16 bits, plus 0xCCCC.
        let sum: i32 = spawn_x.wrapping_add(spawn_y);
        let shifted = (sum >> 8) & 0xFFFF;
        let derived = shifted.wrapping_div(20).wrapping_add(0xCCCC) as u32;
        write_subclass_dword(this, 0x24, derived);

        write_subclass_dword(this, 0x3C, 0x9999);
        write_subclass_dword(this, 0x40, 0x9999);
        write_subclass_dword(this, 0x44, 0); // anim flag
        write_subclass_dword(this, 0x1C, 0x10000);
        write_subclass_dword(this, 0x10, 0); // armed marker
        write_subclass_dword(this, 0xC, 1);

        // Arm the mine if it spawned already settled (`arm_delay <= 0`).
        // For airborne mines (`arm_delay < 0`) WA leaves the negative
        // value alone; for `arm_delay == 0` it explicitly clears (no-op).
        if (*this).arm_delay <= 0 {
            let game_d780 = *((*world).game_info.cast::<u8>().add(0xD780) as *const u32);
            if (*this).arm_delay == 0 {
                (*this).arm_delay = 0;
            }
            write_subclass_dword(this, 0x10, 1); // armed marker
            write_subclass_dword(this, 0x44, game_d780); // anim flag
        }

        // Pre-placed level-gen mines: drop one pixel at a time until
        // collision or water level, snapping `pos_x`/`pos_y` along the way.
        if level_gen_flag != 0 {
            let x = spawn_x;
            let mut y = spawn_y.wrapping_add(0x10000);
            let game_state_stream = (*world).game_state_stream;
            while (y >> 16) < (*world).water_level {
                let field_dc = (*this).base._field_dc;
                let flags = read_subclass_dword(this, 0x4);
                let collided =
                    check_move_collision(game_state_stream, this, x, y, field_dc, flags) != 0;
                if collided {
                    break;
                }
                if (*this).base._field_ac > 0 {
                    (*this).base._field_ac = 0;
                }
                (*this).base.pos_x = Fixed(x);
                (*this).base.pos_y = Fixed(y);
                y = y.wrapping_add(0x10000);
            }
            // `gradient` overwrites subclass_data[0x4..0x8] internally;
            // it restores the pre-call value before returning.
            let mut out_grad: i32 = 0;
            let r = bridge_gradient(this, x, y, 4, &raw mut out_grad);
            if r != 0 {
                (*this).base.angle = Fixed(bridge_fixa1tan16(out_grad));
            }
        }

        // Replay-state projectile-play registration. WA writes
        // `_field_194 = -1`, then if the mine has no preset fuse and the
        // world has a live replay state, allocates a new index in the
        // replay's projectile-play vector and stores it back into
        // `_field_194`.
        (*this)._field_194 = u32::MAX;
        if (*this).fuse_timer < 0 {
            let rs = (*world)._unknown_51c;
            if !rs.is_null() {
                let next_id_ptr = (rs as *mut u8).add(0x14) as *mut u32;
                let next_id = *next_id_ptr;
                (*this)._field_194 = next_id;
                *next_id_ptr = next_id.wrapping_add(1);

                let vec_struct = (rs as *mut u8).add(0x18);
                let vec_first = *((vec_struct.add(4)) as *const *mut u32);
                let size = if vec_first.is_null() {
                    0u32
                } else {
                    let vec_last = *((vec_struct.add(8)) as *const *mut u32);
                    ((vec_last as usize - vec_first as usize) >> 2) as u32
                };
                if next_id >= size {
                    let sentinel: u32 = u32::MAX;
                    bridge_vec_push_back_one(vec_struct, &sentinel);
                }
            }
        }

        construct_pointers(this);
        insert_into_mine_list(this);

        this
    }
}

#[inline]
unsafe fn write_subclass_dword(this: *mut MineEntity, sub_offset: usize, value: u32) {
    unsafe {
        let p = (*this).base.subclass_data.as_mut_ptr().add(sub_offset) as *mut u32;
        *p = value;
    }
}

#[inline]
unsafe fn read_subclass_dword(this: *mut MineEntity, sub_offset: usize) -> u32 {
    unsafe {
        let p = (*this).base.subclass_data.as_ptr().add(sub_offset) as *const u32;
        *p
    }
}
