//! Keyboard hooks (vtable replacements + direct-call full replacement).
//!
//! Thin shim — game logic lives in `openwa_game::input::keyboard`.

use crate::hook;
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
        crate::generated::hooks::install_Keyboard__PollState()?;

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

        // ClearKeyStates: one WA-side caller (GameSession::ProcessFrame at
        // 0x00572D81) plus two Rust call sites already use the impl directly.
        crate::generated::hooks::install_Keyboard__ClearKeyStates()?;

        // Mouse helpers — all `__cdecl void()`, full-replacement hooks.
        crate::generated::hooks::install_Mouse__PollAndAcquire()?;
        crate::generated::hooks::install_Mouse__ReleaseAndCenter()?;
        crate::generated::hooks::install_Cursor__ClipAndRecenter()?;
    }

    Ok(())
}
