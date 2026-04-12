//! Bungee trail rendering — draws the bungee drop trajectory path.
//!
//! Ported from WA.exe `DrawBungeeTrail` (0x500720).
//! Draws:
//!   1. Sprite at trail start position
//!   2. Series of vertices computed by accumulating angle + trig interpolation
//!   3. Final vertex at task position (0x84/0x88)
//!   4. DrawPolygon (if fill != 0) or DrawLineStrip

use crate::address::va;
use crate::fixed::Fixed;
use crate::rebase::rb;
use crate::render::queue::*;
use crate::render::SpriteOp;
use crate::task::BungeeTrailTask;
use crate::trig::trig_lookup;

/// Draw bungee trail for the given task.
///
/// # Safety
///
/// `task_ptr` must point to a valid `BungeeTrailTask`. ASLR rebase must be
/// initialized.
pub unsafe fn draw_bungee_trail(task: *const BungeeTrailTask, style: u32, fill: u32) {
    let task = &*task;

    if task.trail_visible == 0 {
        return;
    }

    let ddgame = &*task.base.ddgame;
    let rq = &mut *ddgame.render_queue;

    let seg_data = task.segment_data;
    if seg_data.is_null() {
        return;
    }

    let segment_count = task.segment_count;
    if segment_count <= 0 {
        return;
    }

    let mut x = task.trail_start_x;
    let mut y = task.trail_start_y;

    let first_angle = *(seg_data.add(4) as *const i32);

    // Enqueue start sprite (command type 5 = local)
    if let Some(entry) = rq.alloc::<DrawSpriteCmd>() {
        *entry = DrawSpriteCmd {
            command_type: command_type::DRAW_SPRITE_LOCAL,
            layer: 0xDFFFF,
            x: Fixed(x).floor(),
            y: Fixed(y).floor(),
            sprite: SpriteOp(0x45),
            palette: (first_angle + 0x8100) as u32,
        };
    }

    // Build vertex array from trail segments
    const MAX_VERTICES: usize = 256;
    let mut verts = [[0i32; 3]; MAX_VERTICES];
    let mut vert_count: usize = 0;
    let mut accumulated_angle: u32 = 0;

    let sin_table = rb(va::G_SIN_TABLE) as *const i32;
    let cos_table = rb(va::G_COS_TABLE) as *const i32;

    for i in 0..segment_count {
        let seg_angle = *(seg_data.add(4 + i as usize * 8) as *const i32);

        // Include vertex if: first segment, or segment has nonzero angle, or fill mode
        if (i == 0 || seg_angle != 0 || fill != 0) && vert_count < MAX_VERTICES {
            verts[vert_count] = [x, y, 0];
            vert_count += 1;
        }

        accumulated_angle = accumulated_angle.wrapping_add(seg_angle as u32);

        let sin_interp = trig_lookup(sin_table, accumulated_angle);
        let cos_interp = trig_lookup(cos_table, accumulated_angle);

        x = x.wrapping_add(sin_interp.0.wrapping_mul(8));
        y = y.wrapping_sub(cos_interp.0.wrapping_mul(8));
    }

    // Final vertex = task position (target)
    if vert_count < MAX_VERTICES {
        verts[vert_count] = [task.pos_x.0, task.pos_y.0, 0];
        vert_count += 1;
    }

    // Enqueue as polygon or line strip
    if fill != 0 {
        let total_size = vert_count * 0xC + 0x20;
        if let Some(ptr) = rq.alloc_raw(total_size) {
            let header = &mut *(ptr as *mut DrawPolygonHeader);
            *header = DrawPolygonHeader {
                command_type: command_type::DRAW_POLYGON,
                layer: 0xE_0000,
                count: vert_count as u32,
                color1: style,
                color2: fill,
            };
            core::ptr::copy_nonoverlapping(
                verts.as_ptr() as *const u8,
                ptr.add(core::mem::size_of::<DrawPolygonHeader>()),
                vert_count * 0xC,
            );
        }
    } else {
        let total_size = vert_count * 0xC + 0x1C;
        if let Some(ptr) = rq.alloc_raw(total_size) {
            let header = &mut *(ptr as *mut DrawLineStripHeader);
            *header = DrawLineStripHeader {
                command_type: command_type::DRAW_LINE_STRIP,
                layer: 0xE_0000,
                count: vert_count as u32,
                color: style,
            };
            core::ptr::copy_nonoverlapping(
                verts.as_ptr() as *const u8,
                ptr.add(core::mem::size_of::<DrawLineStripHeader>()),
                vert_count * 0xC,
            );
        }
    }
}
