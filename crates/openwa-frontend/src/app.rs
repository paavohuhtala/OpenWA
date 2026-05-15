//! egui application: match-launcher prototype window.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use openwa_config::DiscoveredFile;
use openwa_core::scheme::{SchemeFile, SchemeVersion};
use openwa_core::wgt::{WgtFile, WgtTeam};
use openwa_game::engine::pending_match::{PendingCustomMatch, PendingTeam};

use crate::launch::{self, LaunchMode, LaunchOutcome, LaunchRequest};

/// Population strategy chosen via the UI radio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiMode {
    Snapshot,
    Fresh,
}

pub struct MatchLauncherApp {
    req: LaunchRequest,
    ui_mode: UiMode,
    call_init_session: bool,
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
            req: LaunchRequest::default(),
            ui_mode: UiMode::Snapshot,
            call_init_session: false,
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
            dump_label: "before".to_owned(),
            log: Arc::default(),
        };
        // Best-effort roster load so the Fresh-mode dropdowns are
        // populated on first frame.
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
        // Snap the current UI mode into the LaunchRequest fired at WA.
        self.req.mode = match self.ui_mode {
            UiMode::Snapshot => LaunchMode::Snapshot {
                call_init_session: self.call_init_session,
            },
            UiMode::Fresh => match self.build_pending_match() {
                Ok(pending) => LaunchMode::Fresh(pending),
                Err(e) => {
                    self.push_log(format!("Launch refused: {e}"));
                    return;
                }
            },
        };

        self.push_log(format!(
            "Launch requested ({}): {} vs {}",
            match self.ui_mode {
                UiMode::Snapshot => "snapshot",
                UiMode::Fresh => "fresh",
            },
            self.req.team_a_name,
            self.req.team_b_name,
        ));
        let outcome = launch::launch(&self.req);
        match outcome {
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
        // Mirror the WGT-derived team names back into the LaunchRequest
        // so the post-launch GameInfo team-name overlay in
        // `launch::overlay_team_names` writes the right strings.
        if let Some(t) = teams.first() {
            self.req.team_a_name = t.name.clone();
        }
        if let Some(t) = teams.get(1) {
            self.req.team_b_name = t.name.clone();
        }
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
        let stamp = chrono_like_stamp();
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

/// Filename-safe `YYYYMMDD-HHMMSS` from `SystemTime::now()` without a chrono dep.
fn chrono_like_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Cheap UTC breakdown — good enough for filenames; absolute correctness
    // doesn't matter, just monotonic + readable.
    let days_since_epoch = secs / 86_400;
    let rem_today = secs % 86_400;
    let h = rem_today / 3_600;
    let m = (rem_today % 3_600) / 60;
    let s = rem_today % 60;
    // Days-since-epoch -> Y/M/D via the standard "civil from days" formula.
    let z = days_since_epoch as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mon = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if mon <= 2 { y + 1 } else { y };
    format!("{year:04}{mon:02}{d:02}-{h:02}{m:02}{s:02}")
}

impl eframe::App for MatchLauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));

        ui.heading("OpenWA Match Launcher");
        ui.label(
            "Prototype v0: replays a GameInfo snapshot captured from a real WA frontend launch.",
        );
        ui.separator();

        let idle = launch::is_idle_at_frontend();
        let snap = launch::has_snapshot();

        ui.horizontal(|ui| {
            ui.label("WA state:");
            if idle {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "idle at frontend");
            } else {
                ui.colored_label(egui::Color32::YELLOW, "in game session");
            }
        });
        ui.horizontal(|ui| {
            ui.label("GameInfo snapshot:");
            if snap {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "captured");
            } else {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "not captured — start one match through WA's frontend first",
                );
            }
        });

        ui.add_space(8.0);
        ui.label("Population mode:");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.ui_mode, UiMode::Snapshot, "Snapshot replay");
            ui.radio_value(
                &mut self.ui_mode,
                UiMode::Fresh,
                "Fresh (PendingCustomMatch)",
            );
        });

        match self.ui_mode {
            UiMode::Snapshot => {
                ui.collapsing("Teams (snapshot overlay)", |ui| {
                    ui.label(
                        "These names are overlaid on the captured snapshot's \
                         team_records[0..2].name fields after restore.",
                    );
                    ui.horizontal(|ui| {
                        ui.label("A:");
                        ui.text_edit_singleline(&mut self.req.team_a_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("B:");
                        ui.text_edit_singleline(&mut self.req.team_b_name);
                    });
                });

                ui.add_space(4.0);
                ui.checkbox(
                    &mut self.call_init_session,
                    "Also call GameInfo__InitSession (refresh rng_seed + replay header)",
                );
            }
            UiMode::Fresh => {
                self.draw_fresh_mode_controls(ui);
            }
        }

        ui.add_space(8.0);
        ui.collapsing("GameInfo dump (RE workflow)", |ui| {
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
                let snap_btn = egui::Button::new("Dump captured snapshot");
                if ui.add_enabled(snap, snap_btn).clicked() {
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

        ui.add_space(12.0);
        let can_launch = idle
            && match self.ui_mode {
                UiMode::Snapshot => snap,
                UiMode::Fresh => self.loaded_wgt.is_some(),
            };
        let launch_button =
            egui::Button::new(egui::RichText::new("Launch match").strong().size(16.0));
        if ui.add_enabled(can_launch, launch_button).clicked() {
            self.do_launch();
        }

        ui.add_space(12.0);
        ui.separator();
        ui.label("Log:");
        egui::ScrollArea::vertical()
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
    /// Draw the Fresh-mode panel (scheme dropdown + roster picker).
    fn draw_fresh_mode_controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.collapsing("Map", |ui| {
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

        ui.add_space(4.0);
        ui.collapsing("Scheme", |ui| {
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

        ui.add_space(4.0);
        ui.collapsing("Teams", |ui| {
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
