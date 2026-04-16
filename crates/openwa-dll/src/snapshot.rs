//! Game state snapshot capture.
//!
//! Walks DDGame, the team arena, and the entity tree to produce a
//! canonicalized text dump suitable for cross-run diffing.

use core::fmt::Write;

use openwa_core::address::va;
use openwa_core::engine::GameSession;
use openwa_core::field_format::{self, FormatContext};
use openwa_core::rebase::rb;
use openwa_core::registry::StructFields;
use openwa_core::snapshot::{Snapshot, hash_pointer_targets, write_raw_region};
use openwa_core::task::{CTask, CTaskBfsIter, CTaskMissile, CTaskWorm};

/// Capture a full game state snapshot as text.
///
/// # Safety
/// Must be called from the DLL while the game is paused (frame breakpoint).
#[cfg(target_arch = "x86")]
pub unsafe fn capture() -> String {
    use openwa_core::engine::TeamArena;

    unsafe {
        let mut out = String::with_capacity(128 * 1024);

        let session_ptr = rb(va::G_GAME_SESSION) as *const *mut GameSession;
        let session = *session_ptr;
        if session.is_null() {
            let _ = writeln!(out, "ERROR: g_GameSession is null");
            return out;
        }
        let wrapper = (*session).ddgame_wrapper;
        if wrapper.is_null() {
            let _ = writeln!(out, "ERROR: DDGameWrapper is null");
            return out;
        }
        let ddgame = (*wrapper).ddgame;
        if ddgame.is_null() {
            let _ = writeln!(out, "ERROR: DDGame is null");
            return out;
        }

        let _ = writeln!(out, "=== Frame {} ===\n", (*ddgame).frame_counter);

        // ── DDGame ──
        let _ = writeln!(out, "[DDGame]");
        let _ = (*ddgame).write_snapshot(&mut out, 1);
        let _ = writeln!(out);

        // ── Sub-object hashes (pointer-independent) ──
        // Hashes the first 256 bytes of every heap object pointed to by DDGame
        // and PCLandscape. Differences here indicate sub-object state mismatches
        // that flat DDGame comparisons miss.
        let _ = writeln!(out, "[SubObjectHashes]");
        let _ = hash_pointer_targets(&mut out, ddgame as *const u8, 0x550, 256, "ddgame");
        let landscape = (*ddgame).landscape as *const u8;
        if !landscape.is_null() {
            let _ = hash_pointer_targets(&mut out, landscape, 0xB44, 256, "landscape");
        }
        let _ = writeln!(out);

        // ── Team blocks + worm entries ──
        let team_count = (*ddgame).team_arena.team_count as usize;
        let _ = writeln!(out, "[Teams] count={}", team_count);
        let arena = &raw mut (*ddgame).team_arena;
        let blocks = TeamArena::blocks_mut(arena);
        for t in 0..team_count {
            let header = TeamArena::team_header_mut(arena, t);
            let name = core::ffi::CStr::from_ptr((*header).team_name.as_ptr() as *const _)
                .to_string_lossy();
            let _ = writeln!(out, "\n  [Team {}] \"{}\"", t, name);
            let _ = writeln!(
                out,
                "    eliminated={} alliance={} worm_count={} active_worm={}",
                (*header).eliminated,
                (*header).alliance,
                (*header).worm_count,
                (*header).active_worm
            );
            let _ = writeln!(
                out,
                "    weapon_alliance={} turn_action_flags=0x{:08X}",
                (*header).weapon_alliance,
                (*header).turn_action_flags
            );

            // Worms are in block[t+1].worms[0..worm_count] (1-indexed blocks)
            let worm_count = (*header).worm_count.max(0) as usize;
            let block = &*blocks.add(t + 1);
            for wi in 0..worm_count.min(7) {
                let worm = &block.worms[wi];
                let _ = write!(out, "    [Worm {}] ", wi);
                let _ = worm.write_snapshot(&mut out, 0);
            }
        }
        let _ = writeln!(out);

        // ── Entity tree ──
        // CTaskTurnGame is the root of the entity tree. Find it by checking
        // CTaskLand's parent chain or the shared data table.
        // CTaskTurnGame vtable = 0x669F70.
        let task_land = (*ddgame).task_land;
        if task_land.is_null() {
            let _ = writeln!(out, "[Entities] task_land=null");
            return out;
        }

        let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);

        // Walk parent chain from CTaskLand to find CTaskTurnGame (root of entity tree).
        // Safety limit to prevent infinite loops on corrupt parent chains.
        let mut root = task_land as *const CTask;
        for _ in 0..10 {
            let parent = (*root).parent as *const CTask;
            if parent.is_null() || parent == root {
                break;
            }
            root = parent;
        }
        let _ = writeln!(
            out,
            "[Entities] root vt=0x{:08X}",
            (*(root as *const u32)).wrapping_sub(delta)
        );

        let iter = CTaskBfsIter::new(root);

        // Census + detailed dump
        let entities: Vec<*const CTask> = iter.collect();
        let _ = writeln!(out, "[Entities] {} total", entities.len());

        // Census by type
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for &task in &entities {
            let vt = (*(task as *const u32)).wrapping_sub(delta);
            let name = vtable_name(vt);
            *counts.entry(name).or_default() += 1;
        }
        let _ = write!(out, "  ");
        for (name, count) in &counts {
            let _ = write!(out, "{}x{} ", name, count);
        }
        let _ = writeln!(out, "\n");

        // Detail per entity
        for &task in &entities {
            let vt = (*(task as *const u32)).wrapping_sub(delta);
            let name = vtable_name(vt);
            let class_type = (*task).class_type;
            let _ = writeln!(out, "  [{}] class_type={:?}", name, class_type);

            match vt {
                x if x == va::CTASK_WORM_VTABLE => {
                    let worm = task as *const CTaskWorm;
                    let _ = (*worm).write_snapshot(&mut out, 2);
                }
                x if x == va::CTASK_MISSILE_VTABLE => {
                    let missile = task as *const CTaskMissile;
                    let _ = (*missile).write_snapshot(&mut out, 2);
                }
                _ => {
                    // Try FieldRegistry-based dump for known types
                    let class = vtable_name(vt);
                    if class != "Unknown" {
                        if let Some(fields) = openwa_core::registry::struct_fields_for(class) {
                            let _ = write_registry_fields(
                                &mut out,
                                task as *const u8,
                                fields,
                                delta,
                                2,
                            );
                        } else {
                            let _ = write_raw_region(&mut out, task as *const u8, 0x100, 2);
                        }
                    } else {
                        let _ = write_raw_region(&mut out, task as *const u8, 0x100, 2);
                    }
                }
            }
            let _ = writeln!(out);
        }

        out
    }
}

fn vtable_name(ghidra_vt: u32) -> &'static str {
    openwa_core::registry::vtable_class_name(ghidra_vt).unwrap_or("Unknown")
}

/// Write struct fields using FieldRegistry metadata and the format_field system.
///
/// Produces output like:
/// ```text
///     +0x0000  vtable           [4]  0x0066A30C (DDGameWrapper vtable)
///     +0x0004  parent           [4]  null
/// ```
unsafe fn write_registry_fields(
    w: &mut dyn Write,
    base: *const u8,
    fields: &StructFields,
    delta: u32,
    indent: usize,
) -> core::fmt::Result {
    unsafe {
        let ctx = FormatContext { delta };
        let pad = "  ".repeat(indent);
        for field in fields.fields {
            let addr = base as u32 + field.offset;
            write!(
                w,
                "{}+0x{:04X}  {:<16} [{:>2}]  ",
                pad, field.offset, field.name, field.size
            )?;
            if openwa_core::mem::can_read(addr, field.size) {
                let ptr = base.add(field.offset as usize);
                let data = core::slice::from_raw_parts(ptr, field.size as usize);
                field_format::format_field(w, data, field, &ctx)?;
            } else {
                write!(w, "<unreadable>")?;
            }
            writeln!(w)?;
        }
        Ok(())
    }
}
