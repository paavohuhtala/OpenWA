//! Keyboard hooks (vtable replacements + direct-call full replacement).
//!
//! Thin shim — game logic lives in `openwa_game::input::keyboard`.

use crate::hook;
use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::input::keyboard;

pub fn install() -> Result<(), String> {
    use openwa_game::vtable_replace;

    vtable_replace!(keyboard::KeyboardVtable, va::KEYBOARD_VTABLE, {
        destructor             => keyboard::keyboard_destructor,
        is_action_pressed      => keyboard::keyboard_is_action_pressed,
        is_action_active       => keyboard::keyboard_is_action_active,
        is_action_active2      => keyboard::keyboard_is_action_active2,
        is_action_held         => keyboard::keyboard_is_action_held,
        read_input_ring_buffer => keyboard::keyboard_read_input_ring_buffer,
        alert_user             => keyboard::keyboard_alert_user,
        vfunc7                 => keyboard::keyboard_vfunc7,
    })?;

    // PollState is non-virtual — called directly by GameEngine::InitHardware,
    // GameSession::ProcessFrame, AcquireInput, OnSYSCOMMAND, etc.
    unsafe {
        hook::install(
            "Keyboard__PollState",
            va::KEYBOARD_POLL_STATE,
            keyboard::keyboard_poll_state as *const (),
        )?;

        // AcquireInput is non-virtual + usercall (ESI=flag); the naked
        // trampoline `keyboard_acquire_input` captures ESI before chaining
        // to the cdecl impl.
        hook::install(
            "Keyboard__AcquireInput",
            va::KEYBOARD_ACQUIRE_INPUT,
            keyboard::keyboard_acquire_input as *const (),
        )?;

        // CheckAction is reached only via the IsAction* vtable wrappers, all
        // four of which are now Rust. Trap to catch any unexpected callers.
        hook::install_trap!("Keyboard__CheckAction", va::KEYBOARD_CHECK_ACTION);
    }

    let _ = log_line(
        "[Keyboard] Destructor + IsAction* + ReadInputRingBuffer + AlertUser + VFunc7 + PollState + AcquireInput hooked; CheckAction trapped",
    );
    Ok(())
}
