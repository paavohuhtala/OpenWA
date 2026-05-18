//! RenderQueue enqueue hooks and the per-frame dispatcher bridge.
//!
//! Trampolines + install helpers come from `crate::generated::hooks` via
//! `crates/openwa-dll/hooks/render.toml`. The cdecl impls below are the
//! WA→Rust forwarding shims.

use openwa_core::fixed::Fixed;
use openwa_game::bitgrid::DisplayBitGrid;
use openwa_game::entity::{WeaponAimEntity, WormEntity};
use openwa_game::render::SpriteOp;
use openwa_game::render::display::DisplayGfx;
use openwa_game::render::display::vtable::TiledBitmapSource;
use openwa_game::render::message::RenderMessage;
use openwa_game::render::queue::RenderQueue;
use openwa_game::render::queue_dispatch::{ClipContext, render_drawing_queue};

// EnqueueTiledBitmap (0x541D60)

pub(crate) unsafe extern "cdecl" fn enqueue_tiled_bitmap_impl(
    queue: *mut RenderQueue,
    y: Fixed,
    source: *const TiledBitmapSource,
    flags: u32,
) {
    unsafe {
        let _ = (*queue).push_typed(
            0x1B_0000,
            RenderMessage::TiledBitmap {
                x: Fixed(0xFF000000u32 as i32),
                y,
                source,
                flags: flags as u8,
            },
        );
    }
}

// EnqueueTiledTerrain (0x5422A0)
// __usercall(ECX = queue, EAX = y, [stack0] = x, [stack1] = count), RET 0x8.
// WA hardcodes layer = 0x180000, mode = 0, ref_z = 0, flags = 1.

pub(crate) unsafe extern "cdecl" fn enqueue_tiled_terrain_impl(
    y: Fixed,
    queue: *mut RenderQueue,
    x: Fixed,
    count: i32,
) {
    unsafe {
        let _ = (*queue).push_typed(
            0x18_0000,
            RenderMessage::TiledTerrain {
                x: x.floor(),
                y: y.floor(),
                count,
            },
        );
    }
}

// DrawLineStrip (0x541DD0) — variable-size: vertex data via alloc_aux

pub(crate) unsafe extern "cdecl" fn draw_line_strip_impl(
    queue: *mut RenderQueue,
    count: u32,
    vertices: *mut i32,
    color: u32,
) {
    unsafe {
        let rq = &mut *queue;
        let byte_len = count as usize * core::mem::size_of::<[i32; 3]>();
        if let Some(vert_ptr) = rq.alloc_aux(byte_len) {
            core::ptr::copy_nonoverlapping(vertices as *const u8, vert_ptr, byte_len);
            let _ = rq.push_typed(
                0xE_0000,
                RenderMessage::LineStrip {
                    count,
                    color,
                    vertices: vert_ptr as *const [i32; 3],
                },
            );
        }
    }
}

// DrawPolygon (0x541E50) — variable-size: vertex data via alloc_aux

pub(crate) unsafe extern "cdecl" fn draw_polygon_impl(
    queue: *mut RenderQueue,
    count: u32,
    vertices: *mut i32,
    color1: u32,
    color2: u32,
) {
    unsafe {
        let rq = &mut *queue;
        let byte_len = count as usize * core::mem::size_of::<[i32; 3]>();
        if let Some(vert_ptr) = rq.alloc_aux(byte_len) {
            core::ptr::copy_nonoverlapping(vertices as *const u8, vert_ptr, byte_len);
            let _ = rq.push_typed(
                0xE_0000,
                RenderMessage::Polygon {
                    count,
                    color1,
                    color2,
                    vertices: vert_ptr as *const [i32; 3],
                },
            );
        }
    }
}

// DrawCrosshair (0x541ED0)

pub(crate) unsafe extern "cdecl" fn draw_crosshair_impl(
    queue: *mut RenderQueue,
    layer: u32,
    x: Fixed,
    y: Fixed,
    color_fg: u32,
    color_bg: u32,
) {
    unsafe {
        let _ = (*queue).push_typed(
            layer,
            RenderMessage::Crosshair {
                color_fg,
                color_bg,
                x,
                y,
            },
        );
    }
}

// DrawRect (0x541F40)

pub(crate) unsafe extern "cdecl" fn draw_rect_impl(
    queue: *mut RenderQueue,
    y_clip: Fixed,
    layer: u32,
    x1: Fixed,
    y1: Fixed,
    x2: Fixed,
    y2: Fixed,
    color: u32,
) {
    unsafe {
        let _ = (*queue).push_typed(
            layer,
            RenderMessage::FillRect {
                color,
                x1: x1.floor(),
                y1: y1.floor(),
                x2: x2.floor(),
                y2: y2.floor(),
                ref_z: y_clip.floor().0,
            },
        );
    }
}

// DrawSpriteGlobal (0x541FE0)

pub(crate) unsafe extern "cdecl" fn draw_sprite_global_impl(
    y_pos: Fixed,
    queue: *mut RenderQueue,
    layer: u32,
    x_pos: Fixed,
    sprite: SpriteOp,
    anim_value: Fixed,
) {
    unsafe {
        let _ = (*queue).push_typed(
            layer,
            RenderMessage::Sprite {
                local: false,
                x: x_pos.floor(),
                y: y_pos.floor(),
                sprite,
                anim_value,
            },
        );
    }
}

// DrawSpriteLocal (0x542060)

pub(crate) unsafe extern "cdecl" fn draw_sprite_local_impl(
    y_pos: Fixed,
    queue: *mut RenderQueue,
    layer: u32,
    x_pos: Fixed,
    sprite: SpriteOp,
    anim_value: Fixed,
) {
    unsafe {
        let _ = (*queue).push_typed(
            layer,
            RenderMessage::Sprite {
                local: true,
                x: x_pos.floor(),
                y: y_pos.floor(),
                sprite,
                anim_value,
            },
        );
    }
}

// DrawSpriteOffset (0x5420E0)

pub(crate) unsafe extern "cdecl" fn draw_sprite_offset_impl(
    queue: *mut RenderQueue,
    y_clip: Fixed,
    layer: u32,
    x_pos: Fixed,
    y_pos: Fixed,
    flags: u32,
    sprite: SpriteOp,
    anim_value: Fixed,
) {
    unsafe {
        let _ = (*queue).push_typed(
            layer,
            RenderMessage::SpriteOffset {
                flags,
                x: x_pos.floor(),
                y: y_pos.floor(),
                ref_z_2: y_clip.floor().0,
                sprite,
                anim_value,
            },
        );
    }
}

// DrawBitmapGlobal (0x542170)

pub(crate) unsafe extern "cdecl" fn draw_bitmap_global_impl(
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
    unsafe {
        let _ = (*queue).push_typed(
            layer,
            RenderMessage::BitmapGlobal {
                x: x_pos.floor(),
                y: y_pos.floor(),
                bitmap,
                src_y,
                src_w,
                src_h,
                flags,
            },
        );
    }
}

// DrawTextboxLocal (0x542200)

pub(crate) unsafe extern "cdecl" fn draw_textbox_local_impl(
    q: *mut RenderQueue,
    y_pos: Fixed,
    layer: u32,
    x_pos: Fixed,
    bitmap: *mut DisplayBitGrid,
    src_w: i32,
    src_h: i32,
    flags: u32,
) {
    unsafe {
        let _ = (*q).push_typed(
            layer,
            RenderMessage::TextboxLocal {
                x: x_pos.floor(),
                y: y_pos.floor(),
                bitmap,
                src_w,
                src_h,
                flags,
            },
        );
    }
}

// WormEntity::DrawAttachedRope (0x00500720)

pub(crate) unsafe extern "stdcall" fn draw_attached_rope_impl(
    this: *const WormEntity,
    style: u32,
    fill: u32,
) {
    unsafe {
        openwa_game::render::worm::draw_attached_rope(this, style, fill);
    }
}

// DrawCrosshairLine (0x5197D0)

pub(crate) unsafe extern "cdecl" fn draw_crosshair_line_impl(entity: *const WeaponAimEntity) {
    unsafe {
        openwa_game::render::crosshair_line::draw_crosshair_line(entity);
    }
}

// RenderDrawingQueue (0x542350)

pub(crate) unsafe extern "cdecl" fn render_drawing_queue_impl(
    rq: *mut RenderQueue,
    display: *mut DisplayGfx,
    clip: *mut ClipContext,
) {
    unsafe {
        render_drawing_queue(rq, display, clip);
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        crate::generated::hooks::install_RQ_EnqueueTiledBitmap()?;
        crate::generated::hooks::install_RenderQueue__PushDrawTiledTerrain()?;
        crate::generated::hooks::install_RQ_DrawLineStrip()?;
        crate::generated::hooks::install_RQ_DrawPolygon()?;
        crate::generated::hooks::install_RQ_DrawCrosshair()?;
        crate::generated::hooks::install_RQ_DrawRect()?;
        crate::generated::hooks::install_RQ_DrawSpriteGlobal()?;
        crate::generated::hooks::install_RQ_DrawSpriteLocal()?;
        crate::generated::hooks::install_RQ_DrawSpriteOffset()?;
        crate::generated::hooks::install_RQ_DrawBitmapGlobal()?;
        crate::generated::hooks::install_RQ_DrawTextboxLocal()?;
        crate::generated::hooks::install_WormEntity__DrawAttachedRope()?;
        crate::generated::hooks::install_DrawCrosshairLine()?;
        crate::generated::hooks::install_RenderDrawingQueue()?;
    }
    Ok(())
}
