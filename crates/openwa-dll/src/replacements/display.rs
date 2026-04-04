//! Display subsystem patches.
//!
//! Patches DisplayBase vtables in WA.exe's .rdata:
//! - Primary vtable (0x6645F8): replaces _purecall slots with safe no-op stubs
//! - Headless vtable (0x66A0F8): replaces destructor with Rust version that
//!   correctly frees our Rust-allocated sprite cache sub-objects

use crate::log_line;
use openwa_core::address::va;
use openwa_core::display::dd_display::{self, DDDisplayVtable};
use openwa_core::display::{DisplayBase, SpriteBufferCtrl, SpriteCacheWrapper};
use openwa_core::rebase::rb;
use openwa_core::vtable::patch_vtable;
use openwa_core::vtable_replace;
use openwa_core::wa_alloc::wa_free;

/// The _purecall function address (calls abort).
const PURECALL: u32 = 0x005D_4E16;

/// Number of slots in the DisplayBase vtable.
const VTABLE_SLOTS: usize = 32;

unsafe extern "thiscall" fn noop_thiscall(_this: *mut u8) {}

/// Rust destructor for headless DisplayBase. Frees the sprite cache chain
/// (wrapper → buffer_ctrl → buffer) that was allocated by new_headless().
unsafe extern "thiscall" fn headless_destructor(
    this: *mut DisplayBase,
    flags: u8,
) -> *mut DisplayBase {
    let wrapper_addr = (*this).sprite_cache;
    if wrapper_addr != 0 {
        let wrapper = wrapper_addr as *mut SpriteCacheWrapper;
        let ctrl_addr = (*wrapper).buffer_ctrl;
        if ctrl_addr != 0 {
            let ctrl = ctrl_addr as *mut SpriteBufferCtrl;
            let buf = (*ctrl).buffer;
            if buf != 0 {
                wa_free(buf as *mut u8);
            }
            wa_free(ctrl as *mut u8);
        }
        wa_free(wrapper as *mut u8);
    }
    if flags & 1 != 0 {
        wa_free(this as *mut u8);
    }
    this
}

/// Capture line-drawing snapshots from WA's native BitGrid line functions.
///
/// Creates a test BitGrid, calls WA line functions with known inputs,
/// saves pixel data to `testdata/snapshots/`. Activated by
/// `OPENWA_CAPTURE_LINE_SNAPSHOTS=1`.
pub unsafe fn capture_line_snapshots() {
    use openwa_core::bitgrid::DisplayBitGrid;
    use std::fs;
    use std::io::Write;

    let grid_w: u32 = 128;
    let grid_h: u32 = 128;
    let grid = DisplayBitGrid::alloc(8, grid_w, grid_h);
    if grid.is_null() {
        let _ = log_line("[Display] SNAPSHOT: Failed to allocate test BitGrid");
        return;
    }

    let row_stride = (*grid).row_stride;
    let data_ptr = (*grid).data;
    let data_size = (row_stride * grid_h) as usize;

    // WA line functions (stdcall)
    let draw_clipped: unsafe extern "stdcall" fn(*mut DisplayBitGrid, i32, i32, i32, i32, u32) =
        core::mem::transmute(rb(va::DRAW_LINE_CLIPPED) as usize);

    let draw_two: unsafe extern "stdcall" fn(*mut DisplayBitGrid, i32, i32, i32, i32, u32, u32) =
        core::mem::transmute(rb(va::DRAW_LINE_TWO_COLOR) as usize);

    let dir = "testdata/snapshots";
    let _ = fs::create_dir_all(dir);

    // Macro: clear grid, reset clip, run drawing code, save to file.
    macro_rules! snap {
        ($name:expr, $body:expr) => {{
            core::ptr::write_bytes(data_ptr, 0, data_size);
            (*grid).clip_left = 0;
            (*grid).clip_top = 0;
            (*grid).clip_right = grid_w;
            (*grid).clip_bottom = grid_h;
            $body;
            let path = format!("{}/{}.bin", dir, $name);
            if let Ok(mut file) = fs::File::create(&path) {
                let _ = file.write_all(&grid_w.to_le_bytes());
                let _ = file.write_all(&grid_h.to_le_bytes());
                let _ = file.write_all(&row_stride.to_le_bytes());
                let _ = file.write_all(core::slice::from_raw_parts(data_ptr, data_size));
            }
        }};
    }

    let f = |x: i32| x << 16; // int to Fixed raw
    let mut count = 0u32;

    // Single-color line tests
    for &(name, x1, y1, x2, y2, color) in &[
        ("clipped_horizontal", f(10), f(64), f(118), f(64), 1u32),
        ("clipped_vertical", f(64), f(10), f(64), f(118), 2),
        ("clipped_diagonal_45", f(10), f(10), f(118), f(118), 3),
        ("clipped_diagonal_steep", f(60), f(10), f(68), f(118), 4),
        ("clipped_diagonal_shallow", f(10), f(60), f(118), f(68), 5),
        ("clipped_negative_slope", f(118), f(10), f(10), f(118), 6),
        (
            "clipped_subpixel",
            f(10) + 0x8000,
            f(20) + 0x4000,
            f(100) + 0xC000,
            f(80) + 0x2000,
            7,
        ),
        ("clipped_zero_length", f(64), f(64), f(64), f(64), 8),
        ("clipped_partially_outside", f(-20), f(64), f(148), f(64), 9),
        ("clipped_fully_outside", f(-50), f(-50), f(-10), f(-10), 10),
    ] {
        snap!(name, draw_clipped(grid, x1, y1, x2, y2, color));
        count += 1;
    }

    // Two-color line tests
    for &(name, x1, y1, x2, y2, c1, c2) in &[
        ("twocol_horizontal", f(10), f(64), f(118), f(64), 1u32, 2u32),
        ("twocol_vertical", f(64), f(10), f(64), f(118), 1, 2),
        ("twocol_diagonal_45", f(10), f(10), f(118), f(118), 1, 2),
        ("twocol_steep", f(60), f(10), f(68), f(118), 3, 4),
        ("twocol_shallow", f(10), f(60), f(118), f(68), 3, 4),
        ("twocol_negative", f(118), f(10), f(10), f(118), 5, 6),
        (
            "twocol_subpixel",
            f(10) + 0x8000,
            f(20) + 0x4000,
            f(100) + 0xC000,
            f(80) + 0x2000,
            7,
            8,
        ),
    ] {
        snap!(name, draw_two(grid, x1, y1, x2, y2, c1, c2));
        count += 1;
    }

    // Restricted clip rect tests
    snap!("clipped_restricted_clip", {
        (*grid).clip_left = 30;
        (*grid).clip_top = 30;
        (*grid).clip_right = 98;
        (*grid).clip_bottom = 98;
        draw_clipped(grid, f(10), f(10), f(118), f(118), 11)
    });
    count += 1;

    snap!("twocol_restricted_clip", {
        (*grid).clip_left = 30;
        (*grid).clip_top = 30;
        (*grid).clip_right = 98;
        (*grid).clip_bottom = 98;
        draw_two(grid, f(10), f(10), f(118), f(118), 9, 10)
    });
    count += 1;

    // Polygon fill tests — call WA's polygon pipeline directly
    let clip_x: unsafe extern "thiscall" fn(*mut DisplayBitGrid, *const i32, i32) -> i32 =
        core::mem::transmute(rb(0x004F_7BA0) as usize);
    let clip_y: unsafe extern "thiscall" fn(*mut DisplayBitGrid, i32) -> i32 =
        core::mem::transmute(rb(0x004F_7D00) as usize);
    let rasterize: unsafe extern "stdcall" fn(*mut DisplayBitGrid, i32, u32) =
        core::mem::transmute(rb(0x004F_7E90) as usize);

    // Helper: write vertices to the global vertex buffer (0x8B1370) and call pipeline
    let vert_buf = rb(0x008B_1370) as *mut i32;

    macro_rules! polygon_snap {
        ($name:expr, $verts:expr, $color:expr) => {{
            let verts: &[(i32, i32)] = $verts;
            for (i, &(x, y)) in verts.iter().enumerate() {
                *vert_buf.add(i * 2) = x;
                *vert_buf.add(i * 2 + 1) = y;
            }
            snap!($name, {
                let n = clip_x(grid, vert_buf, verts.len() as i32);
                if n > 2 {
                    let n = clip_y(grid, n);
                    if n > 2 {
                        rasterize(grid, n, $color);
                    }
                }
            });
            count += 1;
        }};
    }

    // Triangle
    polygon_snap!(
        "poly_triangle",
        &[(f(64), f(10)), (f(118), f(100)), (f(10), f(100))],
        1u32
    );

    // Square
    polygon_snap!(
        "poly_square",
        &[
            (f(20), f(20)),
            (f(100), f(20)),
            (f(100), f(100)),
            (f(20), f(100))
        ],
        2u32
    );

    // Diamond
    polygon_snap!(
        "poly_diamond",
        &[
            (f(64), f(10)),
            (f(118), f(64)),
            (f(64), f(118)),
            (f(10), f(64))
        ],
        3u32
    );

    // Partially outside (triangle extending beyond grid)
    polygon_snap!(
        "poly_partially_outside",
        &[(f(64), f(-30)), (f(160), f(100)), (f(-30), f(100))],
        4u32
    );

    // Restricted clip rect
    snap!("poly_restricted_clip", {
        (*grid).clip_left = 30;
        (*grid).clip_top = 30;
        (*grid).clip_right = 98;
        (*grid).clip_bottom = 98;
        let verts: &[(i32, i32)] = &[(f(64), f(10)), (f(118), f(100)), (f(10), f(100))];
        for (i, &(x, y)) in verts.iter().enumerate() {
            *vert_buf.add(i * 2) = x;
            *vert_buf.add(i * 2 + 1) = y;
        }
        let n = clip_x(grid, vert_buf, verts.len() as i32);
        if n > 2 {
            let n = clip_y(grid, n);
            if n > 2 {
                rasterize(grid, n, 5u32);
            }
        }
    });
    count += 1;

    let _ = log_line(&format!(
        "[Display] SNAPSHOT: Saved {count} snapshots to {dir}/"
    ));

    // Free the grid
    let destructor = (*(*grid).vtable).destructor;
    destructor(grid, 1);
}

/// Capture sprite blit snapshots from WA's native FUN_004f6910.
///
/// Creates source and destination BitGrids, loads test GIF images,
/// calls WA's blit with various orientations and blend modes,
/// saves pixel output to `testdata/snapshots/`. Activated by
/// `OPENWA_CAPTURE_BLIT_SNAPSHOTS=1`.
pub unsafe fn capture_blit_snapshots() {
    use openwa_core::bitgrid::DisplayBitGrid;
    use std::fs;
    use std::io::Write;

    let _ = log_line("[Display] BLIT SNAPSHOT: Starting capture");

    // Project root at compile time — WA.exe runs from its install dir,
    // so we need absolute paths to reach testdata/.
    const PROJECT_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

    // Load test images from GIF files
    let opaque_data = match fs::read(format!("{PROJECT_ROOT}/testdata/assets/sprite_test.gif")) {
        Ok(d) => d,
        Err(e) => {
            let _ = log_line(&format!(
                "[Display] BLIT SNAPSHOT: Failed to read sprite_test.gif: {e}"
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
                "[Display] BLIT SNAPSHOT: Failed to read sprite_transparent_test.gif: {e}"
            ));
            return;
        }
    };

    let opaque_img = match decode_gif_indexed(&opaque_data) {
        Some(img) => img,
        None => {
            let _ = log_line("[Display] BLIT SNAPSHOT: Failed to decode sprite_test.gif");
            return;
        }
    };
    let transparent_img = match decode_gif_indexed(&transparent_data) {
        Some(img) => img,
        None => {
            let _ =
                log_line("[Display] BLIT SNAPSHOT: Failed to decode sprite_transparent_test.gif");
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
            let _ = log_line("[Display] BLIT SNAPSHOT: Failed to allocate source BitGrid");
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
            let _ = log_line("[Display] BLIT SNAPSHOT: Failed to allocate dest BitGrid");
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
        "[Display] BLIT SNAPSHOT: Saved {count} snapshots to {dir}/"
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

/// Call WA's native blit function (FUN_004f6910).
///
/// The function is a usercall: ESI = destination BitGrid, 9 stack params.
/// `target` is the rebased runtime address of FUN_004f6910.
#[unsafe(naked)]
unsafe extern "cdecl" fn wa_blit_sprite_rect(
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
    target: u32,
) {
    // cdecl params on stack: [ESP+4]=dst, [ESP+8]=dst_x, ...
    // [ESP+40]=flags, [ESP+44]=target
    // Need to: set ESI=dst, push 9 params, call target, restore ESI
    //
    // After push esi + push edi, the params start at ESP+12:
    //   ESP+12=dst, ESP+16=dst_x, ESP+20=dst_y, ESP+24=width,
    //   ESP+28=height, ESP+32=src, ESP+36=src_x, ESP+40=src_y,
    //   ESP+44=color_table, ESP+48=flags, ESP+52=target
    //
    // Each `push [esp+48]` pushes the topmost remaining param (flags first),
    // then the sliding window moves: after each push, the NEXT param occupies
    // the same [ESP+48] slot.
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

use core::sync::atomic::{AtomicU32, Ordering};

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
        // Push 9 stack params + ESI as 10 cdecl params
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
///
/// The trampoline expects: ESI=dst, 9 stdcall params.
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
    let _ = log_line("[Display] Patching DisplayBase vtables");

    unsafe {
        let purecall_addr = rb(PURECALL);
        let noop_addr = noop_thiscall as *const () as u32;

        // Patch primary vtable (0x6645F8): replace _purecall with no-ops.
        let primary = rb(va::DISPLAY_BASE_VTABLE) as *mut u32;
        patch_vtable(primary, VTABLE_SLOTS, |vt| {
            let mut patched = 0u32;
            for i in 0..VTABLE_SLOTS {
                let slot = vt.add(i);
                if *slot == purecall_addr {
                    *slot = noop_addr;
                    patched += 1;
                }
            }
            let _ = log_line(&format!(
                "[Display]   Primary: patched {patched}/{VTABLE_SLOTS} _purecall → no-op"
            ));
        })?;

        // Patch headless vtable (0x66A0F8): replace destructor (slot 0)
        // with our Rust version that frees the Rust-allocated sprite cache.
        let headless = rb(va::DISPLAY_BASE_HEADLESS_VTABLE) as *mut u32;
        patch_vtable(headless, VTABLE_SLOTS, |vt| {
            *vt = headless_destructor as *const () as u32;
            let _ = log_line("[Display]   Headless: patched slot 0 (destructor) → Rust");
        })?;

        // Patch DDDisplay vtable (0x66A218): replace ported methods with Rust.
        vtable_replace!(DDDisplayVtable, va::DD_DISPLAY_VTABLE, {
            get_dimensions      => dd_display::get_dimensions,
            draw_polyline       => dd_display::draw_polyline,
            draw_line           => dd_display::draw_line,
            draw_line_clipped   => dd_display::draw_line_clipped,
            draw_pixel_strip    => dd_display::draw_pixel_strip,
            draw_crosshair      => dd_display::draw_crosshair,
            draw_outlined_pixel => dd_display::draw_outlined_pixel,
            fill_rect           => dd_display::fill_rect,
            draw_via_callback   => dd_display::draw_via_callback,
            flush_render        => dd_display::flush_render,
            set_camera_offset   => dd_display::set_camera_offset,
            set_clip_rect       => dd_display::set_clip_rect,
            is_sprite_loaded    => dd_display::is_sprite_loaded,
        })?;
        let _ = log_line("[Display]   DDDisplay: patched 12 methods → Rust");

        // Hook core sprite blit (0x4F6910) via MinHook
        let orig = crate::hook::install(
            "BitGrid__BlitSpriteRect",
            va::BLIT_SPRITE_RECT,
            blit_hook_trampoline as *const (),
        )?;
        ORIG_BLIT.store(orig as u32, Ordering::Relaxed);
    }

    Ok(())
}
