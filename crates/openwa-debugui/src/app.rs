use eframe::egui;
use openwa_game::address::va;
use openwa_game::engine::{GameWorld, game_session};
use openwa_game::entity::{
    BaseEntity, CloudEntity, FireEntity, MatchCtx, TeamEntity, WorldRootEntity, WormEntity,
};
use openwa_game::rebase::rb;
use openwa_game::registry;
use openwa_game::render::capture::{self as render_capture, CapturedData, RenderCapture};

use crate::log;

// ---------------------------------------------------------------------------
// Known entity types for census display
// ---------------------------------------------------------------------------

/// Vtables of entities that are created/destroyed every frame (particles,
/// bubbles, etc.). Filtered from the census by default to reduce noise.
const TRANSIENT_VTABLES: &[u32] = &[va::SEA_BUBBLE_ENTITY_VTABLE];

fn vtable_name(runtime_vtable: u32) -> Option<&'static str> {
    let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);
    let ghidra_va = runtime_vtable.wrapping_sub(delta);
    openwa_game::registry::vtable_class_name(ghidra_va)
}

/// Returns a display name for the entity at `addr`.
/// Tries the known-vtable map first; falls back to BaseEntity.class_type.
unsafe fn entity_type_name(addr: u32) -> String {
    unsafe {
        if addr == 0 {
            return "(null)".to_owned();
        }
        let vtable = *(addr as *const u32);
        if let Some(name) = vtable_name(vtable) {
            return name.to_owned();
        }
        let entity = addr as *const BaseEntity;
        format!("{:?}", (*entity).class_type)
    }
}

/// One-line label for a entity: "TypeName @ 0xADDR"
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

/// Returns a pointer to GameWorld, or None if not in-game.
unsafe fn get_game_world() -> Option<*const GameWorld> {
    unsafe {
        let ptr = game_session::get_game_world();
        if ptr.is_null() { None } else { Some(ptr) }
    }
}

/// Returns a pointer to GameWorld, or None if not in-game.
unsafe fn get_game_world_mut() -> Option<*mut GameWorld> {
    unsafe {
        let ptr = game_session::get_game_world();
        if ptr.is_null() { None } else { Some(ptr) }
    }
}

/// Unlock all weapons: set ammo to unlimited (-1) and delays to 0 for all teams.
unsafe fn cheat_unlock_all_weapons() {
    unsafe {
        let Some(world) = get_game_world_mut() else {
            log::push("[Cheats] Not in game");
            return;
        };
        let arena = &mut (*world).team_arena;
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

/// Read child entity pointers from a BaseEntity's children array.
///
/// The array is **sparse**: slots are nulled when a child is removed rather than
/// compacted. `children_watermark` is the insertion counter (loop upper bound used
/// by BaseEntity::HandleMessage), not the live-child count. We return all slots up to
/// that bound so the caller can filter nulls and display the live set.
unsafe fn read_children(entity: *const BaseEntity) -> Vec<u32> {
    unsafe {
        let slots = (*entity).children_watermark as usize;
        let data = (*entity).children_data as *const u32;
        if data.is_null() || slots == 0 {
            return Vec::new();
        }
        // Hard safety cap: 4096 slots × 4 bytes = 16 KB max read
        let slots = slots.min(4096);
        (0..slots).map(|i| *data.add(i)).collect()
    }
}

// ---------------------------------------------------------------------------
// Live entity snapshot (built once per frame via full entity-tree traversal)
// ---------------------------------------------------------------------------

/// Walk up parent pointers from `start` to find the root entity (no parent).
/// Returns None if entity_land is null or the chain doesn't terminate within
/// MAX_DEPTH steps (guard against corrupt/circular pointers).
unsafe fn find_root_task(world: *const GameWorld) -> Option<u32> {
    unsafe {
        let entity_land = (*world).entity_land as u32;
        if entity_land == 0 {
            return None;
        }
        let mut current = entity_land;
        for _ in 0..64 {
            let parent = (*(current as *const BaseEntity)).parent as u32;
            if parent == 0 {
                return Some(current);
            }
            current = parent;
        }
        None // chain didn't terminate — corrupt data
    }
}

/// DFS the entity tree from `root`, returning (vtable, addr) for every node.
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
            for child in read_children(addr as *const BaseEntity) {
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
        let Some(world) = get_game_world() else {
            return Vec::new();
        };
        let Some(root) = find_root_task(world) else {
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
    /// Most recent render-queue capture, if any.
    last_capture: Option<RenderCapture>,
    /// Whether the render-capture floating window is open.
    show_capture_window: bool,
    /// Per-command-class checkbox state for the capture viewer's filter row.
    /// Slots 0..=0xE are legacy `cmd_type` values; slot 0xF is unused
    /// (kept to align with the legacy-type numbering); slots
    /// `FILTER_TYPED_BASE..` cover each [`RenderMessage`] variant in
    /// `TYPED_VARIANT_NAMES` order.
    capture_type_filter: [bool; FILTER_SLOTS],
    /// Index into `last_capture.commands` of the row currently shown in
    /// the capture detail pane.
    selected_capture_idx: Option<usize>,
    /// Whether the dispatcher should run in step-through mode. Mirrors
    /// `render_capture::is_step_mode`; the UI is the source of truth and
    /// pushes changes via `set_step_mode`.
    step_mode: bool,
    /// Cap on commands dispatched per frame while step mode is on, in
    /// dispatch order. Pushed to `render_capture::set_step_limit` whenever
    /// the slider moves.
    step_limit: u32,
    /// Edge-detection state for the global F9 toggle. Set when F9 is held
    /// and cleared when released, so each press fires exactly once even if
    /// the user holds the key for multiple egui repaint cycles.
    f9_was_down: bool,
}

impl Default for DebugApp {
    fn default() -> Self {
        Self {
            selected_entity: None,
            nav_history: Vec::new(),
            log_auto_scroll: true,
            show_transient: false,
            last_capture: None,
            show_capture_window: false,
            capture_type_filter: [true; FILTER_SLOTS],
            selected_capture_idx: None,
            step_mode: false,
            step_limit: u32::MAX,
            f9_was_down: false,
        }
    }
}

/// Toggle Freeze + step via global keyboard state. Returns the new
/// `step_mode` value when F9 transitioned from up→down this tick;
/// `None` otherwise.
///
/// Uses `GetAsyncKeyState` (not egui's input event stream) so the hotkey
/// works regardless of which window has OS focus — the user normally has
/// the game window focused, not the debug-UI window.
fn poll_f9_hotkey(was_down: &mut bool, current_step_mode: bool) -> Option<bool> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F9};
    // High-order bit of the i16 return = currently held; low-order bit =
    // pressed since last call (per-thread). We track the held bit
    // ourselves so the edge fires once per press across egui repaints.
    let raw = unsafe { GetAsyncKeyState(VK_F9 as i32) };
    let is_down = (raw as u16 & 0x8000) != 0;
    let just_pressed = is_down && !*was_down;
    *was_down = is_down;
    just_pressed.then_some(!current_step_mode)
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

        // Global F9 hotkey for Freeze + step toggle. Polled here (not via
        // egui input events) so it fires regardless of which window has
        // OS focus.
        if let Some(new_state) = poll_f9_hotkey(&mut self.f9_was_down, self.step_mode) {
            let was_on = self.step_mode;
            self.step_mode = new_state;
            render_capture::set_step_mode(self.step_mode);
            if self.step_mode && !was_on {
                // Match the checkbox path: open the viewer and seed the
                // step slider with the current capture's command count
                // (or the cached one if none has landed yet).
                self.show_capture_window = true;
                if let Some(cap) = self.last_capture.as_ref() {
                    self.step_limit = cap.commands.len() as u32;
                    render_capture::set_step_limit(self.step_limit);
                }
            }
            log::push(if self.step_mode {
                "[render] F9 → freeze + step ON"
            } else {
                "[render] F9 → freeze + step OFF"
            });
        }

        // Drain any pending render-queue capture into the dedicated viewer.
        // In step mode this fires every frame; we treat it as a silent
        // refresh — log noise only on the manual one-shot path so step
        // mode doesn't flood the log panel.
        if let Some(capture) = render_capture::take_capture() {
            let new_len = capture.commands.len() as u32;
            // If the queue size changed since the previous capture (most
            // commonly: pause-unpause cycle, or a pre-pause/post-pause
            // frame mismatch in step mode) snap the step slider back to
            // the new max. The old step value would be meaningless
            // against a different command list.
            let queue_changed = self
                .last_capture
                .as_ref()
                .is_some_and(|prev| prev.commands.len() as u32 != new_len);
            if queue_changed && self.step_mode {
                self.step_limit = new_len;
                render_capture::set_step_limit(new_len);
            }

            if !self.step_mode {
                log::push(format!(
                    "[render] captured {} commands  cam=({:.1},{:.1})  pivot=({:.1},{:.1})",
                    capture.commands.len(),
                    capture.clip.cam_x.to_f32(),
                    capture.clip.cam_y.to_f32(),
                    capture.clip.pivot_x.to_f32(),
                    capture.clip.pivot_y.to_f32(),
                ));
                self.show_capture_window = true;
                self.selected_capture_idx = None;
            }
            self.last_capture = Some(capture);
        }

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
                ui.menu_button("Render", |ui| {
                    let pending = render_capture::is_pending();
                    let label = if pending {
                        "Capture armed — waiting…"
                    } else {
                        "Capture next frame"
                    };
                    if ui.add_enabled(!pending, egui::Button::new(label)).clicked() {
                        render_capture::request_capture();
                        log::push("[render] capture armed");
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.last_capture.is_some(),
                            egui::Button::new("Show last capture"),
                        )
                        .clicked()
                    {
                        self.show_capture_window = true;
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

        self.show_capture_window(&ctx);
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
// Raw field viewer for WorldEntity-derived types
// ---------------------------------------------------------------------------

/// WorldEntity field labels for the base class region (0x00..0xFC).
/// Display all DWORDs of a WorldEntity-derived entity with labelled fields.
///
/// Field names are resolved from the global registry via inheritance-aware
/// lookup (entity → WorldEntity → BaseEntity). No hardcoded label tables needed.
unsafe fn show_game_entity_raw_fields(
    ui: &mut egui::Ui,
    addr: u32,
    type_name: &str,
    total_size: usize,
) {
    unsafe {
        let base = addr as *const u32;
        let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);

        // Sections: BaseEntity base, WorldEntity unknowns, pos/speed, more unknowns, emitter,
        // then type-specific in 0x80-byte chunks to keep each section manageable.
        let mut sections: Vec<(usize, usize, String)> = vec![
            (0x000, 0x030, "BaseEntity base".into()),
            (0x030, 0x084, "WorldEntity +0x30".into()),
            (0x084, 0x098, "pos / speed / angle".into()),
            (0x098, 0x0E8, "WorldEntity +0x98".into()),
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

                    let entity = addr as *const BaseEntity;

                    // --- BaseEntity base ---
                    egui::CollapsingHeader::new("BaseEntity base")
                        .default_open(true)
                        .show(ui, |ui| {
                            egui::Grid::new("ctask_grid").striped(true).show(ui, |ui| {
                                // Parent — clickable link
                                let parent = (*entity).parent as u32;
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
                                    (*entity).children_watermark,
                                    (*entity).children_capacity
                                ));
                                ui.end_row();
                                ui.label("class_type");
                                ui.label(format!("{:?}", (*entity).class_type));
                                ui.end_row();
                            });
                        });

                    // --- Children tree ---
                    // read_children returns all slots (sparse); filter nulls for live count.
                    let children = read_children(entity);
                    let live_count = children.iter().filter(|&&a| a != 0).count();
                    let slot_count = (*entity).children_watermark as usize;
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
                                let child_task = child_addr as *const BaseEntity;
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

                    // --- MineEntity-specific fields ---
                    if name == "MineEntity" {
                        show_game_entity_raw_fields(ui, addr, "MineEntity", 0x128);
                    }

                    // --- OilDrumEntity-specific fields ---
                    if name == "OilDrumEntity" {
                        show_game_entity_raw_fields(ui, addr, "OilDrumEntity", 0x110);
                    }

                    // --- CrateEntity-specific fields ---
                    if name == "CrateEntity" {
                        show_game_entity_raw_fields(ui, addr, "CrateEntity", 0x4B0);
                    }

                    // --- CloudEntity-specific fields ---
                    if name == "CloudEntity" {
                        let cloud = addr as *const CloudEntity;
                        egui::CollapsingHeader::new("CloudEntity")
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

                    // --- WorldRootEntity-specific fields ---
                    if name == "WorldRootEntity" {
                        let tg = addr as *const WorldRootEntity;
                        egui::CollapsingHeader::new("WorldRootEntity")
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

                        let ctx = &(*tg).game_ctx as *const MatchCtx;
                        egui::CollapsingHeader::new("MatchCtx (+0x30)")
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

                    // --- TeamEntity-specific fields ---
                    if name == "TeamEntity" {
                        let team = addr as *const TeamEntity;
                        egui::CollapsingHeader::new("TeamEntity")
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

                    // --- FireEntity-specific fields ---
                    if name == "FireEntity" {
                        let fire = addr as *const FireEntity;
                        egui::CollapsingHeader::new("FireEntity")
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

                    // --- WormEntity-specific fields ---
                    if name == "WormEntity"
                        || (*entity).class_type == openwa_game::game::ClassType::Worm
                    {
                        // Summary header with key info
                        let worm = addr as *const WormEntity;
                        let name_bytes = &(*worm).worm_name;
                        let nul = name_bytes
                            .iter()
                            .position(|&b| b == 0)
                            .unwrap_or(name_bytes.len());
                        let worm_name = std::str::from_utf8(&name_bytes[..nul]).unwrap_or("?");
                        ui.label(format!(
                            "Worm: \"{}\"  state={:#04X}  team={}  idx={}",
                            worm_name,
                            (*worm).state().0,
                            (*worm).team_index,
                            (*worm).worm_index
                        ));
                        ui.separator();

                        show_game_entity_raw_fields(ui, addr, "WormEntity", 0x3FC);
                    }

                    // --- MissileEntity-specific fields ---
                    if name == "MissileEntity" {
                        use openwa_game::entity::MissileEntity;
                        let m = &*(addr as *const MissileEntity);
                        ui.label(format!(
                            "Missile: type={:?}  slot={}  homing={}  dir={}",
                            m.missile_type, m.activity_rank_slot, m.homing_enabled, m.direction
                        ));
                        ui.separator();

                        show_game_entity_raw_fields(ui, addr, "MissileEntity", 0x41C);
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

// ---------------------------------------------------------------------------
// Floating window: Render-queue capture viewer
// ---------------------------------------------------------------------------
//
// Filter slot indexing scheme:
//   slots 0..=0xE → legacy `cmd_type` values (one per dispatcher case)
//   slot 0xF      → reserved/padding (no command maps here)
//   slots 0x10+   → one per `RenderMessage` variant in TYPED_VARIANT_NAMES order
const FILTER_TYPED_BASE: usize = 0x10;
const FILTER_SLOTS: usize = FILTER_TYPED_BASE + render_capture::TYPED_VARIANT_COUNT;

fn type_filter_slot(cmd: &render_capture::CapturedCommand) -> usize {
    match &cmd.data {
        CapturedData::Legacy(_) => {
            if cmd.cmd_type <= 0xE {
                cmd.cmd_type as usize
            } else {
                0xF
            }
        }
        CapturedData::Typed(msg) => FILTER_TYPED_BASE + render_capture::typed_variant_index(msg),
    }
}

fn type_filter_label(slot: usize) -> &'static str {
    match slot {
        0x0 => "FillRect",
        0x1 => "BitmapGlobal",
        0x2 => "TextboxLocal",
        0x3 => "ViaCallback",
        0x4 => "SpriteGlobal",
        0x5 => "SpriteLocal",
        0x6 => "SpriteOffset",
        0x7 => "Polyline",
        0x8 => "LineStrip",
        0x9 => "Polygon",
        0xA => "PixelStrip",
        0xB => "Crosshair",
        0xC => "OutlinedPixel",
        0xD => "TiledBitmap",
        0xE => "TiledTerrain",
        0xF => "(reserved)",
        s if (FILTER_TYPED_BASE..FILTER_SLOTS).contains(&s) => {
            render_capture::TYPED_VARIANT_NAMES[s - FILTER_TYPED_BASE]
        }
        _ => "?",
    }
}

impl DebugApp {
    fn show_capture_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_capture_window;
        egui::Window::new("Render Capture")
            .open(&mut open)
            .default_size([960.0, 520.0])
            .resizable(true)
            .show(ctx, |ui| {
                let Some(capture) = self.last_capture.as_ref() else {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        "No capture yet. Use Render → Capture next frame.",
                    );
                    return;
                };

                // Header line + per-type counts.
                let mut counts = [0usize; FILTER_SLOTS];
                for cmd in &capture.commands {
                    counts[type_filter_slot(cmd)] += 1;
                }
                ui.label(format!(
                    "{} commands  cam=({:.1},{:.1})  pivot=({:.1},{:.1})",
                    capture.commands.len(),
                    capture.clip.cam_x.to_f32(),
                    capture.clip.cam_y.to_f32(),
                    capture.clip.pivot_x.to_f32(),
                    capture.clip.pivot_y.to_f32(),
                ));

                // Step-through controls: when on, the dispatcher captures
                // every frame and runs only the first N commands. Slider
                // max tracks the latest capture's command count so the
                // user can scrub over the full frame.
                ui.separator();
                let step_max = capture.commands.len().max(1) as u32;
                ui.horizontal(|ui| {
                    let was_on = self.step_mode;
                    let resp = ui
                        .checkbox(&mut self.step_mode, "Freeze + step  (F9)")
                        .on_hover_text(
                            "Pauses the simulation and dispatches only the first N \
                             commands per frame (slider). Without freeze the queue \
                             contents shift every frame and indices drift.\n\n\
                             Hotkey: F9 (works regardless of focused window)",
                        );
                    if resp.changed() {
                        render_capture::set_step_mode(self.step_mode);
                        if self.step_mode && !was_on {
                            // Entering step mode → start with the full
                            // frame visible; user scrubs back from there.
                            self.step_limit = capture.commands.len() as u32;
                            render_capture::set_step_limit(self.step_limit);
                        }
                    }
                    ui.add_enabled_ui(self.step_mode, |ui| {
                        // Clamp on the user side so a stale slider value
                        // from a previous (longer) frame doesn't show as
                        // out-of-range.
                        if self.step_limit > step_max {
                            self.step_limit = step_max;
                            render_capture::set_step_limit(self.step_limit);
                        }
                        let resp = ui.add(
                            egui::Slider::new(&mut self.step_limit, 0..=step_max)
                                .text(format!("step / {}", step_max))
                                .integer(),
                        );
                        if resp.changed() {
                            render_capture::set_step_limit(self.step_limit);
                        }
                        if ui.small_button("⟸").clicked() && self.step_limit > 0 {
                            self.step_limit -= 1;
                            render_capture::set_step_limit(self.step_limit);
                        }
                        if ui.small_button("⟹").clicked() && self.step_limit < step_max {
                            self.step_limit += 1;
                            render_capture::set_step_limit(self.step_limit);
                        }
                    });

                    // Keyboard shortcuts when the capture window has focus.
                    // Skipped if any widget wants keyboard input (slider /
                    // text edit) so we don't double-step against egui's
                    // native arrow-key handling on focused widgets.
                    if self.step_mode && !ctx.egui_wants_keyboard_input() {
                        let mut new_val = self.step_limit;
                        ctx.input(|i| {
                            if i.key_pressed(egui::Key::ArrowLeft) && new_val > 0 {
                                new_val -= 1;
                            }
                            if i.key_pressed(egui::Key::ArrowRight) && new_val < step_max {
                                new_val += 1;
                            }
                            if i.key_pressed(egui::Key::PageDown) {
                                new_val = new_val.saturating_sub(10);
                            }
                            if i.key_pressed(egui::Key::PageUp) {
                                new_val = (new_val + 10).min(step_max);
                            }
                            if i.key_pressed(egui::Key::Home) {
                                new_val = 0;
                            }
                            if i.key_pressed(egui::Key::End) {
                                new_val = step_max;
                            }
                        });
                        if new_val != self.step_limit {
                            self.step_limit = new_val;
                            render_capture::set_step_limit(new_val);
                        }
                    }
                });

                // Filter row: per-type checkbox with live count, only for
                // types that occur in this capture (keeps the row compact).
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.label("Filter:");
                    if ui.small_button("all").clicked() {
                        self.capture_type_filter = [true; FILTER_SLOTS];
                    }
                    if ui.small_button("none").clicked() {
                        self.capture_type_filter = [false; FILTER_SLOTS];
                    }
                    for (slot, &count) in counts.iter().enumerate() {
                        if count == 0 {
                            continue;
                        }
                        let label = format!("{} ({})", type_filter_label(slot), count);
                        ui.checkbox(&mut self.capture_type_filter[slot], label);
                    }
                });
                ui.separator();

                // Pre-compute the visible-rows projection once; both panes
                // need the count for selection bounds-checking.
                let filter = self.capture_type_filter;
                let visible: Vec<(usize, &_)> = capture
                    .commands
                    .iter()
                    .enumerate()
                    .filter(|(_, cmd)| filter[type_filter_slot(cmd)])
                    .collect();
                ui.label(format!(
                    "Showing {}/{} commands",
                    visible.len(),
                    capture.commands.len()
                ));

                // Two-pane layout: list on the left, detail on the right.
                // The detail pane uses a SidePanel inside the window so the
                // user can drag the splitter; min-width keeps the list usable.
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                let avail_h = ui.available_height().max(120.0);

                // `selected_capture_idx` is borrowed disjointly from
                // `last_capture` (the source of `capture`), so we hand the
                // mutable reference to the helpers instead of going through
                // `&mut self` — that would otherwise re-borrow the whole
                // struct and conflict with the active immutable borrow.
                let selected = &mut self.selected_capture_idx;
                let step_mode = self.step_mode;
                let step_limit = self.step_limit;

                // Out-param: a "Step to here" click sets this; the outer
                // function then enables step mode (if needed) and pushes
                // the new step limit to the dispatcher.
                let mut requested_step_limit: Option<u32> = None;

                ui.horizontal_top(|ui| {
                    let list_width = (ui.available_width() * 0.55).max(360.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(list_width, avail_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            show_capture_list(
                                ui,
                                &visible,
                                row_height,
                                selected,
                                step_mode,
                                step_limit,
                                &mut requested_step_limit,
                            );
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), avail_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            show_capture_detail(
                                ui,
                                capture,
                                selected,
                                step_mode,
                                &mut requested_step_limit,
                            );
                        },
                    );
                });

                // Apply any "Step to here" request from the list/detail
                // panes. Auto-enables step mode + freeze if not already
                // on, since "step to here" without freeze is meaningless
                // (the queue would shift before the user could see it).
                if let Some(new_limit) = requested_step_limit {
                    if !self.step_mode {
                        self.step_mode = true;
                        render_capture::set_step_mode(true);
                    }
                    self.step_limit = new_limit;
                    render_capture::set_step_limit(new_limit);
                }
            });
        if !open && self.show_capture_window {
            // Window was just closed via the X — exit step mode so the
            // game resumes full-frame rendering. No matching `disable` UI
            // exists once the window is gone, so this is the only safe
            // place to clear the flag.
            if self.step_mode {
                self.step_mode = false;
                render_capture::set_step_mode(false);
            }
        }
        self.show_capture_window = open;
    }
}

fn show_capture_list(
    ui: &mut egui::Ui,
    visible: &[(usize, &render_capture::CapturedCommand)],
    row_height: f32,
    selected: &mut Option<usize>,
    step_mode: bool,
    step_limit: u32,
    requested_step_limit: &mut Option<u32>,
) {
    let current = *selected;
    egui::ScrollArea::vertical()
        .id_salt("capture_list")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, visible.len(), |ui, row_range| {
            for row in row_range {
                let (idx, cmd) = visible[row];
                let mut color = match cmd.data {
                    CapturedData::Typed(_) => egui::Color32::LIGHT_GREEN,
                    CapturedData::Legacy(_) => egui::Color32::LIGHT_BLUE,
                };
                // Fade rows past the step boundary: in step mode these
                // commands are not dispatched this frame, so they're
                // visually deemphasised but still inspectable.
                if step_mode && (idx as u32) >= step_limit {
                    color = color.linear_multiply(0.35);
                }
                let line = format!("#{:>4}  {}", idx, render_capture::format_command(cmd));
                let is_selected = current == Some(idx);
                let resp = ui.selectable_label(
                    is_selected,
                    egui::RichText::new(line).monospace().color(color),
                );
                if resp.clicked() {
                    *selected = Some(idx);
                }
                // Right-click → "Step to this command". Sets step_limit so
                // this command is the last one dispatched (idx + 1 in
                // the dispatcher's 0-based count semantics).
                resp.context_menu(|ui| {
                    if ui.button("Step to this command").clicked() {
                        *requested_step_limit = Some(idx as u32 + 1);
                        *selected = Some(idx);
                        ui.close();
                    }
                    if ui.button("Step to just before").clicked() {
                        *requested_step_limit = Some(idx as u32);
                        *selected = Some(idx);
                        ui.close();
                    }
                });
            }
        });
}

fn show_capture_detail(
    ui: &mut egui::Ui,
    capture: &render_capture::RenderCapture,
    selected: &mut Option<usize>,
    step_mode: bool,
    requested_step_limit: &mut Option<u32>,
) {
    let Some(idx) = *selected else {
        ui.colored_label(
            egui::Color32::GRAY,
            "Click a command on the left to inspect.",
        );
        return;
    };
    let Some(cmd) = capture.commands.get(idx) else {
        *selected = None;
        return;
    };

    ui.horizontal(|ui| {
        ui.heading(format!("#{}", idx));
        ui.label(egui::RichText::new(render_capture::captured_name(cmd)).strong());
        ui.label(format!("layer {}", cmd.layer));
        let kind = match &cmd.data {
            CapturedData::Typed(_) => ("Typed", egui::Color32::LIGHT_GREEN),
            CapturedData::Legacy(_) => ("Legacy", egui::Color32::LIGHT_BLUE),
        };
        ui.colored_label(kind.1, kind.0);
        // Step-to-this-command shortcut. Available regardless of
        // step_mode state (the outer applier auto-enables step mode if
        // needed); the label changes to flag the auto-enable side
        // effect when step mode is currently off.
        let label = if step_mode {
            "Step to this →"
        } else {
            "Freeze + step to this →"
        };
        if ui
            .button(label)
            .on_hover_text("Sets the step slider so this command is the last one dispatched.")
            .clicked()
        {
            *requested_step_limit = Some(idx as u32 + 1);
        }
    });
    ui.separator();

    let rows = render_capture::decode_command(cmd);
    let delta = rb(va::IMAGE_BASE).wrapping_sub(va::IMAGE_BASE);

    egui::ScrollArea::vertical()
        .id_salt("capture_detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new(format!("capture_fields_{}", idx))
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    ui.strong("Offset");
                    ui.strong("Field");
                    ui.strong("Value");
                    ui.strong("Points to");
                    ui.end_row();

                    for field in &rows {
                        ui.label(match field.offset {
                            Some(off) => format!("+0x{:03X}", off),
                            None => "—".into(),
                        });
                        ui.label(&field.name);
                        ui.label(egui::RichText::new(&field.value).monospace());
                        // Pointer identification — only for raw u32s
                        // that look pointer-shaped. The threshold of
                        // 0x10000 matches the existing inspector's
                        // heuristic for excluding small ints/flags.
                        // SAFETY: identify_pointer reads game memory to
                        // resolve vtables and registered live objects;
                        // safe to call from the UI thread because it
                        // never writes and tolerates dangling pointers
                        // (returns None instead of dereferencing).
                        let ptr_label = field.raw.filter(|&v| v >= 0x10000).and_then(|v| unsafe {
                            openwa_game::mem::identify_pointer(v, delta).and_then(|id| id.name)
                        });
                        if let Some(label) = ptr_label {
                            ui.colored_label(egui::Color32::LIGHT_BLUE, format!("→ {}", label));
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });

            if !cmd.vertices.is_empty() {
                ui.separator();
                egui::CollapsingHeader::new(format!("Vertices ({})", cmd.vertices.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new(format!("capture_verts_{}", idx))
                            .striped(true)
                            .num_columns(4)
                            .show(ui, |ui| {
                                ui.strong("#");
                                ui.strong("x");
                                ui.strong("y");
                                ui.strong("z");
                                ui.end_row();
                                for (i, v) in cmd.vertices.iter().enumerate() {
                                    let x_f = v[0] as f32 / 65536.0;
                                    let y_f = v[1] as f32 / 65536.0;
                                    ui.label(format!("{}", i));
                                    ui.label(format!("{:.2}", x_f));
                                    ui.label(format!("{:.2}", y_f));
                                    ui.label(format!("{:#X}", v[2]));
                                    ui.end_row();
                                }
                            });
                    });
            }
        });
}
