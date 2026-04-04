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
    use openwa_core::display::bitgrid::DisplayBitGrid;
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

    let _ = log_line(&format!(
        "[Display] SNAPSHOT: Saved {count} line snapshots to {dir}/"
    ));

    // Free the grid
    let destructor = (*(*grid).vtable).destructor;
    destructor(grid, 1);
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
            draw_line           => dd_display::draw_line,
            draw_line_clipped   => dd_display::draw_line_clipped,
            draw_pixel_strip    => dd_display::draw_pixel_strip,
            draw_crosshair      => dd_display::draw_crosshair,
            draw_outlined_pixel => dd_display::draw_outlined_pixel,
            fill_rect           => dd_display::fill_rect,
            flush_render        => dd_display::flush_render,
            set_camera_offset   => dd_display::set_camera_offset,
            set_clip_rect       => dd_display::set_clip_rect,
        })?;
        let _ = log_line("[Display]   DDDisplay: patched 10 methods → Rust");
    }

    Ok(())
}
