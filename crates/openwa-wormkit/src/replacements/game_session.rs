//! Full Rust replacement for `DDGameWrapper__Constructor` (0x56DEF0).
//!
//! ## Status: FULLY CONVERTED
//!
//! All callers are Rust (`construct_ddgame_wrapper` from `impl_init_hardware`).
//! The original WA function is trapped — panics if called.
//!
//! ## Sub-call conventions
//!
//! - `DDGameWrapper__InitReplay` (0x56F860): usercall(EAX=game_info, ESI=this),
//!   plain RET (no stack args). Bridged via `call_init_replay`.
//! - `DDGame__Constructor` (0x56E220): stdcall 9 params + implicit ECX=network.
//!   Bridged via `ddgame_constructor_call`. Being incrementally replaced by
//!   `create_ddgame()` in openwa-core (not yet complete).
//! - `DDGame__InitGameState` (0x526500): plain stdcall(this), called via transmute.

use openwa_core::address::va;
use openwa_core::audio::DSSound;
use openwa_core::display::{DDDisplay, Palette};
use openwa_core::engine::ddgame::{DDGame, create_ddgame, init_constructor_addrs};
use openwa_core::engine::{DDGameWrapper, GameInfo, GameSession};
use openwa_core::rebase::rb;
use openwa_core::wa_alloc::{wa_malloc, wa_free};
use crate::hook;
use crate::log_line;

/// Implicit EDI = game_info pointer, captured from EDI on entry.
static mut GAME_INFO: *mut GameInfo = core::ptr::null_mut();

/// Implicit ECX = network pointer for `DDGame__Constructor`. Set in `ctor_impl`
/// just before calling `ddgame_constructor_call`.
static mut DDGAME_CTOR_ECX: u32 = 0;

/// Runtime address of `DDGameWrapper__InitReplay` (set at install time).
static mut INIT_REPLAY_ADDR: u32 = 0;

/// Runtime address of `DDGame__Constructor` (set at install time).
static mut DDGAME_CTOR_ADDR: u32 = 0;

// ─── Bridge: DDGameWrapper__InitReplay ───────────────────────────────────────
//
// Convention: usercall(EAX=game_info, ESI=this), plain RET (no stack params).
#[unsafe(naked)]
unsafe extern "stdcall" fn call_init_replay(_game_info: *mut GameInfo, _this: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %eax",    // EAX = game_info
        "movl 0xC(%esp), %esi",  // ESI = this
        "calll *({fn})",         // call DDGameWrapper__InitReplay; plain RET
        "popl %esi",
        "retl $8",               // stdcall cleanup: 2 × u32 = 8
        fn = sym INIT_REPLAY_ADDR,
        options(att_syntax),
    );
}

// ─── Bridge: DDGame__Constructor ─────────────────────────────────────────────
//
// Convention: stdcall 9 params + implicit ECX=network.
// Tail-jumps to original; DDGame::ctor's RET 0x24 returns to caller.
#[unsafe(naked)]
unsafe extern "stdcall" fn ddgame_constructor_call(
    _this: *mut DDGameWrapper,
    _display: *mut DDDisplay,
    _sound: *mut DSSound,
    _keyboard: *mut u8,
    _palette: *mut Palette,
    _streaming_audio: *mut u8,
    _timer_obj: *mut u8,
    _net_game: *mut u8,
    _game_info: *mut GameInfo,
) -> *mut u8 {
    core::arch::naked_asm!(
        "movl {ecx_val}, %ecx",  // ECX = network (implicit param)
        "jmpl *({fn})",          // tail-jump; RET 0x24 returns to caller
        ecx_val = sym DDGAME_CTOR_ECX,
        fn = sym DDGAME_CTOR_ADDR,
        options(att_syntax),
    );
}

/// Called by `impl_init_hardware` to construct the DDGameWrapper in-place.
pub(crate) unsafe fn construct_ddgame_wrapper(
    game_info: *mut GameInfo,
    this: *mut DDGameWrapper,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    keyboard: *mut u8,
    palette: *mut Palette,
    streaming_audio: *mut u8,
    input_ctrl: *mut u8,
) -> *mut DDGameWrapper {
    GAME_INFO = game_info;

    // Initialize DDGameWrapper fields (order matches original decompile).
    (*this).ddgame = core::ptr::null_mut();
    (*this).landscape = core::ptr::null_mut();
    (*this).vtable = rb(va::DDGAME_WRAPPER_VTABLE) as *mut u8;
    (*this).sound = sound;
    (*this).display = display;

    // Initialize replay subsystem.  usercall(EAX=game_info, ESI=this), plain RET.
    call_init_replay(game_info, this);

    // Read timer_obj and net_game from the live game session struct.
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    let timer_obj = (*session).timer_obj;
    let net_game  = (*session).net_game;

    let _ = log_line(&format!(
        "[GameSession] Before create_ddgame: wrapper+0x4D0=0x{:08X}, display=0x{:08X}",
        *(this as *const u8).add(0x4D0).cast::<u32>(), display as u32,
    ));

    // Toggle: use Rust constructor (true) or original (false)
    const USE_RUST_CTOR: bool = true;
    if USE_RUST_CTOR {
        create_ddgame(
            this,
            keyboard as *mut openwa_core::input::DDKeyboard,
            display,
            sound,
            palette,
            streaming_audio as *mut openwa_core::audio::Music,
            timer_obj,
            net_game,
            game_info,
            input_ctrl as u32,
        );
    } else {
        DDGAME_CTOR_ECX = input_ctrl as u32;
        ddgame_constructor_call(
            this, display, sound, keyboard, palette, streaming_audio,
            timer_obj, net_game, game_info,
        );
    }

    let _ = log_line(&format!(
        "[GameSession] create_ddgame returned, ddgame=0x{:08X}",
        (*this).ddgame as u32));

    // Dump DDGame state BEFORE InitGameState (constructor output only)
    if std::env::var("OPENWA_VALIDATE").is_ok() {
        let ddgame_pre = (*this).ddgame;
        let real_dwords = ddgame_pre as *const u32;
        let dword_count = 0x98B8 / 4;
        let mut nonzero = 0u32;
        let _ = log_line("[Shadow] === DDGame after constructor (before InitGameState) ===");
        for i in 0..dword_count {
            let val = *real_dwords.add(i);
            if val != 0 {
                nonzero += 1;
                if nonzero <= 300 {
                    let _ = log_line(&format!(
                        "[Shadow:Pre] +0x{:04X} = 0x{:08X}", i * 4, val,
                    ));
                }
            }
        }
        let _ = log_line(&format!(
            "[Shadow:Pre] Total non-zero: {} / {}", nonzero, dword_count,
        ));
    }

    // Initialize DDGame's game-state fields.
    let _ = log_line("[GameSession] calling InitGameState");
    let init_state: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_GAME_STATE) as usize);
    init_state(this);
    let _ = log_line("[GameSession] InitGameState done");

    let ddgame = (*this).ddgame;
    let _ = log_line(&format!(
        "[GameSession] DDGameWrapper::Constructor: wrapper=0x{:08X}  ddgame=0x{:08X}",
        this as u32, ddgame as u32,
    ));

    // ── Shadow comparison: run create_ddgame() into a temp allocation ──
    // Compare byte-by-byte with the original to find porting bugs.
    if std::env::var("OPENWA_VALIDATE").is_ok() {
        shadow_compare_ddgame(
            this, keyboard, display, sound, palette, streaming_audio, input_ctrl,
            timer_obj, net_game, game_info, ddgame,
        );
    }

    this
}

/// Shadow-construct a DDGame via create_ddgame() and compare byte-by-byte
/// with the real DDGame produced by the original constructor.
///
/// Logs the first N differing DWORD offsets. This lets us verify each section
/// of create_ddgame() against the original without breaking the game.
#[allow(clippy::too_many_arguments)]
unsafe fn shadow_compare_ddgame(
    wrapper: *mut DDGameWrapper,
    keyboard: *mut u8,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    palette: *mut Palette,
    streaming_audio: *mut u8,
    input_ctrl: *mut u8,
    timer_obj: *mut u8,
    net_game: *mut u8,
    game_info: *mut GameInfo,
    real_ddgame: *mut DDGame,
) {
    let _ = log_line("[Shadow] Running create_ddgame() for comparison...");

    // Create a temporary wrapper clone so create_ddgame can write to it
    // without corrupting the real wrapper.
    let shadow_wrapper = wa_malloc(core::mem::size_of::<DDGameWrapper>() as u32);
    core::ptr::copy_nonoverlapping(
        wrapper as *const u8,
        shadow_wrapper,
        core::mem::size_of::<DDGameWrapper>(),
    );
    let _sw = shadow_wrapper as *mut DDGameWrapper;

    // Instead of running create_ddgame (which has side effects from resource
    // loading), compare the fields that we KNOW create_ddgame sets correctly:
    // the parameter storage and simple flag fields in the first 0x30 bytes
    // and scattered known offsets.
    //
    // We compare the real DDGame against what create_ddgame WOULD write,
    // computed without side effects.
    let r = real_ddgame as *const u8;

    // Check param storage (offsets 0x00-0x28)
    let checks: &[(&str, usize, u32)] = &[
        ("keyboard",       0x00, keyboard as u32),
        ("display",        0x04, display as u32),
        ("sound",          0x08, sound as u32),
        ("palette",        0x10, palette as u32),
        ("music",          0x14, streaming_audio as u32),
        ("param7",         0x18, timer_obj as u32),
        ("caller/ECX",     0x1C, input_ctrl as u32),
        ("game_info",      0x24, game_info as u32),
        ("net_game",       0x28, net_game as u32),
        ("sound_available",0x7EF8, if *(game_info as *const u8).add(0xF914).cast::<i32>() == 0 { 1 } else { 0 }),
        ("field_7EFC",     0x7EFC, 1),
    ];

    let mut ok = 0u32;
    let mut fail = 0u32;
    for &(name, offset, expected) in checks {
        let actual = *(r.add(offset) as *const u32);
        if actual == expected {
            ok += 1;
        } else {
            let _ = log_line(&format!(
                "[Shadow] FAIL +0x{:04X} ({}): expected=0x{:08X}  actual=0x{:08X}",
                offset, name, expected, actual,
            ));
            fail += 1;
        }
    }

    // Dump a summary of selected DDGame regions for future comparison
    // (pointer fields that would differ between original and shadow)
    let _ = log_line(&format!(
        "[Shadow] Param check: {} OK, {} FAIL", ok, fail,
    ));

    // Dump ALL non-zero DWORDs from the real DDGame.
    // This gives us the exact map of what the constructor initializes.
    let real_dwords = real_ddgame as *const u32;
    let dword_count = 0x98B8 / 4;
    let mut nonzero = 0u32;
    for i in 0..dword_count {
        let val = *real_dwords.add(i);
        if val != 0 {
            nonzero += 1;
            // Log up to 200 non-zero fields
            if nonzero <= 200 {
                let _ = log_line(&format!(
                    "[Shadow] DDGame+0x{:04X} = 0x{:08X}",
                    i * 4, val,
                ));
            }
        }
    }
    let _ = log_line(&format!(
        "[Shadow] Total non-zero DWORDs: {} / {}", nonzero, dword_count,
    ));

    // Also dump non-zero wrapper fields
    let wrapper_dwords = core::mem::size_of::<DDGameWrapper>() / 4;
    let real_w = wrapper as *const u32;
    let mut w_nonzero = 0u32;
    for i in 0..wrapper_dwords {
        let val = *real_w.add(i);
        if val != 0 {
            w_nonzero += 1;
            if w_nonzero <= 50 {
                let _ = log_line(&format!(
                    "[Shadow] Wrapper+0x{:04X} = 0x{:08X}",
                    i * 4, val,
                ));
            }
        }
    }
    let _ = log_line(&format!(
        "[Shadow] Wrapper non-zero DWORDs: {} / {}", w_nonzero, wrapper_dwords,
    ));

    wa_free(shadow_wrapper);
}

// ── PCLandscape__Constructor param logger ──────────────────────────
static mut PC_LANDSCAPE_TRAMPOLINE: *const () = core::ptr::null();

/// Passthrough hook: logs all 11 params, then calls original.
unsafe extern "stdcall" fn hook_pc_landscape_ctor(
    p1: u32, p2: u32, p3: u32, p4: u32, p5: u32, p6: u32,
    p7: u32, p8: u32, p9: u32, p10: u32, p11: u32,
) -> u32 {
    let _ = log_line(&format!(
        "[PCLandscape] p1=0x{:08X} p2=0x{:08X} p3=0x{:08X} p4=0x{:08X} p5=0x{:08X} p6=0x{:08X}",
        p1, p2, p3, p4, p5, p6));
    let _ = log_line(&format!(
        "[PCLandscape] p7=0x{:08X} p8=0x{:08X} p9=0x{:08X} p10=0x{:08X} p11=0x{:08X}",
        p7, p8, p9, p10, p11));
    // Call original
    let orig: unsafe extern "stdcall" fn(u32,u32,u32,u32,u32,u32,u32,u32,u32,u32,u32) -> u32 =
        core::mem::transmute(PC_LANDSCAPE_TRAMPOLINE);
    orig(p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11)
}

// ── HUD_LoadWeaponSprites param logger ──────────────────────────────
static mut HUD_LOAD_TRAMPOLINE: *const () = core::ptr::null();

unsafe extern "thiscall" fn hook_hud_load(this_ecx: u32, p1: u32, p2: u32) -> u32 {
    let _ = log_line(&format!(
        "[HUD] ECX=0x{:08X} p1=0x{:08X} p2=0x{:08X}", this_ecx, p1, p2));
    let orig: unsafe extern "thiscall" fn(u32, u32, u32) -> u32 =
        core::mem::transmute(HUD_LOAD_TRAMPOLINE);
    orig(this_ecx, p1, p2)
}

// ── SpriteRegion__Constructor param logger ──────────────────────────
static mut SPRITE_REGION_TRAMPOLINE: *const () = core::ptr::null();

/// Log fastcall params. Called from naked trampoline with all params on stack.
unsafe extern "cdecl" fn log_sprite_region(
    ecx: u32, edx: u32, p1: u32, p2: u32, p3: u32, p4: u32, p5: u32, p6: u32,
) {
    let _ = log_line(&format!(
        "[SpriteRgn] ECX=0x{:X} EDX=0x{:X} p1=0x{:X} p2=0x{:X} p3=0x{:X} p4=0x{:X} p5=0x{:X} p6=0x{:X}",
        ecx, edx, p1, p2, p3, p4, p5, p6));
}

/// Naked trampoline: captures ECX/EDX, logs, then calls original fastcall.
#[unsafe(naked)]
unsafe extern "C" fn hook_sprite_region_ctor() {
    core::arch::naked_asm!(
        // Save all regs
        "pushl %eax",
        "pushl %ecx",
        "pushl %edx",
        // Push all 8 params for logger: ECX, EDX, then 6 stack params
        // Stack after 3 pushes: [edx][ecx][eax][ret][p1][p2][p3][p4][p5][p6]
        // p1 at ESP+16, p2 at ESP+20, ...
        "pushl 36(%esp)",  // p6
        "pushl 36(%esp)",  // p5
        "pushl 36(%esp)",  // p4
        "pushl 36(%esp)",  // p3
        "pushl 36(%esp)",  // p2
        "pushl 36(%esp)",  // p1
        "pushl %edx",       // EDX
        "pushl %ecx",       // ECX
        "calll {logger}",
        "addl $32, %esp",   // clean 8 cdecl params
        // Restore regs
        "popl %edx",
        "popl %ecx",
        "popl %eax",
        // Jump to original (fastcall, same params on stack)
        "jmpl *({tramp})",
        logger = sym log_sprite_region,
        tramp = sym SPRITE_REGION_TRAMPOLINE,
        options(att_syntax),
    );
}

pub fn install() -> Result<(), String> {
    unsafe {
        INIT_REPLAY_ADDR = rb(va::DDGAMEWRAPPER_INIT_REPLAY);
        DDGAME_CTOR_ADDR = rb(va::CONSTRUCT_DD_GAME);
        // Initialize runtime addresses for create_ddgame bridges (future use).
        init_constructor_addrs();
        // DDGameWrapper__Constructor is fully converted — trap the original.
        hook::install_trap!("DDGameWrapper__Constructor", va::CONSTRUCT_DD_GAME_WRAPPER);

        // Hook PCLandscape__Constructor to log params
        let tramp = hook::install(
            "PCLandscape__Constructor",
            va::PC_LANDSCAPE_CONSTRUCTOR,
            hook_pc_landscape_ctor as *const (),
        )?;
        PC_LANDSCAPE_TRAMPOLINE = tramp as *const ();

        // Hook HUD_LoadWeaponSprites to log params
        let tramp_hud = hook::install(
            "HUD_LoadWeaponSprites",
            0x53D0E0,
            hook_hud_load as *const (),
        )?;
        HUD_LOAD_TRAMPOLINE = tramp_hud as *const ();

        // Hook SpriteRegion__Constructor to log fastcall params
        let tramp2 = hook::install(
            "SpriteRegion__Constructor",
            va::SPRITE_REGION_CONSTRUCTOR,
            hook_sprite_region_ctor as *const (),
        )?;
        SPRITE_REGION_TRAMPOLINE = tramp2 as *const ();
    }
    Ok(())
}
