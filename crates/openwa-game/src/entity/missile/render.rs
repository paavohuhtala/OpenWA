//! Rust port of `Task_Missile::render` (0x005091A0). Per-frame render
//! handler for an airborne projectile, called by case 3 (RenderScene) in
//! [`super::handle_message`].
//!
//! Emits up to two render-queue commands per frame:
//!   1. A fuse-timer countdown textbox (only when the missile is above
//!      water, the fuse is non-zero, and either the textbox is visible
//!      during normal play (`fuse < textbox_visible_threshold`) or the
//!      replay-overlay gate is open).
//!   2. The body sprite, with branching by [`MissileType`] / underwater /
//!      super-animal state. The animation phase (palette param) is
//!      derived from one of four formulas selected by the per-pellet
//!      `animation_rate_kind` discriminator (and overridden for sheep).
//!
//! Originally bridged via `bridge_missile_render` in `super::handle_message`.

use core::ffi::c_char;
use core::fmt::Write as _;
use core::sync::atomic::{AtomicU32, Ordering};

use heapless::String as HString;

use super::{MissileEntity, MissileType};
use crate::engine::world::GameWorld;
use crate::entity::base::BaseEntity;
use crate::rebase::rb;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_op::SpriteOp;
use crate::render::textbox::{Textbox, set_text as set_textbox_text};
use openwa_core::fixed::Fixed;

// ─── Bridges ───────────────────────────────────────────────────────────────

static DROWN_ADDR: AtomicU32 = AtomicU32::new(0);
static mut FIXA2TAN16_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        DROWN_ADDR.store(rb(0x00565D60), Ordering::Relaxed);
        FIXA2TAN16_ADDR = rb(0x00575730);
    }
}

/// `drown` (0x00565D60) — fastcall(ECX = sprite). Maps an in-air sprite
/// ID to its underwater counterpart (low 16 bits substituted via a
/// lookup table; high 16 bits preserved).
unsafe fn drown(sprite: u32) -> u32 {
    unsafe {
        let f: unsafe extern "fastcall" fn(u32) -> u32 =
            core::mem::transmute(DROWN_ADDR.load(Ordering::Relaxed) as usize);
        f(sprite)
    }
}

/// `Math__fixa2tan16` (0x00575730) — `__usercall(ESI = y, EDI = x)`,
/// plain RET. Returns a Fixed16-style atan2 angle in EAX. Both ESI and
/// EDI are callee-saved per the x86 ABI, so the trampoline preserves
/// them across the call.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_fixa2tan16(_y: i32, _x: i32) -> u32 {
    core::arch::naked_asm!(
        "push esi",
        "push edi",
        "mov esi, dword ptr [esp+12]",
        "mov edi, dword ptr [esp+16]",
        "mov eax, dword ptr [{addr}]",
        "call eax",
        "pop edi",
        "pop esi",
        "ret 8",
        addr = sym FIXA2TAN16_ADDR,
    );
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Render-time anchor offset (18 pixels) applied to BOTH axes of the
/// fuse-countdown textbox position. WA's render uses identical X- and
/// Y-shifts; preserve the bit-for-bit behaviour.
const TEXTBOX_OFFSET: Fixed = Fixed::from_raw(0x00120000);

/// HandleMessage and Render select between two "discriminator" slots
/// inside the render-data block based on [`is_cluster_pellet`]:
/// single-shot missiles read [`fire_particle_trigger`] /
/// [`_render_data_07`], cluster pellets read [`render_timer`] /
/// [`_render_data_1a`]. This view returns the matching pair as
/// `(default_sprite_id, animation_rate_kind)`.
///
/// Mirrors WA's `iVar13` setup at the top of `Task_Missile::render`.
///
/// [`is_cluster_pellet`]: MissileEntity::is_cluster_pellet
/// [`fire_particle_trigger`]: MissileEntity::fire_particle_trigger
/// [`_render_data_07`]: MissileEntity::_render_data_07
/// [`render_timer`]: MissileEntity::render_timer
/// [`_render_data_1a`]: MissileEntity::_render_data_1a
#[inline]
unsafe fn render_view(this: *const MissileEntity) -> (u32, u32) {
    unsafe {
        if (*this).is_cluster_pellet != 0 {
            ((*this).render_timer as u32, (*this)._render_data_1a)
        } else {
            ((*this).fire_particle_trigger, (*this)._render_data_07)
        }
    }
}

/// Pick the activity-queue render rank for a missile. Identical fallback
/// shape to [`MineEntity::Render`](super::super::mine::render): when the
/// missile's slot is `< 0` (queue full at construction time), fall back
/// to the queue's `capacity` (when `> 0x100`) or `count`; otherwise
/// return `entity_activity_queue.ages[slot]`.
#[inline]
unsafe fn pick_render_rank(world: *const GameWorld, activity_rank_slot: i32) -> i32 {
    unsafe {
        let queue = &(*world).entity_activity_queue;
        if activity_rank_slot < 0 {
            let capacity = queue.capacity as i32;
            if capacity > 0x100 {
                capacity
            } else {
                queue.count as i32
            }
        } else {
            queue.ages[activity_rank_slot as usize] as i32
        }
    }
}

/// Replay-overlay textbox visibility gate. Mirrors WA's `bVar6` block
/// at the top of `Task_Missile::render`:
///
/// `world._field_7640 < 3 - (terrain_pct_b != 0 ? 1 : 0)
///   && world._field_7648 != 0
///   && replay_flag_a != 0`
///
/// When this returns `true`, the fuse-timer textbox is shown for the
/// whole fuse duration (and formatted as `"%d.%02d"` seconds.cs); when
/// `false`, the textbox is gated by `fuse < textbox_visible_threshold`
/// and formatted as plain `"%d"` ceil-seconds.
unsafe fn textbox_replay_gate(world: *const GameWorld) -> bool {
    unsafe {
        let threshold = 3i32 - if (*world).terrain_pct_b != 0 { 1 } else { 0 };
        if ((*world)._field_7640 as i32) >= threshold {
            return false;
        }
        if (*world)._field_7648 == 0 {
            return false;
        }
        let game_info = (*world).game_info;
        ((*game_info).replay_flags_packed as u8) != 0
    }
}

/// Format the fuse-timer countdown for the textbox.
///
/// - Replay-overlay path (`bVar6 == true`): `"%d.%02d"` of
///   `(fuse / 1000)` seconds and `(fuse % 1000) / 10` centiseconds.
/// - Normal-play path: ceil-divide by 1000 (= `(fuse + 999) / 1000`)
///   and write as plain `"%d"`.
unsafe fn format_fuse_text(fuse_timer: i32, replay_visible: bool, buf: &mut HString<16>) {
    if replay_visible {
        let q = fuse_timer / 1000;
        let r = (fuse_timer % 1000) / 10;
        let _ = write!(buf, "{}.{:02}\0", q, r);
    } else {
        let v = fuse_timer.wrapping_add(0x3E7) / 1000;
        let _ = write!(buf, "{}\0", v);
    }
}

/// Emit the per-missile fuse-countdown textbox.
///
/// Color picks:
/// - Final-3-seconds flicker (`fuse_timer < 3000`): `font_index = 0`,
///   fill = `gfx_color_table[7]`, border alternates every 25 frames
///   between `gfx_color_table[6]` and `gfx_color_table[8]`
///   (`world.frame / 25 & 1`).
/// - Otherwise: `font_index = 6`, fill = `gfx_color_table[7]`, border =
///   `gfx_color_table[6]`.
///
/// Position anchor: `(pos_x - 18, pos_y - 18)` (Fixed). Layer:
/// `render_rank * 2 + 0x50000`.
unsafe fn emit_textbox(
    this: *mut MissileEntity,
    world: *mut GameWorld,
    pos_x: Fixed,
    pos_y: Fixed,
    fuse_timer: i32,
    replay_visible: bool,
    layer_base: u32,
) {
    unsafe {
        let mut buf: HString<16> = HString::new();
        format_fuse_text(fuse_timer, replay_visible, &mut buf);
        let text_ptr = buf.as_ptr() as *const c_char;

        let (font_index, fill_color, border_color) = if fuse_timer < 3000 {
            let parity = ((*world).frame as i32) / 25 & 1;
            let border = if parity == 0 {
                (*world).gfx_color_table[6]
            } else {
                (*world).gfx_color_table[8]
            };
            (0i32, (*world).gfx_color_table[7], border)
        } else {
            (
                6i32,
                (*world).gfx_color_table[7],
                (*world).gfx_color_table[6],
            )
        };

        let mut text_w: i32 = 0;
        let mut text_h: i32 = 0;
        let textbox = (*this).render_handle_a as *mut Textbox;
        let bitmap = set_textbox_text(
            textbox,
            text_ptr,
            font_index,
            fill_color,
            border_color,
            &mut text_w,
            &mut text_h,
            Fixed::ONE,
        );

        let rq = (*world).render_queue;
        let textbox_x = pos_x - TEXTBOX_OFFSET;
        let textbox_y = pos_y - TEXTBOX_OFFSET;
        let _ = (*rq).push_typed(
            layer_base,
            RenderMessage::TextboxLocal {
                x: textbox_x.floor(),
                y: textbox_y.floor(),
                bitmap,
                src_w: text_w,
                src_h: text_h,
                flags: 0,
            },
        );
    }
}

/// Push a missile body sprite to the render queue. Matches WA's
/// `RQ_DrawSpriteLocal` parameter order: `(layer, x, y, sprite,
/// palette)` with the X/Y values floored to integer Fixed (low 16 bits
/// dropped).
#[inline]
unsafe fn emit_sprite(
    rq: *mut crate::render::queue::RenderQueue,
    layer: u32,
    pos_x: Fixed,
    pos_y_or_subframe: Fixed,
    sprite_id: u32,
    palette: u32,
) {
    unsafe {
        let _ = (*rq).push_typed(
            layer,
            RenderMessage::Sprite {
                local: true,
                x: pos_x.floor(),
                y: pos_y_or_subframe.floor(),
                sprite: SpriteOp(sprite_id),
                palette,
            },
        );
    }
}

// ─── Render entry ──────────────────────────────────────────────────────────

/// Rust port of `Task_Missile::render` (0x005091A0). stdcall(this), RET 0x4.
///
/// **Side-effect note:** WA's render writes intermediate values into
/// [`MissileEntity::animation_phase`] for every code path that takes
/// the inner-switch fall-through (cases 0..=2 and the sheep-override
/// tail). The port preserves those writes verbatim — they're observed
/// by HandleMessage case 5 (UpdateNonCritical) and the next-frame
/// case 2 (FrameFinish) tick, so collapsing to a local would silently
/// desync.
pub unsafe fn missile_render(this: *mut MissileEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;
        let speed_x = (*this).base.speed_x;
        let speed_y = (*this).base.speed_y;
        let direction_initial = (*this).direction;
        let activity_rank_slot = (*this).activity_rank_slot as i32;

        let render_rank = pick_render_rank(world, activity_rank_slot);
        let layer_base = (render_rank as u32).wrapping_mul(2).wrapping_add(0x50000);

        // ── Textbox ────────────────────────────────────────────────────────
        let replay_visible = textbox_replay_gate(world);
        let fuse_timer = (*this).fuse_timer;
        let underwater = (*this).base._field_b0 != 0;
        if fuse_timer != 0
            && (replay_visible || fuse_timer < (*this).textbox_visible_threshold)
            && !underwater
        {
            emit_textbox(
                this,
                world,
                pos_x,
                pos_y,
                fuse_timer,
                replay_visible,
                layer_base,
            );
        }

        // ── Homing-specific fast paths (early-return) ──────────────────────
        let rq = (*world).render_queue;
        let sprite_layer = layer_base.wrapping_add(1);
        if matches!((*this).missile_type, MissileType::Homing) {
            // Underwater homing: animation_phase = (frame_counter << 16) / 50,
            // sprite = drown(animation_rate_kind), early return.
            if underwater {
                let (_, anim_kind) = render_view(this);
                let mut sprite = drown(anim_kind);
                if direction_initial < 0 {
                    sprite |= 0x40000;
                }
                let frame_counter = (*world).frame_counter as i64;
                let palette = ((frame_counter << 16) / 50) as u32;
                emit_sprite(rq, sprite_layer, pos_x, pos_y, sprite, palette);
                return;
            }
            // Super-animal active: alternate two walk-cycle sprites every
            // 5 frames, animation_phase = super_animal_torque_accum.
            if (*this).contact_phase == 1 {
                let torque = (*this).super_animal_torque_accum;
                (*this).animation_phase = torque;
                let parity = ((*world).frame_counter as i32) / 5 & 1;
                let mut sprite = if parity == 0 {
                    (*this).super_animal_walk_sprite_alt
                } else {
                    (*this).super_animal_walk_sprite
                };
                // Drowned super-animal sprite when below the kill line —
                // the missile is mid-fall but still visible above water.
                if (pos_y.to_raw() >> 16) >= (*world).water_kill_y {
                    sprite = drown(sprite);
                }
                emit_sprite(rq, sprite_layer, pos_x, pos_y, sprite, torque);
                return;
            }
        }

        // ── Sub-frame Y override for homing (non-underwater, non-super) ────
        // WA's `local_4` is the Y coordinate passed to RQ_DrawSpriteLocal.
        // Initially `pos_y`, but the homing-fall-through path adds
        // `ricochet_counter << 16` so the sprite renders one ricochet-tile
        // higher per remaining bounce (used as a HUD breadcrumb).
        let pos_y_for_sprite = if matches!((*this).missile_type, MissileType::Homing) {
            Fixed::from_raw(
                pos_y
                    .to_raw()
                    .wrapping_add(((*this).ricochet_counter as i32) << 16),
            )
        } else {
            pos_y
        };

        // ── Inner animation-rate-kind switch ───────────────────────────────
        let (default_sprite, anim_kind) = render_view(this);
        let mut sprite_id = if (*this)._unknown_3a4 != 0 {
            (*this).alt_sprite_id
        } else {
            default_sprite
        };
        let mut direction_flag = direction_initial;

        match anim_kind {
            0 => {
                // animation_phase = clamp(0x10000 - (fuse << 16) / fuse_timer_initial, 0..=0xFFFF)
                let raw = ((fuse_timer as i64) << 16) / (*this).fuse_timer_initial as i64;
                let candidate = 0x10000i64 - raw;
                let clamped = if candidate < 0 {
                    0
                } else if candidate > 0xFFFF {
                    0xFFFF
                } else {
                    candidate as i32
                };
                (*this).animation_phase = clamped as u32;
            }
            1 => {
                // animation_phase = angle, optionally folded with
                // `_field_98 * world.render_interp_a` when the missile is
                // mid-action and not in the sheep-stash state.
                let mut new_phase = (*this).base.angle.to_raw();
                let action_flag = (*this).base.subclass_data.action_flag;
                let sheep_state_flag = (*this).base.subclass_data.sheep_state_flag;
                if action_flag != 0 && sheep_state_flag == 0 {
                    let interp_term = (*this)
                        .base
                        ._field_98
                        .mul_raw((*world).render_interp_a)
                        .to_raw();
                    new_phase = interp_term.wrapping_add((*this).base.angle.to_raw());
                }
                (*this).animation_phase = new_phase as u32;
            }
            2 => {
                // animation_phase = atan2(speed_x, -speed_y), only when
                // either velocity component is non-zero.
                if speed_x.to_raw() != 0 || speed_y.to_raw() != 0 {
                    let angle = bridge_fixa2tan16(speed_x.to_raw(), -speed_y.to_raw());
                    (*this).animation_phase = angle;
                }
            }
            3 => {
                // sprite_id += min(abs(speed_x) / 2 >> 16, 3),
                // direction_flag = (speed_x >= 0) ? +1 : -1.
                let abs_sx = speed_x.to_raw().wrapping_abs() as u32;
                let mut delta = ((abs_sx >> 1) >> 16) as i32;
                if delta > 3 {
                    delta = 3;
                }
                direction_flag = if speed_x.to_raw() >= 0 { 1 } else { -1 };
                sprite_id = sprite_id.wrapping_add(delta as u32);
            }
            _ => {}
        }

        // EDI = animation_phase reload, used as the sprite's palette unless
        // the underwater-or-wet swap below forces it to 0.
        let mut palette = (*this).animation_phase as i32;

        // ── Sheep override ─────────────────────────────────────────────────
        if matches!((*this).missile_type, MissileType::Sheep) {
            if (*this).sheep_bailout_counter == 0 {
                // Sheep walking: sprite ID lives in the same slot as
                // `impact_sound_id` (slot 0x340) — re-purposed for sheep
                // since they don't take an impact sound.
                sprite_id = (*this).impact_sound_id;
            } else {
                // Sheep-bailout walk-cycle: rotate through three sprite
                // slots (0x344 / 0x348 / 0x34C) every 3 frames.
                let idx = ((*world).frame_counter as i32 / 3) % 3;
                sprite_id = match idx {
                    0 => (*this).ricochet_side_mask,
                    1 => (*this).ricochet_chance_pct,
                    _ => (*this)._render_data_1e,
                };
            }
            // animation_phase = clamp(atan2(speed_x, -speed_y) * 2, 0..=0xFFFF)
            let angle = bridge_fixa2tan16(speed_x.to_raw(), -speed_y.to_raw());
            let doubled = (angle.wrapping_mul(2)) as i32;
            (*this).animation_phase = doubled as u32;
            palette = if doubled < 0 {
                0
            } else if doubled > 0xFFFF {
                0xFFFF
            } else {
                doubled
            };
        }

        // ── Underwater / wet swap ──────────────────────────────────────────
        if (*this).base._field_b0 != 0 || (*this).base._field_a4 != 0 {
            palette = 0;
            sprite_id = drown(sprite_id);
        }

        if direction_flag < 0 {
            sprite_id |= 0x40000;
        }

        emit_sprite(
            rq,
            sprite_layer,
            pos_x,
            pos_y_for_sprite,
            sprite_id,
            palette as u32,
        );
    }
}
