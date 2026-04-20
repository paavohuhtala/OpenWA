use eframe::egui;

mod archive_viewer;
mod image_viewer;
mod palette_viewer;
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
}

impl AssetViewer {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            windows: Vec::new(),
            next_id: 0,
            error: None,
        }
    }

    fn alloc_id(&mut self) -> egui::Id {
        let id = egui::Id::new(("viewer_window", self.next_id));
        self.next_id += 1;
        id
    }

    fn open_file(&mut self, ctx: &egui::Context) {
        let path = rfd::FileDialog::new()
            .add_filter("All supported", &["img", "pal", "dir"])
            .add_filter("IMG images", &["img"])
            .add_filter("PAL palettes", &["pal"])
            .add_filter("DIR archives", &["dir"])
            .add_filter("All files", &["*"])
            .pick_file();

        let Some(path) = path else { return };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext.as_str() {
            "img" => match ImageViewer::open(ctx, path.clone()) {
                Ok((viewer, palette_colors)) => {
                    let id = self.alloc_id();
                    self.windows.push(ViewerWindow {
                        id,
                        open: true,
                        viewer: ViewerType::Image(viewer),
                    });
                    if !palette_colors.is_empty() {
                        let pal_viewer = PaletteViewer::from_colors(path.clone(), palette_colors);
                        let id = self.alloc_id();
                        self.windows.push(ViewerWindow {
                            id,
                            open: true,
                            viewer: ViewerType::Palette(pal_viewer),
                        });
                    }
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            },
            "pal" => match PaletteViewer::open(path) {
                Ok(viewer) => {
                    let id = self.alloc_id();
                    self.windows.push(ViewerWindow {
                        id,
                        open: true,
                        viewer: ViewerType::Palette(viewer),
                    });
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            },
            "dir" => match ArchiveViewer::open(path) {
                Ok(viewer) => {
                    let id = self.alloc_id();
                    self.windows.push(ViewerWindow {
                        id,
                        open: true,
                        viewer: ViewerType::Archive(viewer),
                    });
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            },
            _ => self.error = Some(format!("Unsupported file extension: .{ext}")),
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

        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        ui.close();
                        self.open_file(&ctx);
                    }
                });
            });
        });

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
