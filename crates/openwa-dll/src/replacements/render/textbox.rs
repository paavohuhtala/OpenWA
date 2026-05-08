//! Hook for `SetTextboxText` (0x004FB070) — full replacement.
//!
//! WA's stdcall RET 0x20; thin wrapper around the
//! [`openwa_game::render::textbox::set_text`] Rust port.

use core::ffi::c_char;

use openwa_core::fixed::Fixed;
use openwa_game::address::va;
use openwa_game::bitgrid::DisplayBitGrid;
use openwa_game::render::textbox::{Textbox, set_text};

use crate::hook;

unsafe extern "stdcall" fn hook_set_textbox_text(
    this: *mut Textbox,
    text: *const c_char,
    font_index: i32,
    fill_color: u32,
    border_color: u32,
    out_w: *mut i32,
    out_h: *mut i32,
    scale: Fixed,
) -> *mut DisplayBitGrid {
    unsafe {
        set_text(
            this,
            text,
            font_index,
            fill_color,
            border_color,
            out_w,
            out_h,
            scale,
        )
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "SetTextboxText",
            va::SET_TEXTBOX_TEXT,
            hook_set_textbox_text as *const (),
        )?;
    }
    Ok(())
}
