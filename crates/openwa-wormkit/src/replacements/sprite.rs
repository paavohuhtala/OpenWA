//! RenderQueue enqueue hooks — full Rust replacements.
//!
//! All functions enqueue commands to the RenderQueue's downward-growing buffer.
//! Calling conventions are __usercall variants with register + stack params.

use openwa_types::address::va;
use openwa_types::render::*;

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
// Installation
// ---------------------------------------------------------------------------

pub unsafe fn install() -> Result<(), String> {
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

    Ok(())
}
