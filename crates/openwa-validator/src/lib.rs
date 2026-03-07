#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

mod hooks;

use openwa_types::address::va;
use openwa_types::task::{CTask, CGameTask};
use openwa_types::ddgame::DDGame;
use openwa_types::ddgame_wrapper::DDGameWrapper;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

// ---------------------------------------------------------------------------
// ASLR rebasing
// ---------------------------------------------------------------------------

/// Delta to add to Ghidra addresses to get runtime addresses.
/// Computed as: actual_base - IMAGE_BASE (0x400000)
static REBASE_DELTA: AtomicU32 = AtomicU32::new(0);
static DUMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Rebase a Ghidra VA to the actual runtime address.
#[inline]
pub(crate) fn rb(ghidra_addr: u32) -> u32 {
    ghidra_addr.wrapping_add(REBASE_DELTA.load(Ordering::Relaxed))
}

fn init_rebase() -> i32 {
    let base = unsafe { GetModuleHandleA(std::ptr::null()) } as u32;
    let delta = base.wrapping_sub(va::IMAGE_BASE);
    REBASE_DELTA.store(delta, Ordering::Relaxed);
    delta as i32
}

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

pub(crate) fn log_line(msg: &str) -> std::io::Result<()> {
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
    addr >= rb(va::TEXT_START) && addr <= rb(va::TEXT_END)
}

#[inline]
fn is_in_rdata(addr: u32) -> bool {
    addr >= rb(va::RDATA_START) && addr < rb(va::DATA_START)
}

// ---------------------------------------------------------------------------
// Task 3: Address validation
// ---------------------------------------------------------------------------

fn validate_addresses(result: &mut ValidationResult) {
    let _ = log_line("");
    let _ = log_line("--- Address Validation ---");

    // Vtable addresses (Ghidra VAs — will be rebased via rb())
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
    for (name, ghidra_addr) in vtables {
        let addr = rb(*ghidra_addr);
        let in_rdata = is_in_rdata(addr);
        result.check(
            &format!("{} location", name),
            in_rdata,
            &format!("0x{:08X} (ghidra 0x{:08X}) {}", addr, ghidra_addr, if in_rdata { "in .rdata" } else { "NOT in .rdata" }),
        );
    }

    // 2. Vtable content checks: first entry should be a .text pointer
    let _ = log_line("");
    let _ = log_line("  Vtable first-entry checks (should point to .text):");
    for (name, ghidra_addr) in vtables {
        let addr = rb(*ghidra_addr);
        unsafe {
            let first_entry = read_u32(addr);
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

    // 3. CTask vtable method verification (compare rebased expected vs actual)
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
    for (name, offset, expected_ghidra) in ctask_vt_methods {
        unsafe {
            let actual = read_u32(rb(va::CTASK_VTABLE) + offset);
            let expected = rb(*expected_ghidra);
            let ok = actual == expected;
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
        0x64, // FS: segment prefix (SEH/stack cookies)
        0x85, // test
        0x8D, // lea
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

    for (name, ghidra_addr) in functions {
        let addr = rb(*ghidra_addr);
        let in_text = is_in_text(addr);
        if !in_text {
            result.check(
                &format!("{} prologue", name),
                false,
                &format!("0x{:08X} (ghidra 0x{:08X}) not in .text range", addr, ghidra_addr),
            );
            continue;
        }
        unsafe {
            let first_byte = read_u8(addr);
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
    check_offset!(result, CTask, children_max_size, 0x08);
    check_offset!(result, CTask, children_data, 0x14);
    check_offset!(result, CTask, class_type, 0x20);
    check_offset!(result, CTask, shared_data, 0x24);
    check_offset!(result, CTask, owns_shared_data, 0x28);
    check_offset!(result, CTask, ddgame, 0x2C);

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
    check_offset!(result, DDGameWrapper, _field_4c0, 0x4C0);
    check_offset!(result, DDGameWrapper, landscape, 0x4CC);
    check_offset!(result, DDGameWrapper, display, 0x4D0);
}

// ---------------------------------------------------------------------------
// Deferred validation via global pointer polling
// ---------------------------------------------------------------------------
// Constructor hooks failed due to retour trampoline issues with SEH prologues.
// Instead, we read the DDGameWrapper pointer from the global game session
// after waiting for initialization to complete.

fn deferred_global_validation() {
    let _ = log_line("");
    let _ = log_line("--- Deferred Global Validation (10s after load) ---");

    let mut result = ValidationResult::new();

    unsafe {
        // g_GameSession is a pointer to the game session context
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        let _ = log_line(&format!("  g_GameSession = 0x{:08X}", session_ptr));

        if session_ptr == 0 {
            let _ = log_line("  Game session not initialized yet — no game started?");
            return;
        }

        // DDGameWrapper at session+0xA0
        let wrapper_addr = read_u32(session_ptr + 0xA0);
        let _ = log_line(&format!("  DDGameWrapper = 0x{:08X}", wrapper_addr));

        if wrapper_addr == 0 {
            let _ = log_line("  DDGameWrapper not created — need to start a game first.");
            return;
        }

        // vtable at +0x00 should be DDGAME_WRAPPER_VTABLE
        let vtable_ptr = read_u32(wrapper_addr);
        let expected_vt = rb(va::DDGAME_WRAPPER_VTABLE);
        result.check(
            "DDGameWrapper.vtable",
            vtable_ptr == expected_vt,
            &format!("expected 0x{:08X}, got 0x{:08X}", expected_vt, vtable_ptr),
        );

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

            // Subsystem vtable checks
            let display_gfx = read_u32(ddgame_ptr + 0x138);
            if display_gfx != 0 {
                let dgfx_vt = read_u32(display_gfx);
                let expected = rb(va::DISPLAY_GFX_VTABLE);
                result.check(
                    "DDGame.display_gfx vtable",
                    dgfx_vt == expected,
                    &format!("expected 0x{:08X}, got 0x{:08X}", expected, dgfx_vt),
                );
            } else {
                result.check("DDGame.display_gfx", false, "NULL pointer");
            }

            let tsm = read_u32(ddgame_ptr + 0x380);
            if tsm != 0 {
                let tsm_vt = read_u32(tsm);
                let expected = rb(va::TASK_STATE_MACHINE_VTABLE);
                result.check(
                    "DDGame.task_state_machine vtable",
                    tsm_vt == expected,
                    &format!("expected 0x{:08X}, got 0x{:08X}", expected, tsm_vt),
                );
            } else {
                result.check("DDGame.task_state_machine", false, "NULL pointer");
            }
        }

        // landscape at wrapper+0x4CC
        let landscape = read_u32(wrapper_addr + 0x4CC);
        if landscape != 0 {
            let land_vt = read_u32(landscape);
            let expected = rb(va::PC_LANDSCAPE_VTABLE);
            result.check(
                "wrapper.landscape vtable",
                land_vt == expected,
                &format!("expected 0x{:08X}, got 0x{:08X}", expected, land_vt),
            );
        } else {
            result.check("wrapper.landscape", false, "NULL pointer");
        }
    }

    let _ = log_line(&format!("  Deferred {}", result.summary_line()));
}

// ---------------------------------------------------------------------------
// Team block memory dump (for WormEntry/FullTeamBlock verification)
// ---------------------------------------------------------------------------

fn dump_team_blocks() {
    use openwa_types::ddgame::{offsets, FullTeamBlock, TeamWeaponState};

    let dump_num = DUMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let _ = log_line("");
    let _ = log_line(&format!("--- Team Block Dump #{} ---", dump_num));

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_line("  No game session — skipping.");
            return;
        }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_line("  No DDGameWrapper — need to start a game first.");
            return;
        }

        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_line("  No DDGame — need to be in gameplay.");
            return;
        }

        let _ = log_line(&format!("  DDGame = 0x{:08X}", ddgame_ptr));

        // Get TeamWeaponState via struct
        let tws_base = (ddgame_ptr + offsets::TEAM_WEAPON_STATE as u32) as *const u8;
        let tws = &*(tws_base as *const TeamWeaponState);
        let _ = log_line(&format!("  team_count = {} (TeamWeaponState.team_count)", tws.team_count));

        // Get FullTeamBlock array via TWS_TO_BLOCKS offset
        let blocks = tws_base.sub(offsets::TWS_TO_BLOCKS) as *const FullTeamBlock;
        let blocks_addr = blocks as u32;
        let _ = log_line(&format!("  blocks_base = 0x{:08X} (DDGame+0x{:X})",
            blocks_addr, blocks_addr - ddgame_ptr));

        let mut result = ValidationResult::new();

        // Validate TWS_TO_BLOCKS: blocks_base should == DDGame + TEAM_BLOCKS
        let expected_blocks = ddgame_ptr + offsets::TEAM_BLOCKS as u32;
        result.check("TWS_TO_BLOCKS derivation",
            blocks_addr == expected_blocks,
            &format!("got 0x{:08X}, expected 0x{:08X}", blocks_addr, expected_blocks));

        // Validate each real team (1-indexed: blocks[1..=team_count])
        let num_blocks = (tws.team_count as u32 + 1).max(3).min(7);
        for b in 0..num_blocks {
            let block = &*blocks.add(b as usize);
            let _ = log_line(&format!("\n  === Block {} (0x{:08X}) ===",
                b, blocks_addr + b * 0x51C));

            // Sentinel: block[b+1].worms[0] holds metadata for entry_ptr(b)
            if (b + 1) < 7 {
                let sentinel = &(*blocks.add(b as usize + 1)).worms[0];
                let worm_count = sentinel.sentinel_worm_count();
                let eliminated = sentinel.sentinel_eliminated();

                // Cross-check sentinel vs raw entry_ptr reads
                let entry_ptr = tws_base.add(b as usize * 0x51C);
                let raw_worm_count = *(entry_ptr.sub(4) as *const i32);
                let raw_alliance = *(entry_ptr.add(4) as *const i32);

                result.check(
                    &format!("block[{}] sentinel_worm_count vs raw", b),
                    worm_count == raw_worm_count,
                    &format!("struct={}, raw={}", worm_count, raw_worm_count),
                );

                let _ = log_line(&format!("  sentinel: worm_count={}, eliminated={}, alliance(entry_ptr+4)={}",
                    worm_count, eliminated, raw_alliance));

                // Dump playable worms using struct field access
                for w in 0..8usize {
                    let worm = &block.worms[w];
                    let active = worm.active_flag;
                    let name_bytes = &worm.name;
                    let name_len = name_bytes.iter().position(|&c| c == 0).unwrap_or(name_bytes.len());
                    let name_str = core::str::from_utf8(&name_bytes[..name_len]).unwrap_or("?");

                    if worm.state != 0 || worm.health != 0 || active != 0 || w == 0 {
                        let _ = log_line(&format!(
                            "  worm[{}]: state=0x{:04X} active={} max_hp={} hp={} name=\"{}\"",
                            w, worm.state, active, worm.max_health, worm.health, name_str
                        ));
                    }
                }

                // Validate GetTeamTotalHealth pattern: sum block.worms[1..=count].health
                if worm_count > 0 && worm_count <= 7 {
                    let struct_total: i32 = (1..=worm_count as usize)
                        .map(|w| block.worms[w].health)
                        .sum();

                    // Cross-check vs raw pointer method (old ARRAY_OFFSET=0x4A0 pattern)
                    let raw_health_ptr = entry_ptr.sub(0x4A0) as *const i32;
                    let mut raw_total = 0i32;
                    for i in 0..worm_count {
                        raw_total += *raw_health_ptr.add(i as usize * (0x9C / 4));
                    }

                    result.check(
                        &format!("block[{}] health sum (struct vs raw)", b),
                        struct_total == raw_total,
                        &format!("struct={}, raw={}", struct_total, raw_total),
                    );
                }

                // Validate IsWormInSpecialState pattern: block.worms[w].state
                // Cross-check worm[1].state via struct vs raw (STATE_OFFSET=0x598)
                if worm_count > 0 {
                    let struct_state = block.worms[1].state;
                    let raw_state = *(entry_ptr.sub(0x598).add(0x9C) as *const u32);
                    result.check(
                        &format!("block[{}] worm[1].state (struct vs raw)", b),
                        struct_state == raw_state,
                        &format!("struct=0x{:X}, raw=0x{:X}", struct_state, raw_state),
                    );
                }

                // Validate CheckWormState0x64 pattern: reads worms[].state, NOT .health
                // Show both values for each worm so we can verify
                if worm_count > 0 {
                    let _ = log_line(&format!("  CheckWormState0x64 field check (state vs health):"));
                    for w in 1..=worm_count as usize {
                        let worm = &block.worms[w];
                        let _ = log_line(&format!(
                            "    worm[{}]: state=0x{:04X}({}) health=0x{:04X}({})",
                            w, worm.state, worm.state, worm.health, worm.health
                        ));
                    }
                }
            }
        }

        let _ = log_line(&format!("\n  Struct Validation {}", result.summary_line()));
    }
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

    // 2b. Detect ASLR rebase
    let delta = init_rebase();
    let base = unsafe { GetModuleHandleA(std::ptr::null()) } as u32;
    log_line(&format!(
        "  Module base: 0x{:08X} (Ghidra base: 0x{:08X}, delta: {}{:08X})",
        base,
        va::IMAGE_BASE,
        if delta >= 0 { "+" } else { "-" },
        if delta >= 0 { delta as u32 } else { (-(delta as i64)) as u32 }
    ))?;

    let mut result = ValidationResult::new();

    // 3. Address validation (vtables, functions)
    validate_addresses(&mut result);

    // 4. Struct offset validation (offset_of! checks)
    validate_struct_offsets(&mut result);

    // 5. Print intermediate summary
    let _ = log_line("");
    let _ = log_line("--- Static Validation Summary ---");
    let _ = log_line(&format!("  {}", result.summary_line()));

    // 6. Install hooks (vtable + inline)
    match hooks::install_all() {
        Ok(()) => {}
        Err(e) => { let _ = log_line(&format!("[ERROR] Hook installation failed: {}", e)); }
    }

    // 7. Deferred validation via polling (for DDGameWrapper — hooks don't cover this yet)
    let _ = log_line("");
    let _ = log_line("--- Deferred Validation (polling) ---");
    std::thread::spawn(move || {
        // Wait for game to finish init
        std::thread::sleep(std::time::Duration::from_secs(10));
        deferred_global_validation();
    });
    let _ = log_line("  Polling thread started (10s delay).");

    // 8. Team block dump — wait 30s for user to start a game
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        dump_team_blocks();
    });
    let _ = log_line("  Team block dump thread started (30s delay).");

    // 9. Hotkey listener — F9 triggers a team block dump on demand
    std::thread::spawn(|| {
        const VK_F9: i32 = 0x78;
        let _ = log_line("  Hotkey listener started (press F9 to dump team blocks).");
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let state = unsafe { GetAsyncKeyState(VK_F9) };
            // Bit 0 = key was pressed since last call
            if state & 1 != 0 {
                dump_team_blocks();
            }
        }
    });

    Ok(())
}
