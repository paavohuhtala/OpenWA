//! egui application: match-launcher prototype window.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use openwa_core::scheme::{SchemeFile, SchemeVersion};
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
    /// User-typed path to a .wsc file. Empty = use the empty-scheme fallback.
    scheme_path: String,
    /// Last load attempt result (cached so the UI shows status across frames).
    scheme_status: SchemeStatus,
    dump_label: String,
    log: Arc<Mutex<Vec<String>>>,
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
        Self {
            req: LaunchRequest::default(),
            ui_mode: UiMode::Snapshot,
            call_init_session: false,
            scheme_path: r"I:\games\SteamLibrary\steamapps\common\Worms Armageddon\User\Schemes\{{02}} Intermediate.wsc".to_owned(),
            scheme_status: SchemeStatus::NotLoaded,
            dump_label: "before".to_owned(),
            log: Arc::default(),
        }
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
        let teams = vec![
            PendingTeam::new(self.req.team_a_name.clone(), 0),
            PendingTeam::new(self.req.team_b_name.clone(), 1),
        ];
        Ok(PendingCustomMatch {
            game_version: 500,
            type_label: None,
            teams,
            scheme,
        })
    }

    fn load_scheme_for_launch(&mut self) -> Result<SchemeFile, String> {
        let raw = self.scheme_path.trim();
        if raw.is_empty() {
            // Fallback: an all-zero V3 payload. Not playable as-is but
            // gets the launch path off the ground; the user will load a
            // real scheme once dump-diffing reveals what's missing.
            let zeros = vec![0u8; openwa_core::scheme::SCHEME_PAYLOAD_V3];
            return Ok(SchemeFile {
                version: SchemeVersion::V3,
                payload: zeros,
            });
        }
        let path = PathBuf::from(raw);
        match SchemeFile::from_file(&path) {
            Ok(s) => {
                self.scheme_status = SchemeStatus::Loaded {
                    version: s.version,
                    path,
                };
                Ok(s)
            }
            Err(e) => {
                let msg = format!("scheme load failed ({path:?}): {e:?}");
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

        ui.add_space(4.0);
        ui.collapsing("Teams", |ui| {
            ui.horizontal(|ui| {
                ui.label("A:");
                ui.text_edit_singleline(&mut self.req.team_a_name);
            });
            ui.horizontal(|ui| {
                ui.label("B:");
                ui.text_edit_singleline(&mut self.req.team_b_name);
            });
        });

        match self.ui_mode {
            UiMode::Snapshot => {
                ui.add_space(4.0);
                ui.checkbox(
                    &mut self.call_init_session,
                    "Also call GameInfo__InitSession (refresh rng_seed + replay header)",
                );
            }
            UiMode::Fresh => {
                ui.add_space(4.0);
                ui.collapsing("Scheme (.wsc path)", |ui| {
                    ui.label(
                        "Absolute path to a .wsc file. Leave empty for an all-zero stub \
                         payload (not playable, but useful for the first dump-diff round).",
                    );
                    ui.text_edit_singleline(&mut self.scheme_path);
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
                UiMode::Fresh => true,
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
