//! Runtime validation of openwa-core against live WA.exe memory.
//!
//! Enabled by setting `OPENWA_VALIDATE=1` environment variable.
//! When `OPENWA_REPLAY_TEST=1` is also set, enters auto-capture mode:
//! waits for the replay to finish (up to 120s), runs validation dumps,
//! then calls ExitProcess(0) as a safety net.

mod hooks;

use std::sync::atomic::{AtomicU32, Ordering};

use openwa_core::rebase::rb;
use openwa_core::address::va;
use openwa_core::task::{CTask, CGameTask};
use openwa_core::ddgame::DDGame;
use openwa_core::ddgame_wrapper::DDGameWrapper;

static DUMP_COUNTER: AtomicU32 = AtomicU32::new(0);

// ---------------------------------------------------------------------------
// Logging (separate file from main OpenWA.log)
// ---------------------------------------------------------------------------

pub(crate) fn log_validation(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("OpenWA_validation.log")?;
    writeln!(f, "{}", msg)?;
    Ok(())
}

fn clear_validation_log() -> std::io::Result<()> {
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
            let _ = log_validation(&format!("[PASS] {} - {}", name, detail));
        } else {
            self.fail += 1;
            let _ = log_validation(&format!("[FAIL] {} - {}", name, detail));
        }
    }

    fn total(&self) -> u32 {
        self.pass + self.fail
    }

    fn summary_line(&self) -> String {
        format!(
            "Results: {}/{} passed, {} failed",
            self.pass, self.total(), self.fail
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
// Address validation
// ---------------------------------------------------------------------------

fn validate_addresses(result: &mut ValidationResult) {
    let _ = log_validation("");
    let _ = log_validation("--- Address Validation ---");

    let vtables: &[(&str, u32)] = &[
        ("CTask vtable", va::CTASK_VTABLE),
        ("CGameTask vtable", va::CGAMETASK_VTABLE),
        ("CGameTask SoundEmitter vtable", va::CGAMETASK_SOUND_EMITTER_VT),
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

    let _ = log_validation("");
    let _ = log_validation("  Vtable location checks (.rdata range):");
    for (name, ghidra_addr) in vtables {
        let addr = rb(*ghidra_addr);
        let in_rdata = is_in_rdata(addr);
        result.check(
            &format!("{} location", name),
            in_rdata,
            &format!("0x{:08X} (ghidra 0x{:08X}) {}", addr, ghidra_addr, if in_rdata { "in .rdata" } else { "NOT in .rdata" }),
        );
    }

    let _ = log_validation("");
    let _ = log_validation("  Vtable first-entry checks (should point to .text):");
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
                    addr, first_entry,
                    if in_text { "in .text" } else { "NOT in .text" }
                ),
            );
        }
    }

    let _ = log_validation("");
    let _ = log_validation("  CTask vtable method verification:");
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

    let _ = log_validation("");
    let _ = log_validation("  Function prologue checks:");
    let valid_prologues: &[u8] = &[
        0x55, 0x53, 0x56, 0x57, 0x83, 0x8B, 0x6A, 0x81,
        0xB8, 0x51, 0x52, 0x64, 0x85, 0x8D,
        0xE9, // JMP — MinHook trampoline (function has been hooked)
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
        ("RenderDrawingQueue", va::RQ_RENDER_DRAWING_QUEUE),
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
                    addr, first_byte,
                    if ok { "valid" } else { "UNEXPECTED" }
                ),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Struct offset validation
// ---------------------------------------------------------------------------

fn validate_struct_offsets(result: &mut ValidationResult) {
    let _ = log_validation("");
    let _ = log_validation("--- Struct Offset Validation (offset_of!) ---");

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

    let _ = log_validation("");
    let _ = log_validation("  CTask:");
    check_offset!(result, CTask, vtable, 0x00);
    check_offset!(result, CTask, parent, 0x04);
    check_offset!(result, CTask, children_max_size, 0x08);
    check_offset!(result, CTask, children_data, 0x14);
    check_offset!(result, CTask, class_type, 0x20);
    check_offset!(result, CTask, shared_data, 0x24);
    check_offset!(result, CTask, owns_shared_data, 0x28);
    check_offset!(result, CTask, ddgame, 0x2C);

    let _ = log_validation("");
    let _ = log_validation("  CGameTask:");
    check_offset!(result, CGameTask, base, 0x00);
    check_offset!(result, CGameTask, pos_x, 0x84);
    check_offset!(result, CGameTask, pos_y, 0x88);
    check_offset!(result, CGameTask, speed_x, 0x90);
    check_offset!(result, CGameTask, speed_y, 0x94);
    check_offset!(result, CGameTask, sound_emitter, 0xE8);

    let _ = log_validation("");
    let _ = log_validation("  DDGame:");
    check_offset!(result, DDGame, landscape, 0x20);
    check_offset!(result, DDGame, game_info, 0x24);
    check_offset!(result, DDGame, arrow_sprites, 0x38);
    check_offset!(result, DDGame, arrow_gfxdirs, 0xB8);
    check_offset!(result, DDGame, display_gfx, 0x138);
    check_offset!(result, DDGame, task_state_machine, 0x380);
    check_offset!(result, DDGame, sprite_regions, 0x46C);
    check_offset!(result, DDGame, coord_list, 0x50C);

    let _ = log_validation("");
    let _ = log_validation("  DDGameWrapper:");
    check_offset!(result, DDGameWrapper, vtable, 0x00);
    check_offset!(result, DDGameWrapper, ddgame, 0x488);
    check_offset!(result, DDGameWrapper, _field_4c0, 0x4C0);
    check_offset!(result, DDGameWrapper, landscape, 0x4CC);
    check_offset!(result, DDGameWrapper, display, 0x4D0);
}

// ---------------------------------------------------------------------------
// Deferred global validation
// ---------------------------------------------------------------------------

fn deferred_global_validation() {
    let _ = log_validation("");
    let _ = log_validation("--- Deferred Global Validation (10s after load) ---");

    let mut result = ValidationResult::new();

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        let _ = log_validation(&format!("  g_GameSession = 0x{:08X}", session_ptr));

        if session_ptr == 0 {
            let _ = log_validation("  Game session not initialized yet — no game started?");
            return;
        }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        let _ = log_validation(&format!("  DDGameWrapper = 0x{:08X}", wrapper_addr));

        if wrapper_addr == 0 {
            let _ = log_validation("  DDGameWrapper not created — need to start a game first.");
            return;
        }

        let vtable_ptr = read_u32(wrapper_addr);
        let expected_vt = rb(va::DDGAME_WRAPPER_VTABLE);
        result.check(
            "DDGameWrapper.vtable",
            vtable_ptr == expected_vt,
            &format!("expected 0x{:08X}, got 0x{:08X}", expected_vt, vtable_ptr),
        );

        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        let ddgame_valid = ddgame_ptr != 0;
        result.check(
            "DDGameWrapper.ddgame != NULL",
            ddgame_valid,
            &format!("0x{:08X}", ddgame_ptr),
        );

        if ddgame_valid {
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

    let _ = log_validation(&format!("  Deferred {}", result.summary_line()));
}

// ---------------------------------------------------------------------------
// Team block dump
// ---------------------------------------------------------------------------

fn dump_team_blocks() {
    use openwa_core::ddgame::{offsets, TeamArenaRef};

    let dump_num = DUMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let _ = log_validation("");
    let _ = log_validation(&format!("--- Team Block Dump #{} ---", dump_num));

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 { let _ = log_validation("  No game session — skipping."); return; }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 { let _ = log_validation("  No DDGameWrapper."); return; }

        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 { let _ = log_validation("  No DDGame."); return; }

        let _ = log_validation(&format!("  DDGame = 0x{:08X}", ddgame_ptr));

        let arena_base = ddgame_ptr + offsets::TEAM_ARENA_STATE as u32;
        let arena = TeamArenaRef::from_raw(arena_base);
        let tws = arena.state();
        let tws_base = arena_base as *const u8;
        let _ = log_validation(&format!("  team_count = {} (TeamArenaState.team_count)", tws.team_count));

        let blocks = arena.blocks();
        let blocks_addr = blocks as u32;
        let _ = log_validation(&format!("  blocks_base = 0x{:08X} (DDGame+0x{:X})",
            blocks_addr, blocks_addr - ddgame_ptr));

        let mut result = ValidationResult::new();

        let expected_blocks = ddgame_ptr + offsets::TEAM_BLOCKS as u32;
        result.check("ARENA_TO_BLOCKS derivation",
            blocks_addr == expected_blocks,
            &format!("got 0x{:08X}, expected 0x{:08X}", blocks_addr, expected_blocks));

        let num_blocks = (tws.team_count as u32 + 1).max(3).min(7);
        for b in 0..num_blocks {
            let block = &*blocks.add(b as usize);
            let _ = log_validation(&format!("\n  === Block {} (0x{:08X}) ===",
                b, blocks_addr + b * 0x51C));

            if (b + 1) < 7 {
                let header = &(*blocks.add(b as usize + 1)).header.team;
                let worm_count = header.worm_count;
                let eliminated = header.eliminated;

                let entry_ptr = tws_base.add(b as usize * 0x51C);
                let raw_worm_count = *(entry_ptr.sub(4) as *const i32);
                let raw_alliance = *(entry_ptr.add(4) as *const i32);

                result.check(
                    &format!("block[{}] header.worm_count vs raw", b),
                    worm_count == raw_worm_count,
                    &format!("struct={}, raw={}", worm_count, raw_worm_count),
                );

                let _ = log_validation(&format!("  header: worm_count={}, eliminated={}, alliance(entry_ptr+4)={}",
                    worm_count, eliminated, raw_alliance));

                for w in 0..8usize {
                    let worm = if w == 0 {
                        &*block.header.worm
                    } else {
                        &block.worms[w - 1]
                    };
                    let active = worm.active_flag;
                    let name_bytes = &worm.name;
                    let name_len = name_bytes.iter().position(|&c| c == 0).unwrap_or(name_bytes.len());
                    let name_str = core::str::from_utf8(&name_bytes[..name_len]).unwrap_or("?");

                    if worm.state != 0 || worm.health != 0 || active != 0 || w == 0 {
                        let _ = log_validation(&format!(
                            "  worm[{}]: state=0x{:04X} active={} max_hp={} hp={} name=\"{}\"",
                            w, worm.state, active, worm.max_health, worm.health, name_str
                        ));
                    }
                }

                if worm_count > 0 && worm_count <= 8 {
                    let struct_total: i32 = (1..=worm_count as usize)
                        .map(|w| arena.team_worm(b as usize, w).health)
                        .sum();

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

                if worm_count > 0 {
                    let struct_state = block.worms[0].state;
                    let raw_state = *(entry_ptr.sub(0x598).add(0x9C) as *const u32);
                    result.check(
                        &format!("block[{}] worm[1].state (struct vs raw)", b),
                        struct_state == raw_state,
                        &format!("struct=0x{:X}, raw=0x{:X}", struct_state, raw_state),
                    );

                    let _ = log_validation("  CheckWormState0x64 field check (state vs health):");
                    for w in 1..=worm_count as usize {
                        let worm = arena.team_worm(b as usize, w);
                        let _ = log_validation(&format!(
                            "    worm[{}]: state=0x{:04X}({}) health=0x{:04X}({})",
                            w, worm.state, worm.state, worm.health, worm.health
                        ));
                    }
                }
            }
        }

        let _ = log_validation(&format!("\n  Struct Validation {}", result.summary_line()));

        // === TeamArenaState memory layout dump ===
        // Dump the region around team entries to understand the actual layout.
        // Each "team entry" has stride 0x51C. We dump the first 16 bytes of each
        // slot (including slot 6 which goes "out of bounds"), plus the area
        // around weapon_slots start (0x1EB4) and team_count (0x1EB0).
        let _ = log_validation("\n  === TeamArenaState Layout Dump ===");
        let _ = log_validation(&format!("  arena_base = 0x{:08X} (DDGame + 0x{:X})",
            arena_base, offsets::TEAM_ARENA_STATE));

        // Dump first 16 bytes at each team_index * 0x51C stride (slots -2 to 7)
        for slot in -2i32..8 {
            let offset = slot as isize * 0x51C as isize;
            let ptr = tws_base.offset(offset);
            let mut hex = String::new();
            for i in 0..16usize {
                if i > 0 && i % 4 == 0 { hex.push(' '); }
                hex.push_str(&format!("{:02X}", *ptr.add(i)));
            }
            let abs_addr = (arena_base as isize + offset) as u32;
            let label = match slot {
                -2 => "team[-2]".to_string(),
                -1 => "team[-1]".to_string(),
                0..=5 => format!("team[{}]", slot),
                6 => "team[6]".to_string(),
                _ => "team[7]".to_string(),
            };
            let _ = log_validation(&format!(
                "  {:+06X} (0x{:08X}) {}: {}  val_at+4={}",
                offset, abs_addr, label, hex,
                *(ptr.add(4) as *const i32)
            ));
        }

        // Dump the region around 0x1EA0-0x1EC0 (team entries end / weapon_slots start)
        let _ = log_validation("\n  --- Region 0x1EA0..0x1EC0 (boundary area) ---");
        for row in (0x1EA0..0x1EC0).step_by(16) {
            let ptr = tws_base.add(row);
            let mut hex = String::new();
            for i in 0..16usize {
                if i > 0 && i % 4 == 0 { hex.push(' '); }
                hex.push_str(&format!("{:02X}", *ptr.add(i)));
            }
            // Label known offsets
            let label = match row {
                0x1EA0 => " (end of teams[5])",
                0x1EB0 => " (team_count + weapon_slots[0..3])",
                _ => "",
            };
            let _ = log_validation(&format!(
                "  +0x{:04X}: {}{}", row, hex, label
            ));
        }

        // Dump team_count and first few weapon_slots
        let _ = log_validation(&format!("\n  team_count (+0x1EB0) = {}",
            *(tws_base.add(0x1EB0) as *const i32)));
        let _ = log_validation(&format!("  weapon_slots[0..4] (+0x1EB4) = [{}, {}, {}, {}]",
            *(tws_base.add(0x1EB4) as *const i32),
            *(tws_base.add(0x1EB8) as *const i32),
            *(tws_base.add(0x1EBC) as *const i32),
            *(tws_base.add(0x1EC0) as *const i32)));

        // Dump game_mode_flag and game_phase for context
        let _ = log_validation(&format!("  game_mode_flag (+0x2C0C) = {}",
            *(tws_base.add(0x2C0C) as *const i32)));
        let _ = log_validation(&format!("  game_phase (+0x2C28) = {}",
            *(tws_base.add(0x2C28) as *const i32)));
    }
}

// ---------------------------------------------------------------------------
// PCLandscape dump
// ---------------------------------------------------------------------------

fn dump_landscape() {
    use openwa_core::landscape::PCLandscape;

    let dump_num = DUMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let _ = log_validation("");
    let _ = log_validation(&format!("--- PCLandscape Dump #{} ---", dump_num));

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 { let _ = log_validation("  No game session — skipping."); return; }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 { let _ = log_validation("  No DDGameWrapper."); return; }

        let landscape_ptr = read_u32(wrapper_addr + 0x4CC);
        if landscape_ptr == 0 { let _ = log_validation("  PCLandscape is NULL."); return; }

        let land = &*(landscape_ptr as *const PCLandscape);
        let _ = log_validation(&format!("  PCLandscape @ 0x{:08X}", landscape_ptr));

        let expected_vt = rb(va::PC_LANDSCAPE_VTABLE);
        let vt_ok = land.vtable as u32 == expected_vt;
        let _ = log_validation(&format!("  vtable: 0x{:08X} (expected 0x{:08X}) {}",
            land.vtable as u32, expected_vt, if vt_ok { "OK" } else { "MISMATCH" }));

        let _ = log_validation(&format!("  ddgame: 0x{:08X}", land.ddgame as usize));
        let _ = log_validation(&format!("  _unknown_900: 0x{:08X}", land._unknown_900 as u32));
        let _ = log_validation(&format!("  collision_bitmap: 0x{:08X}", land.collision_bitmap as u32));
        let _ = log_validation(&format!("  initialized: {}", land.initialized));

        let primary_count = land.crater_sprites.iter().filter(|&&p| !p.is_null()).count();
        let secondary_count = land.crater_sprites_secondary.iter().filter(|&&p| !p.is_null()).count();
        let _ = log_validation(&format!("  crater_sprites: {}/16 non-null, secondary: {}/16 non-null",
            primary_count, secondary_count));

        let _ = log_validation(&format!("  layer_0: 0x{:08X}", land.layer_0 as u32));
        let _ = log_validation(&format!("  layer_1: 0x{:08X}", land.layer_1 as u32));
        let _ = log_validation(&format!("  layer_terrain: 0x{:08X}", land.layer_terrain as u32));
        let _ = log_validation(&format!("  layer_edges: 0x{:08X}", land.layer_edges as u32));
        let _ = log_validation(&format!("  layer_shadow: 0x{:08X}", land.layer_shadow as u32));
        let _ = log_validation(&format!("  layer_5: 0x{:08X}", land.layer_5 as u32));

        if !land.layer_terrain.is_null() {
            let dgfx = land.layer_terrain as u32;
            let pixel_data = read_u32(dgfx + 0x08);
            let stride = read_u32(dgfx + 0x10);
            let width = read_u32(dgfx + 0x14);
            let height = read_u32(dgfx + 0x18);
            let _ = log_validation(&format!("  layer_terrain DisplayGfx: pixels=0x{:08X} stride={} width={} height={}",
                pixel_data, stride, width, height));
        }

        let _ = log_validation(&format!("  dirty_rect_count: {}", land.dirty_rect_count));
        let _ = log_validation(&format!("  dirty_flag: {}", land.dirty_flag));
        if land.dirty_rect_count > 0 && land.dirty_rect_count <= 256 {
            for i in 0..land.dirty_rect_count.min(5) as usize {
                let r = &land.dirty_rects[i];
                let _ = log_validation(&format!("    rect[{}]: ({},{})..({},{})",
                    i, r.x1, r.y1, r.x2, r.y2));
            }
            if land.dirty_rect_count > 5 {
                let _ = log_validation(&format!("    ... and {} more", land.dirty_rect_count - 5));
            }
        }

        let _ = log_validation(&format!("  _unknown_8ec: 0x{:08X}", land._unknown_8ec));
        let _ = log_validation(&format!("  _unknown_8f0: 0x{:08X}", land._unknown_8f0));
        let _ = log_validation(&format!("  _unknown_8f4: 0x{:08X}", land._unknown_8f4));
        let _ = log_validation(&format!("  resource_handle: 0x{:08X}", land.resource_handle as u32));
        let _ = log_validation(&format!("  level_gfx_handler: 0x{:08X}", land.level_gfx_handler as u32));
        let _ = log_validation(&format!("  water_gfx_handler: 0x{:08X}", land.water_gfx_handler as u32));
        let _ = log_validation(&format!("  visible_bounds: left={} top={} right={} bottom={}",
            land.visible_left, land.visible_top, land.visible_right, land.visible_bottom));

        let level_path = std::ffi::CStr::from_ptr(land.level_dir_path.as_ptr() as *const i8);
        let theme_path = std::ffi::CStr::from_ptr(land.theme_dir_path.as_ptr() as *const i8);
        let _ = log_validation(&format!("  level_dir_path: {:?}", level_path));
        let _ = log_validation(&format!("  theme_dir_path: {:?}", theme_path));

        if !land.ddgame.is_null() {
            let dg = &*land.ddgame;
            let _ = log_validation(&format!("  DDGame level dims: {}x{} (total={})",
                dg.level_width, dg.level_height, dg.level_total_pixels));
        }

        let _ = log_validation("  Vtable slots:");
        let vt_addr = land.vtable as u32;
        for slot in 0..10u32 {
            let entry = read_u32(vt_addr + slot * 4);
            let in_text = is_in_text(entry);
            let _ = log_validation(&format!("    [{}]: 0x{:08X} {}",
                slot, entry, if in_text { "" } else { "(NOT .text)" }));
        }

        let _ = log_validation(&format!("  _unknown_b3c: 0x{:08X}", land._unknown_b3c));

        let _ = log_validation("  _unknown_088 (0x44 bytes):");
        log_hex_dump(&land._unknown_088);
        let _ = log_validation("  _unknown_8d9 (0x13 bytes):");
        log_hex_dump(&land._unknown_8d9);
        let _ = log_validation("  _unknown_8f8 (8 bytes):");
        log_hex_dump(&land._unknown_8f8);
        let _ = log_validation("  _unknown_918 (4 bytes):");
        log_hex_dump(&land._unknown_918);
    }
}

fn log_hex_dump(data: &[u8]) {
    for chunk in data.chunks(16) {
        let hex: String = chunk.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
        let ascii: String = chunk.iter().map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' }).collect();
        let _ = log_validation(&format!("    {} | {}", hex, ascii));
    }
}

// ---------------------------------------------------------------------------
// Public entry point — called from wormkit's run()
// ---------------------------------------------------------------------------

pub fn run() -> Result<(), String> {
    clear_validation_log().map_err(|e| format!("Failed to clear validation log: {e}"))?;

    log_validation("============================================").map_err(|e| e.to_string())?;
    log_validation("  OpenWA Runtime Validator").map_err(|e| e.to_string())?;
    log_validation("  Target: WA.exe 3.8.1 (Steam)").map_err(|e| e.to_string())?;
    log_validation("============================================").map_err(|e| e.to_string())?;

    // rb(IMAGE_BASE) gives us the actual runtime base
    let base = rb(va::IMAGE_BASE);
    let delta = base.wrapping_sub(va::IMAGE_BASE);
    let _ = log_validation(&format!(
        "  Module base: 0x{:08X} (Ghidra base: 0x{:08X}, delta: 0x{:08X})",
        base, va::IMAGE_BASE, delta
    ));

    let mut result = ValidationResult::new();

    validate_addresses(&mut result);
    validate_struct_offsets(&mut result);

    let _ = log_validation("");
    let _ = log_validation("--- Static Validation Summary ---");
    let _ = log_validation(&format!("  {}", result.summary_line()));

    match hooks::install_all() {
        Ok(()) => {}
        Err(e) => { let _ = log_validation(&format!("[ERROR] Hook installation failed: {}", e)); }
    }

    let auto_mode = std::env::var("OPENWA_REPLAY_TEST").is_ok();

    if auto_mode {
        let _ = log_validation("");
        let _ = log_validation("--- Auto-Capture Mode (OPENWA_REPLAY_TEST) ---");
        let _ = log_validation("  Replay fast-forward active — game will exit when replay finishes.");
        let _ = log_validation("  Safety timeout: 120s (will force exit if replay hangs).");
        std::thread::spawn(move || {
            // Restore minimized window so the replay can start.
            // WA.exe won't begin replay playback until the window is visible.
            std::thread::sleep(std::time::Duration::from_secs(2));
            unsafe {
                let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowA(
                    core::ptr::null::<u8>(),
                    b"Worms Armageddon\0".as_ptr(),
                );
                if !hwnd.is_null() {
                    windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, 9); // SW_RESTORE
                    let _ = log_validation("  Window restored (SW_RESTORE).");
                } else {
                    let _ = log_validation("  WARNING: Could not find WA window to restore.");
                }
            }

            // Wait for gameplay to start, then run initial validation
            std::thread::sleep(std::time::Duration::from_secs(3));
            let _ = log_validation("  Running deferred global validation...");
            deferred_global_validation();

            // Wait briefly for replay to start, then dump game state
            // (fast-forward finishes the replay in ~10-15s total, so dump early)
            std::thread::sleep(std::time::Duration::from_secs(3));
            let _ = log_validation("  Running team block dump (8s mark)...");
            dump_team_blocks();
            let _ = log_validation("  Running landscape dump...");
            dump_landscape();

            // Safety timeout: if the game hasn't exited on its own by 120s, force exit.
            // The replay fast-forward should finish well before this.
            std::thread::sleep(std::time::Duration::from_secs(112));
            let _ = log_validation("");
            let _ = log_validation("--- Safety timeout reached (120s), forcing exit ---");
            unsafe {
                windows_sys::Win32::System::Threading::ExitProcess(1);
            }
        });
    } else {
        let _ = log_validation("");
        let _ = log_validation("--- Interactive Mode (deferred polling) ---");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(10));
            deferred_global_validation();
        });
        let _ = log_validation("  Polling thread started (10s delay).");

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            dump_team_blocks();
        });
        let _ = log_validation("  Team block dump thread started (30s delay).");

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(15));
            dump_landscape();
        });
        let _ = log_validation("  Landscape dump thread started (15s delay).");
    }

    Ok(())
}

/// Start the debug hotkey listener thread (F9=team blocks, F10=landscape).
///
/// Always available regardless of `OPENWA_VALIDATE`. Skipped in replay-test
/// mode since auto-capture handles dumps automatically.
pub fn start_hotkeys() {
    if std::env::var("OPENWA_REPLAY_TEST").is_ok() {
        return;
    }

    std::thread::spawn(|| {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        const VK_F9: i32 = 0x78;
        const VK_F10: i32 = 0x79;
        let _ = log_validation("  Hotkey listener started (F9=team blocks, F10=landscape).");
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe {
                if GetAsyncKeyState(VK_F9) & 1 != 0 {
                    dump_team_blocks();
                }
                if GetAsyncKeyState(VK_F10) & 1 != 0 {
                    dump_landscape();
                }
            }
        }
    });
}
