//! Rust port of `MineEntity::Render` (0x00506EF0) and its inline helper
//! `MineEntity::CalcSprite` (0x00506E60).
//!
//! Render emits up to two render-queue commands per frame:
//!   1. The mine's body sprite (always emitted), via `RenderMessage::Sprite`.
//!   2. A countdown / state textbox (only when the per-team text gate
//!      passes), via `RenderMessage::TextboxLocal`.
//!
//! Originally bridged via `bridge_mine_render` in `mine_handle_message`.

use core::ffi::c_char;
use core::fmt::Write as _;

use heapless::String as HString;

use super::base::BaseEntity;
use super::mine::MineEntity;
use crate::address::va;
use crate::bitgrid::DisplayBitGrid;
use crate::engine::world::GameWorld;
use crate::rebase::rb;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_op::SpriteOp;
use openwa_core::fixed::Fixed;

// ─── Bridges ───────────────────────────────────────────────────────────────

static mut DROWN_ADDR: u32 = 0;
static mut SET_TEXTBOX_TEXT_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        DROWN_ADDR = rb(0x00565D60);
        SET_TEXTBOX_TEXT_ADDR = rb(va::SET_TEXTBOX_TEXT);
    }
}

/// `drown` (0x00565D60) — fastcall(ECX = sprite), plain RET. Pure
/// substitution table: maps an in-air sprite ID (low 16 bits) to its
/// underwater counterpart, preserving the high 16 bits.
unsafe fn drown(sprite: u32) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32) -> u32 = core::mem::transmute(DROWN_ADDR as usize);
        f(sprite)
    }
}

/// `SetTextboxText` (0x004FB070) — stdcall, RET 0x20. Lays the text into
/// the per-mine textbox object's bitmap and reports the consumed
/// pixel size via `out_w` / `out_h`.
unsafe fn set_textbox_text(
    textbox: *mut u8,
    text: *const c_char,
    color: u32,
    color_shadow_lo: u32,
    color_shadow_hi: u32,
    out_w: *mut i32,
    out_h: *mut i32,
    scale: Fixed,
) -> *mut DisplayBitGrid {
    unsafe {
        let f: unsafe extern "stdcall" fn(
            *mut u8,
            *const c_char,
            u32,
            u32,
            u32,
            *mut i32,
            *mut i32,
            Fixed,
        ) -> *mut DisplayBitGrid = core::mem::transmute(SET_TEXTBOX_TEXT_ADDR as usize);
        f(
            textbox,
            text,
            color,
            color_shadow_lo,
            color_shadow_hi,
            out_w,
            out_h,
            scale,
        )
    }
}

// ─── Static text addresses (in WA's .rdata) ────────────────────────────────

/// `"Dud"` at WA 0x006643D8.
const DUD_TEXT_VA: u32 = 0x006643D8;
/// `"?"` at WA 0x00661654.
const QUESTION_TEXT_VA: u32 = 0x00661654;

// ─── CalcSprite ────────────────────────────────────────────────────────────

/// Rust port of `MineEntity::CalcSprite` (0x00506E60). Returns
/// `(sprite_id, palette_value)`. The "palette" is the WA name for the
/// `frame`/`palette` slot of the legacy `DrawSpriteLocal` command: a
/// Fixed-shaped value derived from `mine.angle` and a world-level
/// frame-scale factor. See [`mine_render`] for how it's threaded into
/// the render-queue command.
unsafe fn calc_sprite(this: *mut MineEntity) -> (u32, i32) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        // palette = (mine + 0x98) * world+0x8150 (Fixed × Fixed → Fixed)
        //         + mine.angle (Fixed)
        // The +0x98 slot lives inside `WorldEntity._unknown_98`; not yet
        // surfaced as a typed field. Read raw.
        let frame_field = *((this as *const u8).add(0x98) as *const i32);
        let scale = (*world).render_interp_a.to_raw();
        let mul = (frame_field as i64).wrapping_mul(scale as i64);
        let mul_fixed = ((mul as u64 >> 16) as u32) | (((mul >> 32) as u32) << 16);
        let mut palette = (mul_fixed as i32).wrapping_add((*this).base.angle.to_raw());

        // Default sprite is 0x2F. When triggered (`_field_128 != 0`) and
        // `(fuse_timer * 4 / 1000) & 1 != 0`, swap to the flashing 0x2D.
        let triggered = (*this)._field_128 != 0;
        let mut sprite = if triggered {
            let fuse = (*this).fuse_timer;
            let v = ((fuse as i64 * 4) / 1000) as i32;
            if v & 1 != 0 { 0x2D } else { 0x2F }
        } else {
            0x2F
        };

        // Drown gate — `_field_b0 != 0 || _field_a4 != 0` swaps to the
        // underwater sprite via the WA lookup table.
        let drown_a = (*this).base._field_b0;
        let drown_b = (*this).base._field_a4 as i32;
        if drown_a != 0 || drown_b != 0 {
            sprite = drown(sprite);
        }
        // When `_field_b0` is set, palette is forced to 0 — anchors the
        // underwater sprite at the mine's screen y rather than offsetting
        // by the angle/frame term.
        if drown_a != 0 {
            palette = 0;
        }

        (sprite, palette)
    }
}

// ─── Render ────────────────────────────────────────────────────────────────

/// Rust port of `MineEntity::Render` (0x00506EF0). stdcall(this), RET 0x4.
pub unsafe fn mine_render(this: *mut MineEntity) {
    unsafe {
        let (sprite_id, palette) = calc_sprite(this);
        let world = (*(this as *const BaseEntity)).world;
        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        let triggered = (*this)._field_128 != 0;
        let activity_rank = (*this).activity_rank_slot as i32;

        let primary_palette_idx = pick_palette_index(world, activity_rank);

        // Emit the body sprite.
        // WA: `EAX = (triggered ? 0xFFF80000 : 0) + 0x120000`. The triggered
        // branch overflows: `0xFFF80000 + 0x120000 = 0x000A0000` (mod 2^32).
        let sprite_op_flags: u32 = if triggered { 0x000A0000 } else { 0x00120000 };
        let layer = sprite_op_flags
            .wrapping_add((primary_palette_idx as u32).wrapping_mul(2))
            .wrapping_add(1);
        let rq = (*world).render_queue;
        let _ = (*rq).push_typed(
            layer,
            RenderMessage::Sprite {
                local: true,
                x: pos_x.floor(),
                y: pos_y.floor(),
                sprite: SpriteOp(sprite_id),
                palette: palette as u32,
            },
        );

        // Textbox-render gate: three world/game_info conditions must hold.
        let textbox_threshold = 3i32 - if (*world).terrain_pct_b != 0 { 1 } else { 0 };
        if ((*world)._field_7640 as i32) >= textbox_threshold {
            return;
        }
        if (*world)._field_7648 == 0 {
            return;
        }
        let game_info = (*world).game_info;
        let game_info_db08 = *((game_info as *const u8).add(0xDB08));
        if game_info_db08 == 0 {
            return;
        }

        // Textbox uses the same palette-index lookup. Recompute (the WA
        // function rereads it to avoid spilling the value across the gate).
        let textbox_palette_idx = pick_palette_index(world, activity_rank);

        // Pick the displayed text.
        let mut text_buf: HString<16> = HString::new();
        let text_ptr = pick_textbox_text(this, world, &mut text_buf);

        // SetTextboxText layout + blit.
        let mut text_w: i32 = 0;
        let mut text_h: i32 = 0;
        let textbox = (*this)._field_198;
        let shadow_lo = (*world).gfx_color_table[7];
        let shadow_hi = (*world).gfx_color_table[6];
        let bitmap = set_textbox_text(
            textbox,
            text_ptr,
            7,
            shadow_lo,
            shadow_hi,
            &mut text_w,
            &mut text_h,
            Fixed::from_raw(0x10000),
        );

        // RQ_DrawTextboxLocal — the textbox is anchored 18 pixels (Fixed
        // 0x120000) above the mine's pos_y.
        let textbox_layer = (textbox_palette_idx as u32)
            .wrapping_mul(2)
            .wrapping_add(0xD0200);
        let textbox_y = Fixed::from_raw(pos_y.to_raw().wrapping_sub(0x00120000));
        let _ = (*rq).push_typed(
            textbox_layer,
            RenderMessage::TextboxLocal {
                x: pos_x.floor(),
                y: textbox_y.floor(),
                bitmap,
                src_w: text_w,
                src_h: text_h,
                flags: 0,
            },
        );
    }
}

/// Look up the per-team palette index for a mine. When
/// `activity_rank_slot < 0` (mine has no team yet — unplaced or anonymous
/// pre-placed) the lookup falls back to one of two world-level slots
/// chosen by a 0x100 threshold; otherwise it indexes the per-team
/// palette table at `world+0x2600`.
unsafe fn pick_palette_index(world: *const GameWorld, activity_rank_slot: i32) -> i32 {
    unsafe {
        if activity_rank_slot < 0 {
            let primary = *((world as *const u8).add(0x3608) as *const i32);
            if primary > 0x100 {
                primary
            } else {
                *((world as *const u8).add(0x3604) as *const i32)
            }
        } else {
            let table = (world as *const u8).add(0x2600) as *const i32;
            *table.add(activity_rank_slot as usize)
        }
    }
}

/// Pick the text shown in the mine's countdown textbox.
///
/// Branches:
/// - `fled != 0` → static `"Dud"` from WA's .rdata.
/// - Positive `fuse_timer` AND not in `?`-mode → `"%d"` of `fuse / 1000`.
/// - Negative `fuse_timer` AND replay-recorded fuse available → `"%d.%02d"`
///   of the recorded fuse, converted to centisecond pairs.
/// - Otherwise (no recorded fuse and still negative) → static `"?"`.
unsafe fn pick_textbox_text(
    this: *mut MineEntity,
    world: *const GameWorld,
    out_buf: &mut HString<16>,
) -> *const c_char {
    unsafe {
        if (*this).fled != 0 {
            return rb(DUD_TEXT_VA) as *const c_char;
        }

        let fuse = (*this).fuse_timer;
        let game_info = (*world).game_info;
        // game_info+0xD934 — signed byte; bit-7 set means "use `?` for
        // negative fuse" (no replay-recorded fuse hint).
        let game_info_d934 = *((game_info as *const u8).add(0xD934)) as i8;
        // mine + 0x16C lives inside `MineEntity._unknown_148` (the
        // WeaponReleaseContext mirror block). Read as signed i32 at raw
        // offset until the field is surfaced.
        let field_16c = *((this as *const u8).add(0x16C) as *const i32);
        let scheme_uses_recorded_fuse =
            field_16c >= 0 && game_info_d934 >= 0 && (*this)._field_128 == 0;

        if fuse >= 0 && scheme_uses_recorded_fuse {
            // Plain seconds — `fuse / 1000`.
            let _ = write!(out_buf, "{}\0", fuse / 1000);
            return out_buf.as_ptr() as *const c_char;
        }

        // Either fuse is negative OR scheme path falls through. Try the
        // replay-recorded fuse before giving up to "?".
        let mut effective_fuse = fuse;
        if effective_fuse < 0 {
            let track_idx = (*this)._field_194;
            let log_ptr = (*world)._unknown_51c;
            if track_idx != 0xFFFFFFFF
                && !log_ptr.is_null()
                && let Some(recorded) = log_lookup(log_ptr, track_idx)
            {
                effective_fuse = recorded;
            }
        }

        if effective_fuse < 0 {
            return rb(QUESTION_TEXT_VA) as *const c_char;
        }

        // "%d.%02d" of `((fuse+19)/20)*2` as centisecond-ish pairs.
        let units = ((effective_fuse.wrapping_add(0x13)) / 0x14).wrapping_mul(2);
        let secs = units / 100;
        let cents = units % 100;
        let _ = write!(out_buf, "{}.{:02}\0", secs, cents);
        out_buf.as_ptr() as *const c_char
    }
}

/// Read `replay_log.recorded_fuses[track_idx]` with the same
/// bounds-check WA's inline `_Vector_at` (FUN_00507c20) performs. The
/// vector lives at `replay_log + 0x18` (`begin`/`end` ptr pair).
/// Returns `None` if the index is out of range.
unsafe fn log_lookup(log_ptr: *mut u8, track_idx: u32) -> Option<i32> {
    unsafe {
        let vec_base = log_ptr.add(0x18) as *const u8;
        let begin = *(vec_base.add(0x4) as *const *const i32);
        let end = *(vec_base.add(0x8) as *const *const i32);
        if begin.is_null() {
            return None;
        }
        let len = (end as usize - begin as usize) / 4;
        if (track_idx as usize) >= len {
            return None;
        }
        Some(*begin.add(track_idx as usize))
    }
}
