//! RenderQueue enqueue hooks and the per-frame dispatcher bridge.

use openwa_core::address::va;
use openwa_core::bitgrid::DisplayBitGrid;
use openwa_core::fixed::Fixed;
use openwa_core::render::display::vtable::TiledBitmapSource;
use openwa_core::render::display::DisplayGfx;
use openwa_core::render::queue::*;
use openwa_core::render::queue_dispatch::{render_drawing_queue, ClipContext};
use openwa_core::render::SpriteOp;
use openwa_core::task::{BungeeTrailTask, WeaponAimTask};

use crate::hook::{self, usercall_trampoline};

// EnqueueTiledBitmap (0x541D60)

usercall_trampoline!(fn trampoline_enqueue_tiled_bitmap; impl_fn = enqueue_tiled_bitmap_impl;
    reg = eax; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn enqueue_tiled_bitmap_impl(
    queue: *mut RenderQueue,
    y: Fixed,
    source: *const TiledBitmapSource,
    flags: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawTiledBitmapCmd>() {
        *entry = DrawTiledBitmapCmd {
            command_type: command_type::DRAW_TILED_BITMAP,
            layer: 0x1B_0000,
            x: Fixed(0xFF00_0000u32 as i32),
            y,
            source,
            flags: flags as u8,
            _pad: [0; 3],
        };
    }
}

// DrawLineStrip (0x541DD0) — variable-size allocation: count * 0xC + 0x1C

usercall_trampoline!(fn trampoline_draw_line_strip; impl_fn = draw_line_strip_impl;
    regs = [eax, edi]; stack_params = 2; ret_bytes = "0x8");

unsafe extern "cdecl" fn draw_line_strip_impl(
    queue: *mut RenderQueue,
    count: u32,
    vertices: *const u8,
    color: u32,
) {
    let total_size = count as usize * 0xC + 0x1C;

    if let Some(ptr) = (*queue).alloc_raw(total_size) {
        let header = &mut *(ptr as *mut DrawLineStripHeader);
        *header = DrawLineStripHeader {
            command_type: command_type::DRAW_LINE_STRIP,
            layer: 0xE_0000,
            count,
            color,
        };
        core::ptr::copy_nonoverlapping(
            vertices,
            ptr.add(core::mem::size_of::<DrawLineStripHeader>()),
            count as usize * 0xC,
        );
    }
}

// DrawPolygon (0x541E50) — variable-size allocation: count * 0xC + 0x20

usercall_trampoline!(fn trampoline_draw_polygon; impl_fn = draw_polygon_impl;
    regs = [ecx, esi]; stack_params = 3; ret_bytes = "0xC");

unsafe extern "cdecl" fn draw_polygon_impl(
    queue: *mut RenderQueue,
    count: u32,
    vertices: *const u8,
    color1: u32,
    color2: u32,
) {
    let total_size = count as usize * 0xC + 0x20;

    if let Some(ptr) = (*queue).alloc_raw(total_size) {
        let header = &mut *(ptr as *mut DrawPolygonHeader);
        *header = DrawPolygonHeader {
            command_type: command_type::DRAW_POLYGON,
            layer: 0xE_0000,
            count,
            color1,
            color2,
        };
        core::ptr::copy_nonoverlapping(
            vertices,
            ptr.add(core::mem::size_of::<DrawPolygonHeader>()),
            count as usize * 0xC,
        );
    }
}

// DrawCrosshair (0x541ED0)

usercall_trampoline!(fn trampoline_draw_crosshair; impl_fn = draw_crosshair_impl;
    reg = ecx; stack_params = 5; ret_bytes = "0x14");

unsafe extern "cdecl" fn draw_crosshair_impl(
    queue: *mut RenderQueue,
    layer: u32,
    x: Fixed,
    y: Fixed,
    color_fg: u32,
    color_bg: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawCrosshairCmd>() {
        *entry = DrawCrosshairCmd {
            command_type: command_type::DRAW_CROSSHAIR,
            layer,
            color_fg,
            color_bg,
            x,
            y,
            ref_z: 0,
        };
    }
}

// DrawRect (0x541F40)

usercall_trampoline!(fn trampoline_draw_rect; impl_fn = draw_rect_impl;
    regs = [ecx, edx]; stack_params = 6; ret_bytes = "0x18");

unsafe extern "cdecl" fn draw_rect_impl(
    queue: *mut RenderQueue,
    y_clip: Fixed,
    layer: u32,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawRectCmd>() {
        *entry = DrawRectCmd {
            command_type: command_type::DRAW_RECT,
            layer,
            color,
            x1: x1.floor(),
            y1: y1.floor(),
            x2: x2.floor(),
            y2: y2.floor(),
            ref_z: y_clip.floor().0,
        };
    }
}

// DrawSpriteGlobal (0x541FE0)

usercall_trampoline!(fn trampoline_draw_sprite_global; impl_fn = draw_sprite_global_impl;
    regs = [eax, ecx]; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn draw_sprite_global_impl(
    y_pos: Fixed,
    queue: *mut RenderQueue,
    layer: u32,
    x_pos: Fixed,
    sprite: SpriteOp,
    frame: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawSpriteCmd>() {
        *entry = DrawSpriteCmd {
            command_type: command_type::DRAW_SPRITE_GLOBAL,
            layer,
            x: x_pos.floor(),
            y: y_pos.floor(),
            sprite,
            palette: frame,
        };
    }
}

// DrawSpriteLocal (0x542060)

usercall_trampoline!(fn trampoline_draw_sprite_local; impl_fn = draw_sprite_local_impl;
    regs = [eax, ecx]; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn draw_sprite_local_impl(
    y_pos: Fixed,
    queue: *mut RenderQueue,
    layer: u32,
    x_pos: Fixed,
    sprite: SpriteOp,
    frame: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawSpriteCmd>() {
        *entry = DrawSpriteCmd {
            command_type: command_type::DRAW_SPRITE_LOCAL,
            layer,
            x: x_pos.floor(),
            y: y_pos.floor(),
            sprite,
            palette: frame,
        };
    }
}

// DrawSpriteOffset (0x5420E0)

usercall_trampoline!(fn trampoline_draw_sprite_offset; impl_fn = draw_sprite_offset_impl;
    regs = [ecx, edx]; stack_params = 6; ret_bytes = "0x18");

unsafe extern "cdecl" fn draw_sprite_offset_impl(
    queue: *mut RenderQueue,
    y_clip: Fixed,
    layer: u32,
    x_pos: Fixed,
    y_pos: Fixed,
    flags: u32,
    sprite: SpriteOp,
    palette: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawSpriteOffsetCmd>() {
        *entry = DrawSpriteOffsetCmd {
            command_type: command_type::DRAW_SPRITE_OFFSET,
            layer,
            flags,
            x: x_pos.floor(),
            y: y_pos.floor(),
            ref_z: 0,
            ref_z_2: y_clip.floor().0,
            sprite,
            palette,
        };
    }
}

// DrawBitmapGlobal (0x542170)

usercall_trampoline!(fn trampoline_draw_bitmap_global; impl_fn = draw_bitmap_global_impl;
    regs = [ecx, edx]; stack_params = 7; ret_bytes = "0x1C");

unsafe extern "cdecl" fn draw_bitmap_global_impl(
    queue: *mut RenderQueue,
    y_pos: Fixed,
    layer: u32,
    x_pos: Fixed,
    bitmap: *mut DisplayBitGrid,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    flags: u32,
) {
    if let Some(entry) = (*queue).alloc::<DrawBitmapGlobalCmd>() {
        *entry = DrawBitmapGlobalCmd {
            command_type: command_type::DRAW_BITMAP_GLOBAL,
            layer,
            x: x_pos.floor(),
            y: y_pos.floor(),
            bitmap,
            src_x: 0,
            src_y,
            src_w,
            src_h,
            flags,
        };
    }
}

// DrawTextboxLocal (0x542200)

usercall_trampoline!(fn trampoline_draw_textbox_local; impl_fn = draw_textbox_local_impl;
    regs = [ecx, edx]; stack_params = 6; ret_bytes = "0x18");

unsafe extern "cdecl" fn draw_textbox_local_impl(
    q: *mut RenderQueue,
    y_pos: Fixed,
    layer: u32,
    x_pos: Fixed,
    bitmap: *mut DisplayBitGrid,
    src_w: i32,
    src_h: i32,
    flags: u32,
) {
    if let Some(entry) = (*q).alloc::<DrawTextboxLocalCmd>() {
        *entry = DrawTextboxLocalCmd {
            command_type: command_type::DRAW_TEXTBOX_LOCAL,
            layer,
            mode: 0,
            x: x_pos.floor(),
            y: y_pos.floor(),
            ref_z: 0,
            ref_z_2: 0,
            bitmap,
            src_x: 0,
            src_y: 0,
            src_w,
            src_h,
            flags,
        };
    }
}

// DrawBungeeTrail (0x500720)

unsafe extern "stdcall" fn draw_bungee_trail_impl(
    task: *const BungeeTrailTask,
    style: u32,
    fill: u32,
) {
    openwa_core::render::bungee_trail::draw_bungee_trail(task, style, fill);
}

// DrawCrosshairLine (0x5197D0)

usercall_trampoline!(fn trampoline_draw_crosshair_line; impl_fn = draw_crosshair_line_impl;
    reg = edi);

unsafe extern "cdecl" fn draw_crosshair_line_impl(task: *const WeaponAimTask) {
    openwa_core::render::crosshair_line::draw_crosshair_line(task);
}

// RenderDrawingQueue (0x542350)

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
