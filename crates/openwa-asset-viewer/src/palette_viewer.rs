use std::path::PathBuf;

use eframe::egui;

use crate::viewer::Viewer;

const SWATCH_SIZE: f32 = 20.0;
const SWATCHES_PER_ROW: usize = 16;

#[allow(dead_code)]
pub struct PaletteViewer {
    title: String,
    path: PathBuf,
    colors: Vec<[u8; 3]>,
}

impl PaletteViewer {
    /// Open a standalone `.pal` file. Assumes raw RGB triplets.
    pub fn open(_path: std::path::PathBuf) -> Result<Self, String> {
        Err("Standalone palette files not yet supported.".to_string())
    }

    /// Create from an already-extracted palette (e.g. embedded in an IMG).
    pub fn from_colors(path: std::path::PathBuf, colors: Vec<[u8; 3]>) -> Self {
        let title = format!(
            "Palette: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        Self {
            title,
            path,
            colors,
        }
    }
}

impl Viewer for PaletteViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label(format!("{} colors", self.colors.len()));
        ui.separator();

        if self.colors.is_empty() {
            ui.label("No palette entries.");
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            let spacing = ui.spacing().item_spacing.x;
            egui::Grid::new("palette_grid")
                .spacing([spacing, spacing])
                .show(ui, |ui| {
                    for (i, &[r, g, b]) in self.colors.iter().enumerate() {
                        let color = egui::Color32::from_rgb(r, g, b);
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(SWATCH_SIZE, SWATCH_SIZE),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 0.0, color);
                        ui.painter().rect_stroke(
                            rect,
                            0.0,
                            egui::Stroke::new(1.0, egui::Color32::GRAY),
                            egui::StrokeKind::Outside,
                        );
                        response
                            .on_hover_text(format!("#{i}: ({r}, {g}, {b}) #{r:02X}{g:02X}{b:02X}"));
                        if (i + 1) % SWATCHES_PER_ROW == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
    }
}
