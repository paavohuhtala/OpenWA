//! Keyboard vtable hooks.
//!
//! Thin shim — game logic lives in `openwa_game::input::keyboard`.

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::input::keyboard;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(keyboard::KeyboardVtable, va::KEYBOARD_VTABLE, {
        destructor             => keyboard::keyboard_destructor,
        read_input_ring_buffer => keyboard::keyboard_read_input_ring_buffer,
    })?;

    let _ = log_line("[Keyboard] Destructor + ReadInputRingBuffer hooked");
    Ok(())
}
