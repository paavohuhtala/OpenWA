//! RenderQueue enqueue hooks — full Rust replacements.
//!
//! All functions enqueue commands to the RenderQueue's downward-growing buffer.
//! Calling conventions are __usercall variants with register + stack params.

use openwa_lib::rebase::rb;
use openwa_types::address::va;
use openwa_types::ddgame::{offsets as dg, DDGame};
use openwa_types::render::*;
use openwa_types::task::CGameTask;

use crate::hook::{self, usercall_trampoline};

// ---------------------------------------------------------------------------
// DrawPixel (0x541D60) — type 0xD, EAX=this, 3 stack, RET 0xC
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_pixel; impl_fn = draw_pixel_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn draw_pixel_impl(
    this: u32,
    x_pos: u32,
    y_pos: u32,
    flags: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawPixelCmd>() {
        *entry = DrawPixelCmd {
            command_type: command_type::DRAW_PIXEL,
            layer: 0x1B_0000,
            color: 0xFF00_0000,
            x_pos,
            y_pos,
            flags: flags as u8,
            _pad: [0; 3],
        };
    }
}

// ---------------------------------------------------------------------------
// DrawLineStrip (0x541DD0) — type 8, EAX=this, EDI=count, 2 stack, RET 0x8
// Allocation: count * 0xC + 0x1C (variable size)
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_line_strip; impl_fn = draw_line_strip_impl;
    regs = [eax, edi]; stack_params = 2; ret_bytes = "0x8");

unsafe extern "cdecl" fn draw_line_strip_impl(
    this: u32,
    count: u32,
    vertex_ptr: u32,
    param_1: u32,
) {
    let q = &mut *(this as *mut RenderQueue);
    let total_size = count as usize * 0xC + 0x1C;

    if let Some(ptr) = q.alloc_raw(total_size) {
        let header = &mut *(ptr as *mut DrawLineStripHeader);
        *header = DrawLineStripHeader {
            command_type: command_type::DRAW_LINE_STRIP,
            layer: 0xE_0000,
            count,
            param_1,
        };
        core::ptr::copy_nonoverlapping(
            vertex_ptr as *const u8,
            ptr.add(core::mem::size_of::<DrawLineStripHeader>()),
            count as usize * 0xC,
        );
    }
}

// ---------------------------------------------------------------------------
// DrawPolygon (0x541E50) — type 9, ECX=this, ESI=count, 3 stack, RET 0xC
// Allocation: count * 0xC + 0x20 (variable size)
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_polygon; impl_fn = draw_polygon_impl;
    regs = [ecx, esi]; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn draw_polygon_impl(
    this: u32,
    count: u32,
    vertex_ptr: u32,
    param_1: u32,
    param_2: u32,
) {
    let q = &mut *(this as *mut RenderQueue);
    let total_size = count as usize * 0xC + 0x20;

    if let Some(ptr) = q.alloc_raw(total_size) {
        let header = &mut *(ptr as *mut DrawPolygonHeader);
        *header = DrawPolygonHeader {
            command_type: command_type::DRAW_POLYGON,
            layer: 0xE_0000,
            count,
            param_1,
            param_2,
        };
        core::ptr::copy_nonoverlapping(
            vertex_ptr as *const u8,
            ptr.add(core::mem::size_of::<DrawPolygonHeader>()),
            count as usize * 0xC,
        );
    }
}

// ---------------------------------------------------------------------------
// DrawScaled (0x541ED0) — type 0xB, ECX=this, 5 stack, RET 0x14
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_scaled; impl_fn = draw_scaled_impl;
    reg = ecx; stack_params = 5; ret_bytes = "0x14");

unsafe extern "cdecl" fn draw_scaled_impl(
    this: u32,
    layer: u32,
    sprite_id: u32,
    frame: u32,
    x_pos: u32,
    y_pos: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawScaledCmd>() {
        *entry = DrawScaledCmd {
            command_type: command_type::DRAW_SCALED,
            layer,
            x_pos,
            y_pos,
            sprite_id,
            frame,
            _reserved: 0,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawRect (0x541F40) — type 0, ECX=this, EDX=y_clip, 6 stack, RET 0x18
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_rect; impl_fn = draw_rect_impl;
    regs = [ecx, edx]; stack_params = 6; ret_bytes = "0x18");

unsafe extern "cdecl" fn draw_rect_impl(
    this: u32,
    y_clip: u32,
    layer: u32,
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    color: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawRectCmd>() {
        *entry = DrawRectCmd {
            command_type: command_type::DRAW_RECT,
            layer,
            color,
            x1: x1 & 0xFFFF0000,
            y1: y1 & 0xFFFF0000,
            x2: x2 & 0xFFFF0000,
            y2: y2 & 0xFFFF0000,
            y_clip: y_clip & 0xFFFF0000,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawSpriteGlobal (0x541FE0) — type 4, EAX=y, ECX=this, 4 stack, RET 0x10
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_sprite_global; impl_fn = draw_sprite_global_impl;
    regs = [eax, ecx]; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn draw_sprite_global_impl(
    y_pos: u32,
    this: u32,
    layer: u32,
    x_pos: u32,
    sprite_id: u32,
    frame: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawSpriteCmd>() {
        *entry = DrawSpriteCmd {
            command_type: command_type::DRAW_SPRITE_GLOBAL,
            layer,
            x_pos: x_pos & 0xFFFF0000,
            y_pos: y_pos & 0xFFFF0000,
            sprite_id,
            frame,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawSpriteLocal (0x542060) — type 5, EAX=y, ECX=this, 4 stack, RET 0x10
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_sprite_local; impl_fn = draw_sprite_local_impl;
    regs = [eax, ecx]; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn draw_sprite_local_impl(
    y_pos: u32,
    this: u32,
    layer: u32,
    x_pos: u32,
    sprite_id: u32,
    frame: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawSpriteCmd>() {
        *entry = DrawSpriteCmd {
            command_type: command_type::DRAW_SPRITE_LOCAL,
            layer,
            x_pos: x_pos & 0xFFFF0000,
            y_pos: y_pos & 0xFFFF0000,
            sprite_id,
            frame,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawSpriteOffset (0x5420E0) — type 6, ECX=this, EDX=y_clip, 6 stack, RET 0x18
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_sprite_offset; impl_fn = draw_sprite_offset_impl;
    regs = [ecx, edx]; stack_params = 6; ret_bytes = "0x18");

unsafe extern "cdecl" fn draw_sprite_offset_impl(
    this: u32,
    y_clip: u32,
    layer: u32,
    x_pos: u32,
    y_pos: u32,
    sprite_id: u32,
    param_7: u32,
    param_8: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawSpriteOffsetCmd>() {
        *entry = DrawSpriteOffsetCmd {
            command_type: command_type::DRAW_SPRITE_OFFSET,
            layer,
            sprite_id,
            x_pos: x_pos & 0xFFFF0000,
            y_pos: y_pos & 0xFFFF0000,
            _reserved: 0,
            y_clip: y_clip & 0xFFFF0000,
            param_7,
            param_8,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawBitmapGlobal (0x542170) — type 1, ECX=this, EDX=y, 7 stack, RET 0x1C
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_bitmap_global; impl_fn = draw_bitmap_global_impl;
    regs = [ecx, edx]; stack_params = 7; ret_bytes = "0x1C");

unsafe extern "cdecl" fn draw_bitmap_global_impl(
    this: u32,
    y_pos: u32,
    layer: u32,
    x_pos: u32,
    bitmap_ptr: u32,
    param_6: u32,
    param_7: u32,
    param_8: u32,
    param_9: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawBitmapGlobalCmd>() {
        *entry = DrawBitmapGlobalCmd {
            command_type: command_type::DRAW_BITMAP_GLOBAL,
            layer,
            x_pos: x_pos & 0xFFFF0000,
            y_pos: y_pos & 0xFFFF0000,
            bitmap_ptr,
            _reserved: 0,
            param_6,
            param_7,
            param_8,
            param_9,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawTextboxLocal (0x542200) — type 2, ECX=this, EDX=y, 6 stack, RET 0x18
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_textbox_local; impl_fn = draw_textbox_local_impl;
    regs = [ecx, edx]; stack_params = 6; ret_bytes = "0x18");

unsafe extern "cdecl" fn draw_textbox_local_impl(
    this: u32,
    y_pos: u32,
    layer: u32,
    x_pos: u32,
    text_ptr: u32,
    param_6: u32,
    param_7: u32,
    param_8: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawTextboxLocalCmd>() {
        *entry = DrawTextboxLocalCmd {
            command_type: command_type::DRAW_TEXTBOX_LOCAL,
            layer,
            _reserved_2: 0,
            x_pos: x_pos & 0xFFFF0000,
            y_pos: y_pos & 0xFFFF0000,
            _reserved_5: 0,
            _reserved_6: 0,
            text_ptr,
            _reserved_8: 0,
            _reserved_9: 0,
            param_6,
            param_7,
            param_8,
        };
    }
}

// ---------------------------------------------------------------------------
// Fixed-point trig helpers
// ---------------------------------------------------------------------------

/// Fixed-point 16.16 multiply: ((a * b) >> 16)
#[inline]
fn fixed_mul(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 16) as i32
}

/// Interpolated lookup from a 1024-entry fixed-point trig table.
/// Index = (angle >> 6) & 0x3FF, fraction = (angle & 0x3F) << 10.
#[inline]
unsafe fn trig_lookup(table: *const i32, angle: u32) -> i32 {
    let index = ((angle as i32) >> 6) as usize & 0x3FF;
    let frac = ((angle & 0x3F) << 10) as i32;
    let base = *table.add(index);
    let next = *table.add(index + 1);
    fixed_mul(next - base, frac) + base
}

// ---------------------------------------------------------------------------
// DrawBungeeTrail (0x500720) — stdcall(task, style, fill), RET 0xC
//
// Draws bungee drop trajectory path:
//   1. Sprite at trail start position
//   2. Series of vertices computed by accumulating angle + trig interpolation
//   3. Final vertex at task position (0x84/0x88)
//   4. DrawPolygon (if fill != 0) or DrawLineStrip
// Triggered by Bungee weapon (field_0x30==4, field_0x34==7) check in FUN_00519F60.
// Gated by task+0xBC flag set by InitWormTrail (0x5008D0).
// ---------------------------------------------------------------------------

unsafe extern "stdcall" fn draw_bungee_trail_impl(
    task_ptr: u32,
    style: u32,
    fill: u32,
) {
    let task = task_ptr as *mut u8;
    let game_task = task_ptr as *const CGameTask;

    // Early exit if trail not visible (set by InitWormTrail when Bungee is used)
    if *(task.add(0xBC) as *const i32) == 0 {
        return;
    }

    let ddgame = &*((*game_task).base.ddgame as *const DDGame);
    let rq = &mut *ddgame.render_queue;

    let seg_data = *(task.add(0xE4) as *const *const u8);
    if seg_data.is_null() {
        return;
    }

    let segment_count = *(task.add(0xD0) as *const i32);
    if segment_count <= 0 {
        return;
    }

    let mut x = *(task.add(0xC0) as *const i32);
    let mut y = *(task.add(0xC4) as *const i32);

    let first_angle = *(seg_data.add(4) as *const i32);

    // Enqueue start sprite (command type 5 = local)
    if let Some(entry) = rq.alloc::<DrawSpriteCmd>() {
        *entry = DrawSpriteCmd {
            command_type: command_type::DRAW_SPRITE_LOCAL,
            layer: 0xDFFFF,
            x_pos: x as u32 & 0xFFFF0000,
            y_pos: y as u32 & 0xFFFF0000,
            sprite_id: 0x45,
            frame: (first_angle + 0x8100) as u32,
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
        if i == 0 || seg_angle != 0 || fill != 0 {
            if vert_count < MAX_VERTICES {
                verts[vert_count] = [x, y, 0];
                vert_count += 1;
            }
        }

        accumulated_angle = accumulated_angle.wrapping_add(seg_angle as u32);

        let sin_interp = trig_lookup(sin_table, accumulated_angle);
        let cos_interp = trig_lookup(cos_table, accumulated_angle);

        x = x.wrapping_add(sin_interp.wrapping_mul(8));
        y = y.wrapping_sub(cos_interp.wrapping_mul(8));
    }

    // Final vertex = task position (target)
    if vert_count < MAX_VERTICES {
        verts[vert_count] = [(*game_task).pos_x.0, (*game_task).pos_y.0, 0];
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
                param_1: style,
                param_2: fill,
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
                param_1: style,
            };
            core::ptr::copy_nonoverlapping(
                verts.as_ptr() as *const u8,
                ptr.add(core::mem::size_of::<DrawLineStripHeader>()),
                vert_count * 0xC,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// DrawCrosshairLine (0x5197D0) — usercall(EDI=task_ptr), plain RET
//
// Draws the weapon aiming crosshair line:
//   1. Compute direction from angle at task+0x264
//   2. Compute line length from DDGame scale + task offset
//   3. Endpoint = start + direction * length (with overflow clamping)
//   4. DrawPolygon (2 vertices) for the line
//   5. Conditionally DrawSpriteLocal at endpoint (crosshair sprite)
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_crosshair_line; impl_fn = draw_crosshair_line_impl;
    reg = edi);

unsafe extern "cdecl" fn draw_crosshair_line_impl(task_ptr: u32) {
    let task = task_ptr as *const u8;
    let game_task = task_ptr as *const CGameTask;

    // Early exit if aiming not active (derived class field)
    if *(task.add(0x258) as *const i32) == 0 {
        return;
    }

    let ddgame_ptr = (*game_task).base.ddgame as *const u8;
    let ddgame = &*(ddgame_ptr as *const DDGame);
    let rq = &mut *ddgame.render_queue;

    let start_x = (*game_task).pos_x.0;
    let start_y = (*game_task).pos_y.0;

    let angle = *(task.add(0x264) as *const u32);

    // Trig interpolation
    let sin_table = rb(va::G_SIN_TABLE) as *const i32;
    let cos_table = rb(va::G_COS_TABLE) as *const i32;
    let sin_interp = trig_lookup(sin_table, angle);
    let cos_interp = trig_lookup(cos_table, angle);

    // Scale = fixed_mul(DDGame[CROSSHAIR_SCALE], 0x140000) + task[0x324]
    let ddgame_scale = *(ddgame_ptr.add(dg::CROSSHAIR_SCALE) as *const i32);
    let scale = fixed_mul(ddgame_scale, 0x14_0000)
        + *(task.add(0x324) as *const i32);

    // Endpoint = start + direction * scale
    let mut endpoint_x = fixed_mul(sin_interp, scale).wrapping_add(start_x);
    let mut endpoint_y = fixed_mul(cos_interp, scale).wrapping_add(start_y);

    // Overflow clamping — when endpoint overflows i32 due to large scale
    let mut overflowed = false;
    let mut clamp_factor = 0i32;

    let game_state = ddgame.game_state as *const u8;
    let threshold = *(game_state.add(0xD778) as *const i32);

    if threshold > 0x11E {
        // Check X overflow: sin > 0 but endpoint wrapped below start
        if sin_interp > 0 && endpoint_x < start_x {
            overflowed = true;
            clamp_factor = (0x7FFFFFFFi32 - start_x) / sin_interp;
        }
        // Check Y overflow: cos > 0 but endpoint wrapped below start
        if cos_interp > 0 && endpoint_y < start_y {
            let y_clamp = (0x7FFFFFFFi32 - start_y) / cos_interp;
            if !overflowed || y_clamp < clamp_factor {
                clamp_factor = y_clamp;
            }
            overflowed = true;
        }
        if overflowed {
            endpoint_x = start_x + clamp_factor * sin_interp;
            endpoint_y = start_y + clamp_factor * cos_interp;
        }
    }

    // Enqueue polygon line (2 vertices)
    let poly_param_1 = *(ddgame_ptr.add(dg::CROSSHAIR_LINE_PARAM_1) as *const u32);
    let poly_param_2 = *(ddgame_ptr.add(dg::CROSSHAIR_LINE_PARAM_2) as *const u32);
    let verts: [[i32; 3]; 2] = [
        [start_x, start_y, 0],
        [endpoint_x, endpoint_y, 0],
    ];
    let total_size = 2 * 0xC + 0x20;
    if let Some(ptr) = rq.alloc_raw(total_size) {
        let header = &mut *(ptr as *mut DrawPolygonHeader);
        *header = DrawPolygonHeader {
            command_type: command_type::DRAW_POLYGON,
            layer: 0xE_0000,
            count: 2,
            param_1: poly_param_1,
            param_2: poly_param_2,
        };
        core::ptr::copy_nonoverlapping(
            verts.as_ptr() as *const u8,
            ptr.add(core::mem::size_of::<DrawPolygonHeader>()),
            2 * 0xC,
        );
    }

    // Draw crosshair sprite at endpoint (only if no overflow clamping)
    if !overflowed {
        if let Some(entry) = rq.alloc::<DrawSpriteCmd>() {
            *entry = DrawSpriteCmd {
                command_type: command_type::DRAW_SPRITE_LOCAL,
                layer: 0x4_0000,
                x_pos: endpoint_x as u32 & 0xFFFF0000,
                y_pos: endpoint_y as u32 & 0xFFFF0000,
                sprite_id: 0x44,
                frame: (0x8000u32).wrapping_sub(angle),
            };
        }
    }
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

pub fn install() -> Result<(), String> {
    unsafe {
    let _ = hook::install(
        "DrawPixel",
        va::DRAW_PIXEL,
        trampoline_draw_pixel as *const (),
    )?;

    let _ = hook::install(
        "DrawLineStrip",
        va::DRAW_LINE_STRIP,
        trampoline_draw_line_strip as *const (),
    )?;

    let _ = hook::install(
        "DrawPolygon",
        va::DRAW_POLYGON,
        trampoline_draw_polygon as *const (),
    )?;

    let _ = hook::install(
        "DrawScaled",
        va::DRAW_SCALED,
        trampoline_draw_scaled as *const (),
    )?;

    let _ = hook::install(
        "DrawRect",
        va::DRAW_RECT,
        trampoline_draw_rect as *const (),
    )?;

    let _ = hook::install(
        "DrawSpriteGlobal",
        va::DRAW_SPRITE_GLOBAL,
        trampoline_draw_sprite_global as *const (),
    )?;

    let _ = hook::install(
        "DrawSpriteLocal",
        va::DRAW_SPRITE_LOCAL,
        trampoline_draw_sprite_local as *const (),
    )?;

    let _ = hook::install(
        "DrawSpriteOffset",
        va::DRAW_SPRITE_OFFSET,
        trampoline_draw_sprite_offset as *const (),
    )?;

    let _ = hook::install(
        "DrawBitmapGlobal",
        va::DRAW_BITMAP_GLOBAL,
        trampoline_draw_bitmap_global as *const (),
    )?;

    let _ = hook::install(
        "DrawTextboxLocal",
        va::DRAW_TEXTBOX_LOCAL,
        trampoline_draw_textbox_local as *const (),
    )?;

    let _ = hook::install(
        "DrawBungeeTrail",
        va::DRAW_BUNGEE_TRAIL,
        draw_bungee_trail_impl as *const (),
    )?;

    let _ = hook::install(
        "DrawCrosshairLine",
        va::DRAW_CROSSHAIR_LINE,
        trampoline_draw_crosshair_line as *const (),
    )?;

    }
    Ok(())
}
