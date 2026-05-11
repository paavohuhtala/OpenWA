//! egui application: match-launcher prototype window.

use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::launch::{self, LaunchOutcome, LaunchRequest};

#[derive(Default)]
pub struct MatchLauncherApp {
    req: LaunchRequest,
    log: Arc<Mutex<Vec<String>>>,
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

    fn do_launch(&self) {
        self.push_log(format!(
            "Launch requested: {} vs {}",
            self.req.team_a_name, self.req.team_b_name
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
        ui.collapsing("Teams (overlaid on snapshot)", |ui| {
            ui.horizontal(|ui| {
                ui.label("A:");
                ui.text_edit_singleline(&mut self.req.team_a_name);
            });
            ui.horizontal(|ui| {
                ui.label("B:");
                ui.text_edit_singleline(&mut self.req.team_b_name);
            });
        });

        ui.add_space(12.0);
        let can_launch = idle && snap;
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
