//! Main loop hooks: `GameSession__ProcessFrame` replacement + traps on
//! `GameRuntime__DispatchFrame` and `GameRuntime__StepFrame`.
//!
//! ProcessFrame is fully replaced in Rust; its only downstream WA callees
//! (DispatchFrame, and StepFrame via DispatchFrame) are now unreachable.
//! The traps catch any regression that routes execution back into the
//! original WA implementations.

use crate::hook;
use openwa_game::address::va;
use openwa_game::engine::runtime::GameRuntime;

unsafe extern "C" fn hook_process_frame() {
    unsafe {
        openwa_game::engine::main_loop::process_frame::process_frame();
    }
}

unsafe extern "cdecl" fn is_hud_active_impl(runtime: *mut GameRuntime) -> u32 {
    unsafe { openwa_game::engine::main_loop::esc_menu::is_hud_active(runtime) as u32 }
}

hook::usercall_trampoline!(fn trampoline_is_hud_active;
    impl_fn = is_hud_active_impl; reg = esi);

// `MenuPanel::AppendItem` (0x005408F0) — usercall(EAX=x, ESI=panel) +
// 6 stack params + RET 0x18. Trampoline forwards to the cdecl impl with
// signature `(eax_x, esi_panel, kind, label, y, centered, slider_value_ptr,
// slider_aux) -> u32`.
hook::usercall_trampoline!(fn trampoline_menu_panel_append_item;
    impl_fn = openwa_game::engine::menu_panel::append_item_impl;
    regs = [eax, esi]; stack_params = 6; ret_bytes = "0x18");

pub fn install() -> Result<(), String> {
    unsafe {
        openwa_game::engine::main_loop::dispatch_frame::init_dispatch_addrs();
        hook::install(
            "GameSession__ProcessFrame",
            va::GAME_SESSION_PROCESS_FRAME,
            hook_process_frame as *const (),
        )?;
        hook::install(
            "GameRuntime__IsHudActive",
            va::GAME_RUNTIME_IS_HUD_ACTIVE,
            trampoline_is_hud_active as *const (),
        )?;
        hook::install(
            "MenuPanel__AppendItem",
            va::MENU_PANEL_APPEND_ITEM,
            trampoline_menu_panel_append_item as *const (),
        )?;
        hook::install_trap!(
            "GameRuntime__DispatchFrame",
            va::GAME_RUNTIME_DISPATCH_FRAME
        );
        hook::install_trap!("GameRuntime__StepFrame", va::GAME_RUNTIME_STEP_FRAME);
        hook::install_trap!(
            "GameRuntime__SetupFrameParams",
            va::GAME_RUNTIME_SETUP_FRAME_PARAMS
        );
        hook::install_trap!(
            "GameRuntime__EscMenu_TickClosed",
            va::GAME_RUNTIME_ESC_MENU_TICK_CLOSED
        );
    }
    Ok(())
}
