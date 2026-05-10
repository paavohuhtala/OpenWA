//! Pure-Rust port of `MissileEntity::Free` (0x00508330, vtable slot 1) and
//! its inlined destructor `Task_Missile::dtor1` (0x005086F0).

use core::sync::atomic::AtomicU32;

use super::super_animal::finish_super_animal;
use super::{MissileEntity, MissileEntityVtable};
use crate::engine::EntityActivityQueue;
use crate::entity::Entity;
use crate::entity::base::BaseEntity;
use crate::game::message::{Unknown83Message, Unknown124Message, WeaponDestroyedMessage};
use crate::rebase::rb;
use crate::render::textbox::Textbox;
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

/// `Task_Missile::dtor1` (0x005086F0).
unsafe fn destructor_1(this: *mut MissileEntity) {
    unsafe {
        (*this).base.base.vtable = rb(super::MISSILE_ENTITY_VTABLE) as *const MissileEntityVtable;

        let world = (*(this as *const BaseEntity)).world;
        (*world).object_pool_count = (*world).object_pool_count.wrapping_sub(7);

        let owner_id = (*this).spawn_params.owner_id;
        if owner_id != 0 {
            (*this).broadcast_via_world_root(WeaponDestroyedMessage {
                team_index: owner_id,
            });
        }

        let queue = core::ptr::addr_of_mut!((*world).entity_activity_queue);
        bridge_free_activity_slot(queue, (*this).activity_rank_slot as i32);

        super::sound::stop_fuse_sound(this);
        super::sound::stop_dig_sound(this);

        if (*this).super_animal_target_locked != 0 {
            (*this).broadcast_via_world_root(Unknown83Message {
                team_index: owner_id,
            });
        }

        match (*this).contact_phase {
            1 => finish_super_animal(this),
            2 => (*this).broadcast_via_world_root(Unknown124Message {
                team_index: owner_id,
            }),
            _ => {}
        }

        if (*world).is_headful != 0 {
            Textbox::destroy((*this).render_handle_a);
            Textbox::destroy((*this).render_handle_b);
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
