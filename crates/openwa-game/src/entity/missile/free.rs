//! Pure-Rust port of `MissileEntity::Free` (0x00508330, vtable slot 1) and
//! its inlined destructor `Task_Missile::dtor1` (0x005086F0).

use core::sync::atomic::AtomicU32;

use super::super_animal::finish_super_animal;
use super::{MissileEntity, MissileEntityVtable};
use crate::entity::Entity;
use crate::entity::base::BaseEntity;
use crate::entity::game_entity::WorldEntity;
use crate::game::message::{Unknown83Message, Unknown124Message, WeaponDestroyedMessage};
use crate::generated::wa_calls;
use crate::rebase::rb;
use crate::render::textbox::Textbox;
use crate::wa_alloc::wa_free;

pub static ORIGINAL_FREE: AtomicU32 = AtomicU32::new(0);

pub unsafe fn init_addrs() {}

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
        wa_calls::EntityActivityQueue::FreeSlotById(queue, (*this).activity_rank_slot as i32);

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

        wa_calls::GameCollisionTask::Destructor_1(this as *mut WorldEntity);
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
