//! CTaskCloud vtable hooks.
//!
//! Replaces CTaskCloud::HandleMessage (vtable slot 2).
//! Handles cloud position updates (wind parallax), rendering, and wind changes.

use core::sync::atomic::{AtomicU32, Ordering};

use openwa_core::address::va;
use openwa_core::fixed::Fixed;
use openwa_core::game::TaskMessage;
use openwa_core::log::log_line;
use openwa_core::task::cloud::CTaskCloud;
use openwa_core::task::{CTask, Task};

/// Original CTaskCloud::HandleMessage, saved for render call-through.
static ORIG_HANDLE_MESSAGE: AtomicU32 = AtomicU32::new(0);

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

            // Wrap X at landscape bounds (with 0x800000 = 128.0 padding)
            let ddgame = &*cloud.ddgame();
            let ddgame_raw = ddgame as *const _ as *const u8;
            let level_left = *(ddgame_raw.add(0x779C) as *const i32) - 0x800000;
            let level_right = *(ddgame_raw.add(0x77A0) as *const i32) + 0x800000;

            if cloud.pos_x.0 < level_left {
                cloud.pos_x = Fixed(level_right);
            } else if cloud.pos_x.0 > level_right {
                cloud.pos_x = Fixed(level_left);
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
            // Delegate to original (uses usercall for RQ_DrawSpriteLocal)
            let orig = ORIG_HANDLE_MESSAGE.load(Ordering::Relaxed);
            let orig_fn: unsafe extern "thiscall" fn(
                *mut CTaskCloud, *mut CTask, u32, u32, *const u8,
            ) = core::mem::transmute(orig as usize);
            orig_fn(this, sender, msg_type, size, data);
            return; // original already calls base handler
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
        handle_message [ORIG_HANDLE_MESSAGE] => cloud_handle_message,
    })?;

    let _ = log_line("[Cloud] HandleMessage hooked via vtable_replace");
    Ok(())
}
