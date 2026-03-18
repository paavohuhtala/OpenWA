//! Sky gradient computation for non-standard level heights.
//!
//! NOTE: This code path is UNTESTED — it only triggers on maps with
//! non-standard dimensions (sky_height >= 0x61 or level_height != 0x2B8).
//! The algorithm is ported from the DDGame constructor decompilation but
//! may have subtle color differences due to the tangled decompiler output.

use crate::engine::ddgame::DDGame;
use crate::rebase::rb;
use crate::render::gfx_handler::call_gfx_find_and_load;
use crate::task::state_machine::TaskStateMachine;
use crate::wa_alloc::{wa_free, wa_malloc};

/// Palette context for gradient color mapping.
/// Reimplements the WA palette management functions:
/// - PaletteContext__Init (0x5411A0)
/// - PaletteContext__MapColor (0x5412B0)
/// - PaletteContext__FindClosest (0x541420)
struct PaletteContext {
    colors: [u32; 256], // RGB packed, indexed by palette index
    valid: [bool; 256],
    used_count: usize,
    used_list: [u8; 256],
}

impl PaletteContext {
    fn new() -> Self {
        Self {
            colors: [0; 256],
            valid: [false; 256],
            used_count: 0,
            used_list: [0; 256],
        }
    }

    /// Map an RGB color to a palette index (PaletteContext__MapColor, 0x5412B0).
    /// Returns existing index if already mapped, otherwise allocates a new one.
    fn map_color(&mut self, rgb: u32) -> Option<u8> {
        let masked = rgb & 0x00FF_FFFF;
        for i in 0..self.used_count {
            let idx = self.used_list[i] as usize;
            if idx != 0 && (self.colors[idx] & 0x00FF_FFFF) == masked {
                return Some(idx as u8);
            }
        }
        for idx in (1..=255usize).rev() {
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

    /// Find closest palette match (PaletteContext__FindClosest, 0x541420).
    /// Uses weighted perceptual distance: R*3 + G*5 + B*2.
    /// Input channels are 8.8 fixed-point (0..0xFF00).
    fn find_closest(&self, r: i32, g: i32, b: i32) -> u8 {
        let mut best_idx = 0u8;
        let mut best_dist = i32::MAX;
        for i in 0..self.used_count {
            let idx = self.used_list[i] as usize;
            if idx == 0 {
                continue;
            }
            let c = self.colors[idx];
            let cr = ((c & 0xFF) as i32) << 8;
            let cg = (((c >> 8) & 0xFF) as i32) << 8;
            let cb = (((c >> 16) & 0xFF) as i32) << 8;
            let dist = (r - cr).abs() * 3 + (g - cg).abs() * 5 + (b - cb).abs() * 2;
            if dist == 0 {
                return idx as u8;
            }
            if dist < best_dist {
                best_dist = dist;
                best_idx = idx as u8;
            }
        }
        best_idx
    }

    /// Read RGB color for a palette index as 8.8 fixed-point (0..0xFF00).
    fn color_fp(&self, idx: u8) -> [i32; 3] {
        let c = self.colors[idx as usize];
        [
            ((c & 0xFF) as i32) << 8,
            (((c >> 8) & 0xFF) as i32) << 8,
            (((c >> 16) & 0xFF) as i32) << 8,
        ]
    }
}

/// Compute the complex sky gradient for non-standard level heights.
///
/// Called when `sky_height >= 0x61 OR level_height != 0x2B8`.
/// Loads gradient.img, samples 7 anchor colors, then creates an interpolated
/// gradient image (64 columns × (level_height + 0xDC) rows).
///
/// NOTE: This code path is untested — no replay or map available that
/// triggers it. The algorithm follows the DDGame constructor decompilation
/// but may have subtle differences in color output.
#[cfg(target_arch = "x86")]
pub(crate) unsafe fn compute_complex_gradient(
    ddgame: *mut DDGame,
    land_layer: *mut u8,
    layer3_ctx: *mut u8,
    sky_height: i16,
) {
    let mut palette = PaletteContext::new();

    // Step 1: Load gradient.img
    let gradient_sprite =
        call_gfx_find_and_load(land_layer, b"gradient.img\0".as_ptr(), layer3_ctx);
    if gradient_sprite.is_null() {
        return;
    }

    // Sprite height from TaskStateMachine layout (shared vtable 0x6640EC)
    let gradient_height = (*(gradient_sprite as *const TaskStateMachine)).height as i32;
    if gradient_height <= 0 {
        return;
    }

    let get_pixel: unsafe extern "thiscall" fn(*mut u8, i32, i32) -> u32 =
        core::mem::transmute(*(*(gradient_sprite as *const *const u32)).add(4));

    // Step 2: Compute target rows and stretch gradient through palette
    let target_rows = (0x70i32 - sky_height as i32).min(gradient_height);

    if target_rows > 0 {
        let mut src_pos = 0i32;
        for _ in 0..target_rows {
            let src_row = src_pos / target_rows;
            let color = get_pixel(gradient_sprite, 0, src_row);
            palette.map_color(color);
            src_pos += gradient_height;
        }
    }

    // Step 3: If heights match, also set gradient_image_2
    if target_rows == gradient_height {
        let gradient2 = call_gfx_find_and_load(land_layer, b"gradient.img\0".as_ptr(), layer3_ctx);
        (*ddgame).gradient_image_2 = gradient2;
    }

    // Step 4: Sample 7 anchor colors by averaging 8×2 pixel blocks.
    // get_pixel returns palette indices; we look up RGB from our palette context.
    let mut anchors = [[0i32; 3]; 7]; // [band][r, g, b] in 8.8 fixed-point
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
                let pidx = get_pixel(gradient_sprite, col, row) as usize;
                if pidx < 256 && palette.valid[pidx] {
                    let c = palette.colors[pidx];
                    r_sum += (c & 0xFF) as i32;
                    g_sum += ((c >> 8) & 0xFF) as i32;
                    b_sum += ((c >> 16) & 0xFF) as i32;
                }
            }
        }

        // Shift left 4: 16 samples × 8-bit → 8.8 fixed-point
        anchors[band as usize] = [r_sum << 4, g_sum << 4, b_sum << 4];
    }

    // Release the gradient sprite
    let gvt = *(gradient_sprite as *const *const u32);
    let release: unsafe extern "thiscall" fn(*mut u8, u8) = core::mem::transmute(*gvt.add(3));
    release(gradient_sprite, 1);

    // Step 5: Establish initial/fallback color via palette closest-match.
    // The original uses PaletteContext__FindClosest + PaletteContext__ReadColor
    // to get the canonical palette color for the first anchor point.
    let initial_idx = palette.find_closest(anchors[0][0], anchors[0][1], anchors[0][2]);
    let mut fallback = palette.color_fp(initial_idx);

    // Step 6: Create the gradient image buffer
    let total_height = (*ddgame).level_height as i32 + 0xDC;
    if total_height <= 0 {
        return;
    }

    let stride = 0x200u32; // 64 columns × 8 bytes per entry
    let data_size = total_height as u32 * stride;
    let data = wa_malloc(data_size + 0x20);
    if data.is_null() {
        return;
    }
    core::ptr::write_bytes(data, 0, data_size as usize);

    // Step 7: Interpolate between anchor colors and write each row.
    // For each row, determine which two anchors to interpolate between.
    // Rows beyond band 5 use the last interpolated color as fallback.
    let band_size = if total_height > 6 {
        total_height / 6
    } else {
        1
    };

    for row in 0..total_height {
        let band_idx = (row * 6) / total_height;

        if band_idx < 6 {
            let pos = row - band_idx * band_size;
            let bi = band_idx as usize;
            let a0 = &anchors[bi];
            let a1 = &anchors[(bi + 1).min(6)];

            for ch in 0..3 {
                fallback[ch] = a0[ch] + ((a1[ch] - a0[ch]) * pos) / band_size;
                fallback[ch] = fallback[ch].clamp(0, 0xFF00);
            }
        }
        // else: rows beyond band 5 keep the last fallback color

        let color_low = (fallback[0] as u32 & 0xFFFF) | ((fallback[1] as u32 & 0xFFFF) << 16);
        let color_high = fallback[2] as u32 & 0xFFFF;

        // Write to all 64 columns (matches GradientImage__WriteRow at 0x4F91C0)
        let row_base = data.add(row as usize * stride as usize);
        for col in 0..64u32 {
            let pixel = row_base.add(col as usize * 8) as *mut u32;
            *pixel = color_low;
            *pixel.add(1) = color_high;
        }
    }

    // Step 8: Wrap pixel data in a TaskStateMachine-compatible object
    // (shares vtable 0x6640EC; CTaskLand reads .height to decide whether to render)
    let gfx_obj =
        wa_malloc(core::mem::size_of::<TaskStateMachine>() as u32) as *mut TaskStateMachine;
    if gfx_obj.is_null() {
        wa_free(data);
        return;
    }
    core::ptr::write_bytes(
        gfx_obj as *mut u8,
        0,
        core::mem::size_of::<TaskStateMachine>(),
    );
    (*gfx_obj).vtable = rb(0x6640EC);
    (*gfx_obj).data = data;
    (*gfx_obj).row_stride = stride;
    (*gfx_obj).width = 0;
    (*gfx_obj).height = total_height as u32;
    (*ddgame).gradient_image = gfx_obj as *mut u8;
}
