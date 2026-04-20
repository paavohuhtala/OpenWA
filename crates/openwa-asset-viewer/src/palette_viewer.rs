use std::path::PathBuf;

use eframe::egui;
use openwa_core::pal;

use crate::palette_grid;
use crate::viewer::Viewer;

#[allow(dead_code)]
pub struct PaletteViewer {
    title: String,
    path: PathBuf,
    colors: Vec<[u8; 3]>,
}

impl PaletteViewer {
    /// Open a standalone `.pal` file (Microsoft RIFF PAL format).
    pub fn open(path: std::path::PathBuf) -> Result<Self, String> {
        let data = std::fs::read(&path).map_err(|e| format!("Failed to read file: {e}"))?;
        let title = format!(
            "Palette: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        Self::open_bytes(title, path, &data)
    }

    /// Decode a `.pal` blob extracted from e.g. a `.dir` archive.
    pub fn open_bytes(title: String, path: PathBuf, data: &[u8]) -> Result<Self, String> {
        let decoded = pal::pal_decode(data).map_err(|e| format!("PAL decode error: {e:?}"))?;
        let colors = decoded
            .entries
            .into_iter()
            .map(|e| [e.r, e.g, e.b])
            .collect();
        Ok(Self {
            title,
            path,
            colors,
        })
    }
}

impl Viewer for PaletteViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label(format!("{} colors", self.colors.len()));
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            palette_grid::show(ui, &self.colors);
        });
    }
}
