//! Reusable swatch-grid widget for rendering a list of RGB colors.
//!
//! Used by both [`PaletteViewer`](crate::palette_viewer::PaletteViewer) (for
//! standalone `.pal` files) and [`ImageViewer`](crate::image_viewer::ImageViewer)
//! (for palettes embedded in `.img` files).

use eframe::egui;

const SWATCH_SIZE: f32 = 16.0;
const SWATCHES_PER_ROW: usize = 16;

/// Render `colors` as a grid of swatches — edge-to-edge, 16 per row. Hover
/// over a swatch to see its index and RGB value.
pub fn show(ui: &mut egui::Ui, colors: &[[u8; 3]]) {
    if colors.is_empty() {
        ui.label("No palette entries.");
        return;
    }

    let rows = colors.len().div_ceil(SWATCHES_PER_ROW);
    let total = egui::vec2(
        SWATCH_SIZE * SWATCHES_PER_ROW as f32,
        rows as f32 * SWATCH_SIZE,
    );

    let (rect, response) = ui.allocate_exact_size(total, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    for (i, &[r, g, b]) in colors.iter().enumerate() {
        let col = i % SWATCHES_PER_ROW;
        let row = i / SWATCHES_PER_ROW;
        let min = egui::pos2(
            rect.min.x + col as f32 * SWATCH_SIZE,
            rect.min.y + row as f32 * SWATCH_SIZE,
        );
        let swatch = egui::Rect::from_min_size(min, egui::vec2(SWATCH_SIZE, SWATCH_SIZE));
        painter.rect_filled(swatch, 0.0, egui::Color32::from_rgb(r, g, b));
    }

    if let Some(pos) = response.hover_pos() {
        let col = ((pos.x - rect.min.x) / SWATCH_SIZE) as isize;
        let row = ((pos.y - rect.min.y) / SWATCH_SIZE) as isize;
        if (0..SWATCHES_PER_ROW as isize).contains(&col) && (0..rows as isize).contains(&row) {
            let i = row as usize * SWATCHES_PER_ROW + col as usize;
            if let Some(&[r, g, b]) = colors.get(i) {
                response.on_hover_text(format!("#{i}: ({r}, {g}, {b}) #{r:02X}{g:02X}{b:02X}"));
            }
        }
    }
}
