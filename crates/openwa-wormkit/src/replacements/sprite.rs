//! Sprite drawing hooks — full Rust replacements.
//!
//! - DrawSpriteGlobal (0x541FE0): enqueue world-space sprite (command type 4)
//! - DrawSpriteLocal  (0x542060): enqueue screen-space sprite (command type 5)
//!
//! Both functions are __thiscall + EAX=y_pos (usercall), 4 stack params, RET 0x10.

use openwa_types::address::va;
use openwa_types::render::{command_type, RenderCommand, RenderQueue};

use crate::hook::{self, usercall_trampoline};

// ---------------------------------------------------------------------------
// DrawSpriteGlobal — full Rust replacement
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_sprite_global; impl_fn = draw_sprite_global_impl;
    regs = [eax, ecx]; stack_params = 4; ret_bytes = "0x10");

/// Full Rust replacement for DrawSpriteGlobal (0x541FE0).
///
/// Enqueues a render command (type 4, world-space) to the drawing queue.
unsafe extern "cdecl" fn draw_sprite_global_impl(
    y_pos: u32,
    this: u32,
    layer: u32,
    x_pos: u32,
    sprite_id: u32,
    frame: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc_entry() {
        *entry = RenderCommand {
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
// DrawSpriteLocal — full Rust replacement
// ---------------------------------------------------------------------------

usercall_trampoline!(fn trampoline_draw_sprite_local; impl_fn = draw_sprite_local_impl;
    regs = [eax, ecx]; stack_params = 4; ret_bytes = "0x10");

/// Full Rust replacement for DrawSpriteLocal (0x542060).
///
/// Enqueues a render command (type 5, screen-space) to the drawing queue.
unsafe extern "cdecl" fn draw_sprite_local_impl(
    y_pos: u32,
    this: u32,
    layer: u32,
    x_pos: u32,
    sprite_id: u32,
    frame: u32,
) {
    let q = &mut *(this as *mut RenderQueue);

    if let Some(entry) = q.alloc_entry() {
        *entry = RenderCommand {
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
// Installation
// ---------------------------------------------------------------------------

pub unsafe fn install() -> Result<(), String> {
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

    Ok(())
}
