//! In-game ESC menu state machine.
//!
//! The ESC menu is the in-round overlay shown by pressing Escape — a
//! scoreboard ("First Team to N Wins" header + per-team leaderboard) plus
//! action buttons (Minimize Game, Force Sudden Death, Draw This Round, Quit
//! The Game) and a volume slider. Lives at `runtime._field_30` (the
//! [`MenuPanel`-shaped] item list) with the canvas at `runtime._field_2c`.
//!
//! State at `runtime.esc_menu_state` (i32):
//!  - **0** — closed. [`tick_closed`] polls for Escape to open.
//!  - **1** — open / accepting nav input. Driven WA-side by
//!    `EscMenu_TickState1` (still bridged via [`bridge_state_1_tick`]).
//!  - **2** — confirm / network-end-of-game flow. Driven WA-side by
//!    `EscMenu_TickState2` (still bridged via [`bridge_state_2_tick`]).
//!
//! [`MenuPanel`-shaped]: a 16-item list with stride 0x38 starting at
//! `panel + 0x30`, count at `+0x3B0`, scroll-region rect at `+0x1C..+0x28`.

use openwa_core::fixed::Fixed;

use crate::address::va;
use crate::audio::known_sound_id::KnownSoundId;
use crate::audio::sound_ops::dispatch_global_sound;
use crate::engine::runtime::GameRuntime;
use crate::input::keyboard::KeyboardAction;
use crate::rebase::rb;

// ─── Bridged WA addresses ──────────────────────────────────────────────────

static mut OPEN_ESC_MENU_ADDR: u32 = 0;
static mut STATE_1_TICK_ADDR: u32 = 0;
static mut STATE_2_TICK_ADDR: u32 = 0;

/// Initialize the ESC-menu bridge addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        OPEN_ESC_MENU_ADDR = rb(va::GAME_RUNTIME_OPEN_ESC_MENU);
        STATE_1_TICK_ADDR = rb(va::GAME_RUNTIME_ESC_MENU_STATE_1_TICK);
        STATE_2_TICK_ADDR = rb(va::GAME_RUNTIME_ESC_MENU_STATE_2_TICK);
    }
}

// ─── Bridges ───────────────────────────────────────────────────────────────

/// Bridge for `GameRuntime__OpenEscMenu` (0x00535200) — builds the in-game
/// ESC menu (scoreboard header, leaderboard rows, action buttons + volume
/// slider) into `runtime._field_30`, then sets `esc_menu_state = 1`.
/// Plain `__stdcall(this)`, RET 0x4. 628 instructions, 30 calls — too big
/// for an incidental port.
pub unsafe fn bridge_open_esc_menu(runtime: *mut GameRuntime) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut GameRuntime) =
            core::mem::transmute(OPEN_ESC_MENU_ADDR as usize);
        func(runtime)
    }
}

/// Bridge for `GameRuntime__EscMenu_TickState1` (0x00535B10) — per-frame
/// tick while the menu is open (`esc_menu_state == 1`); handles arrow-key
/// nav + Enter to activate a menu item. Usercall EDI=this, plain RET.
/// ~159 instructions.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn bridge_state_1_tick(_this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, [esp+8]",
        "call [{addr}]",
        "pop edi",
        "ret 4",
        addr = sym STATE_1_TICK_ADDR,
    );
}

/// Bridge for `GameRuntime__EscMenu_TickState2` (0x00535FC0) — per-frame
/// tick while `esc_menu_state == 2` (confirm / network-end-of-game flow;
/// calls `BeginNetworkGameEnd`). Usercall EDI=this, plain RET. ~176
/// instructions.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn bridge_state_2_tick(_this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, [esp+8]",
        "call [{addr}]",
        "pop edi",
        "ret 4",
        addr = sym STATE_2_TICK_ADDR,
    );
}

// ─── Rust ports ────────────────────────────────────────────────────────────

/// Rust port of `GameRuntime::IsHudActive` (0x00534C30).
///
/// Predicate: "should the ESC menu be allowed to open / stay open?" Calls
/// `WorldRootEntity::hud_data_query` (vtable slot 3) with msg `0x7D3` to
/// fill a 916-byte (`0x394`) scratch buffer with the end-of-round HUD
/// snapshot, then inspects two early DWORDs of that buffer plus several
/// state flags on `runtime` and `world`.
///
/// Returns `true` only when the game is in pure-running mode:
/// - `game_end_phase == 0` (game-over animation not active)
/// - and either `replay_flag_a != 0` (replay short-circuits the buffer
///   and per-runtime flag checks — see WA's `JNZ 0x534C7D` after testing
///   `[ESI+0x490]`), or all of:
///   - `runtime._field_460 == 0`
///   - `world.fast_forward_request == 0`
///   - `buf[1] == 0` and `buf[2] == 0` (DWORDs at offsets +4/+8 of the
///     0x7D3 response — `buf[0]` is intentionally ignored by WA)
///
/// Called from [`super::dispatch_frame::setup_frame_params`] and from
/// [`tick_closed`]. Hooked at the WA address via
/// `usercall_trampoline!(reg = esi)` so the still-WA-side caller
/// `OpenEscMenu` routes through this Rust port.
pub unsafe fn is_hud_active(runtime: *mut GameRuntime) -> bool {
    unsafe {
        let mut buf: [u32; 0xE5] = [0; 0xE5];
        let task = (*runtime).world_root;
        ((*(*task).base.vtable).hud_data_query)(task, 0x7D3, 0x394, buf.as_mut_ptr() as *mut u8);

        if (*runtime).game_end_phase != 0 {
            return false;
        }
        if (*runtime).replay_flag_a != 0 {
            return true;
        }
        if (*runtime)._field_460 != 0 {
            return false;
        }
        if (*(*runtime).world).fast_forward_request != 0 {
            return false;
        }
        if buf[1] != 0 {
            return false;
        }
        if buf[2] != 0 {
            return false;
        }
        true
    }
}

/// Rust port of `GameRuntime::EscMenu_TickClosed` (0x005351B0).
///
/// Per-frame tick while the ESC menu is **closed**
/// (`runtime.esc_menu_state == 0`). Polls the keyboard for the
/// just-pressed edge of `KeyboardAction::Escape`:
///
/// - If Escape isn't pressed this frame → no-op.
/// - If Escape is pressed and [`is_hud_active`] returns `true` → call
///   [`bridge_open_esc_menu`], which builds the menu contents into
///   `runtime._field_30` and transitions `esc_menu_state` to `1`.
/// - If Escape is pressed but the HUD is *not* active (replay tail,
///   end-of-round, fast-forward, etc.) → reject with
///   [`KnownSoundId::WarningBeep`] at `runtime.ui_volume`.
pub unsafe fn tick_closed(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let keyboard = (*world).keyboard;
        if !KeyboardAction::Escape.is_active2(keyboard) {
            return;
        }
        if is_hud_active(runtime) {
            bridge_open_esc_menu(runtime);
        } else {
            dispatch_global_sound(
                runtime,
                KnownSoundId::WarningBeep.into(),
                8,
                Fixed::ONE,
                (*runtime).ui_volume,
            );
        }
    }
}
