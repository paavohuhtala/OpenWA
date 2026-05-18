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

pub(crate) unsafe extern "cdecl" fn is_hud_active_impl(runtime: *mut GameRuntime) -> u32 {
    unsafe { openwa_game::engine::main_loop::esc_menu::is_hud_active(runtime) as u32 }
}

pub(crate) unsafe extern "cdecl" fn render_esc_menu_overlay_impl(runtime: *mut GameRuntime) {
    unsafe { openwa_game::engine::main_loop::esc_menu::render_overlay(runtime) }
}

pub fn install() -> Result<(), String> {
    unsafe {
        openwa_game::engine::main_loop::dispatch_frame::init_dispatch_addrs();
        hook::install(
            "GameSession__ProcessFrame",
            va::GAME_SESSION_PROCESS_FRAME,
            hook_process_frame as *const (),
        )?;
        crate::generated::hooks::install_GameRuntime__IsHudActive()?;
        crate::generated::hooks::install_MenuPanel__AppendItem()?;
        crate::generated::hooks::install_GameRuntime__RenderEscMenuOverlay()?;
        // `GameRender` (0x00533DC0) — Rust port at
        // `engine::main_loop::render_frame::game_render`. Called directly
        // from the Rust `render_frame`; trap as a safety net.
        hook::install_trap!("GameRender", va::GAME_RENDER);
        // `GameRuntime::RenderFrame` (0x0056E040, vtable slot 7) — Rust
        // port at `engine::main_loop::render_frame::render_frame`. The
        // only WA-side caller was `GameSession::ProcessFrame` (also
        // Rust now); trap as a safety net.
        hook::install_trap!("GameRuntime__RenderFrame", va::RENDER_FRAME_MAYBE);
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
        // `GameRuntime::OpenEscMenu` (0x00535200) — fully ported in Rust
        // (`engine::main_loop::esc_menu::open_esc_menu`). The only WA-side
        // caller was `EscMenu_TickClosed`, also Rust now. Trap as a safety
        // net.
        hook::install_trap!("GameRuntime__OpenEscMenu", va::GAME_RUNTIME_OPEN_ESC_MENU);
        // `GameRuntime::EscMenu_TickState1` (0x00535B10) — Rust port at
        // `esc_menu::tick_open`. Dispatched directly from Rust
        // `setup_frame_params`; trap to catch any regression that lands a
        // WA-side caller back here.
        hook::install_trap!(
            "GameRuntime__EscMenu_TickState1",
            va::GAME_RUNTIME_ESC_MENU_STATE_1_TICK
        );
        // `GameRuntime::EscMenu_TickState2` (0x00535FC0) — Rust port at
        // `esc_menu::tick_confirm`. Same dispatch path as state 1.
        hook::install_trap!(
            "GameRuntime__EscMenu_TickState2",
            va::GAME_RUNTIME_ESC_MENU_STATE_2_TICK
        );
        // `GameRuntime::OpenEscMenuConfirmDialog` (0x00535CF0) — Rust
        // port at `esc_menu::open_confirm_dialog`. The only WA-side
        // caller was `EscMenu_TickState1` (now also Rust). Trap as a
        // safety net.
        hook::install_trap!(
            "GameRuntime__OpenEscMenuConfirmDialog",
            va::GAME_RUNTIME_OPEN_ESC_MENU_CONFIRM_DIALOG
        );
        // `MenuPanel::CenterCursorOnFirstKindZero` (0x00540780) — Rust
        // port at `engine::menu_panel::center_cursor_on_first_kind_zero`.
        // Only WA-side caller was `OpenEscMenuConfirmDialog`.
        hook::install_trap!(
            "MenuPanel__CenterCursorOnFirstKindZero",
            va::MENU_PANEL_CENTER_CURSOR_ON_KIND_ZERO
        );
    }
    Ok(())
}
