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

unsafe extern "cdecl" fn render_esc_menu_overlay_impl(runtime: *mut GameRuntime) {
    unsafe { openwa_game::engine::main_loop::esc_menu::render_overlay(runtime) }
}

// `GameRuntime::RenderEscMenuOverlay` (0x00535000) — usercall(EAX = this),
// plain RET. Called from `GameRender_Maybe` (0x00533DC0) once per frame as
// a tail render func. The Rust impl calls back into WA's still-bridged
// `MenuPanel::Render` (0x00540B00) via `bridge_menu_panel_render`.
hook::usercall_trampoline!(fn trampoline_render_esc_menu_overlay;
    impl_fn = render_esc_menu_overlay_impl; reg = eax);

unsafe extern "cdecl" fn game_render_impl(runtime: *mut GameRuntime) {
    unsafe { openwa_game::engine::main_loop::render_frame::game_render(runtime) }
}

// `GameRender_Maybe` (0x00533DC0) — usercall(ECX = this), plain RET.
// Top-level per-frame render dispatcher. Called from `RenderFrame_Maybe`
// (0x0056E040, still bridged). Rust impl in
// `engine::main_loop::render_frame::game_render`.
hook::usercall_trampoline!(fn trampoline_game_render;
    impl_fn = game_render_impl; reg = ecx);

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
        hook::install(
            "GameRuntime__RenderEscMenuOverlay",
            va::GAME_RUNTIME_RENDER_ESC_MENU_OVERLAY,
            trampoline_render_esc_menu_overlay as *const (),
        )?;
        hook::install(
            "GameRender_Maybe",
            va::GAME_RENDER_MAYBE,
            trampoline_game_render as *const (),
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
