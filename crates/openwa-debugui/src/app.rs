use eframe::egui;
use openwa_core::address::va;
use openwa_core::ddgame::DDGame;
use openwa_core::ddgame_wrapper::DDGameWrapper;
use openwa_core::rebase::rb;
use openwa_core::task::{CTask, CTaskCloud, CTaskFire, CTaskMine, CTaskOilDrum, CTaskWorm};

use crate::log;

// ---------------------------------------------------------------------------
// Known task types for census display
// ---------------------------------------------------------------------------

const KNOWN_VTABLES: &[(u32, &str)] = &[
    (va::CTASK_WORM_VTABLE,        "CTaskWorm"),
    (va::CTASK_LAND_VTABLE,        "CTaskLand"),
    (va::CTASK_TURN_GAME_VTABLE,   "CTaskTurnGame"),
    (va::CTASK_TEAM_VTABLE,        "CTaskTeam"),
    (va::CTASK_FILTER_VTABLE,      "CTaskFilter"),
    (va::CTASK_DIRT_VTABLE,        "CTaskDirt"),
    (va::CTASK_SPRITE_ANIM_VTABLE, "CTaskSpriteAnim"),
    (va::CTASK_CPU_VTABLE,         "CTaskCPU"),
    (va::CTASK_MISSILE_VTABLE,     "CTaskMissile"),
    (va::CTASK_MINE_VTABLE,        "CTaskMine"),
    (va::CTASK_OILDRUM_VTABLE,     "CTaskOilDrum"),
    (va::CTASK_CLOUD_VTABLE,       "CTaskCloud"),
    (va::CTASK_SEA_BUBBLE_VTABLE,  "CTaskSeaBubble"),
    (va::CTASK_FIRE_VTABLE,        "CTaskFire"),
];

/// Vtables of entities that are created/destroyed every frame (particles,
/// bubbles, etc.). Filtered from the census by default to reduce noise.
const TRANSIENT_VTABLES: &[u32] = &[
    va::CTASK_SEA_BUBBLE_VTABLE,
];

fn vtable_name(runtime_vtable: u32) -> Option<&'static str> {
    KNOWN_VTABLES.iter()
        .find(|&&(ghidra_va, _)| rb(ghidra_va) == runtime_vtable)
        .map(|&(_, name)| name)
}

/// Returns a display name for the entity at `addr`.
/// Tries the known-vtable map first; falls back to CTask.class_type.
unsafe fn entity_type_name(addr: u32) -> String {
    if addr == 0 { return "(null)".to_owned(); }
    let vtable = *(addr as *const u32);
    if let Some(name) = vtable_name(vtable) {
        return name.to_owned();
    }
    let task = addr as *const CTask;
    format!("{:?}", (*task).class_type)
}

/// One-line label for a task: "TypeName @ 0xADDR"
unsafe fn entity_label(addr: u32) -> String {
    if addr == 0 { return "(null)".to_owned(); }
    format!("{} @ {:#010X}", entity_type_name(addr), addr)
}

// ---------------------------------------------------------------------------
// Game-memory helpers (all unsafe — call from the UI update function only)
// ---------------------------------------------------------------------------

/// Returns a pointer to DDGame, or None if not in-game.
unsafe fn get_ddgame() -> Option<*const DDGame> {
    let session_ptr = *(rb(va::G_GAME_SESSION) as *const u32);
    if session_ptr == 0 { return None; }
    let wrapper_addr = *((session_ptr + 0xA0) as *const u32);
    if wrapper_addr == 0 { return None; }
    let ddgame_ptr = (*(wrapper_addr as *const DDGameWrapper)).ddgame;
    if ddgame_ptr.is_null() { return None; }
    Some(ddgame_ptr)
}

/// Read child task pointers from a CTask's children array.
///
/// The array is **sparse**: slots are nulled when a child is removed rather than
/// compacted. `children_size` is the slot high-watermark (loop upper bound used
/// by CTask::HandleMessage), not the live-child count. We return all slots up to
/// that bound so the caller can filter nulls and display the live set.
unsafe fn read_children(task: *const CTask) -> Vec<u32> {
    let slots = (*task).children_size as usize;
    let data  = (*task).children_data as *const u32;
    if data.is_null() || slots == 0 { return Vec::new(); }
    // Hard safety cap: 4096 slots × 4 bytes = 16 KB max read
    let slots = slots.min(4096);
    (0..slots).map(|i| *data.add(i)).collect()
}

// ---------------------------------------------------------------------------
// Live entity snapshot (built once per frame via full task-tree traversal)
// ---------------------------------------------------------------------------

/// Walk up parent pointers from `start` to find the root task (no parent).
/// Returns None if task_land is null or the chain doesn't terminate within
/// MAX_DEPTH steps (guard against corrupt/circular pointers).
unsafe fn find_root_task(ddgame: *const DDGame) -> Option<u32> {
    let task_land = (*ddgame).task_land as u32;
    if task_land == 0 { return None; }
    let mut current = task_land;
    for _ in 0..64 {
        let parent = (*(current as *const CTask)).parent as u32;
        if parent == 0 { return Some(current); }
        current = parent;
    }
    None // chain didn't terminate — corrupt data
}

/// DFS the task tree from `root`, returning (vtable, addr) for every node.
/// A visited set prevents infinite loops from corrupt/circular pointers.
unsafe fn collect_task_tree(root: u32) -> Vec<(u32, u32)> {
    let mut out     = Vec::new();
    let mut stack   = vec![root];
    let mut visited = std::collections::HashSet::new();
    while let Some(addr) = stack.pop() {
        if addr == 0 || !visited.insert(addr) { continue; }
        let vtable = *(addr as *const u32);
        out.push((vtable, addr));
        for child in read_children(addr as *const CTask) {
            if child != 0 { stack.push(child); }
        }
    }
    out
}

unsafe fn collect_live_entities() -> Vec<(u32, u32)> {
    let Some(ddgame) = get_ddgame() else { return Vec::new(); };
    let Some(root)   = find_root_task(ddgame) else { return Vec::new(); };
    collect_task_tree(root)
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
        Self { selected_entity: None, nav_history: Vec::new(), log_auto_scroll: true, show_transient: false }
    }
}

impl DebugApp {
    /// Navigate to `addr`, pushing the current selection onto the history stack.
    fn navigate_to(&mut self, addr: u32) {
        if let Some(cur) = self.selected_entity {
            if cur != addr {
                self.nav_history.push(cur);
            }
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Repaint at ~30 fps so the display stays live.
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        // Build the live entity snapshot for this frame and prune any stale
        // selections before rendering — prevents UAF crashes when an entity is
        // destroyed (match ended, worm died, etc.) while it is being inspected.
        let live_entities = unsafe { collect_live_entities() };
        let live_addrs: std::collections::HashSet<u32> =
            live_entities.iter().map(|&(_, a)| a).collect();

        if let Some(addr) = self.selected_entity {
            if addr != 0 && !live_addrs.contains(&addr) {
                self.selected_entity = None;
                self.nav_history.clear();
            }
        }
        self.nav_history.retain(|a| live_addrs.contains(a));

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_transient, "Show transient");
            });
        });

        egui::SidePanel::right("inspector_panel")
            .min_width(260.0)
            .default_width(300.0)
            .show(ctx, |ui| {
                let mut navigate_to: Option<u32> = None;
                let mut go_back = false;
                self.show_inspector(ui, &mut navigate_to, &mut go_back);
                if go_back { self.navigate_back(); }
                if let Some(addr) = navigate_to { self.navigate_to(addr); }
            });

        egui::TopBottomPanel::bottom("log_panel")
            .min_height(140.0)
            .default_height(160.0)
            .show(ctx, |ui| {
                self.show_log(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut navigate_to: Option<u32> = None;
            self.show_census(ui, &live_entities, &mut navigate_to);
            if let Some(addr) = navigate_to { self.navigate_to(addr); }
        });
    }
}

// ---------------------------------------------------------------------------
// Panel: Entity Census
// ---------------------------------------------------------------------------

impl DebugApp {
    fn show_census(&mut self, ui: &mut egui::Ui, rows: &[(u32, u32)], navigate_to: &mut Option<u32>) {
        ui.heading("Entity Census");

        if rows.is_empty() {
            ui.colored_label(egui::Color32::YELLOW, "No game session — waiting...");
            return;
        }

        let visible: Vec<&(u32, u32)> = if self.show_transient {
            rows.iter().collect()
        } else {
            rows.iter()
                .filter(|&&(vtable, _)| {
                    !TRANSIENT_VTABLES.iter().any(|&t| rb(t) == vtable)
                })
                .collect()
        };

        if visible.len() == rows.len() {
            ui.label(format!("{} entities", rows.len()));
        } else {
            ui.label(format!("{} entities ({} hidden transient)", visible.len(), rows.len() - visible.len()));
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
            if ui.add_enabled(!self.nav_history.is_empty(), egui::Button::new("← Back")).clicked() {
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
            .show(ui, |ui| { unsafe {
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

                        // children_size = slot high-watermark (sparse array); live = non-null slots
                        ui.label("child slots"); ui.label(format!("{} used / {} cap", (*task).children_size, (*task).children_max_size)); ui.end_row();
                        ui.label("class_type");  ui.label(format!("{:?}", (*task).class_type));                                  ui.end_row();
                    });
                });

            // --- Children tree ---
            // read_children returns all slots (sparse); filter nulls for live count.
            let children = read_children(task);
            let live_count = children.iter().filter(|&&a| a != 0).count();
            let slot_count = (*task).children_size as usize;
            if live_count > 0 || slot_count > 0 {
                egui::CollapsingHeader::new(format!("Children ({} live / {} slots)", live_count, slot_count))
                    .default_open(true)
                    .show(ui, |ui| {
                        for child_addr in &children {
                            let child_addr = *child_addr;
                            if child_addr == 0 { continue; }
                            let child_name = entity_type_name(child_addr);

                            // Expand inline if the child itself has children
                            let child_task = child_addr as *const CTask;
                            let grandchild_count = (*child_task).children_size;

                            if grandchild_count > 0 {
                                // Show as a sub-collapsing header with its own children
                                let header_label = format!(
                                    "{} @ {:#010X}  ({} children)",
                                    child_name, child_addr, grandchild_count
                                );
                                egui::CollapsingHeader::new(&header_label)
                                    .id_source(child_addr)
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        // Link to inspect this child in detail
                                        if ui.link("→ Inspect").clicked() {
                                            *navigate_to = Some(child_addr);
                                        }
                                        // Show grandchildren
                                        let grandchildren = read_children(child_task);
                                        for gc_addr in &grandchildren {
                                            let gc_addr = *gc_addr;
                                            if gc_addr == 0 { continue; }
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
                let mine = addr as *const CTaskMine;
                egui::CollapsingHeader::new("CTaskMine")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("mine_grid").striped(true).show(ui, |ui| {
                            ui.label("pos_x");      ui.label(format!("{:.1}", (*mine).base.pos_x.to_f32()));    ui.end_row();
                            ui.label("pos_y");      ui.label(format!("{:.1}", (*mine).base.pos_y.to_f32()));    ui.end_row();
                            ui.label("speed_x");    ui.label(format!("{:.4}", (*mine).base.speed_x.to_f32())); ui.end_row();
                            ui.label("speed_y");    ui.label(format!("{:.4}", (*mine).base.speed_y.to_f32())); ui.end_row();
                            let ft = (*mine).fuse_timer;
                            let ft_label = if ft < 0 { "disarmed".to_owned() } else if ft == 0 { "ARMED".to_owned() } else { format!("{} ticks", ft) };
                            ui.label("fuse_timer"); ui.label(ft_label);                                         ui.end_row();
                            ui.label("owner_team"); ui.label(format!("{}", (*mine).owner_team));                ui.end_row();
                            ui.label("slot_id");    ui.label(format!("{}", (*mine).slot_id));                   ui.end_row();
                        });
                    });
            }

            // --- CTaskOilDrum-specific fields ---
            if name == "CTaskOilDrum" {
                let drum = addr as *const CTaskOilDrum;
                egui::CollapsingHeader::new("CTaskOilDrum")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("drum_grid").striped(true).show(ui, |ui| {
                            ui.label("pos_x");      ui.label(format!("{:.1}", (*drum).base.pos_x.to_f32()));    ui.end_row();
                            ui.label("pos_y");      ui.label(format!("{:.1}", (*drum).base.pos_y.to_f32()));    ui.end_row();
                            ui.label("speed_x");    ui.label(format!("{:.4}", (*drum).base.speed_x.to_f32())); ui.end_row();
                            ui.label("speed_y");    ui.label(format!("{:.4}", (*drum).base.speed_y.to_f32())); ui.end_row();
                            ui.label("health");     ui.label(format!("{}", (*drum).health));                    ui.end_row();
                            ui.label("on_fire");    ui.label(format!("{}", (*drum).on_fire()));                 ui.end_row();
                            ui.label("triggered");  ui.label(format!("{}", (*drum).triggered != 0));            ui.end_row();
                            ui.label("slot_id");    ui.label(format!("{}", (*drum).slot_id));                   ui.end_row();
                        });
                    });
            }

            // --- CTaskCloud-specific fields ---
            if name == "CTaskCloud" {
                let cloud = addr as *const CTaskCloud;
                egui::CollapsingHeader::new("CTaskCloud")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("cloud_grid").striped(true).show(ui, |ui| {
                            ui.label("pos_x");       ui.label(format!("{:.1}", (*cloud).pos_x.to_f32()));        ui.end_row();
                            ui.label("pos_y");       ui.label(format!("{:.1}", (*cloud).pos_y.to_f32()));        ui.end_row();
                            ui.label("vel_x");       ui.label(format!("{:.4}", (*cloud).vel_x.to_f32()));        ui.end_row();
                            ui.label("vel_y");       ui.label(format!("{:.4}", (*cloud).vel_y.to_f32()));        ui.end_row();
                            ui.label("wind_accel");  ui.label(format!("{:.4}", (*cloud).wind_accel.to_f32()));   ui.end_row();
                            ui.label("wind_target"); ui.label(format!("{:.4}", (*cloud).wind_target.to_f32()));  ui.end_row();
                            ui.label("layer_depth"); ui.label(format!("{:.1}", (*cloud).layer_depth.to_f32())); ui.end_row();
                            ui.label("sprite_id");   ui.label(format!("{:#06X}", (*cloud).sprite_id));          ui.end_row();
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
                            ui.label("spawn_x");       ui.label(format!("{:.1}", (*fire).spawn_x.to_f32()));    ui.end_row();
                            ui.label("spawn_y");       ui.label(format!("{:.1}", (*fire).spawn_y.to_f32()));    ui.end_row();
                            ui.label("timer");         ui.label(format!("{}", (*fire).timer));                  ui.end_row();
                            ui.label("burn_rate");     ui.label(format!("{}", (*fire).burn_rate));              ui.end_row();
                            ui.label("spread_ctr");    ui.label(format!("{}", (*fire).spread_counter));         ui.end_row();
                            ui.label("lifetime");      ui.label(format!("{}", (*fire).lifetime));               ui.end_row();
                            ui.label("slot_index");    ui.label(format!("{}", (*fire).slot_index));             ui.end_row();
                        });
                    });
            }

            // --- CTaskWorm-specific fields ---
            if name == "CTaskWorm" || (*task).class_type == openwa_core::class_type::ClassType::Worm {
                let worm = addr as *const CTaskWorm;
                egui::CollapsingHeader::new("CTaskWorm")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("worm_grid").striped(true).show(ui, |ui| {
                            ui.label("state");      ui.label(format!("{:#04X}", (*worm).state()));              ui.end_row();
                            ui.label("team_index"); ui.label(format!("{}", (*worm).team_index));                ui.end_row();
                            ui.label("worm_index"); ui.label(format!("{}", (*worm).worm_index));                ui.end_row();
                            ui.label("pos_x");      ui.label(format!("{:.2}", (*worm).base.pos_x.to_f32()));   ui.end_row();
                            ui.label("pos_y");      ui.label(format!("{:.2}", (*worm).base.pos_y.to_f32()));   ui.end_row();
                            ui.label("speed_x");    ui.label(format!("{:.4}", (*worm).base.speed_x.to_f32())); ui.end_row();
                            ui.label("speed_y");    ui.label(format!("{:.4}", (*worm).base.speed_y.to_f32())); ui.end_row();
                            let name_bytes = &(*worm).worm_name;
                            let nul = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
                            let worm_name = std::str::from_utf8(&name_bytes[..nul]).unwrap_or("?");
                            ui.label("name");       ui.label(worm_name);                                        ui.end_row();
                        });
                    });
            }

        }});
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
