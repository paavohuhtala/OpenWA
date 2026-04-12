//! RenderQueue enqueue hooks — full Rust replacements for command enqueueing,
//! plus the per-frame RenderDrawingQueue dispatcher bridge.

use openwa_core::address::va;
use openwa_core::fixed::Fixed;
use openwa_core::render::display::DisplayGfx;
use openwa_core::render::queue::*;
use openwa_core::render::queue_dispatch::{render_drawing_queue, ClipContext};
use openwa_core::render::SpriteOp;

use crate::hook::{self, usercall_trampoline};

// ---------------------------------------------------------------------------
// EnqueueTiledBitmap (0x541D60) — type 0xD, EAX=this, 3 stack, RET 0xC
//
// Mis-labelled `RQ_DrawPixel` in earlier reverse-engineering passes. It does
// NOT enqueue a single-pixel draw — it enqueues a tile-cached bitmap draw
// dispatched by `RenderDrawingQueue` case 0xD into vtable slot 11.
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_enqueue_tiled_bitmap; impl_fn = enqueue_tiled_bitmap_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn enqueue_tiled_bitmap_impl(
    this: u32,
    y_fixed16: u32,
    source_descriptor: u32,
    flags: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc::<DrawTiledBitmapCmd>() {
        *entry = DrawTiledBitmapCmd {
            command_type: command_type::DRAW_TILED_BITMAP,
            layer: 0x1B_0000,
            x: Fixed(0xFF00_0000u32 as i32),
            y: Fixed(y_fixed16 as i32),
            source: source_descriptor as *const _,
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
            color: param_1,
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
            color1: param_1,
            color2: param_2,
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
            x: Fixed(x_pos as i32),
            y: Fixed(y_pos as i32),
            ref_z: 0,
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
            x1: Fixed(x1 as i32).floor(),
            y1: Fixed(y1 as i32).floor(),
            x2: Fixed(x2 as i32).floor(),
            y2: Fixed(y2 as i32).floor(),
            ref_z: Fixed(y_clip as i32).floor().0,
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
            x: Fixed(x_pos as i32).floor(),
            y: Fixed(y_pos as i32).floor(),
            sprite: SpriteOp(sprite_id),
            palette: frame,
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
            x: Fixed(x_pos as i32).floor(),
            y: Fixed(y_pos as i32).floor(),
            sprite: SpriteOp(sprite_id),
            palette: frame,
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
            flags: sprite_id,
            x: Fixed(x_pos as i32).floor(),
            y: Fixed(y_pos as i32).floor(),
            ref_z: 0,
            ref_z_2: Fixed(y_clip as i32).floor().0,
            sprite: SpriteOp(param_7),
            palette: param_8,
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
            x: Fixed(x_pos as i32).floor(),
            y: Fixed(y_pos as i32).floor(),
            bitmap: bitmap_ptr as *mut _,
            src_x: 0,
            src_y: param_6 as i32,
            src_w: param_7 as i32,
            src_h: param_8 as i32,
            flags: param_9,
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
            mode: 0,
            x: Fixed(x_pos as i32).floor(),
            y: Fixed(y_pos as i32).floor(),
            ref_z: 0,
            ref_z_2: 0,
            bitmap: text_ptr as *mut _,
            src_x: 0,
            src_y: 0,
            src_w: param_6 as i32,
            src_h: param_7 as i32,
            flags: param_8,
        };
    }
}

// ---------------------------------------------------------------------------
// DrawBungeeTrail (0x500720) — stdcall(task, style, fill), RET 0xC
//
// Draws bungee drop trajectory path. Logic lives in openwa_core::render::bungee_trail.
// ---------------------------------------------------------------------------

unsafe extern "stdcall" fn draw_bungee_trail_impl(task_ptr: u32, style: u32, fill: u32) {
    openwa_core::render::bungee_trail::draw_bungee_trail(task_ptr, style, fill);
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
    openwa_core::render::crosshair_line::draw_crosshair_line(task_ptr);
}

// ---------------------------------------------------------------------------
// RenderDrawingQueue (0x542350) — usercall(EAX=RenderQueue*) + 2 stack
// (DisplayGfx*, ClipContext*), RET 0x8.
//
// The per-frame render-queue dispatcher. Pure Rust port lives in
// `openwa_core::render::queue_dispatch::render_drawing_queue`. The
// trampoline captures EAX and forwards the two stack args.
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_render_drawing_queue;
    impl_fn = render_drawing_queue_impl;
    reg = eax; stack_params = 2; ret_bytes = "0x8");

unsafe extern "cdecl" fn render_drawing_queue_impl(
    rq: *mut RenderQueue,
    display: *mut DisplayGfx,
    clip: *mut ClipContext,
) {
    render_drawing_queue(rq, display, clip);
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "EnqueueTiledBitmap",
            va::RQ_ENQUEUE_TILED_BITMAP,
            trampoline_enqueue_tiled_bitmap as *const (),
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

        let _ = hook::install(
            "RenderDrawingQueue",
            va::RQ_RENDER_DRAWING_QUEUE,
            trampoline_render_drawing_queue as *const (),
        )?;
    }
    Ok(())
}
