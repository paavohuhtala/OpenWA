//! BitGrid blit hook and snapshot capture.
//!
//! Hooks BitGrid__BlitSpriteRect (0x4F6910), the core sprite/bitmap blitting
//! function used for both rendering and collision mask construction.
//! Rust handles 8bpp blend modes 0 (copy) and 1 (color table / transparency),
//! falling through to the original for unsupported modes.

use crate::log_line;
use core::sync::atomic::{AtomicU32, Ordering};
use openwa_core::address::va;
use openwa_core::rebase::rb;

// =========================================================================
// Blit snapshot capture
// =========================================================================

/// Capture sprite blit snapshots from WA's native BitGrid__BlitSpriteRect.
///
/// Creates source and destination BitGrids, loads test GIF images,
/// calls WA's blit with various orientations and blend modes,
/// saves pixel output to `testdata/snapshots/`. Activated by
/// `OPENWA_CAPTURE_BLIT_SNAPSHOTS=1`.
pub unsafe fn capture_blit_snapshots() {
    use openwa_core::bitgrid::DisplayBitGrid;
    use std::fs;
    use std::io::Write;

    let _ = log_line("[BitGrid] BLIT SNAPSHOT: Starting capture");

    // Project root at compile time — WA.exe runs from its install dir,
    // so we need absolute paths to reach testdata/.
    const PROJECT_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

    // Load test images from GIF files
    let opaque_data = match fs::read(format!("{PROJECT_ROOT}/testdata/assets/sprite_test.gif")) {
        Ok(d) => d,
        Err(e) => {
            let _ = log_line(&format!(
                "[BitGrid] BLIT SNAPSHOT: Failed to read sprite_test.gif: {e}"
            ));
            return;
        }
    };
    let transparent_data = match fs::read(format!(
        "{PROJECT_ROOT}/testdata/assets/sprite_transparent_test.gif"
    )) {
        Ok(d) => d,
        Err(e) => {
            let _ = log_line(&format!(
                "[BitGrid] BLIT SNAPSHOT: Failed to read sprite_transparent_test.gif: {e}"
            ));
            return;
        }
    };

    let opaque_img = match decode_gif_indexed(&opaque_data) {
        Some(img) => img,
        None => {
            let _ = log_line("[BitGrid] BLIT SNAPSHOT: Failed to decode sprite_test.gif");
            return;
        }
    };
    let transparent_img = match decode_gif_indexed(&transparent_data) {
        Some(img) => img,
        None => {
            let _ =
                log_line("[BitGrid] BLIT SNAPSHOT: Failed to decode sprite_transparent_test.gif");
            return;
        }
    };

    let dir = format!("{PROJECT_ROOT}/testdata/snapshots");
    let _ = fs::create_dir_all(&dir);
    let mut count = 0u32;

    // Helper: save a BitGrid to a snapshot file
    let save_grid = |grid: *mut DisplayBitGrid, name: &str| {
        let w = (*grid).width;
        let h = (*grid).height;
        let stride = (*grid).row_stride;
        let data = (*grid).data;
        let data_size = (stride * h) as usize;
        let path = format!("{dir}/{name}.bin");
        if let Ok(mut file) = fs::File::create(&path) {
            let _ = file.write_all(&w.to_le_bytes());
            let _ = file.write_all(&h.to_le_bytes());
            let _ = file.write_all(&stride.to_le_bytes());
            let _ = file.write_all(core::slice::from_raw_parts(data, data_size));
        }
    };

    // For each test image, run blit with various parameters
    for (img, prefix) in [
        (&opaque_img, "blit_opaque"),
        (&transparent_img, "blit_transparent"),
    ] {
        // Create source BitGrid from image data
        let src = DisplayBitGrid::alloc(8, img.width, img.height);
        if src.is_null() {
            let _ = log_line("[BitGrid] BLIT SNAPSHOT: Failed to allocate source BitGrid");
            continue;
        }
        // Copy image pixels into source BitGrid
        for y in 0..img.height {
            let src_row = (y * img.width) as usize;
            let dst_row = (*src).data.add((y * (*src).row_stride) as usize);
            core::ptr::copy_nonoverlapping(
                img.pixels.as_ptr().add(src_row),
                dst_row,
                img.width as usize,
            );
        }
        (*src).clip_left = 0;
        (*src).clip_top = 0;
        (*src).clip_right = img.width;
        (*src).clip_bottom = img.height;

        let sw = img.width as i32;
        let sh = img.height as i32;

        // Destination is larger than source to test positioning
        let dw = (sw + 32) as u32;
        let dh = (sh + 32) as u32;
        let dst = DisplayBitGrid::alloc(8, dw, dh);
        if dst.is_null() {
            let destructor = (*(*src).vtable).destructor;
            destructor(src, 1);
            let _ = log_line("[BitGrid] BLIT SNAPSHOT: Failed to allocate dest BitGrid");
            continue;
        }
        let dst_data = (*dst).data;
        let dst_size = ((*dst).row_stride * dh) as usize;

        // Test cases: (name_suffix, dst_x, dst_y, width, height, src_x, src_y, flags, bg_fill)
        let test_cases: &[(&str, i32, i32, i32, i32, i32, i32, u32, u8)] = &[
            // Basic orientations (blend mode 0 = direct copy)
            ("identity", 16, 16, sw, sh, 0, 0, 0x0000_0000, 0),
            ("mirror_x", 16, 16, sw, sh, 0, 0, 0x0001_0000, 0),
            ("mirror_y", 16, 16, sw, sh, 0, 0, 0x0002_0000, 0),
            ("mirror_xy", 16, 16, sw, sh, 0, 0, 0x0003_0000, 0),
            ("rotate90", 16, 16, sh, sw, 0, 0, 0x0004_0000, 0),
            // Clipped (negative offset, only bottom-right visible)
            ("clipped", -16, -16, sw, sh, 0, 0, 0x0000_0000, 0),
            // Color-table blend (mode 1) with identity table — tests transparency
            ("colortable", 16, 16, sw, sh, 0, 0, 0x0000_0001, 77),
            // Color-table + mirror_x
            ("colortable_mx", 16, 16, sw, sh, 0, 0, 0x0001_0001, 77),
            // Source sub-rect
            (
                "subrect",
                16,
                16,
                sw / 2,
                sh / 2,
                sw / 4,
                sh / 4,
                0x0000_0000,
                0,
            ),
        ];

        let blit_target = rb(va::BLIT_SPRITE_RECT);

        for &(suffix, dx, dy, w, h, sx, sy, flags, bg) in test_cases {
            // Clear destination
            core::ptr::write_bytes(dst_data, bg, dst_size);

            // Call WA's native blit: ESI=dst, 9 stack params
            wa_blit_sprite_rect(
                dst,
                dx,
                dy,
                w,
                h,
                src,
                sx,
                sy,
                0, // color_table = 0 (none) for mode 0; ignored for mode 1 without table
                flags,
                blit_target,
            );

            let name = format!("{prefix}_{suffix}");
            save_grid(dst, &name);
            count += 1;
        }

        // Free grids
        let destructor = (*(*src).vtable).destructor;
        destructor(src, 1);
        let destructor = (*(*dst).vtable).destructor;
        destructor(dst, 1);
    }

    let _ = log_line(&format!(
        "[BitGrid] BLIT SNAPSHOT: Saved {count} snapshots to {dir}/"
    ));
}

/// Decoded indexed image (no WA dependencies).
struct IndexedImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// Decode a GIF to raw indexed pixels, remapping transparent index to 0.
fn decode_gif_indexed(data: &[u8]) -> Option<IndexedImage> {
    use gif::DecodeOptions;
    use std::io::Cursor;

    let mut opts = DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = opts.read_info(Cursor::new(data)).ok()?;
    let _global_pal = decoder.global_palette().map(|p| p.to_vec());
    let frame = decoder.read_next_frame().ok()?.cloned()?;

    let w = frame.width as u32;
    let h = frame.height as u32;
    let mut pixels = frame.buffer.to_vec();

    // Remap transparent index to 0 (WA convention)
    if let Some(ti) = frame.transparent {
        if ti != 0 {
            for p in &mut pixels {
                if *p == 0 {
                    *p = ti;
                } else if *p == ti {
                    *p = 0;
                }
            }
        }
    }

    Some(IndexedImage {
        width: w,
        height: h,
        pixels,
    })
}

/// Call WA's native blit function (BitGrid__BlitSpriteRect, 0x4F6910).
///
/// The function is a usercall: ESI = destination BitGrid, 9 stack params.
/// `target` is the rebased runtime address.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_blit_sprite_rect(
    _dst: *mut openwa_core::bitgrid::DisplayBitGrid,
    _dst_x: i32,
    _dst_y: i32,
    _width: i32,
    _height: i32,
    _src: *mut openwa_core::bitgrid::DisplayBitGrid,
    _src_x: i32,
    _src_y: i32,
    _color_table: u32,
    _flags: u32,
    _target: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov esi, [esp + 12]",       // ESI = dst BitGrid
        "mov edi, [esp + 52]",       // EDI = target function address
        "push dword ptr [esp + 48]", // flags
        "push dword ptr [esp + 48]", // color_table
        "push dword ptr [esp + 48]", // src_y
        "push dword ptr [esp + 48]", // src_x
        "push dword ptr [esp + 48]", // src
        "push dword ptr [esp + 48]", // height
        "push dword ptr [esp + 48]", // width
        "push dword ptr [esp + 48]", // dst_y
        "push dword ptr [esp + 48]", // dst_x
        "call edi",                  // RET 0x24 cleans 9 params
        "pop edi",
        "pop esi",
        "ret",
    );
}

// =========================================================================
// Core sprite blit hook (BitGrid__BlitSpriteRect, 0x4F6910)
// =========================================================================

/// Trampoline to the original WA blit function (for unsupported modes).
static ORIG_BLIT: AtomicU32 = AtomicU32::new(0);

/// Naked trampoline: captures ESI (dst BitGrid) and forwards to cdecl impl.
///
/// Original calling convention: ESI=dst, 9 stdcall params, RET 0x24.
#[unsafe(naked)]
unsafe extern "C" fn blit_hook_trampoline() {
    core::arch::naked_asm!(
        "push ebp",
        "mov ebp, esp",
        "push dword ptr [ebp+0x28]", // flags
        "push dword ptr [ebp+0x24]", // color_table
        "push dword ptr [ebp+0x20]", // src_y
        "push dword ptr [ebp+0x1C]", // src_x
        "push dword ptr [ebp+0x18]", // src BitGrid
        "push dword ptr [ebp+0x14]", // height
        "push dword ptr [ebp+0x10]", // width
        "push dword ptr [ebp+0x0C]", // dst_y
        "push dword ptr [ebp+0x08]", // dst_x
        "push esi",                   // dst BitGrid
        "call {impl_fn}",
        "add esp, 40",
        "mov esp, ebp",
        "pop ebp",
        "ret 0x24",
        impl_fn = sym blit_impl,
    );
}

/// Rust implementation of the core sprite blit.
///
/// Handles 8bpp blend modes 0 (copy) and 1 (color table / transparency).
/// Falls through to the original WA function for unsupported modes.
unsafe extern "cdecl" fn blit_impl(
    dst: *mut openwa_core::bitgrid::DisplayBitGrid,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src: *mut openwa_core::bitgrid::DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    color_table: u32,
    flags: u32,
) -> u32 {
    use openwa_core::display::sprite_blit::{
        blit_sprite_rect, BlitBlend, BlitOrientation, BlitSource,
    };

    if width == 0 || height == 0 {
        return 0;
    }

    let blend_mode = flags & 0xFFFF;
    let src_cpp = (*src).cells_per_unit;
    let dst_cpp = (*dst).cells_per_unit;

    // Only handle 8bpp surfaces with blend modes 0 and 1
    if dst_cpp != 8 || src_cpp != 8 || blend_mode > 1 {
        return call_original_blit(
            dst,
            dst_x,
            dst_y,
            width,
            height,
            src,
            src_x,
            src_y,
            color_table,
            flags,
        );
    }

    // For mode 1 with a color table pointer, fall through for now
    // (we'd need to read 256 bytes from the pointer, which could be a mixing LUT)
    if blend_mode == 1 && color_table != 0 {
        return call_original_blit(
            dst,
            dst_x,
            dst_y,
            width,
            height,
            src,
            src_x,
            src_y,
            color_table,
            flags,
        );
    }

    let orientation = BlitOrientation::from_flags(flags);
    let blend = BlitBlend::from_flags(flags);

    // Build source view
    let src_data =
        core::slice::from_raw_parts((*src).data, ((*src).row_stride * (*src).height) as usize);
    let blit_src = BlitSource {
        data: src_data,
        width: (*src).width,
        height: (*src).height,
        row_stride: (*src).row_stride,
    };

    // Build mutable destination view
    let dst_stride = (*dst).row_stride;
    let dst_h = (*dst).height;
    let dst_w = (*dst).width;
    let dst_data_len = (dst_stride * dst_h) as usize;

    // Create a temporary PixelGrid wrapping the destination BitGrid's memory
    let mut dst_grid = openwa_core::display::line_draw::PixelGrid {
        data: Vec::new(), // placeholder — we'll swap in the real data
        width: dst_w,
        height: dst_h,
        row_stride: dst_stride,
        clip_left: (*dst).clip_left,
        clip_top: (*dst).clip_top,
        clip_right: (*dst).clip_right,
        clip_bottom: (*dst).clip_bottom,
    };
    // Swap the real data in (avoids allocation)
    let mut real_data = Vec::from_raw_parts((*dst).data, dst_data_len, dst_data_len);
    core::mem::swap(&mut dst_grid.data, &mut real_data);

    let result = blit_sprite_rect(
        &mut dst_grid,
        &blit_src,
        dst_x,
        dst_y,
        width,
        height,
        src_x,
        src_y,
        None, // no color table for modes we handle
        orientation,
        blend,
    );

    // Swap data back — we must NOT let Vec drop the BitGrid's data
    core::mem::swap(&mut dst_grid.data, &mut real_data);
    core::mem::forget(real_data); // BitGrid owns this memory

    result as u32
}

/// Call the original WA blit function via trampoline.
unsafe fn call_original_blit(
    dst: *mut openwa_core::bitgrid::DisplayBitGrid,
    dst_x: i32,
    dst_y: i32,
    width: i32,
    height: i32,
    src: *mut openwa_core::bitgrid::DisplayBitGrid,
    src_x: i32,
    src_y: i32,
    color_table: u32,
    flags: u32,
) -> u32 {
    let orig = ORIG_BLIT.load(Ordering::Relaxed);
    call_original_blit_asm(
        dst,
        dst_x,
        dst_y,
        width,
        height,
        src,
        src_x,
        src_y,
        color_table,
        flags,
        orig,
    )
}

/// Naked asm to call the original blit with ESI set correctly.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_original_blit_asm(
    _dst: *mut openwa_core::bitgrid::DisplayBitGrid,
    _dst_x: i32,
    _dst_y: i32,
    _width: i32,
    _height: i32,
    _src: *mut openwa_core::bitgrid::DisplayBitGrid,
    _src_x: i32,
    _src_y: i32,
    _color_table: u32,
    _flags: u32,
    _target: u32,
) -> u32 {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov esi, [esp + 12]",       // dst
        "mov edi, [esp + 52]",       // target
        "push dword ptr [esp + 48]", // flags
        "push dword ptr [esp + 48]", // color_table
        "push dword ptr [esp + 48]", // src_y
        "push dword ptr [esp + 48]", // src_x
        "push dword ptr [esp + 48]", // src
        "push dword ptr [esp + 48]", // height
        "push dword ptr [esp + 48]", // width
        "push dword ptr [esp + 48]", // dst_y
        "push dword ptr [esp + 48]", // dst_x
        "call edi",
        "pop edi",
        "pop esi",
        "ret",
    );
}

pub fn install() -> Result<(), String> {
    let _ = log_line("[BitGrid] Hooking BitGrid__BlitSpriteRect");

    unsafe {
        let orig = crate::hook::install(
            "BitGrid__BlitSpriteRect",
            va::BLIT_SPRITE_RECT,
            blit_hook_trampoline as *const (),
        )?;
        ORIG_BLIT.store(orig as u32, Ordering::Relaxed);
    }

    Ok(())
}
