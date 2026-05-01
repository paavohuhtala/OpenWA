//! Hook installation for `GameSession::Run` and friends.
//!
//! `GameRuntime__Constructor` (0x0056DEF0) and `GameEngine::InitHardware`
//! (0x0056D350) are fully replaced in Rust (`openwa_game::engine::hardware_init`).
//! WA-side callers are trapped; Rust callers go through the Rust impls
//! directly.

use crate::hook::{self, usercall_trampoline};
use openwa_game::address::va;
use openwa_game::engine::GameInfo;
use openwa_game::engine::game_session_run::{on_headless_pre_loop, run_game_session};
use openwa_game::engine::init_constructor_addrs;
use openwa_game::engine::pump_messages::pump_messages;
use openwa_game::engine::window_proc::{engine_wnd_proc, init_window_proc_addrs};

// ─── GameSession::Run hook ──────────────────────────────────────────────────
//
// __usercall(ESI=GameInfo, stack: arg1..arg4), RET 0x10. Returns 0/1 in EAX.
usercall_trampoline!(fn trampoline_game_session_run;
    impl_fn = run_game_session_impl;
    reg = esi; stack_params = 4; ret_bytes = "0x10");

unsafe extern "cdecl" fn run_game_session_impl(
    game_info: *mut GameInfo,
    arg1_module_state: u32,
    state_buf: *mut u8,
    display_p3: u32,
    display_p4: u32,
) -> u32 {
    unsafe {
        run_game_session(
            game_info,
            arg1_module_state,
            state_buf,
            display_p3,
            display_p4,
        )
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        init_constructor_addrs();
        hook::install_trap!("GameRuntime__Constructor", va::CONSTRUCT_GAME_RUNTIME);
        hook::install_trap!("GameWorld__InitGameState", va::GAME_WORLD_INIT_GAME_STATE);
        // GameSession::Constructor — only WA-side caller is GameSession::Run
        // (fully replaced); inlined as `construct_session` in Rust.
        hook::install_trap!("GameSession::Constructor", va::GAME_SESSION_CONSTRUCTOR);
        hook::install(
            "GameSession::Run",
            va::GAME_SESSION_RUN,
            trampoline_game_session_run as *const (),
        )?;
        // GameSession::OnHeadlessPreLoop_Maybe — full replacement; two
        // remaining WA-side callers in the SYSCOMMAND minimize path still
        // dispatch through this address.
        hook::install(
            "GameSession::OnHeadlessPreLoop_Maybe",
            va::GAME_SESSION_ON_HEADLESS_PRE_LOOP,
            on_headless_pre_loop as *const (),
        )?;
        // GameSession::PumpMessages — full replacement; second WA-side
        // caller is `GameRuntime::LoadingProgressTick`.
        hook::install(
            "GameSession::PumpMessages",
            va::GAME_SESSION_PUMP_MESSAGES,
            pump_messages as *const (),
        )?;
        // GameSession::WindowProc — full replacement of the engine-mode
        // WNDPROC. Windows dispatches it via the WNDPROC slot installed
        // by `FUN_004ECD40` (which still runs in WA); MinHook on the
        // entry redirects the dispatch into the Rust impl.
        init_window_proc_addrs();
        hook::install(
            "GameSession::WindowProc",
            va::GAME_SESSION_WINDOW_PROC,
            engine_wnd_proc as *const (),
        )?;
    }
    Ok(())
}
