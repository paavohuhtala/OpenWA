//! Pure-Rust port of `MissileEntity::Free` (0x00508330, vtable slot 1) and
//! its inlined destructor `Task_Missile::dtor1` (0x005086F0).
//!
//! The destructor is a fixed deregistration sequence: vtable restore,
//! object-pool counter decrement, three SharedData-routed broadcasts (msg
//! 0x4E `WeaponDestroyed`, 0x53, 0x7C — each gated on a missile-state
//! field), activity-queue slot release, sound-handle teardown, super-animal
//! cleanup, and (in headful mode) the two render-handle wrapper objects.
//! Finally chains into the parent `WorldEntity::Destructor` (0x004FEF30),
//! which is kept bridged.
//!
//! The two SharedData broadcasts in this file mirror the same pattern
//! `WormEntity::HandleMessage` uses (lookup `(esi=0, edi=0x14)` → call
//! `HandleMessage` slot on the result), but cut out the `Task__deliver`
//! wrapper since this code path doesn't need its 5-arg trampoline.

use core::sync::atomic::AtomicU32;

use super::handle_message::bridge_finish_super_animal;
use super::{MissileEntity, MissileEntityVtable};
use crate::engine::EntityActivityQueue;
use crate::entity::base::{BaseEntity, SharedDataTable};
use crate::rebase::rb;
use crate::wa_alloc::wa_free;

// `EntityActivityQueue::FreeSlotById` (0x00541860) —
// `__usercall(EAX = queue, [stack] = slot)`, RET 0x4.
static mut FREE_ACTIVITY_SLOT_ADDR: u32 = 0;
// `WorldEntity::Destructor` (0x004FEF30) — `__thiscall(this)`, plain RET.
// SEH-protected children-list walk; kept bridged.
static mut CGAMETASK_DESTRUCTOR_ADDR: u32 = 0;
// `Task_Missile::stop_fuse_sound` (0x00508C10) — `__usercall(ESI = this)`,
// no stack args, plain RET. Releases the fuse-sound channel via the
// world's sound subsystem.
static mut STOP_FUSE_SOUND_ADDR: u32 = 0;
// `Task_Missile::stop_dig_sound` (0x00508970) — same shape, slot 0x3E0.
static mut STOP_DIG_SOUND_ADDR: u32 = 0;

/// Saved original `MissileEntity::Free` (0x00508330), populated by
/// `vtable_replace!` at install time.
pub static ORIGINAL_FREE: AtomicU32 = AtomicU32::new(0);

pub unsafe fn init_addrs() {
    unsafe {
        FREE_ACTIVITY_SLOT_ADDR = rb(0x00541860);
        CGAMETASK_DESTRUCTOR_ADDR = rb(0x004FEF30);
        STOP_FUSE_SOUND_ADDR = rb(0x00508C10);
        STOP_DIG_SOUND_ADDR = rb(0x00508970);
    }
}

#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_free_activity_slot(_queue: *mut EntityActivityQueue, _slot: i32) {
    core::arch::naked_asm!(
        "mov eax, dword ptr [esp+4]",
        "push dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "ret 8",
        addr = sym FREE_ACTIVITY_SLOT_ADDR,
    );
}

#[inline]
unsafe fn bridge_cgametask_destructor(this: *mut MissileEntity) {
    type Fn = unsafe extern "thiscall" fn(*mut MissileEntity);
    let f: Fn = unsafe { core::mem::transmute(CGAMETASK_DESTRUCTOR_ADDR as usize) };
    unsafe { f(this) }
}

/// `__usercall(ESI = this)`, no stack args, plain RET. ESI is callee-saved
/// per the x86 ABI, so the trampoline preserves it across the call.
#[unsafe(naked)]
pub(super) unsafe extern "stdcall" fn bridge_stop_fuse_sound(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop esi",
        "ret 4",
        addr = sym STOP_FUSE_SOUND_ADDR,
    );
}

/// Same shape as [`bridge_stop_fuse_sound`].
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_stop_dig_sound(_this: *mut MissileEntity) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, dword ptr [esp+8]",
        "mov ecx, dword ptr [{addr}]",
        "call ecx",
        "pop esi",
        "ret 4",
        addr = sym STOP_DIG_SOUND_ADDR,
    );
}

/// Send `msg_id` (one of 0x4E/0x53/0x7C in this file) to the entity that
/// SharedData maps to key `(esi=0, edi=0x14)` — the WorldRoot dispatcher.
/// Payload is a 0x408-byte buffer with `owner_id` at offset 0; the rest is
/// zeroed. WA's original passes uninitialised stack here, but the receivers
/// only read the first dword for these message ids, so zeroing the tail is
/// equivalent (and headless tests cover the equivalence).
unsafe fn broadcast_via_world_root(this: *mut MissileEntity, msg_id: u32, owner_id: u32) {
    unsafe {
        let table = SharedDataTable::from_task(this as *const BaseEntity);
        let target = table.lookup(0, 0x14);
        if target.is_null() {
            return;
        }
        let mut buf = [0u32; 0x408 / 4];
        buf[0] = owner_id;
        let vt = *(target as *const *const usize);
        let handle_message_slot: unsafe extern "thiscall" fn(
            *mut u8,
            *mut BaseEntity,
            u32,
            u32,
            *const u8,
        ) = core::mem::transmute(*vt.add(2));
        handle_message_slot(
            target,
            this as *mut BaseEntity,
            msg_id,
            0x408,
            buf.as_ptr() as *const u8,
        );
    }
}

/// Free a render-handle wrapper (the `+0xC` / `+0x10` two-child layout
/// allocated by `Task_Missile::ConstructPointers`). Each non-null child
/// is released through its own vtable slot 3 (`thiscall(this, flag=1)` —
/// the C++ scalar-deleting destructor convention), then the wrapper itself
/// is `wa_free`-d.
unsafe fn free_render_handle(handle: *mut u8) {
    unsafe {
        if handle.is_null() {
            return;
        }
        type Vt3Free = unsafe extern "thiscall" fn(*mut u8, u32);
        for child_offset in [0xCusize, 0x10] {
            let child = *(handle.add(child_offset) as *const *mut u8);
            if !child.is_null() {
                let vt = *(child as *const *const u32);
                let f: Vt3Free = core::mem::transmute(*vt.add(3));
                f(child, 1);
            }
        }
        wa_free(handle);
    }
}

/// Pure-Rust port of `Task_Missile::dtor1` (0x005086F0). Mirrors the
/// in-order WA sequence:
///
/// 1. Restore own vtable so the parent destructor's virtual dispatch
///    resolves against `MissileEntity` slots.
/// 2. `world.object_pool_count -= 7` — release this missile's slice of
///    the spawn budget (counterpart to the `+= 7` in the constructor).
/// 3. Broadcast `WeaponDestroyed` (0x4E) when this missile has a
///    non-zero owner.
/// 4. Free the activity-queue rank slot.
/// 5. Stop both sound channels.
/// 6. Broadcast 0x53 when [`homing_enabled`](MissileEntity::homing_enabled).
/// 7. If `contact_phase == 1` (active super-animal control), call
///    `finish_super_animal` to drain residual velocity.
/// 8. If `contact_phase == 2`, broadcast 0x7C.
/// 9. Headful only: free both render-handle wrappers.
/// 10. Chain into the parent `WorldEntity::Destructor`.
unsafe fn destructor_1(this: *mut MissileEntity) {
    unsafe {
        (*this).base.base.vtable = rb(super::MISSILE_ENTITY_VTABLE) as *const MissileEntityVtable;

        let world = (*(this as *const BaseEntity)).world;
        (*world).object_pool_count = (*world).object_pool_count.wrapping_sub(7);

        let owner_id = (*this).spawn_params.owner_id;
        if owner_id != 0 {
            broadcast_via_world_root(this, 0x4E, owner_id);
        }

        let queue = core::ptr::addr_of_mut!((*world).entity_activity_queue);
        bridge_free_activity_slot(queue, (*this).activity_rank_slot as i32);

        bridge_stop_fuse_sound(this);
        bridge_stop_dig_sound(this);

        if (*this).homing_enabled != 0 {
            broadcast_via_world_root(this, 0x53, owner_id);
        }

        match (*this).contact_phase {
            1 => bridge_finish_super_animal(this),
            2 => broadcast_via_world_root(this, 0x7C, owner_id),
            _ => {}
        }

        if (*world).is_headful != 0 {
            free_render_handle((*this).render_handle_a);
            free_render_handle((*this).render_handle_b);
        }

        bridge_cgametask_destructor(this);
    }
}

/// Pure-Rust port of `MissileEntity::Free` (0x00508330, vtable slot 1).
/// Runs the destructor and, when bit 0 of `flags` is set, frees the heap
/// allocation. Returns the `this` pointer in EAX (the `extern "thiscall"`
/// signature handles ECX = this and the `i8` stack arg + return-in-EAX).
pub unsafe extern "thiscall" fn free(this: *mut MissileEntity, flags: u8) -> *mut MissileEntity {
    unsafe {
        destructor_1(this);
        if (flags & 1) != 0 {
            wa_free(this as *mut u8);
        }
        this
    }
}
