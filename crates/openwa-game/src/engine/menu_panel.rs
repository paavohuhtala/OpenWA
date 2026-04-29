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
use crate::bitgrid::DisplayBitGrid;
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
    /// 0x28: Highlight state currently committed on the canvas. Tri-valued:
    /// `-1` = "dirty / never drawn" (forces a repaint on the next
    /// [`MenuPanel::Render`] pass), `0` = currently drawn as un-highlighted,
    /// `1` = currently drawn as highlighted (with border).
    /// [`append_item_impl`] inits this to `-1` (so the first render frame
    /// always paints the item); [`activate_at_cursor`] resets it to `-1` after
    /// a slider value change to force a redraw with the new value.
    pub was_highlighted: i32,
    /// 0x2C: This-frame hit-test result, populated by `MenuPanel::Render`'s
    /// hit-test pass (0x00540B00). `0` = cursor not over this item, `1` =
    /// cursor over (or `slider_lock` points here). When this differs from
    /// [`Self::was_highlighted`], the render pass repaints the item label
    /// + border in the new state, then commits `was = is`.
    pub is_highlighted: i32,
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
    /// 0x00: Primary render target — the [`DisplayBitGrid`] canvas this
    /// panel paints into. For `menu_panel_a` this is `runtime.display_gfx_d`;
    /// for `menu_panel_b`, `runtime.display_gfx_e`. `MenuPanel::Render`
    /// (0x00540B00) returns this pointer so the caller can blit the canvas
    /// to screen via `world.display.draw_scaled_sprite` (DisplayGfx slot 20).
    pub display_a: *mut DisplayBitGrid,
    /// 0x04: `world.display` (the global [`DisplayGfx`]). Used for text
    /// measurement (slot 10 `measure_text`) during item layout, and for
    /// rasterizing labels onto [`Self::display_a`] (slot 7
    /// `draw_text_on_bitmap`).
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
    /// 0x18: Cursor-active flag — set to `1` by [`MenuPanel::SetCursorAt`]
    /// (any cursor move enables the highlight); cleared to `0` by
    /// `OpenEscMenu`'s reset block and `OpenEscMenuConfirmDialog`. The
    /// menu render code reads this to decide whether to draw the
    /// hover/selection box around the item under the cursor.
    pub cursor_active: i32,
    /// 0x1C: Visible region — left (zeroed by `OpenEscMenu`).
    pub clip_left: i32,
    /// 0x20: Visible region — top.
    pub clip_top: i32,
    /// 0x24: Visible region — right. Init = `display_a.width`.
    pub clip_right: i32,
    /// 0x28: Visible region — bottom. Init = `display_a.height`.
    pub clip_bottom: i32,
    /// 0x2C: Slider drag lock — when non-zero, holds the index of the
    /// slider item currently being dragged. While set,
    /// [`MenuPanel::HitTestCursor`] short-circuits to this index instead
    /// of testing the cursor against each item's clip rect, making
    /// continuous drag feel sticky even if the cursor strays outside the
    /// slider's row. `EscMenu_TickState1` clears it when LMB is released
    /// (debounced output is 0); `OpenEscMenu` resets it to 0 each open.
    pub slider_lock: i32,
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
/// "Dirty / never drawn" sentinel for `MenuItem.was_highlighted` — forces
/// a repaint on the next render frame.
const HIGHLIGHT_DIRTY: i32 = -1;

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
/// `panel` must point at a valid `MenuPanel` whose `display_a` (a
/// [`DisplayBitGrid`]) and `display_b` (a [`DisplayGfx`]) are valid
/// allocations with their vtables initialized. `label` must be a valid C
/// string for the duration of the menu. `slider_value_ptr` is null for
/// buttons; non-null for sliders.
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
        (*item_ptr).was_highlighted = HIGHLIGHT_DIRTY;
        (*item_ptr).is_highlighted = HIGHLIGHT_DIRTY;
        (*item_ptr).render_ctx = render_ctx;

        if !render_ctx.is_null() {
            // Wide-row override (slider, or "Minimize Game"-style first
            // button). Clip rect spans from "just after the label" out to
            // near the right edge of `display_a`, and `render_aux` is
            // stored as the auxiliary render object.
            let display_a = (*panel).display_a;
            let display_w = (*display_a).width as i32;
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

/// Outcome of [`activate_at_cursor`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivateOutcome {
    /// No item under the cursor — caller should play the "miss" sound and
    /// re-arm the mouse latch.
    Miss,
    /// A button (`render_ctx == null`) was activated; carries `item.kind`.
    Button(i32),
    /// A slider (`render_ctx != null`) was adjusted; carries the item
    /// index. The slider's value (`*item.render_ctx`) was updated and
    /// `panel.slider_lock` is now sticky to this index until LMB releases.
    Slider(i32),
}

/// Rust port of `MenuPanel::SetCursorAt` (0x005407D0).
///
/// Sets the cursor to `(x, y)`, clamped to the panel's clip rect, and marks
/// [`MenuPanel::cursor_active`] = 1 so the next render draws the highlight
/// box. WA's ABI is `__usercall(EAX=panel, EDX=x, ESI=y)` — Rust callers
/// just use a normal function signature.
pub unsafe fn set_cursor_at(panel: *mut MenuPanel, x: i32, y: i32) {
    unsafe {
        (*panel).cursor_x = x;
        (*panel).cursor_y = y;
        (*panel).cursor_active = 1;
        if x < (*panel).clip_left {
            (*panel).cursor_x = (*panel).clip_left;
        }
        if y < (*panel).clip_top {
            (*panel).cursor_y = (*panel).clip_top;
        }
        if (*panel).clip_right < (*panel).cursor_x {
            (*panel).cursor_x = (*panel).clip_right;
        }
        if (*panel).clip_bottom < (*panel).cursor_y {
            (*panel).cursor_y = (*panel).clip_bottom;
        }
    }
}

/// Rust port of `MenuPanel::HitTestCursor` (0x005408B0).
///
/// Finds the item whose clip rect contains `(panel.cursor_x, panel.cursor_y)`,
/// returning its index (0-based) or `-1` if none. When
/// [`MenuPanel::slider_lock`] is non-zero, returns it directly without
/// scanning — this is what makes mid-drag cursor straying outside the
/// slider's row still hit the same item.
///
/// WA's ABI is `__thiscall(ECX=panel)` with EDI=cursor_x and ESI=cursor_y
/// inherited from the caller; the Rust port reads them off `panel`
/// directly.
pub unsafe fn hit_test_cursor(panel: *mut MenuPanel) -> i32 {
    unsafe {
        let lock = (*panel).slider_lock;
        if lock != 0 {
            return lock;
        }
        let item_count = (*panel).item_count;
        if item_count <= 0 {
            return -1;
        }
        let cx = (*panel).cursor_x;
        let cy = (*panel).cursor_y;
        for i in 0..item_count as usize {
            let item = &(*panel).items[i];
            if cx >= item.clip_left
                && cy >= item.clip_top
                && cx <= item.clip_right
                && cy <= item.clip_bottom
            {
                return i as i32;
            }
        }
        -1
    }
}

/// Rust port of `MenuPanel::CenterCursorOnFirstKindZero` (0x00540780).
///
/// Walks the items array and, for any item whose `kind == 0`, sets
/// the panel cursor to the center of that item's clip rect and marks
/// `cursor_active = 1`. If multiple items have `kind == 0`, the **last**
/// one wins (the loop overwrites).
///
/// Used by `OpenEscMenuConfirmDialog` to default-park the cursor on the
/// "Yes" button (which is appended with `kind = 0`) when the confirm
/// overlay opens. Also called for the same purpose from the network
/// game-end flow.
///
/// WA's ABI is `__usercall(ESI=panel)`, plain RET; the Rust port takes
/// a normal pointer arg.
pub unsafe fn center_cursor_on_first_kind_zero(panel: *mut MenuPanel) {
    unsafe {
        let count = (*panel).item_count;
        if count <= 0 {
            return;
        }
        for i in 0..count as usize {
            let item = &(*panel).items[i];
            if item.kind == 0 {
                (*panel).cursor_x = (item.clip_left + item.clip_right) / 2;
                (*panel).cursor_y = (item.clip_top + item.clip_bottom) / 2;
                (*panel).cursor_active = 1;
            }
        }
    }
}

/// Rust port of `MenuPanel::ActivateAtCursor` (0x00540810).
///
/// Resolves the cursor's current target via [`hit_test_cursor`] and either
/// reports the activated button's `kind` or advances the slider value:
///
/// - **No hit** → returns [`ActivateOutcome::Miss`].
/// - **Slider** (`item.render_ctx` is non-null): maps `cursor_x` to the
///   slider's `0..0x10000` Fixed range using
///   `((cursor_x - clip_left - 3) << 16) / (clip_right - clip_left - 5)`,
///   clamps to `[0, 0x10000]`, writes through `*render_ctx`, marks the
///   item dirty (`was_highlighted = -1`), and locks the panel's
///   [`slider_lock`](MenuPanel::slider_lock) to this index for drag
///   stickiness. Returns [`ActivateOutcome::Slider`] with the index.
/// - **Button** (`item.render_ctx` is null): returns
///   [`ActivateOutcome::Button`] carrying `item.kind`.
///
/// WA's ABI is `__cdecl(panel, *out_kind) -> i32` returning `0`/`1`/`-idx`;
/// the Rust port returns a typed enum instead.
pub unsafe fn activate_at_cursor(panel: *mut MenuPanel) -> ActivateOutcome {
    unsafe {
        let idx = hit_test_cursor(panel);
        if idx < 0 {
            return ActivateOutcome::Miss;
        }
        let item: *mut MenuItem = (*panel).items.as_mut_ptr().add(idx as usize);
        if (*item).render_ctx.is_null() {
            return ActivateOutcome::Button((*item).kind);
        }
        // Slider: map cursor_x onto 0..0x10000.
        let denom = ((*item).clip_right - (*item).clip_left - 5) as i64;
        let raw = ((((*panel).cursor_x - (*item).clip_left - 3) as i64) << 16) / denom;
        let value = raw.clamp(0, 0x10000) as i32;
        *((*item).render_ctx as *mut i32) = value;
        (*item).was_highlighted = -1;
        (*panel).slider_lock = idx;
        ActivateOutcome::Slider(idx)
    }
}
