use eframe::egui;

use crate::viewer::Viewer;

#[allow(dead_code)]
pub struct ArchiveViewer {
    title: String,
    path: std::path::PathBuf,
    data: Vec<u8>,
}

impl ArchiveViewer {
    pub fn open(path: std::path::PathBuf) -> Result<Self, String> {
        let data = std::fs::read(&path).map_err(|e| format!("Failed to read file: {e}"))?;
        Ok(Self {
            title: format!(
                "Archive: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            path,
            data,
        })
    }
}

impl Viewer for ArchiveViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.label(format!("{} bytes", self.data.len()));
        ui.separator();
        ui.label("Archive viewer not yet implemented.");
    }
}
