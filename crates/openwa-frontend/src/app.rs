//! egui application: match-launcher window.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use openwa_config::DiscoveredFile;
use openwa_core::scheme::{SchemeFile, SchemeVersion};
use openwa_core::wgt::{WgtFile, WgtTeam};
use openwa_game::engine::pending_match::{PendingCustomMatch, PendingTeam};

use crate::launch::{self, LaunchOutcome};

/// Env var that enables the debug section (GameInfo dump + watchpoints).
const DEBUG_FEATURES_ENV: &str = "OPENWA_LAUNCHER_DEBUG";

pub struct MatchLauncherApp {
    /// Schemes discovered under `User/Schemes` at startup. Empty if the
    /// install can't be located.
    schemes: Vec<DiscoveredFile>,
    /// Index into `schemes` selected by the dropdown, or `None` for the
    /// fallback all-zero stub scheme.
    scheme_choice: Option<usize>,
    /// Last scheme-load attempt result (cached so the UI keeps status
    /// across frames).
    scheme_status: SchemeStatus,
    /// `.WGT` files discovered under `User/Teams` at startup.
    wgt_files: Vec<DiscoveredFile>,
    /// Index into `wgt_files` of the currently-active roster.
    wgt_choice: Option<usize>,
    /// Parsed roster matching `wgt_choice` (`None` until first load).
    loaded_wgt: Option<LoadedWgt>,
    /// Selected team slot from `loaded_wgt` for the two match seats.
    team_a_idx: usize,
    team_b_idx: usize,
    /// Landscape-generator seed (`G_GEN_MAP_SEED`). Distinct from any
    /// scheme RNG state — this single u32 determines the entire map.
    map_seed: u32,
    /// Whether the debug-features section is rendered at all (env-gated).
    debug_features_enabled: bool,
    dump_label: String,
    log: Arc<Mutex<Vec<String>>>,
}

struct LoadedWgt {
    path: PathBuf,
    file: WgtFile,
}

#[derive(Clone, Debug, Default)]
enum SchemeStatus {
    #[default]
    NotLoaded,
    Loaded {
        version: SchemeVersion,
        path: PathBuf,
    },
    Error(String),
}

impl Default for MatchLauncherApp {
    fn default() -> Self {
        let schemes = openwa_config::list_schemes();
        let wgt_files = openwa_config::list_team_files();
        // Prefer "{{02}} Intermediate" as a familiar starting point; fall
        // back to whatever's alphabetically first.
        let scheme_choice = schemes
            .iter()
            .position(|s| s.name.contains("Intermediate"))
            .or_else(|| (!schemes.is_empty()).then_some(0));
        let wgt_choice = wgt_files
            .iter()
            .position(|w| w.name.eq_ignore_ascii_case("WG"))
            .or_else(|| (!wgt_files.is_empty()).then_some(0));
        let mut app = Self {
            schemes,
            scheme_choice,
            scheme_status: SchemeStatus::NotLoaded,
            wgt_files,
            wgt_choice,
            loaded_wgt: None,
            team_a_idx: 0,
            team_b_idx: 1,
            // Use the system RNG once so the first launch isn't always
            // the same seed across processes; the Regenerate button bumps it.
            map_seed: rand::random::<u32>(),
            debug_features_enabled: std::env::var(DEBUG_FEATURES_ENV).is_ok(),
            dump_label: "before".to_owned(),
            log: Arc::default(),
        };
        // Best-effort roster load so the team dropdowns are populated on
        // first frame.
        app.reload_wgt();
        app
    }
}

impl MatchLauncherApp {
    fn push_log(&self, line: impl Into<String>) {
        if let Ok(mut g) = self.log.lock() {
            g.push(line.into());
            if g.len() > 64 {
                let drop = g.len() - 64;
                g.drain(..drop);
            }
        }
    }

    /// (Re)load the WGT file currently selected by `wgt_choice`.
    fn reload_wgt(&mut self) {
        let Some(idx) = self.wgt_choice else {
            self.loaded_wgt = None;
            return;
        };
        let Some(entry) = self.wgt_files.get(idx).cloned() else {
            self.loaded_wgt = None;
            return;
        };
        match WgtFile::from_file(&entry.path) {
            Ok(file) => {
                let n = file.teams.len();
                self.team_a_idx = self.team_a_idx.min(n.saturating_sub(1));
                self.team_b_idx = self.team_b_idx.min(n.saturating_sub(1));
                self.loaded_wgt = Some(LoadedWgt {
                    path: entry.path,
                    file,
                });
            }
            Err(e) => {
                self.push_log(format!("WGT load failed ({}): {e}", entry.path.display()));
                self.loaded_wgt = None;
            }
        }
    }

    fn do_launch(&mut self) {
        let pending = match self.build_pending_match() {
            Ok(p) => p,
            Err(e) => {
                self.push_log(format!("Launch refused: {e}"));
                return;
            }
        };

        let (a, b) = match (pending.teams.first(), pending.teams.get(1)) {
            (Some(a), Some(b)) => (a.name.clone(), b.name.clone()),
            _ => ("?".into(), "?".into()),
        };
        self.push_log(format!("Launch requested: {a} vs {b}"));

        match launch::launch(pending) {
            LaunchOutcome::Scheduled => {
                self.push_log(
                    "Scheduled onto main thread — match will start on next MFC idle tick",
                );
            }
            LaunchOutcome::Refused(why) => self.push_log(format!("Launch refused: {why}")),
        }
    }

    /// Build a `PendingCustomMatch` from the current UI fields. Errors
    /// out (rather than substituting silent defaults) when the scheme is
    /// missing or unreadable, so the user sees the failure instead of a
    /// surprise empty scheme inside the match.
    fn build_pending_match(&mut self) -> Result<PendingCustomMatch, String> {
        let scheme = self.load_scheme_for_launch()?;
        let teams = self.build_teams_from_wgt()?;
        Ok(PendingCustomMatch {
            game_version: 500,
            type_label: None,
            teams,
            scheme,
            map_seed: self.map_seed,
            // Pinned to WA's frontend mid-slider default (0x80 / 20 = bin 6).
            // The slider was removed from the UI after we confirmed it
            // doesn't affect visible terrain — see memory note
            // `project_terrain_pct_slider_is_not_visual`. The gameplay
            // byte it feeds (GameInfo + 0xD952) stays at a sensible
            // baseline so we don't silently change drop-density behaviour.
            terrain_pct_raw: 0x80,
        })
    }

    fn build_teams_from_wgt(&self) -> Result<Vec<PendingTeam>, String> {
        let wgt = self
            .loaded_wgt
            .as_ref()
            .ok_or("no .WGT roster loaded — pick one from the Teams dropdown")?;
        if wgt.file.teams.is_empty() {
            return Err(format!("{}: contains zero teams", wgt.path.display()));
        }
        if self.team_a_idx == self.team_b_idx {
            return Err("Team A and Team B refer to the same WGT entry".to_string());
        }
        let pick = |idx: usize, color: u8| -> Result<PendingTeam, String> {
            let entry: &WgtTeam = wgt.file.teams.get(idx).ok_or_else(|| {
                format!(
                    "team index {idx} out of range ({} teams)",
                    wgt.file.teams.len()
                )
            })?;
            Ok(PendingTeam::from_wgt(entry, color))
        };
        Ok(vec![pick(self.team_a_idx, 0)?, pick(self.team_b_idx, 1)?])
    }

    fn load_scheme_for_launch(&mut self) -> Result<SchemeFile, String> {
        let Some(idx) = self.scheme_choice else {
            // Fallback: an all-zero V3 payload. Not playable as-is but
            // keeps the launch path functional when no scheme is picked.
            let zeros = vec![0u8; openwa_core::scheme::SCHEME_PAYLOAD_V3];
            return Ok(SchemeFile {
                version: SchemeVersion::V3,
                payload: zeros,
            });
        };
        let entry = self
            .schemes
            .get(idx)
            .ok_or("scheme selection points past discovered list")?
            .clone();
        match SchemeFile::from_file(&entry.path) {
            Ok(s) => {
                self.scheme_status = SchemeStatus::Loaded {
                    version: s.version,
                    path: entry.path,
                };
                Ok(s)
            }
            Err(e) => {
                let msg = format!("scheme load failed ({}): {e:?}", entry.path.display());
                self.scheme_status = SchemeStatus::Error(msg.clone());
                Err(msg)
            }
        }
    }

    fn make_label(&self, suffix: &str) -> String {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let raw_tag = self.dump_label.trim();
        let tag = if raw_tag.is_empty() { "dump" } else { raw_tag };
        let safe_tag: String = tag
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if suffix.is_empty() {
            format!("{stamp}_{safe_tag}")
        } else {
            format!("{stamp}_{safe_tag}_{suffix}")
        }
    }

    fn do_dump_live(&self) {
        let label = self.make_label("");
        match openwa_game::engine::game_info_snapshot::dump_to_disk(&label) {
            Ok(path) => self.push_log(format!("Dumped GameInfo to {}", path.display())),
            Err(e) => self.push_log(format!("Dump failed: {e}")),
        }
    }

    fn do_dump_snapshot(&self) {
        let label = self.make_label("snapshot");
        match openwa_game::engine::game_info_snapshot::dump_snapshot_to_disk(&label) {
            Ok(path) => self.push_log(format!("Dumped snapshot to {}", path.display())),
            Err(e) => self.push_log(format!("Snapshot dump failed: {e}")),
        }
    }
}

impl eframe::App for MatchLauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));

        ui.heading("OpenWA Match Launcher");
        ui.separator();

        let idle = launch::is_idle_at_frontend();
        ui.horizontal(|ui| {
            ui.label("WA state:");
            if idle {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "idle at frontend");
            } else {
                ui.colored_label(egui::Color32::YELLOW, "in game session");
            }
        });

        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 220.0)
            .show(ui, |ui| {
                self.draw_map_panel(ui);
                ui.add_space(6.0);
                self.draw_scheme_panel(ui);
                ui.add_space(6.0);
                self.draw_teams_panel(ui);

                if self.debug_features_enabled {
                    ui.add_space(8.0);
                    self.draw_debug_features(ui);
                }
            });

        ui.add_space(8.0);
        let can_launch = idle && self.loaded_wgt.is_some();
        let launch_button =
            egui::Button::new(egui::RichText::new("Launch match").strong().size(16.0));
        if ui.add_enabled(can_launch, launch_button).clicked() {
            self.do_launch();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label("Log:");
        egui::ScrollArea::vertical()
            .id_salt("launcher-log")
            .max_height(160.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if let Ok(g) = self.log.lock() {
                    for line in g.iter() {
                        ui.monospace(line);
                    }
                }
            });
    }
}

impl MatchLauncherApp {
    fn draw_map_panel(&mut self, ui: &mut egui::Ui) {
        labeled_group(ui, "Map", |ui| {
            ui.horizontal(|ui| {
                ui.label("Seed:");
                let mut buf = format!("{:08X}", self.map_seed);
                if ui.text_edit_singleline(&mut buf).changed()
                    && let Ok(v) = u32::from_str_radix(buf.trim_start_matches("0x"), 16)
                {
                    self.map_seed = v;
                }
                if ui.button("Regenerate").clicked() {
                    self.map_seed = rand::random::<u32>();
                }
            });
        });
    }

    fn draw_scheme_panel(&mut self, ui: &mut egui::Ui) {
        labeled_group(ui, "Scheme", |ui| {
            if self.schemes.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "No schemes discovered. Check User/Schemes under your WA install.",
                );
                return;
            }
            let selected_label = self
                .scheme_choice
                .and_then(|i| self.schemes.get(i))
                .map(|s| s.name.as_str())
                .unwrap_or("<empty stub>");
            egui::ComboBox::from_label("Scheme (.wsc)")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.scheme_choice, None, "<empty stub>");
                    for (i, s) in self.schemes.iter().enumerate() {
                        ui.selectable_value(&mut self.scheme_choice, Some(i), s.name.as_str());
                    }
                });
            match &self.scheme_status {
                SchemeStatus::NotLoaded => {
                    ui.colored_label(egui::Color32::GRAY, "Scheme not yet loaded.");
                }
                SchemeStatus::Loaded { version, path } => {
                    ui.colored_label(
                        egui::Color32::LIGHT_GREEN,
                        format!("Loaded {version:?} from {}", path.display()),
                    );
                }
                SchemeStatus::Error(msg) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, msg);
                }
            }
        });
    }

    fn draw_teams_panel(&mut self, ui: &mut egui::Ui) {
        labeled_group(ui, "Teams", |ui| {
            if self.wgt_files.is_empty() {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "No .WGT rosters discovered. Check User/Teams under your WA install.",
                );
                return;
            }
            let selected_label = self
                .wgt_choice
                .and_then(|i| self.wgt_files.get(i))
                .map(|w| w.name.as_str())
                .unwrap_or("<none>");
            let mut reload = false;
            egui::ComboBox::from_label("Roster (.WGT)")
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    for (i, w) in self.wgt_files.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.wgt_choice, Some(i), w.name.as_str())
                            .changed()
                        {
                            reload = true;
                        }
                    }
                });
            if reload {
                self.reload_wgt();
            }
            let Some(wgt) = self.loaded_wgt.as_ref() else {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    "Pick a roster to populate the team dropdowns.",
                );
                return;
            };

            // Two side-by-side seat pickers. We clamp the indices to the
            // current roster's bounds in `reload_wgt` so unwrap is safe.
            let teams = &wgt.file.teams;
            ui.label(format!(
                "{} team(s) in {}",
                teams.len(),
                wgt.path.file_name().map_or_else(
                    || wgt.path.display().to_string(),
                    |f| f.to_string_lossy().into_owned(),
                ),
            ));

            team_dropdown(ui, "Team A", teams, &mut self.team_a_idx);
            team_dropdown(ui, "Team B", teams, &mut self.team_b_idx);

            if self.team_a_idx == self.team_b_idx {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Pick two different teams (same-team matches aren't supported).",
                );
            }

            // Preview the chosen teams' soundbank + grave so the user can
            // see what's about to be applied.
            if let (Some(a), Some(b)) = (teams.get(self.team_a_idx), teams.get(self.team_b_idx)) {
                preview_team_row(ui, "A", a);
                preview_team_row(ui, "B", b);
            }
        });
    }

    fn draw_debug_features(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Debug features")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(
                    "Dump under gameinfo_dumps/<stamp>_<tag>.bin + .hex (next to WA.exe). \
                     Diff two .hex files (e.g. before/after a menu action) to find which \
                     offsets that action writes.",
                );
                ui.horizontal(|ui| {
                    ui.label("Tag:");
                    ui.text_edit_singleline(&mut self.dump_label);
                });
                ui.horizontal(|ui| {
                    if ui.button("Dump live GameInfo").clicked() {
                        self.do_dump_live();
                    }
                    let snap_captured = openwa_game::engine::game_info_snapshot::is_captured();
                    let snap_btn = egui::Button::new("Dump captured snapshot");
                    if ui.add_enabled(snap_captured, snap_btn).clicked() {
                        self.do_dump_snapshot();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    "Hardware watchpoints (DR0-DR3) on the 4 GameInfo offsets listed in \
                     debug_watchpoint.rs's WATCH_OFFSETS. Click then go to WA's frontend and \
                     click Start — OpenWA.log gets a stack trace per write.",
                );
                if ui.button("Arm GameInfo watchpoints").clicked() {
                    if openwa_game::main_thread::request_arm_gameinfo_watchpoints() {
                        self.push_log("Watchpoint arm scheduled onto main thread");
                    } else {
                        self.push_log("Watchpoint arm not registered (DLL feature?)");
                    }
                }
            });
    }
}

/// Render `add_contents` inside a bordered group with a bold heading label
/// at the top. egui doesn't have a built-in "titled frame" so we compose
/// `ui.group` + a strong label.
fn labeled_group<R>(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.group(|ui| {
        ui.label(egui::RichText::new(title).strong());
        ui.separator();
        add_contents(ui)
    })
    .inner
}

fn team_dropdown(ui: &mut egui::Ui, label: &str, teams: &[WgtTeam], idx: &mut usize) {
    let selected_label = teams
        .get(*idx)
        .map(display_team_label)
        .unwrap_or_else(|| "<none>".to_string());
    egui::ComboBox::from_label(label)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for (i, t) in teams.iter().enumerate() {
                ui.selectable_value(idx, i, display_team_label(t));
            }
        });
}

fn display_team_label(team: &WgtTeam) -> String {
    let name = team.name_str();
    let ctrl = match team.control {
        0 => "Player",
        1 => "CPU1",
        2 => "CPU2",
        3 => "CPU3",
        4 => "CPU4",
        5 => "CPU5",
        _ => "?",
    };
    format!("{name} ({ctrl})")
}

fn preview_team_row(ui: &mut egui::Ui, label: &str, team: &WgtTeam) {
    ui.monospace(format!(
        "  {label}: voice={}, fanfare={}, special={}, grave={}, flag={}",
        team.soundbank_str(),
        team.fanfare_str(),
        team.special_weapon_str(),
        team.grave_id,
        team.flag_filename_str(),
    ));
}
