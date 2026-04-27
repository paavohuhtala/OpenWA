//! `MenuPanel` — the 0x3D4-byte UI widget shared between the in-round game
//! camera (`runtime.menu_panel_a`/`_b`) and the ESC menu.
//!
//! Allocated by `create_camera_object` in `game_state_init.rs` and stored at
//! `GameRuntime+0x30` (paired with `display_gfx_d`) and `+0x38` (paired with
//! `display_gfx_e`). The struct doubles as:
//!
//! - **Game viewport**: `cursor_x/_y` is the camera target (init to display
//!   center), `clip_left..clip_bottom` is the viewport rect (init to display
//!   bounds), `display_a` is the render target.
//! - **ESC menu panel**: `cursor_x/_y` is the selection cursor clamped to the
//!   visible region, the `items` array (16 × `MenuItem`, stride 0x38) holds
//!   the per-row layout + text + clip-rect + slider state, `item_count` is
//!   live count.
//!
//! Reset to ESC-menu state by `GameRuntime::OpenEscMenu` (0x00535200): zeroes
//! the rect-low fields, clamps the cursor to viewport, zeroes the item count.

use std::ffi::c_char;

use crate::FieldRegistry;
use crate::render::display::font::TextMeasurement;
use crate::render::display::gfx::DisplayGfx;
use crate::render::display::vtable::measure_text;

/// One row in [`MenuPanel::items`]. Stride 0x38 = 14 ints. Populated by
/// `MenuPanel::AppendItem` (0x005408F0).
///
/// Two distinct uses:
/// - **Plain action button** (`render_ctx` is null): a centered text
///   label wrapped in a default clip-rect (`x-3..x+width+1`,
///   `y..y+height+1`). `kind` is the icon/sprite code.
/// - **Wide button or slider** (`render_ctx` non-null): a wide-clip-rect
///   row spanning most of the panel. WA's ESC menu uses this both for
///   the volume slider (`render_ctx = &runtime.ui_volume`) and for
///   "Minimize Game" (`render_ctx = world.display`).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MenuItem {
    /// 0x00: Icon / kind code. WA's ESC menu uses 0..3 for the four action
    /// buttons (Force SD / Draw / Quit / Minimize) and 4 for the volume
    /// slider.
    pub kind: i32,
    /// 0x04: Label string pointer (null-terminated C string).
    pub label: *const c_char,
    /// 0x08: Item x-position (top-left of the text).
    pub x: i32,
    /// 0x0C: Item y-position.
    pub y: i32,
    /// 0x10: Clip rect — left.
    pub clip_left: i32,
    /// 0x14: Clip rect — top.
    pub clip_top: i32,
    /// 0x18: Clip rect — right.
    pub clip_right: i32,
    /// 0x1C: Clip rect — bottom.
    pub clip_bottom: i32,
    /// 0x20: Primary color index (0x10 default).
    pub color_a: i32,
    /// 0x24: Secondary color index (0x0F default).
    pub color_b: i32,
    /// 0x28: Neighbor link — previous item (-1 sentinel; populated later by
    /// the ESC menu's nav-link layout pass).
    pub neighbor_prev: i32,
    /// 0x2C: Neighbor link — next item (-1 sentinel).
    pub neighbor_next: i32,
    /// 0x30: Auxiliary render-context pointer — overloaded.
    ///   * Slider items: points at the slider's value (`Fixed*`).
    ///   * "Minimize Game"-style buttons: points at `world.display` —
    ///     the menu's render code reads it to query renderer state when
    ///     painting the highlight box.
    ///   * Plain action buttons (Force SD / Draw / Quit): null.
    /// `MenuPanel::AppendItem` (0x005408F0) treats null vs non-null as
    /// the gate that selects the wider "slider" clip-rect override.
    pub render_ctx: *mut u8,
    /// 0x34: Auxiliary render data — set only when `render_ctx` is
    /// non-null. For sliders this is the slider's secondary render target;
    /// for the "Minimize Game" first-button case, the same display pointer
    /// passed through `render_ctx`.
    pub render_aux: *mut u8,
}

const _: () = assert!(core::mem::size_of::<MenuItem>() == 0x38);

/// Maximum number of items the panel can hold. Hard-capped by
/// `MenuPanel::AppendItem` against `item_count < 16`.
pub const MENU_PANEL_CAPACITY: usize = 16;

/// `MenuPanel` — the 0x3D4-byte panel/viewport widget.
///
/// Allocation site: `create_camera_object` in `game_state_init.rs`. The
/// constructor populates `display_a`, `display_b`, `color_low`/`_high`,
/// `cursor_x`/`_y` (= half the display dims), and `clip_right`/`_bottom`
/// (= the display dims). The remaining fields stay zeroed until first use.
///
/// Vtable identity: this struct itself has no vtable — vtable calls during
/// the ESC menu go through the bound `display_a` (via the BitGrid display
/// vtable, see `bitgrid::BitGridDisplayVtable`).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct MenuPanel {
    /// 0x00: Primary display target — the BitGrid layer this panel renders
    /// into. For `menu_panel_a` this is `runtime.display_gfx_d`; for
    /// `menu_panel_b`, `runtime.display_gfx_e`.
    pub display_a: *mut DisplayGfx,
    /// 0x04: Secondary display target — `world.display`, used for text
    /// measurement during item layout.
    pub display_b: *mut DisplayGfx,
    /// 0x08: Color index, low — `world.gfx_color_table[7]`.
    pub color_low: i32,
    /// 0x0C: Color index, high — `world.gfx_color_table[0]`.
    pub color_high: i32,
    /// 0x10: Cursor / camera target — x. Init = `display_a.width / 2`. In
    /// the ESC menu this is the active selection's screen x; clamped on
    /// each frame to the visible region.
    pub cursor_x: i32,
    /// 0x14: Cursor / camera target — y.
    pub cursor_y: i32,
    /// 0x18: Reserved (zeroed by `OpenEscMenu` panel reset).
    pub _field_18: i32,
    /// 0x1C: Visible region — left (zeroed by `OpenEscMenu`).
    pub clip_left: i32,
    /// 0x20: Visible region — top.
    pub clip_top: i32,
    /// 0x24: Visible region — right. Init = `display_a.width`.
    pub clip_right: i32,
    /// 0x28: Visible region — bottom. Init = `display_a.height`.
    pub clip_bottom: i32,
    /// 0x2C: Reserved (zeroed by `OpenEscMenu`).
    pub _field_2c: i32,
    /// 0x30: Item array (stride 0x38, capacity 16).
    pub items: [MenuItem; MENU_PANEL_CAPACITY],
    /// 0x3B0: Live item count. Reset to 0 by `OpenEscMenu`; bumped by each
    /// successful `AppendItem` call.
    pub item_count: i32,
    /// 0x3B4-0x3D3: Trailing storage (used by other menu code paths;
    /// unmapped here).
    pub _unknown_3b4: [u8; 0x3D4 - 0x3B4],
}

const _: () = assert!(core::mem::size_of::<MenuPanel>() == 0x3D4);

/// Default primary color for newly-appended items (item +0x20).
const DEFAULT_COLOR_A: i32 = 0x10;
/// Default secondary color for newly-appended items (item +0x24).
const DEFAULT_COLOR_B: i32 = 0x0F;
/// Sentinel for unlinked neighbor pointers (items +0x28 / +0x2C).
const NEIGHBOR_SENTINEL: i32 = -1;

/// Rust port of `MenuPanel::AppendItem` (0x005408F0). Plain-Rust
/// implementation of the body — the WA-side usercall ABI is bridged via
/// a naked trampoline in `openwa-dll/src/replacements/main_loop.rs`.
///
/// Appends one item (button or slider) to `panel`. Returns 1 on success,
/// 0 if the panel is at capacity or text measurement failed. On success,
/// `panel.item_count` is incremented.
///
/// Layout details:
/// - The label is measured against `panel.display_b` via
///   [`measure_text`] (slot 10): the function returns
///   `(total_advance, font_max_width)`. WA's font is square so
///   `font_max_width` doubles as the cell height.
/// - When `centered != 0`, the input `x` is taken as the desired *center*
///   of the label and is shifted left by `total_advance / 2`.
/// - Default clip rect surrounds the text by 3px on the left and 1px on
///   the right/bottom (matches WA's selection-highlight box).
/// - When `slider_value_ptr` is non-null the item is a slider: the clip
///   rect is overridden to span from `(label_end + 1)` to near the right
///   edge of `panel.display_a`, and `slider_aux` is stored at the item's
///   `+0x34` slot for the inline slider rendering.
///
/// # Safety
///
/// `panel` must point at a valid `MenuPanel` whose `display_a` and
/// `display_b` are valid `DisplayGfx` allocations with their vtables
/// initialized. `label` must be a valid C string for the duration of the
/// menu. `slider_value_ptr` is null for buttons; non-null for sliders.
pub unsafe extern "cdecl" fn append_item_impl(
    x: i32,
    panel: *mut MenuPanel,
    kind: i32,
    label: *const c_char,
    y: i32,
    centered: u32,
    render_ctx: *mut u8,
    render_aux: *mut u8,
) -> u32 {
    unsafe {
        let count = (*panel).item_count;
        if count >= MENU_PANEL_CAPACITY as i32 {
            return 0;
        }

        // Measure the label via `display_b`. measure_text's p3/p4/p5 are
        // (input_string, &out_total_advance, &out_font_max_width).
        let display_b = (*panel).display_b;
        let Some(TextMeasurement {
            total_advance,
            line_height,
        }) = measure_text(display_b, 0xF, label)
        else {
            return 0;
        };

        let x_final = if centered != 0 {
            x - total_advance / 2
        } else {
            x
        };

        let item_ptr: *mut MenuItem = (*panel).items.as_mut_ptr().add(count as usize);
        (*item_ptr).kind = kind;
        (*item_ptr).label = label;
        (*item_ptr).x = x_final;
        (*item_ptr).y = y;
        (*item_ptr).clip_left = x_final - 3;
        (*item_ptr).clip_top = y;
        (*item_ptr).clip_right = x_final + total_advance + 1;
        (*item_ptr).clip_bottom = y + line_height + 1;
        (*item_ptr).color_a = DEFAULT_COLOR_A;
        (*item_ptr).color_b = DEFAULT_COLOR_B;
        (*item_ptr).neighbor_prev = NEIGHBOR_SENTINEL;
        (*item_ptr).neighbor_next = NEIGHBOR_SENTINEL;
        (*item_ptr).render_ctx = render_ctx;

        if !render_ctx.is_null() {
            // Wide-row override (slider, or "Minimize Game"-style first
            // button). Clip rect spans from "just after the label" out to
            // near the right edge of `display_a`, and `render_aux` is
            // stored as the auxiliary render object.
            let display_a = (*panel).display_a;
            let display_w = *((display_a as *const u8).add(0x14) as *const i32);
            let new_clip_left = x_final + total_advance + 1;
            (*item_ptr).clip_left = new_clip_left;
            (*item_ptr).clip_top = y + 1;
            // Faithful to WA's expression — algebraically equals
            // `display_w - 5`, but kept literal in case operand ordering
            // matters under signed-overflow edge cases.
            (*item_ptr).clip_right = display_w - new_clip_left + total_advance + x_final - 4;
            (*item_ptr).clip_bottom = y + line_height;
            (*item_ptr).render_aux = render_aux;
        }

        (*panel).item_count = count + 1;
        1
    }
}
