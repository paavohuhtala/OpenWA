use std::path::PathBuf;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use openwa_core::dir;

use crate::viewer::Viewer;

/// A single entry in the archive listing. Names are owned so the listing
/// survives the lifetime of the borrowed [`DirArchive`].
struct Entry {
    name: String,
    data_offset: u32,
    data_size: u32,
}

/// A spawn request raised when the user clicks an "Open" button. Main loop
/// drains this after each frame and builds a new viewer window from it.
pub struct PendingOpen {
    pub title: String,
    pub bytes: Vec<u8>,
    pub kind: PendingOpenKind,
}

pub enum PendingOpenKind {
    Image,
    Palette,
    Sprite,
}

pub struct ArchiveViewer {
    title: String,
    path: PathBuf,
    data: Vec<u8>,
    entries: Vec<Entry>,
    pending_opens: Vec<PendingOpen>,
}

impl ArchiveViewer {
    pub fn open(path: std::path::PathBuf) -> Result<Self, String> {
        let data = std::fs::read(&path).map_err(|e| format!("Failed to read file: {e}"))?;
        let archive = dir::dir_decode(&data).map_err(|e| format!("DIR decode error: {e:?}"))?;

        let mut entries: Vec<Entry> = archive
            .entries
            .iter()
            .map(|e| Entry {
                name: e.name.to_owned(),
                data_offset: e.data_offset,
                data_size: e.data_size,
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Self {
            title: format!(
                "Archive: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            path,
            data,
            entries,
            pending_opens: Vec::new(),
        })
    }

    pub fn take_pending_opens(&mut self) -> Vec<PendingOpen> {
        std::mem::take(&mut self.pending_opens)
    }
}

fn known_kind(name: &str) -> Option<PendingOpenKind> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".img") {
        Some(PendingOpenKind::Image)
    } else if lower.ends_with(".pal") {
        Some(PendingOpenKind::Palette)
    } else if lower.ends_with(".spr") {
        Some(PendingOpenKind::Sprite)
    } else {
        None
    }
}

impl Viewer for ArchiveViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let total_bytes: u64 = self.entries.iter().map(|e| e.data_size as u64).sum();
        ui.label(format!(
            "{} entries, {} bytes archive, {} bytes of resources",
            self.entries.len(),
            self.data.len(),
            total_bytes,
        ));
        ui.separator();

        let row_height = ui.spacing().interact_size.y;

        TableBuilder::new(ui)
            .striped(true)
            .auto_shrink(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto_with_initial_suggestion(120.0).resizable(true))
            .column(Column::exact(90.0))
            .column(Column::exact(80.0))
            .column(Column::remainder().at_least(60.0))
            .header(row_height, |mut header| {
                header.col(|ui| {
                    ui.strong("name");
                });
                header.col(|ui| {
                    ui.strong("offset");
                });
                header.col(|ui| {
                    ui.strong("size");
                });
                header.col(|_| {});
            })
            .body(|body| {
                body.rows(row_height, self.entries.len(), |mut row| {
                    let entry = &self.entries[row.index()];
                    row.col(|ui| {
                        ui.label(&entry.name);
                    });
                    row.col(|ui| {
                        ui.monospace(format!("0x{:08X}", entry.data_offset));
                    });
                    row.col(|ui| {
                        ui.monospace(format!("{}", entry.data_size));
                    });
                    row.col(|ui| {
                        if let Some(kind) = known_kind(&entry.name)
                            && ui.button("Open").clicked()
                        {
                            let start = entry.data_offset as usize;
                            let end = start + entry.data_size as usize;
                            if let Some(slice) = self.data.get(start..end) {
                                self.pending_opens.push(PendingOpen {
                                    title: format!(
                                        "{}: {}",
                                        self.path.file_name().unwrap_or_default().to_string_lossy(),
                                        entry.name,
                                    ),
                                    bytes: slice.to_vec(),
                                    kind,
                                });
                            }
                        }
                    });
                });
            });
    }
}
