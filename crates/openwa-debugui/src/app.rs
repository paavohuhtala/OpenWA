use eframe::egui;
use openwa_game::address::va;
use openwa_game::engine::{DDGame, game_session};
use openwa_game::rebase::rb;
use openwa_game::registry;
use openwa_game::task::{
    CTask, CTaskCloud, CTaskFire, CTaskTeam, CTaskTurnGame, CTaskWorm, TurnGameCtx,
};

use crate::log;

// ---------------------------------------------------------------------------
// Known task types for census display
// ---------------------------------------------------------------------------

/// Vtables of entities that are created/destroyed every frame (particles,
/// bubbles, etc.). Filtered from the census by default to reduce noise.
const TRANSIENT_VTABLES: &[u32] = &[va::CTASK_SEA_BUBBLE_VTABLE];

fn vtable_name(runtime_vtable: u32) -> Option<&'static str> {
    let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);
    let ghidra_va = runtime_vtable.wrapping_sub(delta);
    openwa_game::registry::vtable_class_name(ghidra_va)
}

/// Returns a display name for the entity at `addr`.
/// Tries the known-vtable map first; falls back to CTask.class_type.
unsafe fn entity_type_name(addr: u32) -> String {
    unsafe {
        if addr == 0 {
            return "(null)".to_owned();
        }
        let vtable = *(addr as *const u32);
        if let Some(name) = vtable_name(vtable) {
            return name.to_owned();
        }
        let task = addr as *const CTask;
        format!("{:?}", (*task).class_type)
    }
}

/// One-line label for a task: "TypeName @ 0xADDR"
unsafe fn entity_label(addr: u32) -> String {
    unsafe {
        if addr == 0 {
            return "(null)".to_owned();
        }
        format!("{} @ {:#010X}", entity_type_name(addr), addr)
    }
}

// ---------------------------------------------------------------------------
// Game-memory helpers (all unsafe — call from the UI update function only)
// ---------------------------------------------------------------------------

/// Returns a pointer to DDGame, or None if not in-game.
unsafe fn get_ddgame() -> Option<*const DDGame> {
    unsafe {
        let ptr = game_session::get_ddgame();
        if ptr.is_null() { None } else { Some(ptr) }
    }
}

/// Unlock all weapons: set ammo to unlimited (-1) and delays to 0 for all teams.
unsafe fn cheat_unlock_all_weapons() {
    unsafe {
        let Some(ddgame) = get_ddgame() else {
            log::push("[Cheats] Not in game");
            return;
        };
        let ddgame = ddgame as *mut DDGame;
        let arena = &mut (*ddgame).team_arena;
        for team in &mut arena.weapon_slots.teams {
            for ammo in &mut team.ammo {
                *ammo = -1; // unlimited
            }
            for delay in &mut team.delay {
                *delay = 0; // no delay
            }
        }
        log::push("[Cheats] All weapons unlocked (infinite ammo, no delays)");
    }
}

/// Read child task pointers from a CTask's children array.
///
/// The array is **sparse**: slots are nulled when a child is removed rather than
/// compacted. `children_watermark` is the insertion counter (loop upper bound used
/// by CTask::HandleMessage), not the live-child count. We return all slots up to
/// that bound so the caller can filter nulls and display the live set.
unsafe fn read_children(task: *const CTask) -> Vec<u32> {
    unsafe {
        let slots = (*task).children_watermark as usize;
        let data = (*task).children_data as *const u32;
        if data.is_null() || slots == 0 {
            return Vec::new();
        }
        // Hard safety cap: 4096 slots × 4 bytes = 16 KB max read
        let slots = slots.min(4096);
        (0..slots).map(|i| *data.add(i)).collect()
    }
}

// ---------------------------------------------------------------------------
// Live entity snapshot (built once per frame via full task-tree traversal)
// ---------------------------------------------------------------------------

/// Walk up parent pointers from `start` to find the root task (no parent).
/// Returns None if task_land is null or the chain doesn't terminate within
/// MAX_DEPTH steps (guard against corrupt/circular pointers).
unsafe fn find_root_task(ddgame: *const DDGame) -> Option<u32> {
    unsafe {
        let task_land = (*ddgame).task_land as u32;
        if task_land == 0 {
            return None;
        }
        let mut current = task_land;
        for _ in 0..64 {
            let parent = (*(current as *const CTask)).parent as u32;
            if parent == 0 {
                return Some(current);
            }
            current = parent;
        }
        None // chain didn't terminate — corrupt data
    }
}

/// DFS the task tree from `root`, returning (vtable, addr) for every node.
/// A visited set prevents infinite loops from corrupt/circular pointers.
unsafe fn collect_task_tree(root: u32) -> Vec<(u32, u32)> {
    unsafe {
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut visited = std::collections::HashSet::new();
        while let Some(addr) = stack.pop() {
            if addr == 0 || !visited.insert(addr) {
                continue;
            }
            let vtable = *(addr as *const u32);
            out.push((vtable, addr));
            for child in read_children(addr as *const CTask) {
                if child != 0 {
                    stack.push(child);
                }
            }
        }
        out
    }
}

unsafe fn collect_live_entities() -> Vec<(u32, u32)> {
    unsafe {
        let Some(ddgame) = get_ddgame() else {
            return Vec::new();
        };
        let Some(root) = find_root_task(ddgame) else {
            return Vec::new();
        };
        collect_task_tree(root)
    }
}

// ---------------------------------------------------------------------------
// DebugApp
// ---------------------------------------------------------------------------

pub struct DebugApp {
    /// Currently selected entity address for the struct inspector.
    selected_entity: Option<u32>,
    /// Navigation history — addresses we came from (supports ← Back).
    nav_history: Vec<u32>,
    /// Whether the log panel should auto-scroll to the bottom.
    log_auto_scroll: bool,
    /// Show transient entities (sea bubbles, etc.) in the census.
    show_transient: bool,
}

impl Default for DebugApp {
    fn default() -> Self {
        Self {
            selected_entity: None,
            nav_history: Vec::new(),
            log_auto_scroll: true,
            show_transient: false,
        }
    }
}

impl DebugApp {
    /// Navigate to `addr`, pushing the current selection onto the history stack.
    fn navigate_to(&mut self, addr: u32) {
        if let Some(cur) = self.selected_entity
            && cur != addr
        {
            self.nav_history.push(cur);
        }
        self.selected_entity = Some(addr);
    }

    /// Navigate back to the previous address, if any.
    fn navigate_back(&mut self) {
        if let Some(prev) = self.nav_history.pop() {
            self.selected_entity = Some(prev);
        }
    }
}

impl eframe::App for DebugApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        // Repaint at ~30 fps so the display stays live.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        // Build the live entity snapshot for this frame and prune any stale
        // selections before rendering — prevents UAF crashes when an entity is
        // destroyed (match ended, worm died, etc.) while it is being inspected.
        let live_entities = unsafe { collect_live_entities() };
        let live_addrs: std::collections::HashSet<u32> =
            live_entities.iter().map(|&(_, a)| a).collect();

        if let Some(addr) = self.selected_entity
            && addr != 0
            && !live_addrs.contains(&addr)
        {
            self.selected_entity = None;
            self.nav_history.clear();
        }
        self.nav_history.retain(|a| live_addrs.contains(a));

        egui::Panel::top("toolbar").show_inside(root_ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Cheats", |ui| {
                    if ui.button("Unlock all weapons").clicked() {
                        unsafe { cheat_unlock_all_weapons() };
                        ui.close();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_transient, "Show transient");
            });
        });

        egui::Panel::right("inspector_panel")
            .min_size(260.0)
            .default_size(300.0)
            .show_inside(root_ui, |ui| {
                let mut navigate_to: Option<u32> = None;
                let mut go_back = false;
                self.show_inspector(ui, &mut navigate_to, &mut go_back);
                if go_back {
                    self.navigate_back();
                }
                if let Some(addr) = navigate_to {
                    self.navigate_to(addr);
                }
            });

        egui::Panel::bottom("log_panel")
            .min_size(140.0)
            .default_size(160.0)
            .show_inside(root_ui, |ui| {
                self.show_log(ui);
            });

        egui::CentralPanel::default().show_inside(root_ui, |ui| {
            let mut navigate_to: Option<u32> = None;
            self.show_census(ui, &live_entities, &mut navigate_to);
            if let Some(addr) = navigate_to {
                self.navigate_to(addr);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Panel: Entity Census
// ---------------------------------------------------------------------------

impl DebugApp {
    fn show_census(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[(u32, u32)],
        navigate_to: &mut Option<u32>,
    ) {
        ui.heading("Entity Census");

        if rows.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, "No game session — waiting...");
            return;
        }

        let visible: Vec<&(u32, u32)> = if self.show_transient {
            rows.iter().collect()
        } else {
            rows.iter()
                .filter(|&&(vtable, _)| !TRANSIENT_VTABLES.iter().any(|&t| rb(t) == vtable))
                .collect()
        };

        if visible.len() == rows.len() {
            ui.label(format!("{} entities", rows.len()));
        } else {
            ui.label(format!(
                "{} entities ({} hidden transient)",
                visible.len(),
                rows.len() - visible.len()
            ));
        }
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("census_grid")
                    .striped(true)
                    .num_columns(3)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        ui.strong("Type");
                        ui.strong("Address");
                        ui.strong("Vtable");
                        ui.end_row();

                        for &&(vtable, entity) in &visible {
                            let name = unsafe { entity_type_name(entity) };
                            let is_selected = self.selected_entity == Some(entity);
                            if ui.selectable_label(is_selected, name).clicked() {
                                *navigate_to = Some(entity);
                            }
                            ui.label(format!("{:#010X}", entity));
                            ui.label(format!("{:#010X}", vtable));
                            ui.end_row();
                        }
                    });
            });
    }
}

// ---------------------------------------------------------------------------
// Raw field viewer for CGameTask-derived types
// ---------------------------------------------------------------------------

/// CGameTask field labels for the base class region (0x00..0xFC).
/// Display all DWORDs of a CGameTask-derived entity with labelled fields.
///
/// Field names are resolved from the global registry via inheritance-aware
/// lookup (entity → CGameTask → CTask). No hardcoded label tables needed.
unsafe fn show_game_task_raw_fields(
    ui: &mut egui::Ui,
    addr: u32,
    type_name: &str,
    total_size: usize,
) {
    unsafe {
        let base = addr as *const u32;
        let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);

        // Sections: CTask base, CGameTask unknowns, pos/speed, more unknowns, emitter,
        // then type-specific in 0x80-byte chunks to keep each section manageable.
        let mut sections: Vec<(usize, usize, String)> = vec![
            (0x000, 0x030, "CTask base".into()),
            (0x030, 0x084, "CGameTask +0x30".into()),
            (0x084, 0x098, "pos / speed / angle".into()),
            (0x098, 0x0E8, "CGameTask +0x98".into()),
            (0x0E8, 0x0FC, "SoundEmitter".into()),
        ];
        // Split type-specific region into chunks of 0x80 bytes
        let mut chunk_start = 0x0FC;
        while chunk_start < total_size {
            let chunk_end = (chunk_start + 0x80).min(total_size);
            sections.push((
                chunk_start,
                chunk_end,
                format!("{} +0x{:03X}..0x{:03X}", type_name, chunk_start, chunk_end),
            ));
            chunk_start = chunk_end;
        }

        for (start, end, section_name) in &sections {
            let (start, end) = (*start, *end);
            if start >= total_size {
                break;
            }
            let end = end.min(total_size);
            let header = format!("{} (0x{:03X}..0x{:03X})", section_name, start, end);
            let default_open = false;
            egui::CollapsingHeader::new(header)
                .id_salt(format!("{}_{}_{:03X}", type_name, addr, start))
                .default_open(default_open)
                .show(ui, |ui| {
                    egui::Grid::new(format!("raw_{}_{}_{:03X}", type_name, addr, start))
                        .striped(true)
                        .num_columns(4)
                        .show(ui, |ui| {
                            ui.strong("Offset");
                            ui.strong("Value");
                            ui.strong("Field");
                            ui.strong("Points to");
                            ui.end_row();

                            let dwords = (end - start) / 4;
                            for i in 0..dwords {
                                let off = start + i * 4;
                                let val = *base.add(off / 4);
                                let field_name =
                                    registry::field_at_inherited(type_name, off as u32)
                                        .map(|f| f.name)
                                        .unwrap_or("");

                                ui.label(format!("+0x{:03X}", off));
                                ui.label(format!("{:#010X} ({})", val, val as i32));
                                ui.label(field_name);

                                // Pointer identification via registry
                                use openwa_game::mem;
                                let ptr_label = if val >= 0x10000 {
                                    mem::identify_pointer(val, delta).and_then(|id| id.name)
                                } else {
                                    None
                                };
                                if let Some(label) = ptr_label {
                                    ui.colored_label(
                                        egui::Color32::LIGHT_BLUE,
                                        format!("→ {}", label),
                                    );
                                } else {
                                    ui.label("");
                                }
                                ui.end_row();
                            }
                        });
                });
        }
    }
}

// ---------------------------------------------------------------------------
// Panel: Struct Inspector
// ---------------------------------------------------------------------------

impl DebugApp {
    fn show_inspector(
        &mut self,
        ui: &mut egui::Ui,
        navigate_to: &mut Option<u32>,
        go_back: &mut bool,
    ) {
        // Navigation bar
        ui.horizontal(|ui| {
            ui.heading("Inspector");
            ui.add_space(8.0);
            if ui
                .add_enabled(!self.nav_history.is_empty(), egui::Button::new("← Back"))
                .clicked()
            {
                *go_back = true;
            }
            if !self.nav_history.is_empty() {
                ui.weak(format!("({} deep)", self.nav_history.len()));
            }
        });

        let Some(addr) = self.selected_entity else {
            ui.colored_label(egui::Color32::GRAY, "Select an entity in the census.");
            return;
        };

        if addr == 0 {
            ui.label("(null entity)");
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                unsafe {
                    let vtable = *(addr as *const u32);
                    let name = entity_type_name(addr);

                    ui.label(format!("Entity: {:#010X}", addr));
                    ui.label(format!("Type:   {}", name));
                    ui.label(format!("Vtable: {:#010X}", vtable));
                    ui.separator();

                    let task = addr as *const CTask;

                    // --- CTask base ---
                    egui::CollapsingHeader::new("CTask base")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("ctask_grid").striped(true).show(ui, |ui| {
                                // Parent — clickable link
                                let parent = (*task).parent as u32;
                                ui.label("parent");
                                if parent != 0 {
                                    if ui.link(entity_label(parent)).clicked() {
                                        *navigate_to = Some(parent);
                                    }
                                } else {
                                    ui.label("(none)");
                                }
                                ui.end_row();

                                // children_watermark = total insertions (sparse array); live = non-null slots
                                ui.label("child slots");
                                ui.label(format!(
                                    "{} watermark / {} cap",
                                    (*task).children_watermark,
                                    (*task).children_capacity
                                ));
                                ui.end_row();
                                ui.label("class_type");
                                ui.label(format!("{:?}", (*task).class_type));
                                ui.end_row();
                            });
                        });

                    // --- Children tree ---
                    // read_children returns all slots (sparse); filter nulls for live count.
                    let children = read_children(task);
                    let live_count = children.iter().filter(|&&a| a != 0).count();
                    let slot_count = (*task).children_watermark as usize;
                    if live_count > 0 || slot_count > 0 {
                        egui::CollapsingHeader::new(format!(
                            "Children ({} live / {} slots)",
                            live_count, slot_count
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            for child_addr in &children {
                                let child_addr = *child_addr;
                                if child_addr == 0 {
                                    continue;
                                }
                                let child_name = entity_type_name(child_addr);

                                // Expand inline if the child itself has children
                                let child_task = child_addr as *const CTask;
                                let grandchildren = read_children(child_task);
                                let grandchild_count =
                                    grandchildren.iter().filter(|&&a| a != 0).count();

                                if grandchild_count > 0 {
                                    // Show as a sub-collapsing header with its own children
                                    let header_label = format!(
                                        "{} @ {:#010X}  ({} children)",
                                        child_name, child_addr, grandchild_count
                                    );
                                    egui::CollapsingHeader::new(&header_label)
                                        .id_salt(child_addr)
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            // Link to inspect this child in detail
                                            if ui.link("→ Inspect").clicked() {
                                                *navigate_to = Some(child_addr);
                                            }
                                            // Show grandchildren
                                            for gc_addr in &grandchildren {
                                                let gc_addr = *gc_addr;
                                                if gc_addr == 0 {
                                                    continue;
                                                }
                                                if ui.link(entity_label(gc_addr)).clicked() {
                                                    *navigate_to = Some(gc_addr);
                                                }
                                            }
                                        });
                                } else {
                                    // Leaf child — single clickable link
                                    let label = format!("  {}  @ {:#010X}", child_name, child_addr);
                                    if ui.link(label).clicked() {
                                        *navigate_to = Some(child_addr);
                                    }
                                }
                            }
                        });
                    }

                    // --- CTaskMine-specific fields ---
                    if name == "CTaskMine" {
                        show_game_task_raw_fields(ui, addr, "CTaskMine", 0x128);
                    }

                    // --- CTaskOilDrum-specific fields ---
                    if name == "CTaskOilDrum" {
                        show_game_task_raw_fields(ui, addr, "CTaskOilDrum", 0x110);
                    }

                    // --- CTaskCrate-specific fields ---
                    if name == "CTaskCrate" {
                        show_game_task_raw_fields(ui, addr, "CTaskCrate", 0x4B0);
                    }

                    // --- CTaskCloud-specific fields ---
                    if name == "CTaskCloud" {
                        let cloud = addr as *const CTaskCloud;
                        egui::CollapsingHeader::new("CTaskCloud")
                            .default_open(true)
                            .show(ui, |ui| {
                                egui::Grid::new("cloud_grid").striped(true).show(ui, |ui| {
                                    ui.label("pos_x");
                                    ui.label(format!("{:.1}", (*cloud).pos_x.to_f32()));
                                    ui.end_row();
                                    ui.label("anim_phase");
                                    ui.label(format!("{:.1}", (*cloud).anim_phase.to_f32()));
                                    ui.end_row();
                                    ui.label("vel_x");
                                    ui.label(format!("{:.4}", (*cloud).vel_x.to_f32()));
                                    ui.end_row();
                                    ui.label("phase_speed");
                                    ui.label(format!("{:.4}", (*cloud).phase_speed.to_f32()));
                                    ui.end_row();
                                    ui.label("wind_accel");
                                    ui.label(format!("{:.4}", (*cloud).wind_accel.to_f32()));
                                    ui.end_row();
                                    ui.label("wind_target");
                                    ui.label(format!("{:.4}", (*cloud).wind_target.to_f32()));
                                    ui.end_row();
                                    ui.label("layer_depth");
                                    ui.label(format!("{:.1}", (*cloud).layer_depth.to_f32()));
                                    ui.end_row();
                                    ui.label("sprite_id");
                                    ui.label(format!("{:#06X}", (*cloud).sprite_id));
                                    ui.end_row();
                                });
                            });
                    }

                    // --- CTaskTurnGame-specific fields ---
                    if name == "CTaskTurnGame" {
                        let tg = addr as *const CTaskTurnGame;
                        egui::CollapsingHeader::new("CTaskTurnGame")
                            .default_open(true)
                            .show(ui, |ui| {
                                egui::Grid::new("tg_grid").striped(true).show(ui, |ui| {
                                    ui.label("current_team");
                                    ui.label(format!("{}", (*tg).current_team));
                                    ui.end_row();
                                    ui.label("current_worm");
                                    ui.label(format!("{}", (*tg).current_worm));
                                    ui.end_row();
                                    ui.label("arena_team");
                                    ui.label(format!("{}", (*tg).arena_team));
                                    ui.end_row();
                                    ui.label("arena_worm");
                                    ui.label(format!("{}", (*tg).arena_worm));
                                    ui.end_row();
                                    ui.label("worm_active");
                                    ui.label(format!("{}", (*tg).worm_active != 0));
                                    ui.end_row();
                                    ui.label("turn_ended");
                                    ui.label(format!("{}", (*tg).turn_ended != 0));
                                    ui.end_row();
                                    ui.label("no_time_lim");
                                    ui.label(format!("{}", (*tg).no_time_limit != 0));
                                    ui.end_row();
                                    ui.label("turn_timer");
                                    ui.label(format!("{:.1}s", (*tg).turn_timer as f32 / 1000.0));
                                    ui.end_row();
                                    ui.label("retreat");
                                    ui.label(format!(
                                        "{:.1}s",
                                        (*tg).retreat_timer as f32 / 1000.0
                                    ));
                                    ui.end_row();
                                    ui.label("idle_timer");
                                    ui.label(format!("{:.1}s", (*tg).idle_timer as f32 / 1000.0));
                                    ui.end_row();
                                    ui.label("num_teams");
                                    ui.label(format!("{}", (*tg).num_teams));
                                    ui.end_row();
                                    ui.label("active_frm");
                                    ui.label(format!("{}", (*tg).active_worm_frames));
                                    ui.end_row();
                                    ui.label("retreat_frm");
                                    ui.label(format!("{}", (*tg).retreat_frames));
                                    ui.end_row();
                                });
                            });

                        let ctx = &(*tg).game_ctx as *const TurnGameCtx;
                        egui::CollapsingHeader::new("TurnGameCtx (+0x30)")
                            .default_open(false)
                            .show(ui, |ui| {
                                egui::Grid::new("ctx_grid").striped(true).show(ui, |ui| {
                                    ui.label("land_height");
                                    ui.label(format!("{:.1}", (*ctx).land_height.to_f32()));
                                    ui.end_row();
                                    ui.label("land_height_2");
                                    ui.label(format!("{:.1}", (*ctx).land_height_2.to_f32()));
                                    ui.end_row();
                                    ui.label("sentinel_18");
                                    ui.label(format!("{}", (*ctx)._sentinel_18));
                                    ui.end_row();
                                    ui.label("sentinel_28");
                                    ui.label(format!("{}", (*ctx)._sentinel_28));
                                    ui.end_row();
                                    ui.label("sentinel_38");
                                    ui.label(format!("{}", (*ctx)._sentinel_38));
                                    ui.end_row();
                                    ui.label("team_count");
                                    ui.label(format!("{}", (*ctx).team_count));
                                    ui.end_row();
                                    ui.label("_slot_d0");
                                    ui.label(format!("{}", (*ctx)._slot_d0));
                                    ui.end_row();
                                    let ta = (*ctx)._hud_textbox_a;
                                    let tb = (*ctx)._hud_textbox_b;
                                    ui.label("hud_tb_a");
                                    ui.label(if ta == 0 {
                                        "(null)".into()
                                    } else {
                                        format!("{:#010X}", ta)
                                    });
                                    ui.end_row();
                                    ui.label("hud_tb_b");
                                    ui.label(if tb == 0 {
                                        "(null)".into()
                                    } else {
                                        format!("{:#010X}", tb)
                                    });
                                    ui.end_row();
                                });
                            });
                    }

                    // --- CTaskTeam-specific fields ---
                    if name == "CTaskTeam" {
                        let team = addr as *const CTaskTeam;
                        egui::CollapsingHeader::new("CTaskTeam")
                            .default_open(true)
                            .show(ui, |ui| {
                                egui::Grid::new("team_grid").striped(true).show(ui, |ui| {
                                    ui.label("team_index");
                                    ui.label(format!("{}", (*team).team_index));
                                    ui.end_row();
                                    ui.label("alive_worms");
                                    ui.label(format!("{}", (*team).alive_worm_count));
                                    ui.end_row();
                                    ui.label("worm_count");
                                    ui.label(format!("{}", (*team).worm_count));
                                    ui.end_row();
                                    ui.label("last_weapon");
                                    ui.label(format!("{}", (*team).last_launched_weapon));
                                    ui.end_row();
                                    ui.label("pos_x");
                                    ui.label(format!("{:.2}", (*team).pos_x.to_f32()));
                                    ui.end_row();
                                    ui.label("pos_y");
                                    ui.label(format!("{:.2}", (*team).pos_y.to_f32()));
                                    ui.end_row();
                                });
                            });
                    }

                    // --- CTaskFire-specific fields ---
                    if name == "CTaskFire" {
                        let fire = addr as *const CTaskFire;
                        egui::CollapsingHeader::new("CTaskFire")
                            .default_open(true)
                            .show(ui, |ui| {
                                egui::Grid::new("fire_grid").striped(true).show(ui, |ui| {
                                    ui.label("spawn_x");
                                    ui.label(format!("{:.1}", (*fire).spawn_x.to_f32()));
                                    ui.end_row();
                                    ui.label("spawn_y");
                                    ui.label(format!("{:.1}", (*fire).spawn_y.to_f32()));
                                    ui.end_row();
                                    ui.label("timer");
                                    ui.label(format!("{}", (*fire).timer));
                                    ui.end_row();
                                    ui.label("burn_rate");
                                    ui.label(format!("{}", (*fire).burn_rate));
                                    ui.end_row();
                                    ui.label("spread_ctr");
                                    ui.label(format!("{}", (*fire).spread_counter));
                                    ui.end_row();
                                    ui.label("lifetime");
                                    ui.label(format!("{}", (*fire).lifetime));
                                    ui.end_row();
                                    ui.label("slot_index");
                                    ui.label(format!("{}", (*fire).slot_index));
                                    ui.end_row();
                                });
                            });
                    }

                    // --- CTaskWorm-specific fields ---
                    if name == "CTaskWorm"
                        || (*task).class_type == openwa_game::game::ClassType::Worm
                    {
                        // Summary header with key info
                        let worm = addr as *const CTaskWorm;
                        let name_bytes = &(*worm).worm_name;
                        let nul = name_bytes
                            .iter()
                            .position(|&b| b == 0)
                            .unwrap_or(name_bytes.len());
                        let worm_name = std::str::from_utf8(&name_bytes[..nul]).unwrap_or("?");
                        ui.label(format!(
                            "Worm: \"{}\"  state={:#04X}  team={}  idx={}",
                            worm_name,
                            (*worm).state(),
                            (*worm).team_index,
                            (*worm).worm_index
                        ));
                        ui.separator();

                        show_game_task_raw_fields(ui, addr, "CTaskWorm", 0x3FC);
                    }

                    // --- CTaskMissile-specific fields ---
                    if name == "CTaskMissile" {
                        use openwa_game::task::CTaskMissile;
                        let m = &*(addr as *const CTaskMissile);
                        ui.label(format!(
                            "Missile: type={:?}  slot={}  homing={}  dir={}",
                            m.missile_type, m.slot_id, m.homing_enabled, m.direction
                        ));
                        ui.separator();

                        show_game_task_raw_fields(ui, addr, "CTaskMissile", 0x41C);
                    }
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Panel: Log Stream
// ---------------------------------------------------------------------------

impl DebugApp {
    fn show_log(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Log");
            ui.checkbox(&mut self.log_auto_scroll, "auto-scroll");
            if ui.button("Clear").clicked() {
                log::clear();
            }
        });
        ui.separator();

        let entries = log::snapshot(200);

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(self.log_auto_scroll)
            .show(ui, |ui| {
                for (ts, text) in &entries {
                    let elapsed = ts.elapsed().as_secs_f32();
                    let color = if elapsed < 1.0 {
                        egui::Color32::WHITE
                    } else if elapsed < 5.0 {
                        egui::Color32::LIGHT_GRAY
                    } else {
                        egui::Color32::GRAY
                    };
                    ui.colored_label(color, text);
                }
            });
    }
}
