use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use openwa_core::lzss_decode::lzss_decode_slice;
use openwa_core::pixel_grid::PixelGrid;
use openwa_core::sprite::{
    BlitBlend, BlitOrientation, BlitSource, ParsedSprite, SprHeader, blit_sprite_rect,
    parse_spr_header, pixel_grid_from_indexed,
};

use crate::palette_grid;
use crate::viewer::Viewer;

#[allow(dead_code)]
pub struct SpriteViewer {
    title: String,
    path: PathBuf,
    sprite: ParsedSprite,
    spr_header: SprHeader,
    raw_data: Vec<u8>,
    decoded_subframes: HashMap<i32, Vec<u8>>,
    frame_index: usize,
    texture: egui::TextureHandle,
    zoom: Option<f32>,
}

impl SpriteViewer {
    pub fn open(ctx: &egui::Context, path: PathBuf) -> Result<Self, String> {
        let data = std::fs::read(&path).map_err(|e| format!("Failed to read file: {e}"))?;
        let title = format!(
            "Sprite: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        Self::open_bytes(ctx, title, path, &data)
    }

    pub fn open_bytes(
        ctx: &egui::Context,
        title: String,
        path: PathBuf,
        data: &[u8],
    ) -> Result<Self, String> {
        let spr_header = parse_spr_header(data).map_err(|e| format!("SPR decode error: {e}"))?;
        let sprite = ParsedSprite::parse(data).map_err(|e| format!("SPR decode error: {e}"))?;

        let mut viewer = Self {
            title,
            path,
            sprite,
            spr_header,
            raw_data: data.to_vec(),
            decoded_subframes: HashMap::new(),
            frame_index: 0,
            texture: ctx.load_texture(
                "spr-preview-placeholder",
                egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]),
                egui::TextureOptions::NEAREST,
            ),
            zoom: None,
        };

        viewer.texture = viewer.build_texture(ctx, 0)?;

        Ok(viewer)
    }
}

impl Viewer for SpriteViewer {
    fn title(&self) -> &str {
        &self.title
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(format!(
                "{}x{}, {} frame(s), palette {}",
                self.sprite.width,
                self.sprite.height,
                self.sprite.frames.len(),
                self.sprite.palette.len()
            ));
            ui.separator();
            if ui.button("Fit").clicked() {
                self.zoom = None;
            }
            if ui.button("1:1").clicked() {
                self.zoom = Some(1.0);
            }
        });

        if self.sprite.frames.len() > 1 {
            let mut next_frame = self.frame_index as u32;
            let max = (self.sprite.frames.len() - 1) as u32;
            let changed = ui
                .add(egui::Slider::new(&mut next_frame, 0..=max).text("frame"))
                .changed();
            if changed {
                let next = next_frame as usize;
                if let Ok(tex) = self.build_texture(ui.ctx(), next) {
                    self.frame_index = next;
                    self.texture = tex;
                }
            }
        }

        ui.separator();

        if !self.sprite.palette.is_empty() {
            let panel_id = ui.id().with("palette_panel");
            egui::Panel::bottom(panel_id)
                .resizable(false)
                .show_inside(ui, |ui| {
                    egui::CollapsingHeader::new(format!(
                        "Palette ({} colors)",
                        self.sprite.palette.len()
                    ))
                    .default_open(false)
                    .show(ui, |ui| {
                        palette_grid::show(ui, &self.sprite.palette);
                    });
                });
        }

        self.show_image(ui);
    }
}

impl SpriteViewer {
    fn show_image(&mut self, ui: &mut egui::Ui) {
        let img_w = self.sprite.width as f32;
        let img_h = self.sprite.height as f32;
        let avail = ui.available_size();

        let scale = match self.zoom {
            Some(z) => z,
            None => {
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

        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 && ui.rect_contains_pointer(response.inner_rect) {
            let old_scale = scale;
            let factor = (scroll / 120.0).exp2();
            let new_scale = (old_scale * factor).clamp(0.1, 64.0);
            self.zoom = Some(new_scale);
        }
    }
}

impl SpriteViewer {
    fn build_texture(
        &mut self,
        ctx: &egui::Context,
        frame_index: usize,
    ) -> Result<egui::TextureHandle, String> {
        let frame = *self
            .sprite
            .frames
            .get(frame_index)
            .ok_or_else(|| format!("Frame index out of range: {frame_index}"))?;

        let frame_w = frame.end_x.saturating_sub(frame.start_x) as usize;
        let frame_h = frame.end_y.saturating_sub(frame.start_y) as usize;
        if frame_w == 0 || frame_h == 0 {
            return Err("Frame has zero width/height".to_owned());
        }

        let frame_pixels = frame_w * frame_h;
        let frame_bitmap: Vec<u8> = if (self.sprite.header_flags & 0x4000) == 0 {
            let src_off = frame.bitmap_offset as usize;
            let src_end = src_off + frame_pixels;
            self.sprite
                .bitmap
                .get(src_off..src_end)
                .ok_or_else(|| "Frame bitmap range exceeds sprite bitmap data".to_owned())?
                .to_vec()
        } else {
            let packed = frame.bitmap_offset;
            let subframe_idx_signed = ((packed >> 24) as i8) as i32;
            let pixel_offset = (packed & 0x00FF_FFFF) as usize;

            let decoded = self.decode_subframe(subframe_idx_signed)?;
            let src_end = pixel_offset + frame_pixels;
            decoded
                .get(pixel_offset..src_end)
                .ok_or_else(|| "Subframe pixel offset exceeds decoded payload".to_owned())?
                .to_vec()
        };

        let src_grid = pixel_grid_from_indexed(frame_w as u32, frame_h as u32, &frame_bitmap);
        let src = BlitSource::from(&src_grid);

        let mut dst = PixelGrid::new(self.sprite.width as u32, self.sprite.height as u32);
        blit_sprite_rect(
            dst.as_grid_mut(),
            &src,
            frame.start_x as i32,
            frame.start_y as i32,
            frame_w as i32,
            frame_h as i32,
            0,
            0,
            None,
            BlitOrientation::Normal,
            BlitBlend::Copy,
        );

        let w = self.sprite.width as usize;
        let h = self.sprite.height as usize;
        let mut rgba = vec![0u8; w * h * 4];
        for row in 0..h {
            for col in 0..w {
                let dst_px = (row * w + col) * 4;
                let idx = dst.data[row * dst.row_stride as usize + col] as usize;
                if idx == 0 {
                    let checker = if (row / 8 + col / 8) % 2 == 0 {
                        180
                    } else {
                        220
                    };
                    rgba[dst_px..dst_px + 4].copy_from_slice(&[checker, checker, checker, 255]);
                } else if idx < self.sprite.palette.len() {
                } else if idx <= self.sprite.palette.len() {
                    let [r, g, b] = self.sprite.palette[idx - 1];
                    rgba[dst_px..dst_px + 4].copy_from_slice(&[r, g, b, 255]);
                } else {
                    let v = idx as u8;
                    rgba[dst_px..dst_px + 4].copy_from_slice(&[v, v, v, 255]);
                }
            }
        }

        let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        Ok(ctx.load_texture("spr-preview", color_image, egui::TextureOptions::NEAREST))
    }

    fn decode_subframe(&mut self, subframe_idx_signed: i32) -> Result<&[u8], String> {
        if !self.decoded_subframes.contains_key(&subframe_idx_signed) {
            let base = self.spr_header.secondary_frame_offset as isize;
            let entry_off = base + (subframe_idx_signed as isize) * 12;
            if entry_off < 0 {
                return Err(format!(
                    "Subframe index {} points before subframe table",
                    subframe_idx_signed
                ));
            }
            let entry_off = entry_off as usize;
            let entry_end = entry_off + 12;
            if entry_end > self.raw_data.len() {
                return Err(format!(
                    "Subframe table entry out of range: idx={} off=0x{:X}",
                    subframe_idx_signed, entry_off
                ));
            }

            let compressed_offset = read_u32(&self.raw_data[entry_off..entry_off + 4]) as usize;
            let decoded_size = read_u32(&self.raw_data[entry_off + 8..entry_off + 12]) as usize;

            let src_off = self.spr_header.bitmap_offset + compressed_offset;
            if src_off >= self.raw_data.len() {
                return Err(format!(
                    "Compressed subframe source offset out of range: 0x{:X}",
                    src_off
                ));
            }
            if decoded_size == 0 {
                return Err("Compressed subframe has zero decoded size".to_owned());
            }

            let mut decoded = vec![0u8; decoded_size];
            let src = &self.raw_data[src_off..];
            let lut = identity_lut();
            lzss_decode_slice(&mut decoded, src, &lut);

            self.decoded_subframes.insert(subframe_idx_signed, decoded);
        }

        Ok(self
            .decoded_subframes
            .get(&subframe_idx_signed)
            .expect("inserted subframe must exist"))
    }
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn identity_lut() -> [u8; 256] {
    let mut lut = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        lut[i] = i as u8;
        i += 1;
    }
    lut
}
