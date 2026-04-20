use std::path::PathBuf;

use eframe::egui;
use openwa_core::img::{self, DecodedImg};

use crate::viewer::Viewer;

#[allow(dead_code)]
pub struct ImageViewer {
    title: String,
    path: PathBuf,
    decoded: DecodedImg,
    texture: egui::TextureHandle,
    /// `None` means "fit to window". `Some(scale)` is a manual zoom level.
    zoom: Option<f32>,
}

impl ImageViewer {
    /// Opens an IMG file. Returns the viewer and the embedded palette colors
    /// (empty if the image has no palette).
    pub fn open(
        ctx: &egui::Context,
        path: std::path::PathBuf,
    ) -> Result<(Self, Vec<[u8; 3]>), String> {
        let data = std::fs::read(&path).map_err(|e| format!("Failed to read file: {e}"))?;

        let mut palette_colors = Vec::new();
        let decoded = img::img_decode(&data, false, |rgb| {
            let r = (rgb & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = ((rgb >> 16) & 0xFF) as u8;
            let idx = palette_colors.len() as u8 + 1;
            palette_colors.push([r, g, b]);
            idx
        })
        .map_err(|e| format!("IMG decode error: {e:?}"))?;

        let texture = create_texture(ctx, &decoded, &palette_colors);

        Ok((
            Self {
                title: format!(
                    "Image: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
                path,
                decoded,
                texture,
                zoom: None,
            },
            palette_colors,
        ))
    }
}

impl Viewer for ImageViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let img_w = self.decoded.width as f32;
        let img_h = self.decoded.height as f32;

        ui.horizontal(|ui| {
            ui.label(format!(
                "{}×{}, {}bpp",
                self.decoded.width, self.decoded.height, self.decoded.bpp,
            ));
            ui.separator();
            if ui.button("Fit").clicked() {
                self.zoom = None;
            }
            if ui.button("1:1").clicked() {
                self.zoom = Some(1.0);
            }
            let zoom_pct = self.zoom.unwrap_or(0.0) * 100.0;
            let label = if self.zoom.is_some() {
                format!("{zoom_pct:.0}%")
            } else {
                "Fit".to_owned()
            };
            ui.label(label);
        });
        ui.separator();

        let avail = ui.available_size();

        // Compute the effective scale.
        let scale = match self.zoom {
            Some(z) => z,
            None => {
                // Fit: scale so the image fills the available space.
                let sx = avail.x / img_w;
                let sy = avail.y / img_h;
                sx.min(sy).max(0.01)
            }
        };

        let display_w = img_w * scale;
        let display_h = img_h * scale;

        let response = egui::ScrollArea::both().show(ui, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(display_w, display_h), egui::Sense::drag());
            ui.painter().image(
                self.texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            response
        });

        // Scroll-wheel zoom: zoom toward the cursor position.
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 && ui.rect_contains_pointer(response.inner_rect) {
            let old_scale = scale;
            let factor = (scroll / 120.0).exp2();
            let new_scale = (old_scale * factor).clamp(0.1, 64.0);
            self.zoom = Some(new_scale);
        }
    }
}

fn create_texture(
    ctx: &egui::Context,
    img: &DecodedImg,
    palette_colors: &[[u8; 3]],
) -> egui::TextureHandle {
    let w = img.width as usize;
    let h = img.height as usize;
    let mut rgba = vec![0u8; w * h * 4];

    for row in 0..h {
        for col in 0..w {
            let dst = (row * w + col) * 4;
            if img.bpp == 1 {
                let src_byte = img.pixels[row * img.row_stride as usize + col / 8];
                let bit = (src_byte >> (7 - (col % 8))) & 1;
                if bit != 0 {
                    rgba[dst..dst + 4].copy_from_slice(&[255, 255, 255, 255]);
                } else {
                    let checker = if (row / 8 + col / 8) % 2 == 0 {
                        180
                    } else {
                        220
                    };
                    rgba[dst..dst + 4].copy_from_slice(&[checker, checker, checker, 255]);
                }
            } else {
                let idx = img.pixels[row * img.row_stride as usize + col] as usize;
                if idx == 0 {
                    let checker = if (row / 8 + col / 8) % 2 == 0 {
                        180
                    } else {
                        220
                    };
                    rgba[dst..dst + 4].copy_from_slice(&[checker, checker, checker, 255]);
                } else if idx <= palette_colors.len() {
                    let [r, g, b] = palette_colors[idx - 1];
                    rgba[dst..dst + 4].copy_from_slice(&[r, g, b, 255]);
                } else {
                    let v = idx as u8;
                    rgba[dst..dst + 4].copy_from_slice(&[v, v, v, 255]);
                }
            }
        }
    }

    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    ctx.load_texture("img-preview", color_image, egui::TextureOptions::NEAREST)
}
