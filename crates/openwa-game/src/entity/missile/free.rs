//! Pure-Rust port of `MissileEntity::Free` (0x00508330, vtable slot 1) and
//! its inlined destructor `Task_Missile::dtor1` (0x005086F0).

use core::sync::atomic::AtomicU32;

use super::handle_message::bridge_finish_super_animal;
use super::{MissileEntity, MissileEntityVtable};
use crate::engine::EntityActivityQueue;
use crate::entity::base::{BaseEntity, SharedDataTable};
use crate::rebase::rb;
use crate::wa_alloc::wa_free;

static mut FREE_ACTIVITY_SLOT_ADDR: u32 = 0;
static mut CGAMETASK_DESTRUCTOR_ADDR: u32 = 0;

pub static ORIGINAL_FREE: AtomicU32 = AtomicU32::new(0);

pub unsafe fn init_addrs() {
    unsafe {
        FREE_ACTIVITY_SLOT_ADDR = rb(0x00541860);
        CGAMETASK_DESTRUCTOR_ADDR = rb(0x004FEF30);
    }
}

/// `EntityActivityQueue::FreeSlotById` (0x00541860) — `__usercall(EAX = queue,
/// [stack] = slot)`, RET 0x4.
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

/// `WorldEntity::Destructor` (0x004FEF30) — SEH-protected children-list walk.
#[inline]
unsafe fn bridge_cgametask_destructor(this: *mut MissileEntity) {
    type Fn = unsafe extern "thiscall" fn(*mut MissileEntity);
    let f: Fn = unsafe { core::mem::transmute(CGAMETASK_DESTRUCTOR_ADDR as usize) };
    unsafe { f(this) }
}

/// Send `msg_id` to the WorldRoot dispatcher (SharedData key `(0, 0x14)`).
/// WA passes uninitialised stack here, but the receivers only read the first
/// dword for these msg ids, so zeroing the tail is equivalent.
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
/// allocated by `Task_Missile::ConstructPointers`). Each child is released
/// through its own vtable slot 3 (C++ scalar-deleting destructor) before the
/// wrapper itself is freed.
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

/// `Task_Missile::dtor1` (0x005086F0).
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

        super::sound::stop_fuse_sound(this);
        super::sound::stop_dig_sound(this);

        if (*this).super_animal_target_locked != 0 {
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

/// `MissileEntity::Free` (0x00508330, vtable slot 1). Runs the destructor
/// and frees the heap allocation when bit 0 of `flags` is set.
pub unsafe extern "thiscall" fn free(this: *mut MissileEntity, flags: u8) -> *mut MissileEntity {
    unsafe {
        destructor_1(this);
        if (flags & 1) != 0 {
            wa_free(this as *mut u8);
        }
        this
    }
}
