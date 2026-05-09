//! Rust port of `Task_Missile::render` (0x005091A0) and
//! `Task_Missile::render_indicator` (0x00508F90). Called from case 3
//! (RenderScene) in [`super::handle_message`].

use core::ffi::c_char;
use core::fmt::Write as _;

use heapless::String as HString;

use super::{MissileEntity, MissileType};
use crate::engine::game_info::GameInfo;
use crate::engine::world::{GameWorld, Vec2WorldExt};
use crate::entity::base::BaseEntity;
use crate::rebase::rb;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_id::KnownSpriteId;
use crate::render::sprite::sprite_op::{SpriteFlags, SpriteOp};
use crate::render::textbox::{Textbox, set_text as set_textbox_text};
use openwa_core::fixed::Fixed;
use openwa_core::vec2::Vec2;

// ─── Bridges ───────────────────────────────────────────────────────────────

static mut FIXA2TAN16_ADDR: u32 = 0;

pub unsafe fn init_addrs() {
    unsafe {
        FIXA2TAN16_ADDR = rb(0x00575730);
    }
}

const INDICATOR_INSET: Fixed = Fixed::from_raw(0x00300000);
const TEXTBOX_VELOCITY_SCALE: i32 = 32;
const TEXTBOX_OFFSET: Fixed = Fixed::from_raw(0x00120000);

/// Pure-Rust port of `drown` (0x00565D60). Maps an in-air [`SpriteOp`] to its
/// underwater counterpart via a sparse LUT keyed by [`KnownSpriteId`]. The
/// caller's transform flags are preserved; only the sprite index is rewritten
/// when a mapping exists.
fn drown(sprite: SpriteOp) -> SpriteOp {
    use KnownSpriteId::*;
    let flags = sprite.flags();
    let mapped = match KnownSpriteId::try_from(sprite.index() as u32) {
        Ok(known) => match known {
            Grave1 => SpriteOp::from(Grave1V2),
            Grave2 => SpriteOp::from(Grave2V2),
            Grave3 => SpriteOp::from(Grave3V2),
            Grave4 => SpriteOp::from(Grave4V2),
            Grave5 => SpriteOp::from(Grave5V2),
            Grave6 => SpriteOp::from(Grave6V2),
            MineOn => SpriteOp::from(MineOnV2),
            Arrow => SpriteOp::from(ArrowV2),
            MineOff => SpriteOp::from(MineOffV2),
            Missile => SpriteOp::from(MissileV2),
            Bullet => SpriteOp::from(BulletV2),
            Grenade => SpriteOp::from(GrenadeV2),
            Banana => SpriteOp::from(BananaV2),
            Mortar => SpriteOp::from(MortarV2),
            Cluster => SpriteOp::from(ClusterV2),
            Clustlet => SpriteOp::from(ClustletV2),
            PetrolBm => SpriteOp::from(PetrolBmV2),
            HGrenade => SpriteOp::from(HGrenadeV2),
            Tamborin => SpriteOp::from(TamborinV2),
            HMissil1 => SpriteOp::from(HMissil1V2),
            HMissil2 => SpriteOp::from(HMissil2V2),
            AirMisl => SpriteOp::from(AirMislV2),
            SheepFal | SheepBrn => SpriteOp::from(SheepFalV2),
            Carpet1 => SpriteOp::from(Carpet1V2),
            Carpet2 => SpriteOp::from(Carpet2V2),
            SheepLau | LShpWlk | SheepWlk => SpriteOp::from(Sheep),
            Meteor => SpriteOp::from(MeteorV2),
            MbBomb => SpriteOp::from(MbBombV2),
            Letter1 => SpriteOp::from(Letter1V2),
            Letter2 => SpriteOp::from(Letter2V2),
            Vase => SpriteOp::from(VaseV2),
            VaseBit1 => SpriteOp::from(VaseBit1V2),
            VaseBit2 => SpriteOp::from(VaseBit2V2),
            VaseBit3 => SpriteOp::from(VaseBit3V2),
            Dynamite => SpriteOp::from(DynamiteV2),
            Donkey => SpriteOp::from(DonkeyV2),
            Targets | TargetsV => SpriteOp::from(TargetsV2),
            WCrate1 | WCrateV => SpriteOp::from(WCrate1V2),
            MCrate1 | MCrateV => SpriteOp::from(MCrate1V2),
            UCrate1 | UCrateV => SpriteOp::from(UCrate1V2),
            Donor => SpriteOp::from(DonorV2),
            OilDrum1 | OilDrum2 | OilDrum3 | OilDrum4 => SpriteOp::from(OilDrum),
            AquaShp1 => SpriteOp::from(AquaShp1V2),
            AquaShp2 => SpriteOp::from(AquaShp2V2),
            MoleDive => SpriteOp::from(Mole),
            Woman => SpriteOp::from(WomanV2),
            Sally => SpriteOp::from(SallyV2),
            CowWalk | CowJump => SpriteOp::from(CowWalkV2),
            Skunk1 | Skunk2 => SpriteOp::from(SkunkV2),
            PigeonR0 => SpriteOp::from(PigeonR0V2),
            PigeonR1 => SpriteOp::from(PigeonR1V2),
            PigeonR2 => SpriteOp::from(PigeonR2V2),
            PigeonR3 => SpriteOp::from(PigeonR3V2),
            // Unmapped: pass the index through unchanged.
            other => SpriteOp::from(other),
        },
        Err(raw) => SpriteOp::from_index(raw as u16),
    };
    SpriteOp(mapped.0 | flags.bits())
}

/// `Math::fixa2tan16` (0x00575730) — `__usercall(ESI = y, EDI = x)`. Both
/// regs callee-saved per ABI.
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

#[inline]
unsafe fn render_view(this: *const MissileEntity) -> (u32, u32) {
    unsafe {
        if (*this).homing_engaged_latch != 0 {
            ((*this).render_timer as u32, (*this)._render_data_1a)
        } else {
            ((*this).fire_particle_trigger, (*this)._render_data_07)
        }
    }
}

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
            let parity = (((*world).frame as i32) / 25) & 1;
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

#[inline]
unsafe fn emit_sprite(
    rq: *mut crate::render::queue::RenderQueue,
    layer: u32,
    pos_x: Fixed,
    pos_y_or_subframe: Fixed,
    sprite: SpriteOp,
    palette: u32,
) {
    unsafe {
        let _ = (*rq).push_typed(
            layer,
            RenderMessage::Sprite {
                local: true,
                x: pos_x.floor(),
                y: pos_y_or_subframe.floor(),
                sprite,
                palette,
            },
        );
    }
}

// ─── Render entry ──────────────────────────────────────────────────────────

/// `Task_Missile::render` (0x005091A0). stdcall(this), RET 0x4.
///
/// Side-effect note: every fall-through path writes `animation_phase`
/// — those writes are observed by HandleMessage cases 5 (UpdateNonCritical)
/// and 2 (FrameFinish) on the next tick, so collapsing them to a local
/// would silently desync.
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

        let rq = (*world).render_queue;
        let sprite_layer = layer_base.wrapping_add(1);
        if matches!((*this).missile_type, MissileType::Animal) {
            if underwater {
                // WA reads [ESI+4] (= the `default_sprite` slot, NOT
                // [ESI+8] anim_kind). Sheep's anim_kind is a small int
                // (e.g. 6) which fell through drown's passthrough as
                // KnownSpriteId::CrshairP — visible as a crosshair.
                let (default_sprite, _) = render_view(this);
                let mut sprite = drown(SpriteOp(default_sprite));
                if direction_initial < 0 {
                    sprite.0 |= SpriteFlags::MIRROR_X.bits();
                }
                let frame_counter = (*world).frame_counter as i64;
                let palette = ((frame_counter << 16) / 50) as u32;
                emit_sprite(rq, sprite_layer, pos_x, pos_y, sprite, palette);
                return;
            }
            if (*this).contact_phase == 1 {
                let torque = (*this).super_animal_torque_accum;
                (*this).animation_phase = torque;
                let parity = (((*world).frame_counter as i32) / 5) & 1;
                let mut sprite = SpriteOp(if parity == 0 {
                    (*this).super_animal_walk_sprite_alt
                } else {
                    (*this).super_animal_walk_sprite
                });
                if (pos_y.to_raw() >> 16) >= (*world).water_kill_y {
                    sprite = drown(sprite);
                }
                emit_sprite(rq, sprite_layer, pos_x, pos_y, sprite, torque);
                return;
            }
        }

        // Animal fall-through breadcrumb: draw one ricochet-tile higher
        // per remaining bounce.
        let pos_y_for_sprite = if matches!((*this).missile_type, MissileType::Animal) {
            pos_y.wrapping_add(Fixed::from_int((*this).ricochet_counter as i32))
        } else {
            pos_y
        };

        let (default_sprite, anim_kind) = render_view(this);
        let mut sprite = SpriteOp(if (*this)._unknown_3a4 != 0 {
            (*this).alt_sprite_id
        } else {
            default_sprite
        });
        let mut direction_flag = direction_initial;

        match anim_kind {
            0 => {
                let raw = ((fuse_timer as i64) << 16) / (*this).fuse_timer_initial as i64;
                let clamped = (0x10000i64 - raw).clamp(0, 0xFFFF) as u32;
                (*this).animation_phase = clamped;
            }
            1 => {
                let mut new_phase = (*this).base.angle.to_raw();
                let action_flag = (*this).base.subclass_data.action_flag;
                let digger_state_flag = (*this).base.subclass_data.digger_state_flag;
                if action_flag != 0 && digger_state_flag == 0 {
                    let interp_term = (*this)
                        .base
                        ._field_98
                        .mul_raw((*world).render_interp_a)
                        .to_raw();
                    new_phase = interp_term.wrapping_add((*this).base.angle.to_raw());
                }
                (*this).animation_phase = new_phase as u32;
            }
            2 if speed_x.to_raw() != 0 || speed_y.to_raw() != 0 => {
                let angle = bridge_fixa2tan16(speed_x.to_raw(), -speed_y.to_raw());
                (*this).animation_phase = angle;
            }
            3 => {
                let abs_sx = speed_x.to_raw().wrapping_abs() as u32;
                let mut delta = ((abs_sx >> 1) >> 16) as i32;
                if delta > 3 {
                    delta = 3;
                }
                direction_flag = if speed_x.to_raw() >= 0 { 1 } else { -1 };
                sprite.0 = sprite.0.wrapping_add(delta as u32);
            }
            _ => {}
        }

        let mut palette = (*this).animation_phase as i32;

        if matches!((*this).missile_type, MissileType::Digger) {
            if (*this).digger_bailout_counter == 0 {
                // Pre-burrow sprite ID is co-located with `impact_sound_id`
                // (slot 0x340) — diggers don't take an impact sound.
                sprite = SpriteOp((*this).impact_sound_id);
            } else {
                // Bailout walk-cycle: 3 sprite slots (0x344/0x348/0x34C),
                // one per 3 frames.
                let idx = ((*world).frame_counter as i32 / 3) % 3;
                sprite = SpriteOp(match idx {
                    0 => (*this).ricochet_side_mask,
                    1 => (*this).ricochet_chance_pct,
                    _ => (*this)._render_data_1e,
                });
            }
            let angle = bridge_fixa2tan16(speed_x.to_raw(), -speed_y.to_raw());
            let doubled = angle.wrapping_mul(2) as i32;
            (*this).animation_phase = doubled as u32;
            palette = doubled.clamp(0, 0xFFFF);
        }

        if (*this).base._field_b0 != 0 || (*this).base._field_a4 != 0 {
            palette = 0;
            sprite = drown(sprite);
        }

        if direction_flag < 0 {
            sprite.0 |= SpriteFlags::MIRROR_X.bits();
        }

        emit_sprite(
            rq,
            sprite_layer,
            pos_x,
            pos_y_for_sprite,
            sprite,
            palette as u32,
        );
    }
}

// ─── Off-screen indicator ──────────────────────────────────────────────────

/// `Task_Missile::render_indicator` (0x00508F90). stdcall(this), RET 0x4.
pub unsafe fn render_indicator(this: *mut MissileEntity) {
    unsafe {
        let world = (*(this as *const BaseEntity)).world;
        let pos_x = (*this).base.pos_x;
        let pos_y = (*this).base.pos_y;

        let bound_min_x = (*world).level_bound_min_x;
        let bound_max_x = (*world).level_bound_max_x;
        let bound_min_y = (*world).level_bound_min_y;

        let on_screen = pos_x >= bound_min_x && pos_x <= bound_max_x && pos_y >= bound_min_y;
        if on_screen {
            return;
        }

        let render_rank = pick_render_rank(world, (*this).activity_rank_slot as i32);

        let speed_x = (*this).base.speed_x;
        let speed_y = (*this).base.speed_y;
        let angle = if speed_x.to_raw() == 0 && speed_y.to_raw() == 0 {
            0u32
        } else {
            let tan = bridge_fixa2tan16(speed_x.to_raw(), -speed_y.to_raw());
            (0x8000_i32).wrapping_sub(tan as i32) as u32
        };

        let mut indicator_x = pos_x;
        let lo_x = bound_min_x + INDICATOR_INSET;
        let hi_x = bound_max_x - INDICATOR_INSET;
        if indicator_x < lo_x {
            indicator_x = lo_x;
        }
        if indicator_x > hi_x {
            indicator_x = hi_x;
        }
        let mut indicator_y = pos_y;
        let lo_y = bound_min_y + INDICATOR_INSET;
        if indicator_y < lo_y {
            indicator_y = lo_y;
        }

        let rq = (*world).render_queue;

        let owner_id = (*this).spawn_params.owner_id;
        if owner_id != 0 {
            let game_info = (*world).game_info;
            let team_record = GameInfo::team_record_1based(game_info, owner_id as i32);
            let sprite = SpriteOp(((*team_record).font_palette_idx as u32).wrapping_add(0x20));
            let sprite_layer = (render_rank as u32).wrapping_mul(4).wrapping_add(0x50001);
            let _ = (*rq).push_typed(
                sprite_layer,
                RenderMessage::Sprite {
                    local: true,
                    x: indicator_x.floor(),
                    y: indicator_y.floor(),
                    sprite,
                    palette: angle,
                },
            );
        }

        let mut delta = Vec2::new(pos_x - indicator_x, pos_y - indicator_y);
        let distance = delta.normalize_via_world(world);
        // Distance in decameters (10:1 compression for a two-digit readout).
        let display_value = distance.to_int() / 10;

        let mut text: HString<16> = HString::new();
        let _ = write!(text, "{}\0", display_value);

        let mut speed_unit = Vec2::new(speed_x, speed_y);
        let _ = speed_unit.normalize_via_world(world);

        let mut text_w: i32 = 0;
        let mut text_h: i32 = 0;
        let textbox = (*this).render_handle_b as *mut Textbox;
        let bitmap = set_textbox_text(
            textbox,
            text.as_ptr() as *const c_char,
            6,
            (*world).gfx_color_table[7],
            (*world).gfx_color_table[6],
            &mut text_w,
            &mut text_h,
            Fixed::ONE,
        );

        let textbox_pos = Vec2::new(indicator_x, indicator_y) - speed_unit * TEXTBOX_VELOCITY_SCALE;

        let textbox_layer = (render_rank as u32).wrapping_mul(4).wrapping_add(0xD0000);
        let _ = (*rq).push_typed(
            textbox_layer,
            RenderMessage::TextboxLocal {
                x: textbox_pos.x.floor(),
                y: textbox_pos.y.floor(),
                bitmap,
                src_w: text_w,
                src_h: text_h,
                flags: 0,
            },
        );
    }
}
