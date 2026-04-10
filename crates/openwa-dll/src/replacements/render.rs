//! Render subsystem hooks — RenderQueue enqueue + DisplayGfx vtable patches.
//!
//! Combines two subsystems:
//! - RenderQueue enqueue hooks (full Rust replacements for command enqueueing)
//! - DisplayGfx vtable patches (DisplayBase pure-call stubs, headless destructor,
//!   and ported DisplayGfx methods)

use openwa_core::address::va;
use openwa_core::rebase::rb;
use openwa_core::render::queue::*;
use openwa_core::task::{BungeeTrailTask, WeaponAimTask};

use crate::hook::{self, usercall_trampoline};
use crate::log_line;

// ==========================================================================
// RenderQueue enqueue hooks
// ==========================================================================
//
// All functions enqueue commands to the RenderQueue's downward-growing buffer.
// Calling conventions are __usercall variants with register + stack params.

// ---------------------------------------------------------------------------
// DrawPixel (0x541D60) — type 0xD, EAX=this, 3 stack, RET 0xC
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_pixel; impl_fn = draw_pixel_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn draw_pixel_impl(this: u32, x_pos: u32, y_pos: u32, flags: u32) {
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
// DrawCrosshair (0x541ED0) — type 0xB, ECX=this, 5 stack, RET 0x14
// Enqueues a crosshair draw command. Dispatched by RenderDrawingQueue
// case 0xB → DisplayGfx::draw_crosshair (vtable slot 16).
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_crosshair; impl_fn = draw_crosshair_impl;
    reg = ecx; stack_params = 5; ret_bytes = "0x14");

unsafe extern "cdecl" fn draw_crosshair_impl(
    this: u32,
    layer: u32,
    x_pos: u32,
    y_pos: u32,
    color_fg: u32,
    color_bg: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawCrosshairCmd>() {
        *entry = DrawCrosshairCmd {
            command_type: command_type::DRAW_CROSSHAIR,
            layer,
            color_fg,
            color_bg,
            x_pos,
            y_pos,
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

unsafe extern "stdcall" fn draw_bungee_trail_impl(task_ptr: u32, style: u32, fill: u32) {
    let task = &*(task_ptr as *const BungeeTrailTask);

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
        if (i == 0 || seg_angle != 0 || fill != 0) && vert_count < MAX_VERTICES {
            verts[vert_count] = [x, y, 0];
            vert_count += 1;
        }

        accumulated_angle = accumulated_angle.wrapping_add(seg_angle as u32);

        let sin_interp = trig_lookup(sin_table, accumulated_angle);
        let cos_interp = trig_lookup(cos_table, accumulated_angle);

        x = x.wrapping_add(sin_interp.wrapping_mul(8));
        y = y.wrapping_sub(cos_interp.wrapping_mul(8));
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
    let task = &*(task_ptr as *const WeaponAimTask);
    let gt = &task.game_task;

    if task.aim_active == 0 {
        return;
    }

    let ddgame = &*gt.base.ddgame;
    let rq = &mut *ddgame.render_queue;

    let start_x = gt.pos_x.0;
    let start_y = gt.pos_y.0;

    let angle = task.aim_angle;

    // Trig interpolation
    let sin_table = rb(va::G_SIN_TABLE) as *const i32;
    let cos_table = rb(va::G_COS_TABLE) as *const i32;
    let sin_interp = trig_lookup(sin_table, angle);
    let cos_interp = trig_lookup(cos_table, angle);

    // Scale = fixed_mul(DDGame.parallax_scale, 0x140000) + task.aim_range_offset
    let scale = fixed_mul(ddgame.parallax_scale, 0x14_0000) + task.aim_range_offset;

    // Endpoint = start + direction * scale
    let mut endpoint_x = fixed_mul(sin_interp, scale).wrapping_add(start_x);
    let mut endpoint_y = fixed_mul(cos_interp, scale).wrapping_add(start_y);

    // Overflow clamping — when endpoint overflows i32 due to large scale
    let mut overflowed = false;
    let mut clamp_factor = 0i32;

    let threshold = (*ddgame.game_info).game_version;

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
    let poly_param_1 = ddgame.gfx_color_table[8]; // crosshair line style
    let poly_param_2 = ddgame.gfx_color_table[6]; // crosshair line color
    let verts: [[i32; 3]; 2] = [[start_x, start_y, 0], [endpoint_x, endpoint_y, 0]];
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
// RenderQueue installation
// ---------------------------------------------------------------------------

fn install_render_queue() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "DrawPixel",
            va::RQ_DRAW_PIXEL,
            trampoline_draw_pixel as *const (),
        )?;

        let _ = hook::install(
            "DrawLineStrip",
            va::RQ_DRAW_LINE_STRIP,
            trampoline_draw_line_strip as *const (),
        )?;

        let _ = hook::install(
            "DrawPolygon",
            va::RQ_DRAW_POLYGON,
            trampoline_draw_polygon as *const (),
        )?;

        let _ = hook::install(
            "DrawCrosshair",
            va::RQ_DRAW_CROSSHAIR,
            trampoline_draw_crosshair as *const (),
        )?;

        let _ = hook::install(
            "DrawRect",
            va::RQ_DRAW_RECT,
            trampoline_draw_rect as *const (),
        )?;

        let _ = hook::install(
            "DrawSpriteGlobal",
            va::RQ_DRAW_SPRITE_GLOBAL,
            trampoline_draw_sprite_global as *const (),
        )?;

        let _ = hook::install(
            "DrawSpriteLocal",
            va::RQ_DRAW_SPRITE_LOCAL,
            trampoline_draw_sprite_local as *const (),
        )?;

        let _ = hook::install(
            "DrawSpriteOffset",
            va::RQ_DRAW_SPRITE_OFFSET,
            trampoline_draw_sprite_offset as *const (),
        )?;

        let _ = hook::install(
            "DrawBitmapGlobal",
            va::RQ_DRAW_BITMAP_GLOBAL,
            trampoline_draw_bitmap_global as *const (),
        )?;

        let _ = hook::install(
            "DrawTextboxLocal",
            va::RQ_DRAW_TEXTBOX_LOCAL,
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

// ==========================================================================
// DisplayGfx vtable hooks
// ==========================================================================
//
// Patches DisplayBase vtables in WA.exe's .rdata:
// - Primary vtable (0x6645F8): replaces _purecall slots with safe no-op stubs
// - Headless vtable (0x66A0F8): replaces destructor with Rust version that
//   correctly frees our Rust-allocated sprite cache sub-objects

use openwa_core::bitgrid::DisplayBitGrid;
use openwa_core::fixed::Fixed;
use openwa_core::render::display::vtable::{self as display_vtable, DisplayVtable};
use openwa_core::render::display::DisplayBase;
use openwa_core::render::display::DisplayGfx;
use openwa_core::vtable::patch_vtable;
use openwa_core::vtable_replace;
use openwa_core::wa_alloc::wa_free;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D_4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

/// Rust destructor for headless DisplayBase. Frees the sprite cache chain
/// (wrapper -> buffer_ctrl -> buffer) that was allocated by new_headless().
unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
    let sprite_cache = (*this).sprite_cache;
    if !sprite_cache.is_null() {
        let ctrl = (*sprite_cache).buffer_ctrl;
        if !ctrl.is_null() {
            let buf = (*ctrl).buffer;
            if !buf.is_null() {
                wa_free(buf);
            }
            wa_free(ctrl);
        }
        wa_free(sprite_cache);
    }
    if flags & 1 != 0 {
        wa_free(this);
    }
    this
}

// No saved originals needed — all paths are fully ported or use direct bridges.

/// Rust port of DisplayGfx::BlitSprite (slot 19, 0x56B080).
///
/// Standard thiscall: ECX=this, stack params: x, y, sprite_flags, palette (RET 0x10).
///
/// sprite_flags layout:
///   low 16 bits  = sprite ID (0 = no sprite)
///   high 16 bits = orientation/flags:
///     bit 16 (0x10000): tiled mode
///     bit 17: additional orientation
///     bit 18 (0x40000): extra mirror X
///     bit 19 (0x80000): extra mirror Y
///     bit 20 (0x100000): stippled palette adjust
///     bit 21 (0x200000): additive blend
///     bit 22 (0x400000): shadow clear
///     bit 23 (0x800000): invert palette
///     bit 24 (0x1000000): palette x4 adjust
///     bit 25 (0x2000000): palette transform
///     bit 26 (0x4000000): color blend
///     bit 27 (0x8000000): stippled mode 0
///     bit 28 (0x10000000): stippled mode 1
unsafe extern "thiscall" fn blit_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite_flags: u32,
    palette: u32,
) {
    use openwa_core::bitgrid::DisplayBitGrid;
    use openwa_core::render::display::gfx::DisplayGfx;
    use openwa_core::render::display::vtable as display_vtable;

    let gfx = this as *mut DisplayGfx;
    let base = this as *const u8;

    // ---------------------------------------------------------------
    // Extract sprite ID and high flags
    // ---------------------------------------------------------------
    let high_flags = sprite_flags & 0xFFFF_0000;
    let sprite_id = sprite_flags & 0xFFFF;

    if sprite_id == 0 {
        return;
    }

    // ---------------------------------------------------------------
    // Palette manipulation
    // ---------------------------------------------------------------
    let mut pal: u32 = palette;
    if (high_flags & 0x0080_0000) != 0 {
        // Bit 23: invert palette
        pal = 0x10000u32.wrapping_sub(palette);
        if sprite_id.wrapping_sub(0x1D5) < 3 {
            // Special sprite IDs: scale by 8/18
            pal = (0x10000u32.wrapping_sub(palette).wrapping_mul(8)) / 0x12;
        }
    }
    if (high_flags & 0x0200_0000) != 0 {
        // Bit 25: palette transform (modular arithmetic for color cycling)
        let tmp = ((pal.wrapping_mul(0x1F) as i32)
            .wrapping_add(((pal.wrapping_mul(0x1F) as i32) >> 31) & 0x1F)
            >> 5) as u32;
        let tmp = tmp.wrapping_add(0x400) & 0xFFFF;
        pal = (tmp.wrapping_rem(0xF800)) / 2;
        if (pal & 0x400) != 0 {
            pal = (pal & !0x400) | 0x8000;
        }
    }

    // ---------------------------------------------------------------
    // Check sprite arrays — bitmap path if not in primary arrays
    // ---------------------------------------------------------------
    let arr1 = *(base.add(sprite_id as usize * 4 + 0x1008) as *const u32);
    let arr2 = *(base.add(sprite_id as usize * 4 + 0x2008) as *const u32);

    if arr1 == 0 && arr2 == 0 {
        // Bitmap sprite path — sprite is in the bitmap table at 0x3DD4.
        let bitmap_obj = (*gfx).sprite_table[sprite_id as usize];
        if bitmap_obj == 0 {
            return;
        }

        // Get frame data and dimensions from bitmap sprite object
        let mut sprite_w: i32 = 0;
        let mut sprite_h: i32 = 0;
        let mut rect_left: i32 = 0;
        let mut rect_top: i32 = 0;
        let mut rect_right: i32 = 0;
        let mut rect_bottom: i32 = 0;
        let frame_data = wa_get_bitmap_sprite_info(
            bitmap_obj as *mut u8,
            pal,
            &mut sprite_w,
            &mut sprite_h,
            &mut rect_left,
            &mut rect_top,
            &mut rect_right,
            &mut rect_bottom,
            rb(openwa_core::address::va::DISPLAY_GFX_GET_BITMAP_SPRITE_INFO),
        );
        if frame_data.is_null() {
            return;
        }

        let camera_x = (*gfx).camera_x;
        let camera_y = (*gfx).camera_y;
        let half_w = sprite_w / 2;
        let half_h = sprite_h / 2;
        let blit_h = rect_bottom - rect_top;

        let dst_y = (y.0 >> 16) + (camera_y - half_h) + rect_top;

        if (high_flags & 0x0001_0000) == 0 {
            // Non-tiled: BlitBitmapClipped
            let dst_x = (x.0 >> 16) + (camera_x - half_w) + rect_left;
            wa_blit_bitmap_clipped(
                this as *mut u8,
                sprite_w as u32,
                dst_x,
                dst_y,
                blit_h,
                frame_data,
                2,
                rb(openwa_core::address::va::DISPLAY_GFX_BLIT_BITMAP_CLIPPED),
            );
        } else {
            // Tiled: BlitBitmapTiled
            let dst_x = (x.0 >> 16) + (camera_x - half_w) + rect_left;
            wa_blit_bitmap_tiled(
                dst_x,
                sprite_w,
                this as *mut u8,
                dst_y,
                blit_h,
                frame_data,
                rb(openwa_core::address::va::DISPLAY_GFX_BLIT_BITMAP_TILED),
            );
        }
        return;
    }

    // ---------------------------------------------------------------
    // Bit 24: palette x4 adjust with orientation-dependent high bits
    // ---------------------------------------------------------------
    // The original ASM at 0x56B145 does a complex palette*4 + orientation mapping
    // that writes extra orientation bits into the local orient variable.
    // For now, handle the simple case:
    let mut orient_local: u32 = 0x0000_0001; // blend=1 (ColorTable/transparency), orientation=0 (Normal)
    if (high_flags & 0x0100_0000) != 0 {
        // The ASM computes: pal = pal * 4 + 0x8000, then maps (pal >> 16) & 3
        // to set specific orient values (0x80001, 0xC0001, 0x40001)
        let scaled = pal.wrapping_mul(4).wrapping_add(0x8000);
        pal = scaled & 0xFFFF;
        let quad = ((scaled as i32) >> 16) & 3;
        orient_local = match quad {
            0 => 0x0008_0001,
            1 => 0x000C_0001,
            2 => 0x0004_0001,
            _ => 0x0000_0001, // shouldn't happen, keep default blend=1
        };
    }

    // ---------------------------------------------------------------
    // Sprite data lookup via vtable[33]
    // ---------------------------------------------------------------
    let vtable_ptr = *(this as *const *const u32);
    let slot33_addr = *vtable_ptr.add(33);

    // vtable[33] is thiscall with 9 stack params (RET 0x24).
    // Output semantics (traced from ASM ESP offsets through LEA/PUSH sequence):
    //   param 3 -> sprite full width (for centering)
    //   param 4 -> sprite full height (for centering)
    //   param 5 -> render rect LEFT
    //   param 6 -> render rect TOP (overwrites palette on original stack!)
    //   param 7 -> render rect RIGHT
    //   param 8 -> render rect BOTTOM
    //   param 9 -> unknown (unused)
    let mut out_sprite_w: i32 = 0;
    let mut out_sprite_h: i32 = 0;
    let mut out_rect_left: i32 = 0;
    let mut out_rect_top: i32 = 0;
    let mut out_rect_right: i32 = 0;
    let mut out_rect_bottom: i32 = 0;
    let mut out_unknown: u32 = 0;

    let fn33: unsafe extern "thiscall" fn(
        *mut DisplayGfx,
        u32,
        u32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut i32,
        *mut u32,
    ) -> *mut DisplayBitGrid = core::mem::transmute(slot33_addr as usize);

    let mut sprite_surface = fn33(
        this,
        sprite_id,
        pal,
        &mut out_sprite_w,
        &mut out_sprite_h,
        &mut out_rect_left,
        &mut out_rect_top,
        &mut out_rect_right,
        &mut out_rect_bottom,
        &mut out_unknown,
    );

    if sprite_surface.is_null() {
        return;
    }

    let sprite_w = out_sprite_w;
    let sprite_h = out_sprite_h;
    let rect_left = out_rect_left;
    let rect_top = out_rect_top;
    let rect_right = out_rect_right;
    let rect_bottom = out_rect_bottom;

    // Size checks
    if rect_left >= rect_right || rect_top >= rect_bottom {
        return;
    }

    let mut blit_w = rect_right - rect_left;
    let mut blit_h = rect_bottom - rect_top;

    // ---------------------------------------------------------------
    // Shadow clear (high_flags bit 22)
    // ---------------------------------------------------------------
    if (high_flags & 0x0040_0000) != 0 {
        // Blit sprite to layer_2 as shadow base
        let layer2 = (*gfx).layer_2;
        super::bitgrid::blit_impl(
            layer2,
            0,
            0,
            blit_w,
            blit_h,
            sprite_surface,
            0,
            0,
            core::ptr::null(),
            0, // mode 0 = copy
        );
        // Manipulate color_add_table entry for shadow
        let color_idx = ((*gfx)._unknown_356c as usize) * 0x100;
        let table_byte = &mut (*gfx).color_add_table[color_idx];
        let saved = *table_byte;
        *table_byte = 0;

        // Call BitGrid__ClearColumn_Maybe (0x4F6590) — clears shadow channel
        let clear_fn: unsafe extern "cdecl" fn(*mut u8) =
            core::mem::transmute(rb(0x004F6590) as usize);
        clear_fn(table_byte as *mut u8);

        *table_byte = saved;

        // Replace sprite surface with layer_2 (shadow-processed)
        sprite_surface = layer2;
    }

    // ---------------------------------------------------------------
    // Extra orientation flags from high_flags
    // ---------------------------------------------------------------
    if (high_flags & 0x0004_0000) != 0 {
        orient_local |= 0x0001_0000;
    }
    if (high_flags & 0x0008_0000) != 0 {
        orient_local |= 0x0002_0000;
    }

    // ---------------------------------------------------------------
    // 16-case orientation switch for camera coordinate mapping
    // ---------------------------------------------------------------
    let camera_x = (*gfx).camera_x;
    let camera_y = (*gfx).camera_y;

    // Signed divide toward zero (matches MSVC CDQ+SUB+SAR pattern)
    let half_w = if sprite_w < 0 {
        (sprite_w + 1) / 2
    } else {
        sprite_w / 2
    };
    let half_h = if sprite_h < 0 {
        (sprite_h + 1) / 2
    } else {
        sprite_h / 2
    };

    let x_px = x.0 >> 16;
    let y_px = y.0 >> 16;

    let (dst_x, dst_y);
    let orientation_key = (orient_local >> 16) as i32;

    match orientation_key {
        1 | 10 => {
            // MirrorX
            dst_x = camera_x + half_w + x_px - rect_right;
            dst_y = camera_y - half_h + rect_top + y_px;
        }
        2 | 9 => {
            // MirrorY — X same as Normal, Y mirrored
            dst_x = camera_x - half_w + rect_left + x_px;
            dst_y = camera_y + half_h + y_px - rect_bottom;
        }
        3 | 8 => {
            // MirrorXY
            dst_x = camera_x + half_w + x_px - rect_right;
            dst_y = camera_y + half_h + y_px - rect_bottom;
        }
        4 | 15 => {
            // Rotate90 — swap axes
            dst_x = camera_x - half_h + rect_top + x_px;
            dst_y = camera_y + half_w + y_px - rect_right;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        5 | 14 => {
            // Rotate90MirrorX
            dst_x = camera_x + half_h + x_px - rect_bottom;
            dst_y = camera_y + half_w + y_px - rect_right;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        6 | 13 => {
            // Rotate90MirrorY
            dst_x = camera_x - half_h + rect_top + x_px;
            dst_y = camera_y - half_w + rect_left + y_px;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        7 | 12 => {
            // Rotate90MirrorXY
            dst_x = camera_x + half_h + x_px - rect_bottom;
            dst_y = camera_y - half_w + rect_left + y_px;
            blit_w = rect_bottom - rect_top;
            blit_h = rect_right - rect_left;
        }
        _ => {
            // Normal (0, 11, and any other value)
            dst_x = camera_x - half_w + rect_left + x_px;
            dst_y = camera_y - half_h + rect_top + y_px;
        }
    }

    // ---------------------------------------------------------------
    // Blit dispatch based on high_flags
    // ---------------------------------------------------------------

    if blit_w <= 0 || blit_h <= 0 {
        return;
    }

    // Stippled mode (checkerboard per-pixel blit)
    if (high_flags & 0x0800_0000) != 0 || (high_flags & 0x1000_0000) != 0 {
        let stipple_mode: u32 = if (high_flags & 0x1000_0000) != 0 {
            1
        } else {
            0
        };
        let parity = *(rb(openwa_core::address::va::G_STIPPLE_PARITY) as *const u32);

        display_vtable::acquire_render_lock(gfx);

        super::bitgrid::blit_stippled_raw(
            (*gfx).layer_0,
            sprite_surface,
            dst_x,
            dst_y,
            blit_w,
            blit_h,
            0,
            0,
            stipple_mode,
            parity,
        );
        return;
    }

    // Tiled mode (horizontal sprite tiling)
    if (high_flags & 0x0001_0000) != 0 {
        display_vtable::acquire_render_lock(gfx);

        super::bitgrid::blit_tiled_raw(
            (*gfx).layer_0,
            sprite_surface,
            dst_x,
            dst_y,
            blit_w,
            blit_h,
            (*gfx).base.clip_x1,
            (*gfx).base.clip_x2,
            orient_local,
        );
        return;
    }

    // Determine color table pointer
    let color_table: *const u8 = if (high_flags & 0x0020_0000) != 0 {
        (*gfx).color_add_table.as_ptr()
    } else if (high_flags & 0x0400_0000) != 0 {
        (*gfx).color_blend_table.as_ptr()
    } else {
        core::ptr::null()
    };

    display_vtable::acquire_render_lock(gfx);

    // src_x=0, src_y=0 always — vtable[33] already set up the sprite surface
    super::bitgrid::blit_impl(
        (*gfx).layer_0,
        dst_x,
        dst_y,
        blit_w,
        blit_h,
        sprite_surface,
        0,
        0,
        color_table,
        orient_local,
    );
}

// =========================================================================
// Bitmap sprite bridges (naked asm for usercall conventions)
// =========================================================================

/// Call DisplayGfx__GetBitmapSpriteInfo (0x573C50).
/// Usercall: EAX=bitmap_obj, EDX=palette, 6 stack params (output ptrs), RET 0x18.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_get_bitmap_sprite_info(
    _bitmap_obj: *mut u8,
    _palette: u32,
    _out_w: *mut i32,
    _out_h: *mut i32,
    _out_left: *mut i32,
    _out_top: *mut i32,
    _out_right: *mut i32,
    _out_bottom: *mut i32,
    _target: u32,
) -> *const u8 {
    core::arch::naked_asm!(
        "mov eax, [esp + 4]",        // bitmap_obj
        "mov edx, [esp + 8]",        // palette
        "mov ecx, [esp + 36]",       // target
        "push dword ptr [esp + 32]", // out_bottom
        "push dword ptr [esp + 32]", // out_right
        "push dword ptr [esp + 32]", // out_top
        "push dword ptr [esp + 32]", // out_left
        "push dword ptr [esp + 32]", // out_h
        "push dword ptr [esp + 32]", // out_w
        "call ecx",                  // RET 0x18 cleans 6 params
        "ret",
    );
}

/// Call DisplayGfx__BlitBitmapClipped (0x56A700).
/// Usercall: EAX=this, EDX=width, 5 stack params (dst_x, dst_y, height, frame_data, flags), RET 0x14.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_blit_bitmap_clipped(
    _this: *mut u8,
    _width: u32,
    _dst_x: i32,
    _dst_y: i32,
    _height: i32,
    _frame_data: *const u8,
    _flags: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        "mov eax, [esp + 4]",        // this
        "mov edx, [esp + 8]",        // width
        "mov ecx, [esp + 32]",       // target
        "push dword ptr [esp + 28]", // flags
        "push dword ptr [esp + 28]", // frame_data
        "push dword ptr [esp + 28]", // height
        "push dword ptr [esp + 28]", // dst_y
        "push dword ptr [esp + 28]", // dst_x
        "call ecx",                  // RET 0x14 cleans 5 params
        "ret",
    );
}

/// Call DisplayGfx__BlitBitmapTiled (0x56A7D0).
/// Usercall: EAX=initial_x, EDI=tile_width, 4 stack params (this, dst_y, height, frame_data), RET 0x10.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_blit_bitmap_tiled(
    _initial_x: i32,
    _tile_width: i32,
    _this: *mut u8,
    _dst_y: i32,
    _height: i32,
    _frame_data: *const u8,
    _target: u32,
) {
    core::arch::naked_asm!(
        "push edi",
        "mov eax, [esp + 8]",        // initial_x
        "mov edi, [esp + 12]",       // tile_width
        "mov ecx, [esp + 32]",       // target (offset +4 from push edi)
        "push dword ptr [esp + 28]", // frame_data
        "push dword ptr [esp + 28]", // height
        "push dword ptr [esp + 28]", // dst_y
        "push dword ptr [esp + 28]", // this
        "call ecx",                  // RET 0x10 cleans 4 params
        "pop edi",
        "ret",
    );
}

/// Thiscall wrapper for DisplayGfx::DrawScaledSprite (slot 20).
///
/// Computes coordinates in core, then dispatches the blit via blit_impl.
unsafe extern "thiscall" fn draw_scaled_sprite(
    this: *mut DisplayGfx,
    x: Fixed,
    y: Fixed,
    sprite: *mut DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    flags: u32,
) {
    use openwa_core::render::display::vtable::{self as display_vtable, DrawScaledSpriteResult};

    match display_vtable::draw_scaled_sprite(this, x, y, sprite, src_x, src_y, src_w, src_h, flags)
    {
        DrawScaledSpriteResult::Blit {
            layer,
            dst_x,
            dst_y,
            width,
            height,
            sprite,
            src_x,
            src_y,
            color_table,
            blit_flags,
        } => {
            super::bitgrid::blit_impl(
                layer,
                dst_x,
                dst_y,
                width,
                height,
                sprite,
                src_x,
                src_y,
                color_table,
                blit_flags,
            );
        }
        DrawScaledSpriteResult::Stippled {
            layer,
            dst_x,
            dst_y,
            width,
            height,
            sprite,
            src_x,
            src_y,
            stipple_mode,
        } => {
            let parity = *(rb(openwa_core::address::va::G_STIPPLE_PARITY) as *const u32);
            super::bitgrid::blit_stippled_raw(
                layer,
                sprite,
                dst_x,
                dst_y,
                width,
                height,
                src_x,
                src_y,
                stipple_mode,
                parity,
            );
        }
        DrawScaledSpriteResult::Handled => {}
    }
}

// =========================================================================
// Font vtable method bridges and implementations
// =========================================================================
//
// Font methods are thin wrappers: validate font_id (1..31), check font slot
// is populated, then delegate to an internal usercall function on the font
// object. The font object pointer is stored at DisplayBase+0x309C[font_id].

/// Port of DisplayGfx::SetFontPalette (vtable slot 36, 0x523690).
///
/// Loads font object from font_table[font_count], then calls
/// Font__SetPalette (0x4f9f20): usercall(ESI=font_obj) + stack(palette_value), RET 0x4.
///
/// The original has NO bounds check on font_count — it trusts the caller.
unsafe extern "thiscall" fn set_font_palette(
    this: *mut DisplayGfx,
    font_count: u32,
    palette_value: u32,
) {
    let font_obj = (*this).base.font_table[font_count as usize];
    wa_font_set_palette(font_obj, palette_value, rb(va::FONT_OBJ_SET_PALETTE));
}

/// Bridge to Font__SetPalette (0x4f9f20).
/// Usercall: ESI=font_obj, stack(palette_value), RET 0x4.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_font_set_palette(_font_obj: u32, _palette_value: u32, _target: u32) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, [esp+8]",        // font_obj
        "mov ecx, [esp+16]",       // target
        "push dword ptr [esp+12]", // palette_value
        "call ecx",                // RET 0x4 cleans 1 param
        "pop esi",
        "ret",
    );
}

/// Port of DisplayGfx::SetFontParam (vtable slot 10, 0x523710).
///
/// Validates font_id, then calls Font__SetParam (0x4fa720):
/// usercall(ECX=p4, EDX=font_obj) + stack(p3, p5), RET 0x8.
unsafe extern "thiscall" fn set_font_param(
    this: *mut DisplayGfx,
    font_id: i32,
    p3: u32,
    p4: u32,
    p5: u32,
) -> u32 {
    if !(1..=31).contains(&font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize];
    if font_obj == 0 {
        return 0;
    }
    wa_font_set_param(font_obj, p3, p4, p5, rb(va::FONT_OBJ_SET_PARAM));
    1
}

/// Bridge to Font__SetParam (0x4fa720).
/// Usercall: ECX=p4, EDX=font_obj, stack(p3, p5), RET 0x8.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_font_set_param(
    _font_obj: u32,
    _p3: u32,
    _p4: u32,
    _p5: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        "mov edx, [esp+4]",        // font_obj → EDX
        "mov ecx, [esp+12]",       // p4 → ECX
        "mov eax, [esp+20]",       // target
        "push dword ptr [esp+16]", // p5
        "push dword ptr [esp+12]", // p3 (shifted +4 by push)
        "call eax",                // RET 0x8 cleans 2 params
        "ret",
    );
}

/// Port of DisplayGfx::GetFontInfo (vtable slot 8, 0x523790).
///
/// Validates font_id, then calls Font__GetInfo (0x4fa7d0):
/// usercall(EAX=font_obj, EDX=out_1, EDI=out_2), plain RET.
unsafe extern "thiscall" fn get_font_info(
    this: *mut DisplayGfx,
    font_id: i32,
    out_1: *mut u32,
    out_2: *mut u32,
) -> u32 {
    if !(1..=31).contains(&font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize];
    if font_obj == 0 {
        return 0;
    }
    wa_font_get_info(font_obj, out_1, out_2, rb(va::FONT_OBJ_GET_INFO))
}

/// Bridge to Font__GetInfo (0x4fa7d0).
/// Usercall: EAX=font_obj, EDX=out_2, EDI=out_1, no stack params, plain RET.
///
/// Register mapping verified from caller at 0x523790:
///   EDX ← [ESP+0xC] = out_2 (3rd vtable param)
///   EDI ← [ESP+0x8] after PUSH EDI = out_1 (2nd vtable param)
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_font_get_info(
    _font_obj: u32,
    _out_1: *mut u32,
    _out_2: *mut u32,
    _target: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push edi",
        "mov eax, [esp+8]",  // font_obj → EAX
        "mov edx, [esp+16]", // out_2 → EDX
        "mov edi, [esp+12]", // out_1 → EDI
        "mov ecx, [esp+20]", // target
        "call ecx",          // plain RET
        "pop edi",
        "ret",
    );
}

/// Port of DisplayGfx::GetFontMetric (vtable slot 9, 0x523750).
///
/// Validates font_id, then calls Font__GetMetric (0x4fa780):
/// usercall(AL=char_code, EDX=out_1, EDI=out_2) + stack(font_obj), RET 0x4.
unsafe extern "thiscall" fn get_font_metric(
    this: *mut DisplayGfx,
    font_id: i32,
    char_code: u32,
    out_1: *mut u32,
    out_2: *mut u32,
) -> u32 {
    if !(1..=31).contains(&font_id) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id as usize];
    if font_obj == 0 {
        return 0;
    }
    wa_font_get_metric(
        font_obj,
        char_code,
        out_1,
        out_2,
        rb(va::FONT_OBJ_GET_METRIC),
    )
}

/// Bridge to Font__GetMetric (0x4fa780).
/// Usercall: AL=char_code, EDX=out_1, EDI=out_2, stack(font_obj), RET 0x4.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_font_get_metric(
    _font_obj: u32,
    _char_code: u32,
    _out_1: *mut u32,
    _out_2: *mut u32,
    _target: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push edi",
        "mov al, [esp+12]",       // char_code (byte) → AL
        "mov edx, [esp+16]",      // out_1 → EDX
        "mov edi, [esp+20]",      // out_2 → EDI
        "mov ecx, [esp+24]",      // target
        "push dword ptr [esp+8]", // font_obj → stack (shifted +4 by push edi)
        "call ecx",               // RET 0x4 cleans 1 param
        "pop edi",
        "ret",
    );
}

/// Port of DisplayGfx::DrawTextOnBitmap (vtable slot 7, 0x5236B0).
///
/// Extracts font_id from low 16 bits, extra flags from high 16 bits.
/// Validates font_id, then calls Font__DrawText (0x4fa4e0):
/// usercall(EAX=bitmap, EDX=a8, ESI=font_obj) + stack(h_align, v_align, msg, a7, high_bits), RET 0x14.
///
/// Calling convention verified by tracing the wrapper at 0x5236B0:
///   EAX receives bitmap (loaded last via [ESP+0x20] after 5 pushes)
///   EDX receives a8 (loaded via [ESP+0x30] after 4 pushes)
///   Stack params pushed: h_align, v_align, msg, a7, high_bits
unsafe extern "thiscall" fn draw_text_on_bitmap(
    this: *mut DisplayGfx,
    font_id: i32,
    bitmap: i32,
    h_align: i32,
    v_align: i32,
    msg: *const core::ffi::c_char,
    a7: i32,
    a8: i32,
) -> i32 {
    let font_id_low = (font_id as u32) & 0xFFFF;
    if !(1..=31).contains(&font_id_low) {
        return 0;
    }
    let font_obj = (*this).base.font_table[font_id_low as usize];
    if font_obj == 0 {
        return 0;
    }
    let font_id_high = ((font_id as u32) >> 16) as i32;
    wa_font_draw_text(
        font_obj,
        bitmap,
        a8,
        h_align,
        v_align,
        msg,
        a7,
        font_id_high,
        rb(va::FONT_OBJ_DRAW_TEXT),
    )
}

/// Bridge to Font__DrawText (0x4fa4e0).
/// Usercall: EAX=bitmap, EDX=a8, ESI=font_obj, stack(h_align, v_align, msg, a7, high_bits), RET 0x14.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_font_draw_text(
    _font_obj: u32,
    _bitmap: i32,
    _a8: i32,
    _h_align: i32,
    _v_align: i32,
    _msg: *const core::ffi::c_char,
    _a7: i32,
    _high_bits: i32,
    _target: u32,
) -> i32 {
    core::arch::naked_asm!(
        "push esi",
        // After push esi: +8=font_obj, +12=bitmap, +16=a8, +20=h_align,
        //   +24=v_align, +28=msg, +32=a7, +36=high_bits, +40=target
        "mov esi, [esp+8]",  // font_obj → ESI
        "mov eax, [esp+12]", // bitmap → EAX
        "mov edx, [esp+16]", // a8 → EDX
        "mov ecx, [esp+40]", // target
        // Push 5 stack params in reverse order. Each push shifts ESP by -4,
        // and the next param is 4 bytes lower, so the offset stays constant.
        "push dword ptr [esp+36]", // high_bits
        "push dword ptr [esp+36]", // a7
        "push dword ptr [esp+36]", // msg
        "push dword ptr [esp+36]", // v_align
        "push dword ptr [esp+36]", // h_align
        "call ecx",                // RET 0x14 cleans 5 params
        "pop esi",
        "ret",
    );
}

// =========================================================================
// Sprite loading vtable method wrappers
// =========================================================================

/// Thiscall wrapper for DisplayGfx::LoadSprite (vtable slot 31, 0x523400).
unsafe extern "thiscall" fn load_sprite(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    flag: u32,
    gfx: *mut u8,
    name: *const core::ffi::c_char,
) -> i32 {
    display_vtable::load_sprite(this, layer, id, flag, gfx, name, wa_load_sprite_from_vfs)
}

/// Bridge to LoadSpriteFromVfs (0x4FAAF0).
/// Usercall: ECX=gfx, EAX=name, stack(sprite, layer_ctx), RET 0x8.
///
/// Verified from caller at 0x523489:
///   ECX ← layer_contexts[layer] (gfx/VFS context)... wait, re-checked:
///   ECX ← gfx param, EAX ← name param.
///   Stack: sprite (EDI from ConstructSprite), layer_ctx.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_load_sprite_from_vfs(
    _sprite: *mut openwa_core::render::sprite::Sprite,
    _gfx: *mut u8,
    _name: *const core::ffi::c_char,
    _layer_ctx: u32,
) -> i32 {
    core::arch::naked_asm!(
        // cdecl: +4=sprite, +8=gfx, +12=name, +16=layer_ctx
        "mov ecx, [esp+8]",         // gfx → ECX
        "mov eax, [esp+12]",        // name → EAX
        "push dword ptr [esp+16]",  // layer_ctx
        "push dword ptr [esp+8]",   // sprite (shifted +4 by push)
        "call [{ADDR}]",            // RET 0x8 cleans 2 stack params
        "ret",
        ADDR = sym LOAD_SPRITE_FROM_VFS_ADDR,
    );
}

static mut LOAD_SPRITE_FROM_VFS_ADDR: u32 = 0;

// LoadSpriteComplex (slot 33) is NOT ported — its internal functions
// (0x4FAD30, 0x4F9710) use ESI for sprite/bank pointers in complex
// usercall conventions that are impractical to bridge.

unsafe extern "thiscall" fn load_sprite_by_layer(
    this: *mut DisplayGfx,
    layer: u32,
    id: u32,
    gfx: *mut u8,
    name: *const core::ffi::c_char,
) -> i32 {
    display_vtable::load_sprite_by_layer(this, layer, id, gfx, name)
}

unsafe extern "thiscall" fn load_font(
    this: *mut DisplayGfx,
    mode: i32,
    font_id: i32,
    gfx: *mut u8,
    filename: *const core::ffi::c_char,
) -> u32 {
    display_vtable::load_font(this, mode, font_id, gfx, filename)
}

unsafe extern "thiscall" fn load_font_extension(
    this: *mut DisplayGfx,
    font_id: i32,
    path: *const core::ffi::c_char,
    char_map: *const u8,
    palette_value: u32,
    flag: i32,
) -> u32 {
    display_vtable::load_font_extension(this, font_id, path, char_map, palette_value, flag)
}

// ---------------------------------------------------------------------------
// Display installation
// ---------------------------------------------------------------------------

fn install_display() -> Result<(), String> {
    let _ = log_line("[Display] Patching DisplayBase vtables");

    unsafe {
        let purecall_addr = rb(PURECALL);
        let noop_addr = noop_thiscall as *const () as u32;

        // Patch primary vtable (0x6645F8): replace _purecall with no-ops.
        let primary = rb(va::DISPLAY_BASE_VTABLE) as *mut u32;
        patch_vtable(primary, VTABLE_SLOTS, |vt| {
            let mut patched = 0u32;
            for i in 0..VTABLE_SLOTS {
                let slot = vt.add(i);
                if *slot == purecall_addr {
                    *slot = noop_addr;
                    patched += 1;
                }
            }
            let _ = log_line(&format!(
                "[Display]   Primary: patched {patched}/{VTABLE_SLOTS} _purecall -> no-op"
            ));
        })?;

        // Patch headless vtable (0x66A0F8): replace destructor (slot 0)
        // with our Rust version that frees the Rust-allocated sprite cache.
        let headless = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *mut u32;
        patch_vtable(headless, VTABLE_SLOTS, |vt| {
            *vt = headless_destructor as *const () as u32;
            let _ = log_line("[Display]   Headless: patched slot 0 (destructor) -> Rust");
        })?;

        // Initialize bridge address statics for sprite loading
        LOAD_SPRITE_FROM_VFS_ADDR = rb(va::LOAD_SPRITE_FROM_VFS);

        // Patch DisplayGfx vtable (0x66A218): replace ported methods with Rust.
        vtable_replace!(DisplayVtable, va::DISPLAY_GFX_VTABLE, {
            get_dimensions      => display_vtable::get_dimensions,
            set_layer_color     => display_vtable::set_layer_color,
            set_active_layer    => display_vtable::set_active_layer,
            get_sprite_info     => display_vtable::get_sprite_info,
            draw_text_on_bitmap => draw_text_on_bitmap,
            get_font_info       => get_font_info,
            get_font_metric     => get_font_metric,
            set_font_param      => set_font_param,
            draw_polyline       => display_vtable::draw_polyline,
            draw_line           => display_vtable::draw_line,
            draw_line_clipped   => display_vtable::draw_line_clipped,
            draw_pixel_strip    => display_vtable::draw_pixel_strip,
            draw_crosshair      => display_vtable::draw_crosshair,
            draw_outlined_pixel => display_vtable::draw_outlined_pixel,
            fill_rect           => display_vtable::fill_rect,
            draw_via_callback   => display_vtable::draw_via_callback,
            draw_tiled_terrain  => display_vtable::draw_tiled_terrain,
            flush_render        => display_vtable::flush_render,
            set_camera_offset   => display_vtable::set_camera_offset,
            set_clip_rect       => display_vtable::set_clip_rect,
            is_sprite_loaded    => display_vtable::is_sprite_loaded,
            load_sprite          => load_sprite,
            draw_scaled_sprite  => draw_scaled_sprite,
            set_layer_visibility => display_vtable::set_layer_visibility,
            update_palette      => display_vtable::update_palette,
            set_font_palette    => set_font_palette,
            slot 19 => blit_sprite,
            load_sprite_by_layer => load_sprite_by_layer,
            load_font            => load_font,
            load_font_extension  => load_font_extension,
        })?;
        let _ = log_line("[Display]   DisplayGfx: patched 31 methods -> Rust");
    }

    Ok(())
}

// ==========================================================================
// Combined installation
// ==========================================================================

pub fn install() -> Result<(), String> {
    install_render_queue()?;
    install_display()?;
    Ok(())
}
