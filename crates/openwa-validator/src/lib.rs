#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use openwa_types::address::va;
use openwa_types::task::{CTask, CGameTask};
use openwa_types::ddgame::DDGame;
use openwa_types::ddgame_wrapper::DDGameWrapper;

use retour::static_detour;

// ---------------------------------------------------------------------------
// DllMain
// ---------------------------------------------------------------------------

const DLL_PROCESS_ATTACH: u32 = 1;

#[no_mangle]
unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        std::thread::spawn(|| {
            if let Err(e) = run_validation() {
                let _ = log_line(&format!("[FATAL] Validation failed to run: {}", e));
            }
        });
    }
    1 // TRUE
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

fn log_line(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("OpenWA_validation.log")?;
    writeln!(f, "{}", msg)?;
    Ok(())
}

fn clear_log() -> std::io::Result<()> {
    std::fs::write("OpenWA_validation.log", "")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ValidationResult
// ---------------------------------------------------------------------------

struct ValidationResult {
    pass: u32,
    fail: u32,
}

impl ValidationResult {
    fn new() -> Self {
        Self { pass: 0, fail: 0 }
    }

    fn check(&mut self, name: &str, ok: bool, detail: &str) {
        if ok {
            self.pass += 1;
            let _ = log_line(&format!("[PASS] {} - {}", name, detail));
        } else {
            self.fail += 1;
            let _ = log_line(&format!("[FAIL] {} - {}", name, detail));
        }
    }

    fn total(&self) -> u32 {
        self.pass + self.fail
    }

    fn summary_line(&self) -> String {
        format!(
            "Results: {}/{} passed, {} failed",
            self.pass,
            self.total(),
            self.fail
        )
    }
}

// ---------------------------------------------------------------------------
// Memory helpers
// ---------------------------------------------------------------------------

#[inline]
unsafe fn read_u32(addr: u32) -> u32 {
    *(addr as *const u32)
}

#[inline]
unsafe fn read_u8(addr: u32) -> u8 {
    *(addr as *const u8)
}

#[inline]
fn is_in_text(addr: u32) -> bool {
    addr >= va::TEXT_START && addr <= va::TEXT_END
}

#[inline]
fn is_in_rdata(addr: u32) -> bool {
    addr >= va::RDATA_START && addr < va::DATA_START
}

// ---------------------------------------------------------------------------
// Task 3: Address validation
// ---------------------------------------------------------------------------

fn validate_addresses(result: &mut ValidationResult) {
    let _ = log_line("");
    let _ = log_line("--- Address Validation ---");

    // Vtable addresses and their names
    let vtables: &[(&str, u32)] = &[
        ("CTask vtable", va::CTASK_VTABLE),
        ("CGameTask vtable", va::CGAMETASK_VTABLE),
        ("CGameTask vtable2", va::CGAMETASK_VTABLE2),
        ("DDGameWrapper vtable", va::DDGAME_WRAPPER_VTABLE),
        ("GfxHandler vtable", va::GFX_HANDLER_VTABLE),
        ("DisplayGfx vtable", va::DISPLAY_GFX_VTABLE),
        ("PCLandscape vtable", va::PC_LANDSCAPE_VTABLE),
        ("LandscapeShader vtable", va::LANDSCAPE_SHADER_VTABLE),
        ("DSSound vtable", va::DS_SOUND_VTABLE),
        ("TaskStateMachine vtable", va::TASK_STATE_MACHINE_VTABLE),
        ("OpenGLCPU vtable", va::OPENGL_CPU_VTABLE),
        ("WaterEffect vtable", va::WATER_EFFECT_VTABLE),
    ];

    // 1. Vtable location checks
    let _ = log_line("");
    let _ = log_line("  Vtable location checks (.rdata range):");
    for (name, addr) in vtables {
        let in_rdata = is_in_rdata(*addr);
        result.check(
            &format!("{} location", name),
            in_rdata,
            &format!("0x{:08X} {}", addr, if in_rdata { "in .rdata" } else { "NOT in .rdata" }),
        );
    }

    // 2. Vtable content checks: first entry should be a .text pointer
    let _ = log_line("");
    let _ = log_line("  Vtable first-entry checks (should point to .text):");
    for (name, addr) in vtables {
        unsafe {
            let first_entry = read_u32(*addr);
            let in_text = is_in_text(first_entry);
            result.check(
                &format!("{} first entry", name),
                in_text,
                &format!(
                    "[0x{:08X}] = 0x{:08X} {}",
                    addr,
                    first_entry,
                    if in_text { "in .text" } else { "NOT in .text" }
                ),
            );
        }
    }

    // 3. CTask vtable method verification
    let _ = log_line("");
    let _ = log_line("  CTask vtable method verification:");
    let ctask_vt_methods: &[(&str, u32, u32)] = &[
        ("vt0 init", 0, va::CTASK_VT0_INIT),
        ("vt1 Free", 4, va::CTASK_VT1_FREE),
        ("vt2 HandleMessage", 8, va::CTASK_VT2_HANDLE_MESSAGE),
        ("vt3", 12, va::CTASK_VT3),
        ("vt4", 16, va::CTASK_VT4),
        ("vt5", 20, va::CTASK_VT5),
        ("vt6", 24, va::CTASK_VT6),
        ("vt7 ProcessFrame", 28, va::CTASK_VT7_PROCESS_FRAME),
    ];
    for (name, offset, expected) in ctask_vt_methods {
        unsafe {
            let actual = read_u32(va::CTASK_VTABLE + offset);
            let ok = actual == *expected;
            result.check(
                &format!("CTask::{}", name),
                ok,
                &format!(
                    "vtable+0x{:02X}: expected 0x{:08X}, got 0x{:08X}",
                    offset, expected, actual
                ),
            );
        }
    }

    // 4. Function prologue checks
    let _ = log_line("");
    let _ = log_line("  Function prologue checks:");
    let valid_prologues: &[u8] = &[
        0x55, // push ebp
        0x53, // push ebx
        0x56, // push esi
        0x57, // push edi
        0x83, // sub esp, ...
        0x8B, // mov reg, ...
        0x6A, // push imm8
        0x81, // sub esp, imm32
        0xB8, // mov eax, imm32
        0x51, // push ecx
        0x52, // push edx
    ];

    let functions: &[(&str, u32)] = &[
        ("CTask::Constructor", va::CTASK_CONSTRUCTOR),
        ("CGameTask::Constructor", va::CGAMETASK_CONSTRUCTOR),
        ("DDGameWrapper::Constructor", va::CONSTRUCT_DD_GAME_WRAPPER),
        ("DDGame::Constructor", va::CONSTRUCT_DD_GAME),
        ("CreateExplosion", va::CREATE_EXPLOSION),
        ("SpawnObject", va::SPAWN_OBJECT),
        ("WeaponRelease", va::WEAPON_RELEASE),
        ("FireWeapon", va::FIRE_WEAPON),
        ("InitWeaponTable", va::INIT_WEAPON_TABLE),
        ("BlitScreen", va::BLIT_SCREEN),
        ("RenderDrawingQueue", va::RENDER_DRAWING_QUEUE),
        ("DrawLandscape", va::DRAW_LANDSCAPE),
        ("ConstructPCLandscape", va::CONSTRUCT_PC_LANDSCAPE),
        ("ConstructDSSound", va::CONSTRUCT_DS_SOUND),
        ("ShowChatMessage", va::SHOW_CHAT_MESSAGE),
        ("FrontendChangeScreen", va::FRONTEND_CHANGE_SCREEN),
        ("WA_MallocMemset", va::WA_MALLOC_MEMSET),
        ("CTask::vt0_init", va::CTASK_VT0_INIT),
        ("CTask::vt1_Free", va::CTASK_VT1_FREE),
        ("CTask::vt2_HandleMessage", va::CTASK_VT2_HANDLE_MESSAGE),
        ("CGameTask::vt0", va::CGAMETASK_VT0),
        ("CGameTask::vt1_Free", va::CGAMETASK_VT1_FREE),
        ("CGameTask::vt2_HandleMessage", va::CGAMETASK_VT2_HANDLE_MESSAGE),
    ];

    for (name, addr) in functions {
        let in_text = is_in_text(*addr);
        if !in_text {
            result.check(
                &format!("{} prologue", name),
                false,
                &format!("0x{:08X} not in .text range", addr),
            );
            continue;
        }
        unsafe {
            let first_byte = read_u8(*addr);
            let ok = valid_prologues.contains(&first_byte);
            result.check(
                &format!("{} prologue", name),
                ok,
                &format!(
                    "0x{:08X}: first byte 0x{:02X} {}",
                    addr,
                    first_byte,
                    if ok { "valid" } else { "UNEXPECTED" }
                ),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Task 6: offset_of! validation
// ---------------------------------------------------------------------------

fn validate_struct_offsets(result: &mut ValidationResult) {
    let _ = log_line("");
    let _ = log_line("--- Struct Offset Validation (offset_of!) ---");

    // Helper macro to reduce boilerplate
    macro_rules! check_offset {
        ($result:expr, $struct:ty, $field:ident, $expected:expr) => {
            let actual = core::mem::offset_of!($struct, $field);
            $result.check(
                &format!("{}::{}", stringify!($struct), stringify!($field)),
                actual == $expected,
                &format!("expected 0x{:X}, got 0x{:X}", $expected, actual),
            );
        };
    }

    // CTask offsets
    let _ = log_line("");
    let _ = log_line("  CTask:");
    check_offset!(result, CTask, vtable, 0x00);
    check_offset!(result, CTask, parent, 0x04);
    check_offset!(result, CTask, _unknown_08, 0x08);
    check_offset!(result, CTask, class_type, 0x20);

    // CGameTask offsets
    let _ = log_line("");
    let _ = log_line("  CGameTask:");
    check_offset!(result, CGameTask, base, 0x00);
    check_offset!(result, CGameTask, pos_x, 0x84);
    check_offset!(result, CGameTask, pos_y, 0x88);
    check_offset!(result, CGameTask, speed_x, 0x90);
    check_offset!(result, CGameTask, speed_y, 0x94);
    check_offset!(result, CGameTask, vtable2, 0xE8);

    // DDGame offsets
    let _ = log_line("");
    let _ = log_line("  DDGame:");
    check_offset!(result, DDGame, landscape, 0x20);
    check_offset!(result, DDGame, game_state, 0x24);
    check_offset!(result, DDGame, arrow_sprites, 0x38);
    check_offset!(result, DDGame, arrow_gfxdirs, 0xB8);
    check_offset!(result, DDGame, display_gfx, 0x138);
    check_offset!(result, DDGame, task_state_machine, 0x380);
    check_offset!(result, DDGame, sprite_regions, 0x46C);
    check_offset!(result, DDGame, coord_list, 0x50C);

    // DDGameWrapper offsets
    let _ = log_line("");
    let _ = log_line("  DDGameWrapper:");
    check_offset!(result, DDGameWrapper, vtable, 0x00);
    check_offset!(result, DDGameWrapper, ddgame, 0x488);
    check_offset!(result, DDGameWrapper, gfx_handler_0, 0x4C0);
    check_offset!(result, DDGameWrapper, landscape, 0x4CC);
    check_offset!(result, DDGameWrapper, display, 0x4D0);
}

// ---------------------------------------------------------------------------
// Task 4: Constructor hooks
// ---------------------------------------------------------------------------

static CTASK_HOOKED: AtomicBool = AtomicBool::new(false);
static DDGAME_WRAPPER_HOOKED: AtomicBool = AtomicBool::new(false);

static_detour! {
    static CTaskCtorHook: unsafe extern "thiscall" fn(u32, u32, u32) -> u32;
    static DDGameWrapperCtorHook: unsafe extern "thiscall" fn(u32) -> u32;
}

fn ctask_ctor_detour(this: u32, parent: u32, class_type: u32) -> u32 {
    // Call original constructor
    let ret = unsafe { CTaskCtorHook.call(this, parent, class_type) };

    // Only validate the first CTask construction
    if CTASK_HOOKED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        let _ = log_line("");
        let _ = log_line("--- CTask Constructor Hook Triggered ---");
        let _ = log_line(&format!("  this=0x{:08X}, parent=0x{:08X}, class_type={}", this, parent, class_type));

        let mut result = ValidationResult::new();

        unsafe {
            // vtable at +0x00 should be in .rdata and point to .text
            let vtable_ptr = read_u32(this);
            let vt_in_rdata = is_in_rdata(vtable_ptr);
            result.check(
                "CTask.vtable location",
                vt_in_rdata,
                &format!("0x{:08X} {}", vtable_ptr, if vt_in_rdata { "in .rdata" } else { "NOT in .rdata" }),
            );

            if vt_in_rdata {
                let first_method = read_u32(vtable_ptr);
                let in_text = is_in_text(first_method);
                result.check(
                    "CTask.vtable[0] -> .text",
                    in_text,
                    &format!("0x{:08X} {}", first_method, if in_text { "in .text" } else { "NOT in .text" }),
                );
            }

            // field at +0x08 should be 0x10
            let field_08 = read_u32(this + 0x08);
            result.check(
                "CTask._unknown_08 == 0x10",
                field_08 == 0x10,
                &format!("expected 0x10, got 0x{:X}", field_08),
            );

            // class_type at +0x20 should be in reasonable range (<200)
            let ct = read_u32(this + 0x20);
            result.check(
                "CTask.class_type < 200",
                ct < 200,
                &format!("class_type = {} (arg was {})", ct, class_type),
            );
        }

        let _ = log_line(&format!("  CTask hook {}", result.summary_line()));
    }

    ret
}

fn ddgame_wrapper_ctor_detour(this: u32) -> u32 {
    // Call original constructor
    let ret = unsafe { DDGameWrapperCtorHook.call(this) };

    // Only validate the first DDGameWrapper construction
    if DDGAME_WRAPPER_HOOKED.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        let _ = log_line("");
        let _ = log_line("--- DDGameWrapper Constructor Hook Triggered ---");
        let _ = log_line(&format!("  this=0x{:08X}", this));

        let mut result = ValidationResult::new();

        unsafe {
            // vtable at +0x00 should be DDGAME_WRAPPER_VTABLE
            let vtable_ptr = read_u32(this);
            result.check(
                "DDGameWrapper.vtable == 0x66A30C",
                vtable_ptr == va::DDGAME_WRAPPER_VTABLE,
                &format!("expected 0x{:08X}, got 0x{:08X}", va::DDGAME_WRAPPER_VTABLE, vtable_ptr),
            );

            // field at +0x4E0 should be 0xFFFFFF9C (-100)
            let field_4e0 = read_u32(this + 0x4E0);
            result.check(
                "DDGameWrapper._field_4e0 == -100",
                field_4e0 == 0xFFFFFF9C,
                &format!("expected 0xFFFFFF9C, got 0x{:08X}", field_4e0),
            );
        }

        let _ = log_line(&format!("  DDGameWrapper immediate {}", result.summary_line()));

        // Spawn deferred validation thread
        let wrapper_addr = this;
        std::thread::spawn(move || {
            // Wait 5 seconds for game initialization to complete
            std::thread::sleep(std::time::Duration::from_secs(5));

            let _ = log_line("");
            let _ = log_line("--- DDGameWrapper Deferred Validation (5s after ctor) ---");
            let _ = log_line(&format!("  wrapper=0x{:08X}", wrapper_addr));

            let mut result = ValidationResult::new();

            unsafe {
                // ddgame pointer at +0x488 should be non-null
                let ddgame_ptr = read_u32(wrapper_addr + 0x488);
                let ddgame_valid = ddgame_ptr != 0;
                result.check(
                    "DDGameWrapper.ddgame != NULL",
                    ddgame_valid,
                    &format!("0x{:08X}", ddgame_ptr),
                );

                if ddgame_valid {
                    // DDGame.landscape at +0x20 should match wrapper.landscape at +0x4CC
                    let ddgame_landscape = read_u32(ddgame_ptr + 0x20);
                    let wrapper_landscape = read_u32(wrapper_addr + 0x4CC);
                    result.check(
                        "DDGame.landscape == wrapper.landscape",
                        ddgame_landscape == wrapper_landscape,
                        &format!(
                            "DDGame+0x20=0x{:08X}, wrapper+0x4CC=0x{:08X}",
                            ddgame_landscape, wrapper_landscape
                        ),
                    );

                    // DDGame+0x7EFC should be 1
                    let field_7efc = read_u32(ddgame_ptr + 0x7EFC);
                    result.check(
                        "DDGame+0x7EFC == 1",
                        field_7efc == 1,
                        &format!("expected 1, got {}", field_7efc),
                    );

                    // Subsystem vtable checks
                    // display_gfx at DDGame+0x138
                    let display_gfx = read_u32(ddgame_ptr + 0x138);
                    if display_gfx != 0 {
                        let dgfx_vt = read_u32(display_gfx);
                        result.check(
                            "DDGame.display_gfx vtable",
                            dgfx_vt == va::DISPLAY_GFX_VTABLE,
                            &format!("expected 0x{:08X}, got 0x{:08X}", va::DISPLAY_GFX_VTABLE, dgfx_vt),
                        );
                    } else {
                        result.check("DDGame.display_gfx", false, "NULL pointer");
                    }

                    // task_state_machine at DDGame+0x380
                    let tsm = read_u32(ddgame_ptr + 0x380);
                    if tsm != 0 {
                        let tsm_vt = read_u32(tsm);
                        result.check(
                            "DDGame.task_state_machine vtable",
                            tsm_vt == va::TASK_STATE_MACHINE_VTABLE,
                            &format!("expected 0x{:08X}, got 0x{:08X}", va::TASK_STATE_MACHINE_VTABLE, tsm_vt),
                        );
                    } else {
                        result.check("DDGame.task_state_machine", false, "NULL pointer");
                    }
                }

                // gfx_handler_0 at wrapper+0x4C0
                let gfx0 = read_u32(wrapper_addr + 0x4C0);
                if gfx0 != 0 {
                    let gfx0_vt = read_u32(gfx0);
                    result.check(
                        "wrapper.gfx_handler_0 vtable",
                        gfx0_vt == va::GFX_HANDLER_VTABLE,
                        &format!("expected 0x{:08X}, got 0x{:08X}", va::GFX_HANDLER_VTABLE, gfx0_vt),
                    );
                } else {
                    result.check("wrapper.gfx_handler_0", false, "NULL pointer");
                }

                // landscape at wrapper+0x4CC
                let landscape = read_u32(wrapper_addr + 0x4CC);
                if landscape != 0 {
                    let land_vt = read_u32(landscape);
                    result.check(
                        "wrapper.landscape vtable",
                        land_vt == va::PC_LANDSCAPE_VTABLE,
                        &format!("expected 0x{:08X}, got 0x{:08X}", va::PC_LANDSCAPE_VTABLE, land_vt),
                    );
                } else {
                    result.check("wrapper.landscape", false, "NULL pointer");
                }
            }

            let _ = log_line(&format!("  DDGameWrapper deferred {}", result.summary_line()));
        });
    }

    ret
}

fn install_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let _ = log_line("");
    let _ = log_line("--- Installing Constructor Hooks ---");

    unsafe {
        // CTask constructor hook
        let ctask_ctor: unsafe extern "thiscall" fn(u32, u32, u32) -> u32 =
            std::mem::transmute(va::CTASK_CONSTRUCTOR as usize);
        CTaskCtorHook
            .initialize(ctask_ctor, |a, b, c| ctask_ctor_detour(a, b, c))?
            .enable()?;
        let _ = log_line(&format!(
            "  CTask constructor hook installed at 0x{:08X}",
            va::CTASK_CONSTRUCTOR
        ));

        // DDGameWrapper constructor hook
        let ddgw_ctor: unsafe extern "thiscall" fn(u32) -> u32 =
            std::mem::transmute(va::CONSTRUCT_DD_GAME_WRAPPER as usize);
        DDGameWrapperCtorHook
            .initialize(ddgw_ctor, |a| ddgame_wrapper_ctor_detour(a))?
            .enable()?;
        let _ = log_line(&format!(
            "  DDGameWrapper constructor hook installed at 0x{:08X}",
            va::CONSTRUCT_DD_GAME_WRAPPER
        ));
    }

    let _ = log_line("  All hooks installed successfully.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Main validation entry point
// ---------------------------------------------------------------------------

fn run_validation() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Clear log file
    clear_log()?;

    // 2. Log header
    log_line("============================================")?;
    log_line("  OpenWA Runtime Validator")?;
    log_line("  Target: WA.exe 3.8.1 (Steam)")?;
    log_line("============================================")?;

    let mut result = ValidationResult::new();

    // 3. Address validation (vtables, functions)
    validate_addresses(&mut result);

    // 4. Struct offset validation (offset_of! checks)
    validate_struct_offsets(&mut result);

    // 5. Print intermediate summary
    let _ = log_line("");
    let _ = log_line("--- Static Validation Summary ---");
    let _ = log_line(&format!("  {}", result.summary_line()));

    // 6. Install hooks (CTask, DDGameWrapper)
    match install_hooks() {
        Ok(()) => {}
        Err(e) => {
            let _ = log_line(&format!("[ERROR] Failed to install hooks: {}", e));
        }
    }

    // 7. Log that hooks are installed
    let _ = log_line("");
    let _ = log_line("Validator initialized. Constructor hooks active.");
    let _ = log_line("Waiting for game to construct CTask / DDGameWrapper...");

    Ok(())
}
