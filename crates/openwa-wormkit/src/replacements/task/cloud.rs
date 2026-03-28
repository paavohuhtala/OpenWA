//! CTaskCloud vtable hooks.
//!
//! Replaces CTaskCloud::HandleMessage (vtable slot 2).
//! Handles cloud position updates (wind parallax), rendering, and wind changes.

use openwa_core::address::va;
use openwa_core::fixed::Fixed;
use openwa_core::game::TaskMessage;
use openwa_core::log::log_line;
use openwa_core::render::queue::{command_type, DrawSpriteCmd};
use openwa_core::task::cloud::CTaskCloud;
use openwa_core::task::CTask;

/// CTaskCloud::HandleMessage replacement.
///
/// Handles three message types:
/// - FrameFinish: per-frame position update (parallax scroll with wind drift)
/// - RenderScene: draw sprite at computed position — delegates to original
/// - SetWind: set wind target from message data
///
/// Always calls base CTask::HandleMessage at the end (0x562F30).
unsafe extern "thiscall" fn cloud_handle_message(
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
                let parallax_x = ((scroll_speed as i64 * (*ddgame).parallax_scale as i64) >> 16) as i32;
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

pub fn install() -> Result<(), String> {
    use openwa_core::vtable_replace;

    vtable_replace!(openwa_core::task::cloud::CTaskCloudVTable, va::CTASK_CLOUD_VTABLE, {
        handle_message => cloud_handle_message,
    })?;

    let _ = log_line("[Cloud] HandleMessage hooked via vtable_replace");
    Ok(())
}
