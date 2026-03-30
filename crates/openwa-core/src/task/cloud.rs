use super::base::CTask;
use crate::fixed::Fixed;
use crate::FieldRegistry;

crate::define_addresses! {
    class "CTaskCloud" {
        /// CTaskCloud vtable - cloud/airstrike entity
        vtable CTASK_CLOUD_VTABLE = 0x0066_9D38;
        ctor CTASK_CLOUD_CTOR = 0x0054_82E0;
    }
}

/// CTaskCloud vtable — 12 slots. Extends CTask base (8 slots) with cloud behavior.
///
/// Vtable at Ghidra 0x669D38.
#[openwa_core::vtable(size = 12, va = 0x0066_9D38, class = "CTaskCloud")]
pub struct CTaskCloudVTable {
    /// HandleMessage — processes cloud messages (wind updates, render).
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message:
        fn(this: *mut CTaskCloud, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
    /// ProcessFrame — per-frame cloud update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut CTaskCloud, flags: u32),
}

/// Airstrike / weather cloud task.
///
/// Extends CTask directly (not CGameTask). Clouds drift horizontally with wind,
/// scroll on a parallax layer, and render as a single sprite.
///
/// Allocation: 0x74 bytes (operator new in CTaskTeam__CreateWeatherFilter).
/// Constructor: 0x5482E0 (usercall ESI=this, EAX=parent).
/// Vtable: 0x669D38. Class type byte: 0x17.
///
/// Three cloud sizes chosen by `cloud_type` param (0/1/2):
/// - type 0: sprite 0x268 (large),  vel_y 0x200
/// - type 1: sprite 0x269 (medium), vel_y 0x166
/// - type 2: sprite 0x26A (small),  vel_y 0xCC
///
/// Source: Ghidra decompilation of 0x5482E0 (constructor) and
///         0x5484C0 (HandleMessage update + render branches).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskCloud {
    /// 0x00–0x2F: CTask base
    pub base: CTask<*const CTaskCloudVTable>,
    /// 0x30: Parallax scroll layer depth (Fixed; 0x190000 = 25.0 at spawn,
    /// decrements by 1 each cloud spawned in a batch)
    pub layer_depth: Fixed,
    /// 0x34: Y position (Fixed 16.16); updated each frame: pos_y += vel_y
    pub pos_y: Fixed,
    /// 0x38: Y velocity (Fixed 16.16; set by cloud type: large=0x200, medium=0x166, small=0xCC)
    pub vel_y: Fixed,
    /// 0x3C: Sprite ID passed to DrawSpriteLocal (0x268=large, 0x269=medium, 0x26A=small)
    pub sprite_id: u32,
    /// 0x40: X position (Fixed 16.16); wraps at landscape bounds each frame
    pub pos_x: Fixed,
    /// 0x44: Unknown (set from EDI at call site)
    pub _unknown_44: u32,
    /// 0x48: X velocity base (Fixed 16.16)
    pub vel_x: Fixed,
    /// 0x4C: Current wind acceleration (Fixed); converges toward wind_target each frame
    pub wind_accel: Fixed,
    /// 0x50: Target wind speed (Fixed); set by message 0x54 (wind-change event)
    pub wind_target: Fixed,
    /// 0x54–0x73: Unknown
    pub _unknown_54: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<CTaskCloud>() == 0x74);

// Generate typed vtable method wrappers: handle_message(), process_frame().
bind_CTaskCloudVTable!(CTaskCloud, base.vtable);

use crate::game::TaskMessage;
use crate::render::queue::{command_type, DrawSpriteCmd};

/// CTaskCloud::HandleMessage replacement — pure game logic.
///
/// Handles three message types:
/// - FrameFinish: per-frame position update (parallax scroll with wind drift)
/// - RenderScene: draw sprite at computed position
/// - SetWind: set wind target from message data
///
/// Always calls base CTask::HandleMessage at the end (broadcast to children).
pub unsafe extern "thiscall" fn cloud_handle_message(
    this: *mut CTaskCloud,
    sender: *mut CTask,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    let msg = TaskMessage::try_from(msg_type);

    match msg {
        Ok(TaskMessage::FrameFinish) => {
            // Advance Y position
            (*this).pos_y = Fixed((*this).pos_y.0 + (*this).vel_y.0);

            // Advance X position: base velocity + wind * 10
            let wind = (*this).wind_accel.0;
            (*this).pos_x = Fixed((*this).pos_x.0 + (*this).vel_x.0 + wind * 10);

            // Wrap X at landscape bounds (with 128.0 Fixed padding)
            let ddgame = CTask::ddgame_raw(this as *const CTask);
            let padding = Fixed::from_int(128);
            let level_left = (*ddgame).level_bound_min_x - padding;
            let level_right = (*ddgame).level_bound_max_x + padding;

            if (*this).pos_x < level_left {
                (*this).pos_x = level_right;
            } else if (*this).pos_x > level_right {
                (*this).pos_x = level_left;
            }

            // Converge wind_accel toward wind_target (clamp step to ±0x147)
            let target = (*this).wind_target.0;
            let current = (*this).wind_accel.0;
            let diff = target - current;
            if diff.abs() < 0x147 {
                (*this).wind_accel = Fixed(target);
            } else if current < target {
                (*this).wind_accel = Fixed(current + 0x147);
            } else {
                (*this).wind_accel = Fixed(current - 0x147);
            }
        }

        Ok(TaskMessage::RenderScene) => {
            let ddgame = CTask::ddgame_raw(this as *const CTask);

            // Only render when rendering phase == 5 (in-game rendering active)
            if (*ddgame).render_phase == 5 {
                // Compute parallax X offset: (vel_x + wind * 10) * parallax_scale
                let scroll_speed = (*this).vel_x.0 + (*this).wind_accel.0 * 10;
                let parallax_x =
                    ((scroll_speed as i64 * (*ddgame).parallax_scale as i64) >> 16) as i32;
                let x = parallax_x + (*this).pos_x.0;

                let rq = &mut *(*ddgame).render_queue;
                if let Some(entry) = rq.alloc::<DrawSpriteCmd>() {
                    *entry = DrawSpriteCmd {
                        command_type: command_type::DRAW_SPRITE_LOCAL,
                        layer: (*this).layer_depth.0 as u32,
                        x_pos: x as u32 & 0xFFFF0000,
                        y_pos: (*this).pos_y.0 as u32 & 0xFFFF0000,
                        sprite_id: (*this).sprite_id,
                        frame: 0,
                    };
                }
            }
        }

        Ok(TaskMessage::SetWind) => {
            if !data.is_null() {
                (*this).wind_target = Fixed(*(data as *const i32));
            }
        }

        _ => {}
    }

    // Broadcast to children — raw-pointer version avoids noalias UB
    CTask::broadcast_message_raw(this as *mut CTask, sender, msg_type, size, data);
}
