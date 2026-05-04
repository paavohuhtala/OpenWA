//! Render-queue capture for the debug UI.
//!
//! Snapshots the post-sort entry list passed to [`render_drawing_queue`] so
//! the debug UI can list / inspect what would have rendered on a given
//! frame. The capture is one-shot: a UI thread calls [`request_capture`]
//! to arm; the next time the dispatcher runs it stores the snapshot and
//! disarms; the UI thread polls [`take_capture`] to retrieve it.
//!
//! Snapshots are taken in *dispatch order* (highest layer first) so the
//! list matches the actual draw sequence. Variable-length payload tails
//! (line-strip / polygon vertices) are not yet captured — the header
//! fields suffice for an at-a-glance command list.
//!
//! [`render_drawing_queue`]: crate::render::queue_dispatch::render_drawing_queue

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::render::message::{COMMAND_TYPE_TYPED, RenderMessage, TypedRenderCmd};
use crate::render::queue_dispatch::ClipContext;

/// One captured render-queue command, in dispatch order.
#[derive(Debug, Clone)]
pub struct CapturedCommand {
    pub cmd_type: u32,
    pub layer: i32,
    pub data: CapturedData,
    /// Variable-length vertex tail. Populated for legacy types `8`/`9`
    /// (line-strip / polygon) and the corresponding `RenderMessage`
    /// variants; empty for everything else. Each triple is `[x, y, z]`
    /// in raw Fixed16 form.
    pub vertices: Vec<[i32; 3]>,
}

#[derive(Debug, Clone)]
pub enum CapturedData {
    /// Legacy byte-format command — raw `u32` fields starting at `cmd[0]`.
    /// Length is determined by `cmd_type` via [`legacy_cmd_size_u32s`].
    /// For vertex-bearing types the header *only* is stored here; the
    /// tail lives in [`CapturedCommand::vertices`].
    Legacy(Vec<u32>),
    /// Strongly-typed command — clone of the underlying [`RenderMessage`].
    /// For `LineStrip`/`Polygon` the `vertices` pointer in the message is
    /// captured into [`CapturedCommand::vertices`] at snapshot time.
    Typed(RenderMessage),
}

#[derive(Debug, Clone)]
pub struct RenderCapture {
    pub clip: ClipContext,
    pub commands: Vec<CapturedCommand>,
}

// `RenderMessage` carries raw pointers to per-frame arena data
// (`*const TiledBitmapSource`, `*mut DisplayBitGrid`, vertex arrays), which
// are not `Send` by default. The captured pointers are only formatted as
// addresses for display — never dereferenced from the UI thread — so it's
// safe to ferry them across thread boundaries.
unsafe impl Send for CapturedCommand {}
unsafe impl Send for RenderCapture {}

/// Lock-free fast path for `try_capture`: when `false`, the per-frame hook
/// returns immediately without touching the mutex. Set to `true` by
/// [`request_capture`] and cleared inside `try_capture` once the snapshot
/// is taken.
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// When `true`, the dispatcher captures every frame (instead of only when
/// armed) and slices the dispatch loop at [`STEP_LIMIT`] entries. Used by
/// the "step-through" debug mode in the capture viewer.
static STEP_MODE: AtomicBool = AtomicBool::new(false);

/// Maximum number of commands to dispatch per frame when [`STEP_MODE`] is
/// on. `u32::MAX` means "no limit" (dispatch everything). Counted in
/// dispatch order — the first N commands the dispatcher would have run.
static STEP_LIMIT: AtomicU32 = AtomicU32::new(u32::MAX);

/// When `true`, `dispatch_frame` early-returns without running the
/// simulation tick. Render still fires (process_frame's render_frame is
/// outside dispatch_frame), so entities re-emit the same commands each
/// frame and the captured queue stays stable. Without this, "step mode"
/// shows entities flickering as their producers shift in/out of the
/// queue between frames.
static PAUSED: AtomicBool = AtomicBool::new(false);

static CAPTURED: Mutex<Option<RenderCapture>> = Mutex::new(None);

/// Arm a one-shot capture. The next [`render_drawing_queue`] call will
/// snapshot its entry list and store it for retrieval via [`take_capture`].
///
/// [`render_drawing_queue`]: crate::render::queue_dispatch::render_drawing_queue
pub fn request_capture() {
    REQUESTED.store(true, Ordering::Release);
}

/// Retrieve and consume the most recent capture, if any.
pub fn take_capture() -> Option<RenderCapture> {
    CAPTURED.lock().ok().and_then(|mut s| s.take())
}

/// True if a capture is armed but not yet taken.
pub fn is_pending() -> bool {
    REQUESTED.load(Ordering::Acquire)
}

/// Enable or disable step-through dispatch mode. While enabled, the
/// dispatcher captures every frame (so the viewer's slider stays in sync
/// with what just rendered) and dispatches only the first
/// [`step_dispatch_limit`] commands.
///
/// Step mode also pauses the simulation (`set_paused(on)`) so the captured
/// command list stays stable while the user scrubs the slider — without
/// this, entities tick between frames and the producers behind any given
/// command index keep changing, which manifests as flicker in the live
/// game window.
pub fn set_step_mode(on: bool) {
    STEP_MODE.store(on, Ordering::Release);
    PAUSED.store(on, Ordering::Release);
}

/// Whether step-through mode is currently enabled.
pub fn is_step_mode() -> bool {
    STEP_MODE.load(Ordering::Acquire)
}

/// Whether the simulation is currently paused. Read by `dispatch_frame`
/// to short-circuit the per-frame tick.
pub fn is_paused() -> bool {
    PAUSED.load(Ordering::Acquire)
}

/// Set the dispatch cap used while [`is_step_mode`] is `true`. The cap is
/// counted in dispatch order — `set_step_limit(0)` renders nothing,
/// `set_step_limit(u32::MAX)` renders the whole frame.
pub fn set_step_limit(n: u32) {
    STEP_LIMIT.store(n, Ordering::Release);
}

/// Read the current step-through dispatch cap. Returns `None` when step
/// mode is off — the dispatcher then runs the full queue.
pub fn step_dispatch_limit() -> Option<u32> {
    if STEP_MODE.load(Ordering::Acquire) {
        Some(STEP_LIMIT.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Called once per frame from `render_drawing_queue` after the entries are
/// sorted by layer. Walks the entries in dispatch order (highest index first)
/// and snapshots them when a capture has been armed.
///
/// # Safety
///
/// `sorted_entries` must contain valid command pointers for the duration
/// of the call. Each pointer must reference at least
/// `legacy_cmd_size_u32s(cmd_type) * 4` bytes (or the full `TypedRenderCmd`
/// for typed commands).
pub(crate) unsafe fn try_capture(clip: &ClipContext, sorted_entries: &[*mut u8]) {
    // Fast path: skip without locking unless a one-shot capture is armed
    // OR step-through mode is on (which auto-captures every frame so the
    // viewer's slider always reflects the just-rendered frame).
    let armed = REQUESTED.swap(false, Ordering::AcqRel);
    let step = STEP_MODE.load(Ordering::Acquire);
    if !armed && !step {
        return;
    }
    let Ok(mut slot) = CAPTURED.lock() else {
        return;
    };

    let mut commands = Vec::with_capacity(sorted_entries.len());
    for &ptr in sorted_entries.iter().rev() {
        let cmd = ptr as *const u32;
        let cmd_type = unsafe { *cmd };
        let layer = unsafe { *(cmd.add(1) as *const i32) };
        let mut vertices = Vec::new();
        let data = if cmd_type == COMMAND_TYPE_TYPED {
            let typed = unsafe { (*(ptr as *const TypedRenderCmd)).message };
            // Capture vertex tails for variable-length typed messages so the
            // detail viewer can decode them without holding live arena pointers.
            match typed {
                RenderMessage::LineStrip {
                    count, vertices: v, ..
                }
                | RenderMessage::Polygon {
                    count, vertices: v, ..
                } => unsafe {
                    let n = count as usize;
                    if !v.is_null() && n > 0 {
                        vertices.reserve(n);
                        for i in 0..n {
                            vertices.push(*v.add(i));
                        }
                    }
                },
                _ => {}
            }
            CapturedData::Typed(typed)
        } else {
            let header = legacy_cmd_size_u32s(cmd_type);
            let mut buf = Vec::with_capacity(header);
            for i in 0..header {
                buf.push(unsafe { *cmd.add(i) });
            }
            // Walk the vertex tail for legacy line-strip / polygon. Layout:
            //   type 8: vertex i at cmd[4 + i*3 .. 4 + i*3 + 3], count at cmd[2]
            //   type 9: vertex i at cmd[5 + i*3 .. 5 + i*3 + 3], count at cmd[2]
            // The dispatcher stops walking on the first failed clip — we
            // always capture all `count` triples so the viewer can show what
            // the producer enqueued (clipping happens at dispatch time).
            if cmd_type == 8 || cmd_type == 9 {
                let count = unsafe { *cmd.add(2) } as usize;
                let base = if cmd_type == 8 { 4 } else { 5 };
                vertices.reserve(count);
                for i in 0..count {
                    let off = base + i * 3;
                    let x = unsafe { *cmd.add(off) } as i32;
                    let y = unsafe { *cmd.add(off + 1) } as i32;
                    let z = unsafe { *cmd.add(off + 2) } as i32;
                    vertices.push([x, y, z]);
                }
            }
            CapturedData::Legacy(buf)
        };
        commands.push(CapturedCommand {
            cmd_type,
            layer,
            data,
            vertices,
        });
    }

    *slot = Some(RenderCapture {
        clip: *clip,
        commands,
    });
}

/// Number of `u32` fields each legacy command type occupies, based on the
/// highest `cmd[N]` index read by the matching arm of `render_drawing_queue`.
/// Variable-length tails (types 7/8/9 vertex arrays) are not included —
/// only the header fields. Unknown types fall back to a conservative 8.
fn legacy_cmd_size_u32s(cmd_type: u32) -> usize {
    match cmd_type {
        0 => 8,   // FillRect:       cmd[0..8]
        1 => 10,  // BitmapGlobal:   cmd[0..10]
        2 => 13,  // TextboxLocal:   cmd[0..13]
        3 => 9,   // ViaCallback:    cmd[0..9]
        4 => 6,   // SpriteGlobal:   cmd[0..6]
        5 => 6,   // SpriteLocal:    cmd[0..6]
        6 => 9,   // SpriteOffset:   cmd[0..9]
        7 => 4,   // Polyline:       cmd[0..4] + variable verts
        8 => 7,   // LineStrip:      cmd[0..7] + variable verts
        9 => 8,   // Polygon:        cmd[0..8] + variable verts
        0xA => 8, // PixelStrip:     cmd[0..8]
        0xB => 7, // Crosshair:      cmd[0..7]
        0xC => 7, // OutlinedPixel:  cmd[0..7]
        0xD => 6, // TiledBitmap:    cmd[0..6]
        0xE => 9, // TiledTerrain:   cmd[0..9]
        _ => 8,
    }
}

/// Human-readable name for a `cmd_type` value (legacy or typed sentinel).
pub fn cmd_type_name(cmd_type: u32) -> &'static str {
    match cmd_type {
        0 => "FillRect",
        1 => "BitmapGlobal",
        2 => "TextboxLocal",
        3 => "ViaCallback",
        4 => "SpriteGlobal",
        5 => "SpriteLocal",
        6 => "SpriteOffset",
        7 => "Polyline",
        8 => "LineStrip",
        9 => "Polygon",
        0xA => "PixelStrip",
        0xB => "Crosshair",
        0xC => "OutlinedPixel",
        0xD => "TiledBitmap",
        0xE => "TiledTerrain",
        COMMAND_TYPE_TYPED => "Typed",
        _ => "Unknown",
    }
}

/// Variant name of a [`RenderMessage`] for display in the capture viewer.
/// Mirrors the enum's variant identifiers exactly.
pub fn typed_variant_name(msg: &RenderMessage) -> &'static str {
    TYPED_VARIANT_NAMES[typed_variant_index(msg)]
}

/// Discriminant index of a [`RenderMessage`] variant in the
/// [`TYPED_VARIANT_NAMES`] table. Used by the debug UI to assign each
/// variant its own filter checkbox slot.
pub fn typed_variant_index(msg: &RenderMessage) -> usize {
    match msg {
        RenderMessage::Sprite { .. } => 0,
        RenderMessage::FillRect { .. } => 1,
        RenderMessage::Crosshair { .. } => 2,
        RenderMessage::TiledBitmap { .. } => 3,
        RenderMessage::SpriteOffset { .. } => 4,
        RenderMessage::BitmapGlobal { .. } => 5,
        RenderMessage::TextboxLocal { .. } => 6,
        RenderMessage::LineStrip { .. } => 7,
        RenderMessage::Polygon { .. } => 8,
    }
}

/// Display labels for [`RenderMessage`] variants, indexed by
/// [`typed_variant_index`]. Order must stay in sync with the enum.
pub const TYPED_VARIANT_NAMES: [&str; 9] = [
    "Sprite",
    "FillRect",
    "Crosshair",
    "TiledBitmap",
    "SpriteOffset",
    "BitmapGlobal",
    "TextboxLocal",
    "LineStrip",
    "Polygon",
];

/// Number of typed variants — re-exported so the debug UI can size its
/// filter array without importing the enum.
pub const TYPED_VARIANT_COUNT: usize = TYPED_VARIANT_NAMES.len();

/// Display name for a captured command — variant name for typed messages,
/// legacy type name for legacy commands.
pub fn captured_name(cmd: &CapturedCommand) -> &'static str {
    match &cmd.data {
        CapturedData::Typed(msg) => typed_variant_name(msg),
        CapturedData::Legacy(_) => cmd_type_name(cmd.cmd_type),
    }
}

/// One-line summary of a captured command, suitable for the debug log panel.
pub fn format_command(cmd: &CapturedCommand) -> String {
    let prefix = format!("[layer {:>5}] {:<14}", cmd.layer, captured_name(cmd));
    match &cmd.data {
        CapturedData::Typed(msg) => format!("{} {}", prefix, format_typed(msg)),
        CapturedData::Legacy(fields) => {
            format!("{} {}", prefix, format_legacy(cmd.cmd_type, fields))
        }
    }
}

fn fixed_raw_to_f32(raw: u32) -> f32 {
    (raw as i32) as f32 / 65536.0
}

/// One row of the per-command detail decode for the debug UI.
///
/// `offset` is the field's byte offset within the legacy command record,
/// or `None` for typed-message fields that don't have a fixed memory layout.
/// `raw` is the underlying `u32` when present — the UI uses it to run
/// pointer identification (`mem::identify_pointer`) without re-reading
/// game memory.
#[derive(Debug, Clone)]
pub struct DecodedField {
    pub offset: Option<u32>,
    pub name: String,
    pub raw: Option<u32>,
    pub value: String,
}

/// Decode every field of a captured command into a flat list of
/// `(name, value, optional raw u32)` rows. Vertex tails are not
/// included — the UI shows them in a separate section so it can
/// render long lists with virtual scrolling.
pub fn decode_command(cmd: &CapturedCommand) -> Vec<DecodedField> {
    match &cmd.data {
        CapturedData::Typed(msg) => decode_typed(msg),
        CapturedData::Legacy(fields) => decode_legacy(cmd.cmd_type, fields),
    }
}

fn row(
    offset: Option<u32>,
    name: impl Into<String>,
    raw: Option<u32>,
    value: impl Into<String>,
) -> DecodedField {
    DecodedField {
        offset,
        name: name.into(),
        raw,
        value: value.into(),
    }
}

fn decode_typed(msg: &RenderMessage) -> Vec<DecodedField> {
    match msg {
        RenderMessage::Sprite {
            local,
            x,
            y,
            sprite,
            palette,
        } => vec![
            row(None, "local", None, format!("{}", local)),
            row(
                None,
                "x",
                Some(x.to_raw() as u32),
                format!("{:.2}  ({:#010X})", x.to_f32(), x.to_raw() as u32),
            ),
            row(
                None,
                "y",
                Some(y.to_raw() as u32),
                format!("{:.2}  ({:#010X})", y.to_f32(), y.to_raw() as u32),
            ),
            row(
                None,
                "sprite",
                Some(sprite.0),
                format!(
                    "{:#010X}  idx={}  flags={:#06X}",
                    sprite.0,
                    sprite.index(),
                    (sprite.0 >> 16)
                ),
            ),
            row(
                None,
                "palette",
                Some(*palette),
                format!("{:#010X}", palette),
            ),
        ],
        RenderMessage::FillRect {
            color,
            x1,
            y1,
            x2,
            y2,
            ref_z,
        } => vec![
            row(None, "color", Some(*color), format!("{:#010X}", color)),
            row(
                None,
                "x1",
                Some(x1.to_raw() as u32),
                format!("{:.2}", x1.to_f32()),
            ),
            row(
                None,
                "y1",
                Some(y1.to_raw() as u32),
                format!("{:.2}", y1.to_f32()),
            ),
            row(
                None,
                "x2",
                Some(x2.to_raw() as u32),
                format!("{:.2}", x2.to_f32()),
            ),
            row(
                None,
                "y2",
                Some(y2.to_raw() as u32),
                format!("{:.2}", y2.to_f32()),
            ),
            row(
                None,
                "ref_z",
                Some(*ref_z as u32),
                format!("{:#X}  ({})", *ref_z as u32, ref_z),
            ),
        ],
        RenderMessage::Crosshair {
            color_fg,
            color_bg,
            x,
            y,
        } => vec![
            row(
                None,
                "color_fg",
                Some(*color_fg),
                format!("{:#010X}", color_fg),
            ),
            row(
                None,
                "color_bg",
                Some(*color_bg),
                format!("{:#010X}", color_bg),
            ),
            row(
                None,
                "x",
                Some(x.to_raw() as u32),
                format!("{:.2}", x.to_f32()),
            ),
            row(
                None,
                "y",
                Some(y.to_raw() as u32),
                format!("{:.2}", y.to_f32()),
            ),
        ],
        RenderMessage::TiledBitmap {
            x,
            y,
            source,
            flags,
        } => vec![
            row(
                None,
                "x",
                Some(x.to_raw() as u32),
                format!("{:.2}", x.to_f32()),
            ),
            row(
                None,
                "y",
                Some(y.to_raw() as u32),
                format!("{:.2}", y.to_f32()),
            ),
            row(
                None,
                "source",
                Some(*source as u32),
                format!("{:#010X}", *source as usize),
            ),
            row(
                None,
                "flags",
                Some(*flags as u32),
                format!("{:#04X}", flags),
            ),
        ],
        RenderMessage::SpriteOffset {
            flags,
            x,
            y,
            ref_z_2,
            sprite,
            palette,
        } => vec![
            row(None, "flags", Some(*flags), format!("{:#X}", flags)),
            row(
                None,
                "x",
                Some(x.to_raw() as u32),
                format!("{:.2}", x.to_f32()),
            ),
            row(
                None,
                "y",
                Some(y.to_raw() as u32),
                format!("{:.2}", y.to_f32()),
            ),
            row(
                None,
                "ref_z_2",
                Some(*ref_z_2 as u32),
                format!("{:#X}", *ref_z_2 as u32),
            ),
            row(
                None,
                "sprite",
                Some(sprite.0),
                format!("{:#010X}  idx={}", sprite.0, sprite.index()),
            ),
            row(
                None,
                "palette",
                Some(*palette),
                format!("{:#010X}", palette),
            ),
        ],
        RenderMessage::BitmapGlobal {
            x,
            y,
            bitmap,
            src_y,
            src_w,
            src_h,
            flags,
        } => vec![
            row(
                None,
                "x",
                Some(x.to_raw() as u32),
                format!("{:.2}", x.to_f32()),
            ),
            row(
                None,
                "y",
                Some(y.to_raw() as u32),
                format!("{:.2}", y.to_f32()),
            ),
            row(
                None,
                "bitmap",
                Some(*bitmap as u32),
                format!("{:#010X}", *bitmap as usize),
            ),
            row(None, "src_y", None, format!("{}", src_y)),
            row(None, "src_w", None, format!("{}", src_w)),
            row(None, "src_h", None, format!("{}", src_h)),
            row(None, "flags", Some(*flags), format!("{:#X}", flags)),
        ],
        RenderMessage::TextboxLocal {
            x,
            y,
            bitmap,
            src_w,
            src_h,
            flags,
        } => vec![
            row(
                None,
                "x",
                Some(x.to_raw() as u32),
                format!("{:.2}", x.to_f32()),
            ),
            row(
                None,
                "y",
                Some(y.to_raw() as u32),
                format!("{:.2}", y.to_f32()),
            ),
            row(
                None,
                "bitmap",
                Some(*bitmap as u32),
                format!("{:#010X}", *bitmap as usize),
            ),
            row(None, "src_w", None, format!("{}", src_w)),
            row(None, "src_h", None, format!("{}", src_h)),
            row(None, "flags", Some(*flags), format!("{:#X}", flags)),
        ],
        RenderMessage::LineStrip {
            count,
            color,
            vertices,
        } => vec![
            row(None, "count", None, format!("{}", count)),
            row(None, "color", Some(*color), format!("{:#010X}", color)),
            row(
                None,
                "vertices_ptr",
                Some(*vertices as u32),
                format!("{:#010X}", *vertices as usize),
            ),
        ],
        RenderMessage::Polygon {
            count,
            color1,
            color2,
            vertices,
        } => vec![
            row(None, "count", None, format!("{}", count)),
            row(None, "color1", Some(*color1), format!("{:#010X}", color1)),
            row(None, "color2", Some(*color2), format!("{:#010X}", color2)),
            row(
                None,
                "vertices_ptr",
                Some(*vertices as u32),
                format!("{:#010X}", *vertices as usize),
            ),
        ],
    }
}

/// Field-name table for legacy command formats. Each entry is the
/// `cmd[N]` slot's role, indexed by `N`. Slots beyond the table or
/// labelled `""` fall back to a generic "fieldN" decode.
fn legacy_field_names(cmd_type: u32) -> &'static [&'static str] {
    // cmd[0] = type, cmd[1] = layer (always present, shown in row prefix
    // anyway but listed here for offset alignment).
    match cmd_type {
        0 => &["type", "layer", "color", "x1", "y1", "x2", "y2", "ref_z"],
        1 => &[
            "type", "layer", "x", "y", "bitmap", "src_x", "src_y", "src_w", "src_h", "flags",
        ],
        2 => &[
            "type", "layer", "mode", "x1", "y1", "ref_z", "ref_z_2", "bitmap", "src_x", "src_y",
            "src_w", "src_h", "flags",
        ],
        3 => &["type", "layer", "x", "y", "ref_z", "", "obj", "p5", "p6"],
        4 => &["type", "layer", "x", "y", "sprite", "palette"],
        5 => &["type", "layer", "x", "y", "sprite", "palette"],
        6 => &[
            "type", "layer", "flags", "x", "y", "ref_z", "ref_z_2", "sprite", "palette",
        ],
        7 => &["type", "layer", "count", "color"],
        8 => &["type", "layer", "count", "color", "v0_x", "v0_y", "v0_z"],
        9 => &[
            "type", "layer", "count", "color1", "color2", "v0_x", "v0_y", "v0_z",
        ],
        0xA => &["type", "layer", "x", "y", "dx", "dy", "count", "color"],
        0xB => &["type", "layer", "color_fg", "color_bg", "x", "y", "ref_z"],
        0xC => &["type", "layer", "color_fg", "color_bg", "x", "y", "ref_z"],
        0xD => &["type", "layer", "y", "ref_z", "source", "flags"],
        0xE => &[
            "type", "layer", "mode", "x", "y", "ref_z", "ref_z_2", "count", "flags",
        ],
        _ => &[],
    }
}

/// Slot indices that should be decoded as `Fixed16` instead of raw `u32`.
fn legacy_field_is_fixed(cmd_type: u32, slot: usize) -> bool {
    matches!(
        (cmd_type, slot),
        (0, 3..=6)               // FillRect: x1,y1,x2,y2
        | (1, 2..=3)             // BitmapGlobal: x,y
        | (2, 3..=4)             // TextboxLocal: x1,y1
        | (3, 2..=3)             // ViaCallback: x,y
        | (4, 2..=3) | (5, 2..=3)// SpriteGlobal/Local: x,y
        | (6, 3..=4)             // SpriteOffset: x,y
        | (8, 4..=5)             // LineStrip header v0_x,v0_y
        | (9, 5..=6)             // Polygon header v0_x,v0_y
        | (0xA, 2..=5)           // PixelStrip: x,y,dx,dy
        | (0xB, 4..=5) | (0xC, 4..=5) // Crosshair/OutlinedPixel: x,y
        | (0xD, 2..=2)           // TiledBitmap: y
        | (0xE, 3..=4) // TiledTerrain: x,y
    )
}

fn decode_legacy(cmd_type: u32, fields: &[u32]) -> Vec<DecodedField> {
    let names = legacy_field_names(cmd_type);
    fields
        .iter()
        .enumerate()
        .map(|(slot, &raw)| {
            let off = (slot * 4) as u32;
            let name = names.get(slot).copied().unwrap_or("");
            let label = if name.is_empty() {
                format!("field{}", slot)
            } else {
                name.to_owned()
            };
            let value = if legacy_field_is_fixed(cmd_type, slot) {
                format!("{:.2}  ({:#010X})", fixed_raw_to_f32(raw), raw)
            } else {
                format!("{:#010X}  ({})", raw, raw as i32)
            };
            row(Some(off), label, Some(raw), value)
        })
        .collect()
}

fn format_typed(msg: &RenderMessage) -> String {
    match msg {
        RenderMessage::Sprite {
            local,
            x,
            y,
            sprite,
            palette,
        } => format!(
            "{} pos=({:.1},{:.1}) op={:#010X} pal={:#010X}",
            if *local { "local " } else { "global" },
            x.to_f32(),
            y.to_f32(),
            sprite.0,
            palette,
        ),
        RenderMessage::FillRect {
            color,
            x1,
            y1,
            x2,
            y2,
            ref_z,
        } => format!(
            "color={:#010X} ({:.1},{:.1})..({:.1},{:.1}) z={:#X}",
            color,
            x1.to_f32(),
            y1.to_f32(),
            x2.to_f32(),
            y2.to_f32(),
            ref_z,
        ),
        RenderMessage::Crosshair {
            color_fg,
            color_bg,
            x,
            y,
        } => format!(
            "fg={:#010X} bg={:#010X} pos=({:.1},{:.1})",
            color_fg,
            color_bg,
            x.to_f32(),
            y.to_f32(),
        ),
        RenderMessage::TiledBitmap {
            x,
            y,
            source,
            flags,
        } => format!(
            "pos=({:.1},{:.1}) src={:#010X} flags={:#04X}",
            x.to_f32(),
            y.to_f32(),
            *source as usize,
            flags,
        ),
        RenderMessage::SpriteOffset {
            flags,
            x,
            y,
            ref_z_2,
            sprite,
            palette,
        } => format!(
            "flags={:#X} pos=({:.1},{:.1}) z2={:#X} op={:#010X} pal={:#010X}",
            flags,
            x.to_f32(),
            y.to_f32(),
            ref_z_2,
            sprite.0,
            palette,
        ),
        RenderMessage::BitmapGlobal {
            x,
            y,
            bitmap,
            src_y,
            src_w,
            src_h,
            flags,
        } => format!(
            "pos=({:.1},{:.1}) bmp={:#010X} src_y={} {}x{} flags={:#X}",
            x.to_f32(),
            y.to_f32(),
            *bitmap as usize,
            src_y,
            src_w,
            src_h,
            flags,
        ),
        RenderMessage::TextboxLocal {
            x,
            y,
            bitmap,
            src_w,
            src_h,
            flags,
        } => format!(
            "pos=({:.1},{:.1}) bmp={:#010X} {}x{} flags={:#X}",
            x.to_f32(),
            y.to_f32(),
            *bitmap as usize,
            src_w,
            src_h,
            flags,
        ),
        RenderMessage::LineStrip {
            count,
            color,
            vertices,
        } => format!(
            "count={} color={:#010X} verts={:#010X}",
            count, color, *vertices as usize,
        ),
        RenderMessage::Polygon {
            count,
            color1,
            color2,
            vertices,
        } => format!(
            "count={} c1={:#010X} c2={:#010X} verts={:#010X}",
            count, color1, color2, *vertices as usize,
        ),
    }
}

fn format_legacy(cmd_type: u32, fields: &[u32]) -> String {
    // Fields 0/1 are cmd_type and layer (already shown in the prefix).
    // Show payload starting at field 2.
    let payload = &fields[fields.len().min(2)..];
    let parts: Vec<String> = match cmd_type {
        // Coordinate-bearing legacy types: pretty-print known offsets as Fixed.
        0 if payload.len() >= 6 => vec![
            format!("color={:#010X}", payload[0]),
            format!(
                "({:.1},{:.1})..({:.1},{:.1})",
                fixed_raw_to_f32(payload[1]),
                fixed_raw_to_f32(payload[2]),
                fixed_raw_to_f32(payload[3]),
                fixed_raw_to_f32(payload[4])
            ),
            format!("z={:#X}", payload[5]),
        ],
        4 | 5 if payload.len() >= 4 => vec![
            format!(
                "pos=({:.1},{:.1})",
                fixed_raw_to_f32(payload[0]),
                fixed_raw_to_f32(payload[1])
            ),
            format!("op={:#010X}", payload[2]),
            format!("pal={:#010X}", payload[3]),
        ],
        _ => payload.iter().map(|v| format!("{:#010X}", v)).collect(),
    };
    parts.join(" ")
}
