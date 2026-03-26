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
use openwa_core::task::{CTask, Task};

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
    let cloud = &mut *this;
    let msg = TaskMessage::try_from(msg_type);

    match msg {
        Ok(TaskMessage::FrameFinish) => {
            // Advance Y position
            cloud.pos_y = Fixed(cloud.pos_y.0 + cloud.vel_y.0);

            // Advance X position: base velocity + wind * 10
            let wind = cloud.wind_accel.0;
            cloud.pos_x = Fixed(cloud.pos_x.0 + cloud.vel_x.0 + wind * 10);

            // Wrap X at landscape bounds (with 128.0 Fixed padding)
            let ddgame = &*cloud.ddgame();
            let padding = Fixed::from_int(128);
            let level_left = ddgame.level_bound_min_x - padding;
            let level_right = ddgame.level_bound_max_x + padding;

            if cloud.pos_x < level_left {
                cloud.pos_x = level_right;
            } else if cloud.pos_x > level_right {
                cloud.pos_x = level_left;
            }

            // Converge wind_accel toward wind_target (clamp step to ±0x147)
            let target = cloud.wind_target.0;
            let current = cloud.wind_accel.0;
            let diff = target - current;
            if diff.abs() < 0x147 {
                cloud.wind_accel = Fixed(target);
            } else if current < target {
                cloud.wind_accel = Fixed(current + 0x147);
            } else {
                cloud.wind_accel = Fixed(current - 0x147);
            }
        }

        Ok(TaskMessage::RenderScene) => {
            let ddgame = &mut *cloud.ddgame();

            // Only render when rendering phase == 5 (in-game rendering active)
            if ddgame.render_phase == 5 {
                // Compute parallax X offset: (vel_x + wind * 10) * parallax_scale
                let scroll_speed = cloud.vel_x.0 + cloud.wind_accel.0 * 10;
                let parallax_x = ((scroll_speed as i64 * ddgame.parallax_scale as i64) >> 16) as i32;
                let x = parallax_x + cloud.pos_x.0;

                let rq = &mut *ddgame.render_queue;
                if let Some(entry) = rq.alloc::<DrawSpriteCmd>() {
                    *entry = DrawSpriteCmd {
                        command_type: command_type::DRAW_SPRITE_LOCAL,
                        layer: cloud.layer_depth.0 as u32,
                        x_pos: x as u32 & 0xFFFF0000,
                        y_pos: cloud.pos_y.0 as u32 & 0xFFFF0000,
                        sprite_id: cloud.sprite_id,
                        frame: 0,
                    };
                }
            }
        }

        Ok(TaskMessage::SetWind) => {
            if !data.is_null() {
                cloud.wind_target = Fixed(*(data as *const i32));
            }
        }

        _ => {}
    }

    // Call base CTask::HandleMessage for all non-render messages
    let base_handler: unsafe extern "thiscall" fn(*mut CTask, *mut CTask, u32, u32, *const u8) =
        core::mem::transmute(openwa_core::rebase::rb(va::CTASK_VT2_HANDLE_MESSAGE) as usize);
    base_handler(cloud.as_task_ptr_mut(), sender, msg_type, size, data);
}

pub fn install() -> Result<(), String> {
    use openwa_core::vtable_replace;

    vtable_replace!(openwa_core::task::cloud::CTaskCloudVTable, va::CTASK_CLOUD_VTABLE, {
        handle_message => cloud_handle_message,
    })?;

    let _ = log_line("[Cloud] HandleMessage hooked via vtable_replace");
    Ok(())
}
