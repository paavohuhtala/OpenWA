use std::path::{Path, PathBuf};

use eframe::egui;

mod archive_viewer;
mod image_viewer;
mod palette_viewer;
mod recent;
mod viewer;

use archive_viewer::{ArchiveViewer, PendingOpen, PendingOpenKind};
use image_viewer::ImageViewer;
use palette_viewer::PaletteViewer;

use crate::viewer::Viewer;

fn main() {
    let native_options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Asset viewer",
        native_options,
        Box::new(|cc| Ok(Box::new(AssetViewer::new(cc)))),
    );
}

enum ViewerType {
    Image(ImageViewer),
    Palette(PaletteViewer),
    Archive(ArchiveViewer),
}

impl Viewer for ViewerType {
    fn title(&self) -> &str {
        match self {
            ViewerType::Image(v) => v.title(),
            ViewerType::Palette(v) => v.title(),
            ViewerType::Archive(v) => v.title(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        match self {
            ViewerType::Image(v) => v.ui(ui),
            ViewerType::Palette(v) => v.ui(ui),
            ViewerType::Archive(v) => v.ui(ui),
        }
    }
}

struct ViewerWindow {
    id: egui::Id,
    open: bool,
    viewer: ViewerType,
}

struct AssetViewer {
    windows: Vec<ViewerWindow>,
    next_id: u64,
    error: Option<String>,
    recent: Vec<PathBuf>,
}

impl AssetViewer {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            windows: Vec::new(),
            next_id: 0,
            error: None,
            recent: recent::load(),
        }
    }

    fn alloc_id(&mut self) -> egui::Id {
        let id = egui::Id::new(("viewer_window", self.next_id));
        self.next_id += 1;
        id
    }

    fn open_file(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("All supported", &["img", "pal", "dir"])
            .add_filter("IMG images", &["img"])
            .add_filter("PAL palettes", &["pal"])
            .add_filter("DIR archives", &["dir"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        self.open_path(ctx, &path);
    }

    /// Open `path` by extension, creating the appropriate viewer window(s).
    /// On success, records the path in the recent-files list.
    fn open_path(&mut self, ctx: &egui::Context, path: &Path) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let result: Result<(), String> = match ext.as_str() {
            "img" => match ImageViewer::open(ctx, path.to_path_buf()) {
                Ok((viewer, palette_colors)) => {
                    let id = self.alloc_id();
                    self.windows.push(ViewerWindow {
                        id,
                        open: true,
                        viewer: ViewerType::Image(viewer),
                    });
                    if !palette_colors.is_empty() {
                        let pal_viewer =
                            PaletteViewer::from_colors(path.to_path_buf(), palette_colors);
                        let id = self.alloc_id();
                        self.windows.push(ViewerWindow {
                            id,
                            open: true,
                            viewer: ViewerType::Palette(pal_viewer),
                        });
                    }
                    Ok(())
                }
                Err(e) => Err(e),
            },
            "pal" => PaletteViewer::open(path.to_path_buf()).map(|viewer| {
                let id = self.alloc_id();
                self.windows.push(ViewerWindow {
                    id,
                    open: true,
                    viewer: ViewerType::Palette(viewer),
                });
            }),
            "dir" => ArchiveViewer::open(path.to_path_buf()).map(|viewer| {
                let id = self.alloc_id();
                self.windows.push(ViewerWindow {
                    id,
                    open: true,
                    viewer: ViewerType::Archive(viewer),
                });
            }),
            _ => Err(format!("Unsupported file extension: .{ext}")),
        };

        match result {
            Ok(()) => {
                self.error = None;
                recent::push(&mut self.recent, path);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn spawn_from_pending(&mut self, ctx: &egui::Context, req: PendingOpen) {
        let dummy_path = std::path::PathBuf::from(&req.title);
        match req.kind {
            PendingOpenKind::Image => {
                match ImageViewer::open_bytes(
                    ctx,
                    req.title.clone(),
                    dummy_path.clone(),
                    &req.bytes,
                ) {
                    Ok((viewer, palette_colors)) => {
                        let id = self.alloc_id();
                        self.windows.push(ViewerWindow {
                            id,
                            open: true,
                            viewer: ViewerType::Image(viewer),
                        });
                        if !palette_colors.is_empty() {
                            let pal_viewer =
                                PaletteViewer::from_colors(dummy_path.clone(), palette_colors);
                            let id = self.alloc_id();
                            self.windows.push(ViewerWindow {
                                id,
                                open: true,
                                viewer: ViewerType::Palette(pal_viewer),
                            });
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            PendingOpenKind::Palette => {
                match PaletteViewer::open_bytes(req.title.clone(), dummy_path, &req.bytes) {
                    Ok(viewer) => {
                        let id = self.alloc_id();
                        self.windows.push(ViewerWindow {
                            id,
                            open: true,
                            viewer: ViewerType::Palette(viewer),
                        });
                    }
                    Err(e) => self.error = Some(e),
                }
            }
        }
    }
}

impl eframe::App for AssetViewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let mut pending_open: Option<PathBuf> = None;
        let mut clear_recent = false;
        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        ui.close();
                        self.open_file(&ctx);
                    }
                    ui.menu_button("Recent files", |ui| {
                        ui.set_min_width(320.0);
                        if self.recent.is_empty() {
                            ui.add_enabled(false, egui::Button::new("(none)"));
                        } else {
                            for path in &self.recent {
                                let full = path.to_string_lossy();
                                let label = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| full.clone().into_owned());
                                let btn = ui.button(label).on_hover_text(full.as_ref());
                                if btn.clicked() {
                                    pending_open = Some(path.clone());
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button("Clear").clicked() {
                                clear_recent = true;
                                ui.close();
                            }
                        }
                    });
                });
            });
        });
        if let Some(path) = pending_open {
            self.open_path(&ctx, &path);
        }
        if clear_recent {
            recent::clear(&mut self.recent);
        }

        // Show each viewer in its own egui::Window.
        for win in &mut self.windows {
            let title = win.viewer.title().to_owned();
            egui::Window::new(&title)
                .id(win.id)
                .open(&mut win.open)
                .resizable(true)
                .default_size([400.0, 300.0])
                .show(&ctx, |ui| win.viewer.ui(ui));
        }

        // Drain any pending-open requests from archive viewers and spawn
        // them as new windows.
        let mut pending: Vec<PendingOpen> = Vec::new();
        for win in &mut self.windows {
            if let ViewerType::Archive(v) = &mut win.viewer {
                pending.extend(v.take_pending_opens());
            }
        }
        for req in pending {
            self.spawn_from_pending(&ctx, req);
        }

        // Remove closed windows.
        self.windows.retain(|w| w.open);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(ref err) = self.error {
                ui.colored_label(egui::Color32::RED, err);
            } else if self.windows.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("Open a file via File → Open…");
                });
            }
        });
    }
}
