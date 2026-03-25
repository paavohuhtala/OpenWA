//! Runtime validation of openwa-core against live WA.exe memory.
//!
//! Enabled by setting `OPENWA_VALIDATE=1` environment variable.
//! When `OPENWA_REPLAY_TEST=1` is also set, enters auto-capture mode:
//! waits for the replay to finish (up to 120s), runs validation dumps,
//! then calls ExitProcess(0) as a safety net.

mod hooks;

use std::sync::atomic::{AtomicU32, Ordering};

use openwa_core::address::va;
use openwa_core::engine::{DDGame, DDGameWrapper};
use openwa_core::rebase::rb;
use openwa_core::task::{
    CGameTask, CTask, CTaskBfsIter, CTaskMissile, CTaskTeam, CTaskTurnGame, SharedDataTable,
    TurnGameCtx,
};

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
            let _ = log_validation(&format!("[STATIC PASS] {} - {}", name, detail));
        } else {
            self.fail += 1;
            let _ = log_validation(&format!("[STATIC FAIL] {} - {}", name, detail));
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
// Address validation
// ---------------------------------------------------------------------------

fn validate_addresses(result: &mut ValidationResult) {
    let _ = log_validation("");
    let _ = log_validation("--- Address Validation ---");

    // Auto-discover all registered vtables from the address registry
    let vtables: Vec<(&str, u32)> = openwa_core::registry::entries_by_kind(
        openwa_core::registry::AddrKind::Vtable,
    )
    .map(|e| {
        let name = e.class_name.map_or(e.name, |c| c);
        (name, e.va)
    })
    .collect();

    let _ = log_validation("");
    let _ = log_validation(&format!(
        "  Vtable location checks (.rdata range) — {} registered vtables:",
        vtables.len()
    ));
    for &(name, ghidra_addr) in &vtables {
        let addr = rb(ghidra_addr);
        let in_rdata = is_in_rdata(addr);
        result.check(
            &format!("{} location", name),
            in_rdata,
            &format!(
                "0x{:08X} (ghidra 0x{:08X}) {}",
                addr,
                ghidra_addr,
                if in_rdata {
                    "in .rdata"
                } else {
                    "NOT in .rdata"
                }
            ),
        );
    }

    let _ = log_validation("");
    let _ = log_validation("  Vtable first-entry checks (should point to .text):");
    for &(name, ghidra_addr) in &vtables {
        let addr = rb(ghidra_addr);
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
        0x55, 0x53, 0x56, 0x57, 0x83, 0x8B, 0x6A, 0x81, 0xB8, 0x51, 0x52, 0x64, 0x85, 0x8D,
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
        (
            "CGameTask::vt2_HandleMessage",
            va::CGAMETASK_VT2_HANDLE_MESSAGE,
        ),
    ];

    for (name, ghidra_addr) in functions {
        let addr = rb(*ghidra_addr);
        let in_text = is_in_text(addr);
        if !in_text {
            result.check(
                &format!("{} prologue", name),
                false,
                &format!(
                    "0x{:08X} (ghidra 0x{:08X}) not in .text range",
                    addr, ghidra_addr
                ),
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
// Typed vtable slot validation (from #[vtable(...)] metadata)
// ---------------------------------------------------------------------------

fn validate_typed_vtable_slots(result: &mut ValidationResult) {
    let _ = log_validation("");
    let _ = log_validation("--- Typed Vtable Slot Validation (#[vtable(...)]) ---");

    let mut vtable_count = 0;
    let mut slot_count = 0;

    for info in openwa_core::registry::all_vtable_info() {
        if info.ghidra_va == 0 {
            continue;
        }
        vtable_count += 1;
        let vt_runtime = rb(info.ghidra_va);

        for slot in info.slots {
            slot_count += 1;
            unsafe {
                let slot_addr = vt_runtime + slot.index * 4;
                let actual = read_u32(slot_addr);
                let in_text = is_in_text(actual);
                result.check(
                    &format!("{}::{} [slot {}]", info.class_name, slot.name, slot.index),
                    in_text,
                    &format!(
                        "0x{:08X} {}",
                        actual,
                        if in_text { "in .text" } else { "NOT in .text — hooked or corrupt?" }
                    ),
                );
            }
        }
    }

    let _ = log_validation(&format!(
        "  Checked {} named slots across {} typed vtables",
        slot_count, vtable_count
    ));
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
    check_offset!(result, CTask, children_capacity, 0x08);
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
    check_offset!(result, DDGame, bit_grid, 0x380);
    check_offset!(result, DDGame, sprite_regions, 0x46C);
    check_offset!(result, DDGame, coord_list, 0x50C);

    let _ = log_validation("");
    let _ = log_validation("  DDGameWrapper:");
    check_offset!(result, DDGameWrapper, vtable, 0x00);
    check_offset!(result, DDGameWrapper, ddgame, 0x488);
    check_offset!(result, DDGameWrapper, primary_gfx_dir, 0x4C0);
    check_offset!(result, DDGameWrapper, landscape, 0x4CC);
    check_offset!(result, DDGameWrapper, display, 0x4D0);

    let _ = log_validation("");
    let _ = log_validation("  CTaskTeam:");
    check_offset!(result, CTaskTeam, team_index, 0x38);
    check_offset!(result, CTaskTeam, alive_worm_count, 0x48);
    check_offset!(result, CTaskTeam, last_launched_weapon, 0x60);
    check_offset!(result, CTaskTeam, worm_count, 0x218);
    check_offset!(result, CTaskTeam, pos_x, 0x404);
    check_offset!(result, CTaskTeam, pos_y, 0x408);

    let _ = log_validation("");
    let _ = log_validation("  TurnGameCtx (embedded at CTaskTurnGame+0x30):");
    check_offset!(result, TurnGameCtx, land_height, 0x10);
    check_offset!(result, TurnGameCtx, land_height_2, 0x14);
    check_offset!(result, TurnGameCtx, _sentinel_18, 0x18);
    check_offset!(result, TurnGameCtx, _sentinel_28, 0x28);
    check_offset!(result, TurnGameCtx, _sentinel_38, 0x38);
    check_offset!(result, TurnGameCtx, team_count, 0x4C);
    check_offset!(result, TurnGameCtx, _slot_d0, 0xA0);
    check_offset!(result, TurnGameCtx, _hud_textbox_a, 0xA4);
    check_offset!(result, TurnGameCtx, _hud_textbox_b, 0xA8);

    let _ = log_validation("");
    let _ = log_validation("  CTaskMissile:");
    check_offset!(result, CTaskMissile, base, 0x00);
    check_offset!(result, CTaskMissile, slot_id, 0x12C);
    check_offset!(result, CTaskMissile, spawn_params, 0x130);
    check_offset!(result, CTaskMissile, weapon_data, 0x15C);
    check_offset!(result, CTaskMissile, render_data, 0x2D4);
    check_offset!(result, CTaskMissile, launch_speed_raw, 0x3A0);
    check_offset!(result, CTaskMissile, homing_enabled, 0x3A8);
    check_offset!(result, CTaskMissile, direction, 0x3C8);

    let _ = log_validation("  CTaskTurnGame:");
    check_offset!(result, CTaskTurnGame, game_ctx, 0x30);
    check_offset!(result, CTaskTurnGame, worm_active, 0x108);
    check_offset!(result, CTaskTurnGame, current_team, 0x12C);
    check_offset!(result, CTaskTurnGame, current_worm, 0x130);
    check_offset!(result, CTaskTurnGame, arena_team, 0x134);
    check_offset!(result, CTaskTurnGame, arena_worm, 0x138);
    check_offset!(result, CTaskTurnGame, turn_ended, 0x150);
    check_offset!(result, CTaskTurnGame, no_time_limit, 0x154);
    check_offset!(result, CTaskTurnGame, retreat_timer, 0x178);
    check_offset!(result, CTaskTurnGame, retreat_time_max, 0x17C);
    check_offset!(result, CTaskTurnGame, idle_timer, 0x184);
    check_offset!(result, CTaskTurnGame, turn_timer_display, 0x188);
    check_offset!(result, CTaskTurnGame, turn_timer, 0x18C);
    check_offset!(result, CTaskTurnGame, active_worm_frames, 0x2D4);
    check_offset!(result, CTaskTurnGame, retreat_frames, 0x2D8);
    check_offset!(result, CTaskTurnGame, _timer_scale, 0x2DC);
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
                    "DDGame.bit_grid vtable",
                    tsm_vt == expected,
                    &format!("expected 0x{:08X}, got 0x{:08X}", expected, tsm_vt),
                );
            } else {
                result.check("DDGame.bit_grid", false, "NULL pointer");
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
    use openwa_core::engine::ddgame::{offsets, TeamArenaRef};

    let dump_num = DUMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let _ = log_validation("");
    let _ = log_validation(&format!("--- Team Block Dump #{} ---", dump_num));

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }

        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        let _ = log_validation(&format!("  DDGame = 0x{:08X}", ddgame_ptr));

        let arena_base = ddgame_ptr + offsets::TEAM_ARENA_STATE as u32;
        let arena = TeamArenaRef::from_raw(arena_base);
        let tws = arena.state();
        let tws_base = arena_base as *const u8;
        let _ = log_validation(&format!(
            "  team_count = {} (TeamArenaState.team_count)",
            tws.team_count
        ));

        let blocks = arena.blocks();
        let blocks_addr = blocks as u32;
        let _ = log_validation(&format!(
            "  blocks_base = 0x{:08X} (DDGame+0x{:X})",
            blocks_addr,
            blocks_addr - ddgame_ptr
        ));

        let mut result = ValidationResult::new();

        let expected_blocks = ddgame_ptr + offsets::TEAM_BLOCKS as u32;
        result.check(
            "ARENA_TO_BLOCKS derivation",
            blocks_addr == expected_blocks,
            &format!(
                "got 0x{:08X}, expected 0x{:08X}",
                blocks_addr, expected_blocks
            ),
        );

        let num_blocks = (tws.team_count as u32 + 1).max(3).min(7);
        for b in 0..num_blocks {
            let block = &*blocks.add(b as usize);
            let _ = log_validation(&format!(
                "\n  === Block {} (0x{:08X}) ===",
                b,
                blocks_addr + b * 0x51C
            ));

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

                let _ = log_validation(&format!(
                    "  header: worm_count={}, eliminated={}, alliance(entry_ptr+4)={}",
                    worm_count, eliminated, raw_alliance
                ));

                for w in 0..8usize {
                    let worm = if w == 0 {
                        &*block.header.worm
                    } else {
                        &block.worms[w - 1]
                    };
                    let active = worm.active_flag;
                    let name_bytes = &worm.name;
                    let name_len = name_bytes
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(name_bytes.len());
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
        let _ = log_validation(&format!(
            "  arena_base = 0x{:08X} (DDGame + 0x{:X})",
            arena_base,
            offsets::TEAM_ARENA_STATE
        ));

        // Dump first 16 bytes at each team_index * 0x51C stride (slots -2 to 7)
        for slot in -2i32..8 {
            let offset = slot as isize * 0x51C_isize;
            let ptr = tws_base.offset(offset);
            let mut hex = String::new();
            for i in 0..16usize {
                if i > 0 && i % 4 == 0 {
                    hex.push(' ');
                }
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
                offset,
                abs_addr,
                label,
                hex,
                *(ptr.add(4) as *const i32)
            ));
        }

        // Dump the region around 0x1EA0-0x1EC0 (team entries end / weapon_slots start)
        let _ = log_validation("\n  --- Region 0x1EA0..0x1EC0 (boundary area) ---");
        for row in (0x1EA0..0x1EC0).step_by(16) {
            let ptr = tws_base.add(row);
            let mut hex = String::new();
            for i in 0..16usize {
                if i > 0 && i % 4 == 0 {
                    hex.push(' ');
                }
                hex.push_str(&format!("{:02X}", *ptr.add(i)));
            }
            // Label known offsets
            let label = match row {
                0x1EA0 => " (end of teams[5])",
                0x1EB0 => " (team_count + weapon_slots[0..3])",
                _ => "",
            };
            let _ = log_validation(&format!("  +0x{:04X}: {}{}", row, hex, label));
        }

        // Dump team_count and first few weapon_slots
        let _ = log_validation(&format!(
            "\n  team_count (+0x1EB0) = {}",
            *(tws_base.add(0x1EB0) as *const i32)
        ));
        let _ = log_validation(&format!(
            "  weapon_slots[0..4] (+0x1EB4) = [{}, {}, {}, {}]",
            *(tws_base.add(0x1EB4) as *const i32),
            *(tws_base.add(0x1EB8) as *const i32),
            *(tws_base.add(0x1EBC) as *const i32),
            *(tws_base.add(0x1EC0) as *const i32)
        ));

        // Dump game_mode_flag and game_phase for context
        let _ = log_validation(&format!(
            "  game_mode_flag (+0x2C0C) = {}",
            *(tws_base.add(0x2C0C) as *const i32)
        ));
        let _ = log_validation(&format!(
            "  game_phase (+0x2C28) = {}",
            *(tws_base.add(0x2C28) as *const i32)
        ));
    }
}

// ---------------------------------------------------------------------------
// PCLandscape dump
// ---------------------------------------------------------------------------

fn dump_landscape() {
    use openwa_core::render::PCLandscape;

    let dump_num = DUMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let _ = log_validation("");
    let _ = log_validation(&format!("--- PCLandscape Dump #{} ---", dump_num));

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }

        let landscape_ptr = read_u32(wrapper_addr + 0x4CC);
        if landscape_ptr == 0 {
            let _ = log_validation("  PCLandscape is NULL.");
            return;
        }

        let land = &*(landscape_ptr as *const PCLandscape);
        let _ = log_validation(&format!("  PCLandscape @ 0x{:08X}", landscape_ptr));

        let expected_vt = rb(va::PC_LANDSCAPE_VTABLE);
        let vt_ok = land.vtable as u32 == expected_vt;
        let _ = log_validation(&format!(
            "  vtable: 0x{:08X} (expected 0x{:08X}) {}",
            land.vtable as u32,
            expected_vt,
            if vt_ok { "OK" } else { "MISMATCH" }
        ));

        let _ = log_validation(&format!("  ddgame: 0x{:08X}", land.ddgame as usize));
        let _ = log_validation(&format!(
            "  _unknown_900: 0x{:08X}",
            land._unknown_900 as u32
        ));
        let _ = log_validation(&format!(
            "  collision_bitmap: 0x{:08X}",
            land.collision_bitmap as u32
        ));
        let _ = log_validation(&format!("  initialized: {}", land.initialized));

        let primary_count = land
            .crater_sprites
            .iter()
            .filter(|&&p| !p.is_null())
            .count();
        let secondary_count = land
            .crater_sprites_secondary
            .iter()
            .filter(|&&p| !p.is_null())
            .count();
        let _ = log_validation(&format!(
            "  crater_sprites: {}/16 non-null, secondary: {}/16 non-null",
            primary_count, secondary_count
        ));

        let _ = log_validation(&format!("  layer_0: 0x{:08X}", land.layer_0 as u32));
        let _ = log_validation(&format!("  layer_1: 0x{:08X}", land.layer_1 as u32));
        let _ = log_validation(&format!(
            "  layer_terrain: 0x{:08X}",
            land.layer_terrain as u32
        ));
        let _ = log_validation(&format!("  layer_edges: 0x{:08X}", land.layer_edges as u32));
        let _ = log_validation(&format!(
            "  layer_shadow: 0x{:08X}",
            land.layer_shadow as u32
        ));
        let _ = log_validation(&format!("  layer_5: 0x{:08X}", land.layer_5 as u32));

        if !land.layer_terrain.is_null() {
            let dgfx = land.layer_terrain as u32;
            let pixel_data = read_u32(dgfx + 0x08);
            let stride = read_u32(dgfx + 0x10);
            let width = read_u32(dgfx + 0x14);
            let height = read_u32(dgfx + 0x18);
            let _ = log_validation(&format!(
                "  layer_terrain DisplayGfx: pixels=0x{:08X} stride={} width={} height={}",
                pixel_data, stride, width, height
            ));
        }

        let _ = log_validation(&format!("  dirty_rect_count: {}", land.dirty_rect_count));
        let _ = log_validation(&format!("  dirty_flag: {}", land.dirty_flag));
        if land.dirty_rect_count > 0 && land.dirty_rect_count <= 256 {
            for i in 0..land.dirty_rect_count.min(5) as usize {
                let r = &land.dirty_rects[i];
                let _ = log_validation(&format!(
                    "    rect[{}]: ({},{})..({},{})",
                    i, r.x1, r.y1, r.x2, r.y2
                ));
            }
            if land.dirty_rect_count > 5 {
                let _ = log_validation(&format!("    ... and {} more", land.dirty_rect_count - 5));
            }
        }

        let _ = log_validation(&format!("  _unknown_8ec: 0x{:08X}", land._unknown_8ec));
        let _ = log_validation(&format!("  _unknown_8f0: 0x{:08X}", land._unknown_8f0));
        let _ = log_validation(&format!("  _unknown_8f4: 0x{:08X}", land._unknown_8f4));
        let _ = log_validation(&format!(
            "  resource_handle: 0x{:08X}",
            land.resource_handle as u32
        ));
        let _ = log_validation(&format!(
            "  level_gfx_dir: 0x{:08X}",
            land.level_gfx_dir as u32
        ));
        let _ = log_validation(&format!(
            "  water_gfx_dir: 0x{:08X}",
            land.water_gfx_dir as u32
        ));
        let _ = log_validation(&format!(
            "  visible_bounds: left={} top={} right={} bottom={}",
            land.visible_left, land.visible_top, land.visible_right, land.visible_bottom
        ));

        let level_path = std::ffi::CStr::from_ptr(land.level_dir_path.as_ptr() as *const i8);
        let theme_path = std::ffi::CStr::from_ptr(land.theme_dir_path.as_ptr() as *const i8);
        let _ = log_validation(&format!("  level_dir_path: {:?}", level_path));
        let _ = log_validation(&format!("  theme_dir_path: {:?}", theme_path));

        if !land.ddgame.is_null() {
            let dg = &*land.ddgame;
            let _ = log_validation(&format!(
                "  DDGame level dims: {}x{} (total={})",
                dg.level_width, dg.level_height, dg.level_total_pixels
            ));
        }

        let _ = log_validation("  Vtable slots:");
        let vt_addr = land.vtable as u32;
        for slot in 0..10u32 {
            let entry = read_u32(vt_addr + slot * 4);
            let in_text = is_in_text(entry);
            let _ = log_validation(&format!(
                "    [{}]: 0x{:08X} {}",
                slot,
                entry,
                if in_text { "" } else { "(NOT .text)" }
            ));
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
        let hex: String = chunk
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7F).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        let _ = log_validation(&format!("    {} | {}", hex, ascii));
    }
}

// ---------------------------------------------------------------------------
// Entity census — enumerate all registered task types via shared_data table
// ---------------------------------------------------------------------------

fn dump_entity_census() {
    let _ = log_validation("");
    let _ = log_validation("--- Entity Census (SharedData hash table) ---");

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }
        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }
        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        let task_land_ptr = read_u32(ddgame_ptr + 0x54C);
        if task_land_ptr == 0 {
            let _ = log_validation("  CTaskLand NULL — game not loaded yet.");
            return;
        }

        let task_land = task_land_ptr as *const CTask;
        let shared_data_ptr = (*task_land).shared_data;
        if shared_data_ptr.is_null() {
            let _ = log_validation("  shared_data is NULL.");
            return;
        }

        let table = SharedDataTable::from_ptr(shared_data_ptr);

        // Use the global address registry for vtable → class name lookup

        // Collect (ghidra_va, entity_ptr) for every node.
        let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);
        let mut entries: Vec<(u32, u32)> = Vec::new();
        let mut total = 0u32;

        for node in table.iter() {
            let entity = (*node).entity;
            if entity.is_null() {
                continue;
            }
            let vtable_runtime = read_u32(entity as u32);
            if !is_in_rdata(vtable_runtime) {
                continue;
            }
            let vtable_ghidra = vtable_runtime.wrapping_sub(delta);
            entries.push((vtable_ghidra, entity as u32));
            total += 1;
        }

        let _ = log_validation(&format!("  Total entities: {}", total));

        // Group by vtable, sort by count descending.
        let mut groups: Vec<(u32, Vec<u32>)> = Vec::new();
        for (vt, ptr) in &entries {
            if let Some(g) = groups.iter_mut().find(|(v, _)| *v == *vt) {
                g.1.push(*ptr);
            } else {
                groups.push((*vt, vec![*ptr]));
            }
        }
        groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        for (vt_ghidra, ptrs) in &groups {
            let name = openwa_core::registry::vtable_class_name(*vt_ghidra)
                .unwrap_or("UNKNOWN");
            let _ = log_validation(&format!(
                "  {:>3}x  {:<20}  (vtable 0x{:08X})",
                ptrs.len(),
                name,
                vt_ghidra
            ));
            for &ptr in ptrs.iter().take(8) {
                use openwa_core::task::CTaskWorm;
                if *vt_ghidra == va::CTASK_WORM_VTABLE {
                    let w = &*(ptr as *const CTaskWorm);
                    let nlen = w
                        .worm_name
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(w.worm_name.len());
                    let wname = core::str::from_utf8(&w.worm_name[..nlen]).unwrap_or("?");
                    let _ = log_validation(&format!(
                        "       @ 0x{:08X}  team={} idx={} state=0x{:02X} name=\"{}\"",
                        ptr,
                        w.team_index,
                        w.worm_index,
                        w.state(),
                        wname
                    ));
                } else {
                    let a = read_u32(ptr + 4);
                    let b = read_u32(ptr + 8);
                    let c = read_u32(ptr + 12);
                    let d = read_u32(ptr + 16);
                    let _ = log_validation(&format!(
                        "       @ 0x{:08X}  [+4:{:08X} +8:{:08X} +c:{:08X} +10:{:08X}]",
                        ptr, a, b, c, d
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CTaskWorm entity dump
// ---------------------------------------------------------------------------

fn dump_worm_tasks() {
    use openwa_core::task::CTaskWorm;

    let _ = log_validation("");
    let _ = log_validation("--- CTaskWorm Entity Dump ---");

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }

        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }

        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        // DDGame+0x54C = CTaskLand* — always present once map is loaded.
        let task_land_ptr = read_u32(ddgame_ptr + 0x54C);
        if task_land_ptr == 0 {
            let _ = log_validation("  CTaskLand is NULL — game not fully loaded yet.");
            return;
        }
        let _ = log_validation(&format!("  DDGame @ 0x{:08X}", ddgame_ptr));
        let _ = log_validation(&format!("  CTaskLand @ 0x{:08X}", task_land_ptr));

        // CTask.shared_data at +0x24 — root tasks own 0x420 bytes with a
        // 256-bucket hash table (FUN_005406a0). All tasks in the same game
        // tree share this block via the inherited shared_data pointer.
        let task_land = task_land_ptr as *const CTask;
        let shared_data_ptr = (*task_land).shared_data;
        if shared_data_ptr.is_null() {
            let _ = log_validation("  shared_data is NULL.");
            return;
        }
        let _ = log_validation(&format!("  shared_data @ 0x{:08X}", shared_data_ptr as u32));

        let table = SharedDataTable::from_ptr(shared_data_ptr);

        let expected_vtable = rb(va::CTASK_WORM_VTABLE) as *const u8;
        let _ = log_validation(&format!(
            "  Expected CTaskWorm vtable: 0x{:08X}",
            expected_vtable as u32
        ));

        let mut worm_count = 0u32;

        for node in table.iter() {
            let candidate = (*node).entity;
            if candidate.is_null() {
                continue;
            }
            // Vtable is the first pointer in the object; must be in .rdata.
            let vtable = read_u32(candidate as u32) as *const u8;
            if !is_in_rdata(vtable as u32) || vtable != expected_vtable {
                continue;
            }

            worm_count += 1;
            let worm = &*(candidate as *const CTaskWorm);

            let state = worm.state();
            let pos_x_f = worm.base.pos_x.0 as f32 / 65536.0;
            let pos_y_f = worm.base.pos_y.0 as f32 / 65536.0;
            let team_idx = worm.team_index;
            let worm_idx = worm.worm_index;
            let slot_id = worm.slot_id;

            let name_len = worm
                .worm_name
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(worm.worm_name.len());
            let name = core::str::from_utf8(&worm.worm_name[..name_len]).unwrap_or("?");

            let _ = log_validation(&format!(
                "  worm#{} @ 0x{:08X}: team={} worm_idx={} slot={} state=0x{:04X} pos=({:.1},{:.1}) name=\"{}\"",
                worm_count, candidate as u32,
                team_idx, worm_idx, slot_id,
                state, pos_x_f, pos_y_f, name
            ));

            // Cross-validate against WormEntry in TeamArena.
            // WormEntry = DDGame+0x4090 + team*0x51C + worm_idx*0x9C
            //   +0x00: state, +0x78: name (17 bytes)
            if team_idx < 6 && worm_idx < 8 {
                let entry_addr = ddgame_ptr + 0x4090 + team_idx * 0x51C + worm_idx * 0x9C;
                let entry_state = read_u32(entry_addr);
                let mut entry_name = [0u8; 17];
                for i in 0..17usize {
                    entry_name[i] = read_u8(entry_addr + 0x78 + i as u32);
                    if entry_name[i] == 0 {
                        break;
                    }
                }
                let entry_name_len = entry_name.iter().position(|&c| c == 0).unwrap_or(17);
                let entry_name_str =
                    core::str::from_utf8(&entry_name[..entry_name_len]).unwrap_or("?");

                let state_msg = if state == entry_state {
                    "state:OK".to_string()
                } else {
                    format!("state:MISMATCH(entry=0x{:04X})", entry_state)
                };
                let name_msg = if name == entry_name_str {
                    "name:OK".to_string()
                } else {
                    format!("name:MISMATCH(entry=\"{}\")", entry_name_str)
                };
                let _ = log_validation(&format!(
                    "    xcheck WormEntry[{},{}] @ 0x{:08X}: {} {}",
                    team_idx, worm_idx, entry_addr, state_msg, name_msg
                ));
            }
        }

        let _ = log_validation(&format!("  Total CTaskWorm entities found: {}", worm_count));
    }
}

// ---------------------------------------------------------------------------
// CTaskTurnGame dump
// ---------------------------------------------------------------------------

fn dump_turngame() {
    use crate::replacements::input::dump_region;
    use openwa_core::task::CTaskTurnGame;

    let _ = log_validation("");
    let _ = log_validation("--- CTaskTurnGame Dump ---");

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }
        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }
        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        // Find CTaskTurnGame via the shared_data entity table.
        let task_land_ptr = read_u32(ddgame_ptr + 0x54C);
        if task_land_ptr == 0 {
            let _ = log_validation("  CTaskLand NULL — skipping.");
            return;
        }
        let task_land = task_land_ptr as *const openwa_core::task::CTask;
        let shared_data_ptr = (*task_land).shared_data;
        if shared_data_ptr.is_null() {
            let _ = log_validation("  shared_data NULL.");
            return;
        }

        let table = SharedDataTable::from_ptr(shared_data_ptr);
        let expected_vt = rb(va::CTASK_TURN_GAME_VTABLE);
        let mut tg_ptr: u32 = 0;
        for node in table.iter() {
            let entity = (*node).entity;
            if entity.is_null() {
                continue;
            }
            if !is_in_rdata(read_u32(entity as u32)) {
                continue;
            }
            if read_u32(entity as u32) == expected_vt {
                tg_ptr = entity as u32;
                break;
            }
        }

        if tg_ptr == 0 {
            let _ = log_validation("  CTaskTurnGame not found in entity table.");
            return;
        }

        let _ = log_validation(&format!("  CTaskTurnGame @ 0x{:08X}", tg_ptr));

        // Print known named fields for cross-validation.
        let tg = &*(tg_ptr as *const CTaskTurnGame);
        let ctx = &tg.game_ctx;
        let _ = log_validation("  TurnGameCtx fields:");
        let _ = log_validation(&format!(
            "    +0x40 land_height   = {} ({:.4})",
            ctx.land_height.0,
            ctx.land_height.to_f32()
        ));
        let _ = log_validation(&format!(
            "    +0x44 land_height_2 = {} ({:.4})",
            ctx.land_height_2.0,
            ctx.land_height_2.to_f32()
        ));
        let _ = log_validation(&format!("    +0x48 _sentinel_18  = {}", ctx._sentinel_18));
        let _ = log_validation(&format!("    +0x58 _sentinel_28  = {}", ctx._sentinel_28));
        let _ = log_validation(&format!("    +0x68 _sentinel_38  = {}", ctx._sentinel_38));
        let _ = log_validation(&format!("    +0x7C team_count    = {}", ctx.team_count));
        let _ = log_validation(&format!("    +0xD0 _slot_d0      = {}", ctx._slot_d0));
        let _ = log_validation(&format!(
            "    +0xD4 _hud_textbox_a = 0x{:08X}",
            ctx._hud_textbox_a
        ));
        let _ = log_validation(&format!(
            "    +0xD8 _hud_textbox_b = 0x{:08X}",
            ctx._hud_textbox_b
        ));
        let _ = log_validation("  Known fields:");
        let _ = log_validation(&format!(
            "    +0x108 worm_active        = {}",
            tg.worm_active
        ));
        let _ = log_validation(&format!(
            "    +0x12C current_team       = {} (1-based, 0=none)",
            tg.current_team
        ));
        let _ = log_validation(&format!(
            "    +0x130 current_worm       = {} (0-based)",
            tg.current_worm
        ));
        let _ = log_validation(&format!(
            "    +0x134 arena_team         = {}",
            tg.arena_team
        ));
        let _ = log_validation(&format!(
            "    +0x138 arena_worm         = {}",
            tg.arena_worm
        ));
        let _ = log_validation(&format!(
            "    +0x150 turn_ended         = {}",
            tg.turn_ended
        ));
        let _ = log_validation(&format!(
            "    +0x154 no_time_limit      = {}",
            tg.no_time_limit
        ));
        let _ = log_validation(&format!(
            "    +0x178 retreat_timer      = {} ms",
            tg.retreat_timer
        ));
        let _ = log_validation(&format!(
            "    +0x17C retreat_time_max   = {} ms",
            tg.retreat_time_max
        ));
        let _ = log_validation(&format!(
            "    +0x184 idle_timer         = {} ms",
            tg.idle_timer
        ));
        let _ = log_validation(&format!(
            "    +0x188 turn_timer_display = {} ms",
            tg.turn_timer_display
        ));
        let _ = log_validation(&format!(
            "    +0x18C turn_timer         = {} ms",
            tg.turn_timer
        ));
        let _ = log_validation(&format!(
            "    +0x2D4 active_worm_frames = {}",
            tg.active_worm_frames
        ));
        let _ = log_validation(&format!(
            "    +0x2D8 retreat_frames     = {}",
            tg.retreat_frames
        ));
        let _ = log_validation(&format!(
            "    +0x2DC _timer_scale       = {}",
            tg._timer_scale
        ));

        // Full memory dump — classify every DWORD to discover unknown fields.
        // Dump in chunks to keep log lines manageable.
        let base = tg_ptr as *const u8;
        dump_region(base, 0x00, 0x30, "CTaskTurnGame"); // CTask base
        dump_region(base, 0x30, 0x38, "TurnGameCtx"); // 0x30..0x67 (vtable + sentinels)
        dump_region(base, 0x68, 0x74, "TurnGameCtx"); // 0x68..0xDB (team_count + unknowns)
        dump_region(base, 0xDC, 0x80, "CTaskTurnGame"); // 0xDC..0x15B
        dump_region(base, 0x15C, 0x80, "CTaskTurnGame"); // 0x15C..0x1DB
        dump_region(base, 0x1DC, 0x80, "CTaskTurnGame"); // 0x1DC..0x25B
        dump_region(base, 0x25C, 0x84, "CTaskTurnGame"); // 0x25C..0x2DF
    }
}

// ---------------------------------------------------------------------------
// CTask children sub-struct live dump
// ---------------------------------------------------------------------------
//
// We need to confirm the layout of the four fields at CTask+0x08..+0x17:
//   +0x08  children_capacity  (u32, starts 0x10)
//   +0x0C  children_dirty     (u32, flag)
//   +0x10  children_watermark (u32, insertion counter)
//   +0x14  children_data      (*mut u8, array pointer)
//
// We dump the raw DWORDs at these offsets for a few representative tasks so
// we can verify against what WA actually stores at runtime.

fn dump_ctask_children() {
    use crate::replacements::input::dump_region;

    let _ = log_validation("");
    let _ = log_validation("--- CTask children sub-struct dump ---");

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session.");
            return;
        }
        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }
        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        let task_land_ptr = read_u32(ddgame_ptr + 0x54C);
        if task_land_ptr == 0 {
            let _ = log_validation("  CTaskLand NULL.");
            return;
        }

        // Walk up to root (CTaskTurnGame)
        let mut cursor = task_land_ptr;
        for _ in 0..10 {
            let parent = read_u32(cursor + 0x04);
            if parent == 0 {
                break;
            }
            cursor = parent;
        }
        let root = cursor;

        // Print the children sub-struct fields for root, then walk its first
        // child (CTaskTeam level) and each of its children (CTaskFilter level).
        let print_task_children = |label: &str, addr: u32| {
            if addr == 0 {
                return;
            }
            let cap = read_u32(addr + 0x08);
            let dirty = read_u32(addr + 0x0C);
            let wmark = read_u32(addr + 0x10);
            let data = read_u32(addr + 0x14);
            let _ = log_validation(&format!(
                "  {}  @ 0x{:08X}  cap={}  dirty={}  watermark={}  data=0x{:08X}",
                label, addr, cap, dirty, wmark, data
            ));
            // Raw dump of the 0x30-byte CTask base so we can cross-check all fields
            dump_region(addr as *const u8, 0x00, 0x30, "CTask");
        };

        print_task_children("root (CTaskTurnGame)", root);

        // First two children of root (CTaskTeam instances)
        let root_watermark = read_u32(root + 0x10) as usize;
        let root_data = read_u32(root + 0x14);
        let mut teams_found = 0;
        for i in 0..root_watermark.min(64) {
            let child = read_u32(root_data + i as u32 * 4);
            if child == 0 {
                continue;
            }
            print_task_children(&format!("  child[{}] (CTaskTeam?)", i), child);
            // And print the first few children of this child (CTaskFilter level)
            let child_wmark = read_u32(child + 0x10) as usize;
            let child_data = read_u32(child + 0x14);
            for j in 0..child_wmark.min(16) {
                let gc = read_u32(child_data + j as u32 * 4);
                if gc == 0 {
                    continue;
                }
                print_task_children(&format!("    grandchild[{}] (CTaskFilter?)", j), gc);
            }
            teams_found += 1;
            if teams_found >= 2 {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CTaskTeam entity dump
// ---------------------------------------------------------------------------

fn dump_ctaskteam_entities() {
    use crate::replacements::input::dump_region;
    use openwa_core::task::CTaskTeam;

    let _ = log_validation("");
    let _ = log_validation("--- CTaskTeam Entity Dump ---");

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }
        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }
        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        let task_land_ptr = read_u32(ddgame_ptr + 0x54C);
        if task_land_ptr == 0 {
            let _ = log_validation("  CTaskLand NULL — skipping.");
            return;
        }
        let task_land = task_land_ptr as *const openwa_core::task::CTask;
        let shared_data_ptr = (*task_land).shared_data;
        if shared_data_ptr.is_null() {
            let _ = log_validation("  shared_data NULL.");
            return;
        }

        let table = SharedDataTable::from_ptr(shared_data_ptr);
        let expected_vt = rb(va::CTASK_TEAM_VTABLE);
        let mut found = 0u32;

        for node in table.iter() {
            let entity = (*node).entity;
            if entity.is_null() {
                continue;
            }
            if !is_in_rdata(read_u32(entity as u32)) {
                continue;
            }
            if read_u32(entity as u32) != expected_vt {
                continue;
            }

            let addr = entity as u32;
            let team = &*(addr as *const CTaskTeam);
            let _ = log_validation(&format!(
                "  CTaskTeam[{}] @ 0x{:08X}  team_index={}  worm_count={}  active={}  last_weapon={}  pos=({:.2},{:.2})",
                found, addr, team.team_index, team.worm_count,
                team.alive_worm_count, team.last_launched_weapon,
                team.pos_x.to_f32(), team.pos_y.to_f32()
            ));

            // Full memory dump in chunks — classify every DWORD.
            let base = addr as *const u8;
            dump_region(base, 0x000, 0x30, "CTaskTeam"); // CTask base
            dump_region(base, 0x030, 0x58, "CTaskTeam"); // 0x30..0x87 (secondary vtable, unknowns)
            dump_region(base, 0x088, 0x90, "CTaskTeam"); // 0x88..0x117 (item_slots start, worm_count region)
            dump_region(base, 0x118, 0x100, "CTaskTeam"); // 0x118..0x217 (item_slots end)
            dump_region(base, 0x218, 0x80, "CTaskTeam"); // 0x218..0x297 (worm_count + unknowns)
            dump_region(base, 0x298, 0x80, "CTaskTeam"); // 0x298..0x317
            dump_region(base, 0x318, 0x80, "CTaskTeam"); // 0x318..0x397
            dump_region(base, 0x398, 0x68, "CTaskTeam"); // 0x398..0x3FF
            dump_region(base, 0x400, 0x60, "CTaskTeam"); // 0x400..0x45F

            found += 1;
        }

        let _ = log_validation(&format!("  Total CTaskTeam entities: {}", found));
    }
}

// ---------------------------------------------------------------------------
// CTaskMissile live dump
// ---------------------------------------------------------------------------
//
// CTaskMissile is NOT registered in the SharedData entity table — it is a
// transient child of a CTaskFilter node. We walk the full task tree from the
// root (CTaskTurnGame) up to 4 levels deep, checking every child's vtable.
// Fire a weapon, then press F8.

/// Hex dump of one CTaskMissile to the validation log.
/// Each non-zero DWORD is printed as: +0xOFFSET: 0xVALUE
/// Sections are labelled so the output can be cross-referenced with the struct.
unsafe fn dump_missile_raw(ptr: u32) {
    let base = ptr as *const u32;
    let sections: &[(usize, usize, &str)] = &[
        (0x000, 0x030, "CTask base"),
        (0x030, 0x084, "CGameTask subclass_data"),
        (0x084, 0x098, "CGameTask pos/speed (0x84–0x97)"),
        (0x098, 0x0FC, "CGameTask _unknown_98"),
        (0x0FC, 0x130, "CTaskMissile 0xFC–0x12F"),
        (0x130, 0x15C, "spawn_params"),
        (0x15C, 0x2D4, "weapon_data"),
        (0x2D4, 0x37C, "render_data"),
        (
            0x37C,
            0x41C,
            "_unknown_37c / launch_speed / homing / direction",
        ),
    ];
    for &(start, end, label) in sections {
        let _ = log_validation(&format!(
            "  -- {} (0x{:03X}..0x{:03X}) --",
            label, start, end
        ));
        let dwords = (end - start) / 4;
        for i in 0..dwords {
            let off = start + i * 4;
            let val = *base.add(off / 4);
            if val != 0 {
                let _ = log_validation(&format!("    +0x{:03X}: 0x{:08X}  ({})", off, val, val));
            }
        }
    }
}

fn dump_missile_tasks() {
    let _ = log_validation("");
    let _ = log_validation("--- CTaskMissile Entity Dump ---");

    unsafe {
        let session_ptr = read_u32(rb(va::G_GAME_SESSION));
        if session_ptr == 0 {
            let _ = log_validation("  No game session — skipping.");
            return;
        }
        let wrapper_addr = read_u32(session_ptr + 0xA0);
        if wrapper_addr == 0 {
            let _ = log_validation("  No DDGameWrapper.");
            return;
        }
        let ddgame_ptr = read_u32(wrapper_addr + 0x488);
        if ddgame_ptr == 0 {
            let _ = log_validation("  No DDGame.");
            return;
        }

        // Walk up from CTaskLand to root (CTaskTurnGame) via parent links.
        let task_land_ptr = read_u32(ddgame_ptr + 0x54C);
        if task_land_ptr == 0 {
            let _ = log_validation("  CTaskLand NULL — game not loaded.");
            return;
        }
        let mut root = task_land_ptr;
        for _ in 0..10 {
            let parent = read_u32(root + 0x04);
            if parent == 0 {
                break;
            }
            root = parent;
        }

        let expected_vt = rb(va::CTASK_MISSILE_VTABLE);
        let _ = log_validation(&format!("  Root (CTaskTurnGame) @ 0x{:08X}", root));
        let _ = log_validation(&format!(
            "  Expected CTaskMissile vtable: 0x{:08X}",
            expected_vt
        ));

        let mut found = 0u32;
        let mut first_ptr: u32 = 0;
        let mut scanned = 0usize;

        for task_ptr in CTaskBfsIter::new(root as *const CTask) {
            let node = task_ptr as u32;
            scanned += 1;

            let vt = read_u32(node);
            if vt == expected_vt {
                found += 1;
                if first_ptr == 0 {
                    first_ptr = node;
                }

                let m = &*(node as *const CTaskMissile);
                let pos_x = m.base.pos_x.to_f32();
                let pos_y = m.base.pos_y.to_f32();
                let spd_x = m.base.speed_x.to_f32();
                let spd_y = m.base.speed_y.to_f32();
                let _ = log_validation(&format!(
                    "  missile#{} @ 0x{:08X}  slot={}  type={:?}  homing={}  dir={}",
                    found,
                    node,
                    m.slot_id,
                    m.missile_type(),
                    m.homing_enabled,
                    m.direction
                ));
                let _ = log_validation(&format!(
                    "    pos=({:.2},{:.2})  speed=({:.3},{:.3})  cursor=({:.1},{:.1})",
                    pos_x,
                    pos_y,
                    spd_x,
                    spd_y,
                    m.cursor_x().to_f32(),
                    m.cursor_y().to_f32()
                ));
                let _ = log_validation(&format!(
                    "    spawn_params: owner={} pellet={}  spawn=({:.1},{:.1})",
                    m.spawn_params[0],
                    m.spawn_params[8],
                    m.spawn_x().to_f32(),
                    m.spawn_y().to_f32()
                ));
                let _ = log_validation(&format!(
                    "    weapon_data[0..10]: {:?}",
                    &m.weapon_data[..10]
                ));
                let _ = log_validation(&format!(
                    "    render_data[0x15..0x1A]: {:?}  (timer={})",
                    &m.render_data[0x15..0x1A],
                    m.render_data[0x19]
                ));
            }
        }

        let _ = log_validation(&format!(
            "  Total CTaskMissile found: {} (scanned {} nodes)",
            found, scanned
        ));

        if found == 0 {
            let _ = log_validation("  No missiles in flight — fire a weapon first, then press F8.");
            return;
        }

        // Full raw dump of the first missile to cross-validate struct layout.
        let _ = log_validation(&format!(
            "\n  === Raw dump: CTaskMissile @ 0x{:08X} ===",
            first_ptr
        ));
        dump_missile_raw(first_ptr);
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
        base,
        va::IMAGE_BASE,
        delta
    ));

    let mut result = ValidationResult::new();

    validate_addresses(&mut result);
    validate_typed_vtable_slots(&mut result);
    validate_struct_offsets(&mut result);

    let _ = log_validation("");
    let _ = log_validation("--- Static Checks (vtables, struct offsets, prologues) ---");
    let _ = log_validation(&format!("  {}", result.summary_line()));

    match hooks::install_all() {
        Ok(()) => {}
        Err(e) => {
            let _ = log_validation(&format!("[ERROR] Hook installation failed: {}", e));
        }
    }

    let auto_mode = std::env::var("OPENWA_REPLAY_TEST").is_ok();

    if auto_mode {
        let _ = log_validation("");
        let _ = log_validation("--- Auto-Capture Mode (OPENWA_REPLAY_TEST) ---");
        let _ =
            log_validation("  Replay fast-forward active — game will exit when replay finishes.");
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
            let _ = log_validation("  Running entity census (5s mark)...");
            dump_entity_census();
            let _ = log_validation("  Running worm entity dump (5s mark)...");
            dump_worm_tasks();
            let _ = log_validation("  Running TurnGame dump (5s mark)...");
            dump_turngame();
            let _ = log_validation("  Running CTaskTeam entity dump (5s mark)...");
            dump_ctaskteam_entities();
            let _ = log_validation("  Running CTask children sub-struct dump (5s mark)...");
            dump_ctask_children();
            let _ = log_validation("  Running CTaskMissile dump (5s mark)...");
            dump_missile_tasks();

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

/// Dump DSSound channel descriptor raw bytes for field layout verification.
unsafe fn dump_dssound_channels() {
    use openwa_core::address::va;
    use openwa_core::engine::game_session::GameSession;
    use openwa_core::rebase::rb;

    let session_ptr = rb(va::G_GAME_SESSION) as *const *const GameSession;
    let session = *session_ptr;
    if session.is_null() {
        let _ = log_validation("[DSSound] No session");
        return;
    }
    let sound = (*session).sound;
    if sound.is_null() {
        let _ = log_validation("[DSSound] No sound");
        return;
    }
    let snd = &*sound;

    let _ = log_validation("=== DSSound Channel Descriptor Dump ===");
    let _ = log_validation(&format!(
        "  DSSound at 0x{:08X}, vtable=0x{:08X}, volume={:?}",
        sound as u32, snd.vtable as u32, snd.volume
    ));

    // Dump each descriptor as raw u32s
    let base = sound as *const u8;
    for i in 0..8 {
        let desc_offset = 0x14 + i * 0x18;
        let desc_ptr = base.add(desc_offset) as *const u32;
        let words: [u32; 6] = [
            *desc_ptr,
            *desc_ptr.add(1),
            *desc_ptr.add(2),
            *desc_ptr.add(3),
            *desc_ptr.add(4),
            *desc_ptr.add(5),
        ];
        let has_buffer = words[5] != 0; // ds_buffer at +0x14 = word[5]
        let _ = log_validation(&format!(
            "  desc[{}] @+0x{:03X}: [{:08X} {:08X} {:08X} {:08X} {:08X} {:08X}]{}",
            i,
            desc_offset,
            words[0],
            words[1],
            words[2],
            words[3],
            words[4],
            words[5],
            if has_buffer { " <-- has buffer" } else { "" }
        ));
    }

    // Also dump buffer pool state
    let _ = log_validation(&format!(
        "  pool: free={}, used={}",
        snd.buffer_pool_free_count, snd.buffer_pool_used_count
    ));
}

/// Start the debug hotkey listener thread.
///
/// F8 = CTaskMissile dump (fire a weapon first, then press F8 while missile is in flight).
///
/// Always available regardless of `OPENWA_VALIDATE`. Skipped in replay-test
/// mode since auto-capture handles dumps automatically.
pub fn start_hotkeys() {
    if std::env::var("OPENWA_REPLAY_TEST").is_ok() {
        return;
    }

    std::thread::spawn(|| {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        const VK_F7: i32 = 0x76;
        const VK_F8: i32 = 0x77;
        let _ = log_validation("  Hotkeys: F7=DSSound channel dump, F8=missile dump");
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe {
                if GetAsyncKeyState(VK_F7) & 1 != 0 {
                    dump_dssound_channels();
                }
                if GetAsyncKeyState(VK_F8) & 1 != 0 {
                    dump_missile_tasks();
                }
            }
        }
    });
}
