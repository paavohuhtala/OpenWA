//! Sky gradient computation for non-standard level heights.

use crate::engine::ddgame::DDGame;
use crate::rebase::rb;
use crate::render::gfx_handler::call_gfx_find_and_load;
use crate::wa_alloc::{wa_free, wa_malloc};

/// Palette context for gradient color mapping.
/// Matches the WA stack-allocated structure used by PaletteContext__Init (0x5411A0).
struct PaletteContext {
    min_index: u8,
    max_index: u8,
    colors: [u32; 256], // RGB packed, indexed by palette index
    valid: [bool; 256], // whether index has a color assigned
    used_count: usize,
    used_list: [u8; 256], // palette indices currently in use
}

impl PaletteContext {
    fn new() -> Self {
        Self {
            min_index: 0,
            max_index: 0xFF,
            colors: [0; 256],
            valid: [false; 256],
            used_count: 0,
            used_list: [0; 256],
        }
    }

    /// Map an RGB color to a palette index. Returns existing index if color
    /// already mapped, otherwise allocates a new one. Returns None if full.
    fn map_color(&mut self, rgb: u32) -> Option<u8> {
        let masked = rgb & 0x00FF_FFFF;
        // Search existing entries
        for i in 0..self.used_count {
            let idx = self.used_list[i] as usize;
            if idx != 0 && (self.colors[idx] & 0x00FF_FFFF) == masked {
                return Some(idx as u8);
            }
        }
        // Allocate new: use indices from max_index downward
        // Find first unused index
        for idx in (self.min_index as usize..=self.max_index as usize).rev() {
            if !self.valid[idx] {
                self.colors[idx] = masked;
                self.valid[idx] = true;
                self.used_list[self.used_count] = idx as u8;
                self.used_count += 1;
                return Some(idx as u8);
            }
        }
        None
    }
}

/// Compute the complex sky gradient for non-standard level heights.
///
/// Called when `sky_height >= 0x61 OR level_height != 0x2B8`.
/// Loads gradient.img, samples 7 anchor colors, then creates an interpolated
/// gradient image (64 columns × (level_height + 0xDC) rows).
///
/// The result is stored at `DDGame+0x30` (gradient_image) and optionally
/// `DDGame+0x34` (gradient_image_2).
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn compute_complex_gradient(
    ddgame: *mut DDGame,
    land_layer: *mut u8,
    layer3_ctx: *mut u8,
    sky_height: i16,
) {
    let mut palette = PaletteContext::new();

    // Load gradient.img
    let gradient_sprite =
        call_gfx_find_and_load(land_layer, b"gradient.img\0".as_ptr(), layer3_ctx);
    if gradient_sprite.is_null() {
        return; // No gradient available — stub fallback handles this
    }

    // Read gradient sprite dimensions via vtable[4] (get_pixel)
    // Sprite height is at sprite+0x18 (field [6] in u32 layout)
    let gradient_height = *(gradient_sprite.add(0x18) as *const i32);
    if gradient_height <= 0 {
        return;
    }

    let get_pixel: unsafe extern "thiscall" fn(*mut u8, i32, i32) -> u32 =
        core::mem::transmute(*(*(gradient_sprite as *const *const u32)).add(4));

    // Compute target rows for stretching
    let target_rows = {
        let max_rows = 0x70i32 - sky_height as i32;
        if gradient_height < max_rows {
            gradient_height
        } else {
            max_rows
        }
    };

    // Stretch gradient through palette: map each source row's pixel color
    if target_rows > 0 {
        let mut src_pos = 0i32;
        for _ in 0..target_rows {
            let src_row = src_pos / target_rows;
            let color = get_pixel(gradient_sprite, 0, src_row);
            palette.map_color(color);
            src_pos += gradient_height;
        }
    }

    // If target_rows == gradient_height, also set gradient_image_2
    if target_rows == gradient_height {
        let gradient2 = call_gfx_find_and_load(land_layer, b"gradient.img\0".as_ptr(), layer3_ctx);
        (*ddgame).gradient_image_2 = gradient2;
    }

    // Sample 7 anchor colors by averaging 8×2 pixel blocks
    let mut anchors = [[0i32; 3]; 7]; // [band][r, g, b] in shifted format
    let grad_height_minus2 = (gradient_height - 2).max(0);

    for band in 0..7u32 {
        let start_row = if grad_height_minus2 > 0 {
            (band as i32 * grad_height_minus2) / 6
        } else {
            0
        };
        let mut r_sum = 0i32;
        let mut g_sum = 0i32;
        let mut b_sum = 0i32;

        for dy in 0..2i32 {
            let row = start_row + dy;
            for col in 0..8i32 {
                let pixel = get_pixel(gradient_sprite, col, row);
                let pidx = pixel as usize;
                if pidx < 256 && palette.valid[pidx] {
                    let c = palette.colors[pidx];
                    r_sum += (c & 0xFF) as i32;
                    g_sum += ((c >> 8) & 0xFF) as i32;
                    b_sum += ((c >> 16) & 0xFF) as i32;
                }
            }
        }

        // Shift left 4 (multiply by 16) — converts 8-bit×16 samples to 8.8 fixed-point
        anchors[band as usize] = [r_sum << 4, g_sum << 4, b_sum << 4];
    }

    // Release the gradient sprite
    let gvt = *(gradient_sprite as *const *const u32);
    let release: unsafe extern "thiscall" fn(*mut u8, u8) = core::mem::transmute(*gvt.add(3));
    release(gradient_sprite, 1);

    // Create the gradient image
    let total_height = (*ddgame).level_height as i32 + 0xDC;
    if total_height <= 0 {
        return;
    }

    let stride = 0x200u32; // 64 columns × 8 bytes per pixel
    let data_size = total_height as u32 * stride;
    let data = wa_malloc(data_size + 0x20);
    if data.is_null() {
        return;
    }
    core::ptr::write_bytes(data, 0, data_size as usize);

    // Build raw image header (0x44 bytes, same layout GradientImage__WriteRow expects)
    let header = wa_malloc(0x44);
    if header.is_null() {
        wa_free(data);
        return;
    }
    core::ptr::write_bytes(header, 0, 0x44);
    let h = header as *mut u32;
    *h.add(0) = data as u32; // data pointer
    *h.add(1) = stride; // row stride
    *h.add(2) = 0x40; // max columns (64)
    *h.add(3) = total_height as u32; // height
    *h.add(4) = 0; // bounds left
    *h.add(5) = 0; // bounds top
    *h.add(6) = 0x40; // bounds right
    *h.add(7) = total_height as u32; // bounds bottom

    // Interpolate between anchor colors and write each row
    let band_size = total_height / 6;
    for row in 0..total_height {
        let band_idx = ((row * 6) / total_height).min(5) as usize;
        let pos = row - band_idx as i32 * band_size;

        // Interpolate RGB between anchor[band_idx] and anchor[band_idx+1]
        let a0 = &anchors[band_idx];
        let a1 = &anchors[(band_idx + 1).min(6)];

        let r = if band_size > 0 {
            a0[0] + ((a1[0] - a0[0]) * pos) / band_size
        } else {
            a0[0]
        };
        let g = if band_size > 0 {
            a0[1] + ((a1[1] - a0[1]) * pos) / band_size
        } else {
            a0[1]
        };
        let b = if band_size > 0 {
            a0[2] + ((a1[2] - a0[2]) * pos) / band_size
        } else {
            a0[2]
        };

        // Clamp to [0, 0xFF00]
        let r = r.clamp(0, 0xFF00) as u32;
        let g = g.clamp(0, 0xFF00) as u32;
        let b = b.clamp(0, 0xFF00) as u32;

        // Pack: color_low = R | (G << 16), color_high = B
        let color_low = (r & 0xFFFF) | ((g & 0xFFFF) << 16);
        let color_high = b & 0xFFFF;

        // Write to all 64 columns
        let row_base = data.add(row as usize * stride as usize);
        for col in 0..64u32 {
            let pixel = row_base.add(col as usize * 8) as *mut u32;
            *pixel = color_low;
            *pixel.add(1) = color_high;
        }
    }

    // Wrap in DisplayGfx: use the same vtable as the gradient stub
    let gfx_obj = wa_malloc(0x2C);
    if gfx_obj.is_null() {
        wa_free(data);
        wa_free(header);
        return;
    }
    core::ptr::write_bytes(gfx_obj, 0, 0x2C);
    let g = gfx_obj as *mut u32;
    *g = rb(0x6640EC); // vtable
    *g.add(2) = data as u32; // pixel data
    *g.add(4) = stride; // bytes per row
    *g.add(5) = 0; // x offset
    *g.add(6) = total_height as u32; // height (checked by CTaskLand: `if (0 < [6])`)
    (*ddgame).gradient_image = gfx_obj;

    // Free the raw header (DisplayGfx wrapper now owns the pixel data)
    wa_free(header);
}
