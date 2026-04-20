use eframe::egui;

pub trait Viewer {
    fn title(&self) -> &str;
    fn ui(&mut self, ui: &mut egui::Ui);
}
