use super::base::CTask;
use crate::FieldRegistry;
use crate::game::class_type::ClassType;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "CTaskCloud" {
        /// CTaskCloud vtable - cloud/airstrike entity
        vtable CTASK_CLOUD_VTABLE = 0x0066_9D38;
        /// CTaskCloud constructor (usercall: ESI=this, EAX=parent, EDI=render_y,
        /// stack: cloud_type, layer_depth, pos_x, vel_x). RET 0x10.
        ctor CTASK_CLOUD_CTOR = 0x0054_82E0;
        /// CTaskCloud::WriteReplayState — serializes cloud state to replay stream.
        /// thiscall + 1 stack param (stream ptr), RET 0x4.
        vmethod CTASK_CLOUD_WRITE_REPLAY_STATE = 0x0054_8430;
        /// CTaskCloud::ReadReplayState — deserializes cloud state from replay stream.
        /// usercall (ESI=stream, EDI=this). Not a vtable method.
        fn CTASK_CLOUD_READ_REPLAY_STATE = 0x0054_8370;
    }
}

/// CTaskCloud vtable — 7 slots. Same layout as CTask base; overrides slots 0 and 2.
///
/// Vtable at Ghidra 0x669D38. Size verified by gap to next vtable (CTaskCPU at 0x669D54).
/// CTaskCloud does NOT override ProcessFrame (slot 6) — all update logic is in
/// HandleMessage, responding to the FrameFinish message.
#[openwa_game::vtable(size = 7, va = 0x0066_9D38, class = "CTaskCloud")]
pub struct CTaskCloudVTable {
    /// WriteReplayState — serializes cloud state to replay stream.
    /// thiscall + 1 stack param (stream ptr), RET 0x4.
    #[slot(0)]
    pub write_replay_state: fn(this: *mut CTaskCloud, stream: *mut u8),
    /// HandleMessage — processes cloud messages (wind updates, render).
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message:
        fn(this: *mut CTaskCloud, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
}

/// Airstrike / weather cloud task.
///
/// Extends CTask directly (not CGameTask). Clouds drift horizontally with wind,
/// scroll on a parallax layer, and render as a single sprite.
///
/// Allocation: 0x74 bytes (operator new in CTaskTeam__CreateWeatherFilter 0x552960).
/// Constructor: 0x5482E0 (usercall ESI=this, EAX=parent, EDI=render_y).
/// Vtable: 0x669D38. Class type byte: 0x17 (ClassType::Cloud).
///
/// Three cloud sizes chosen by `cloud_type` param (0/1/2):
/// - type 0: sprite 0x268 (large),  phase_speed 0x200
/// - type 1: sprite 0x269 (medium), phase_speed 0x166
/// - type 2: sprite 0x26A (small),  phase_speed 0xCC
///
/// CreateWeatherFilter spawns clouds with a deterministic LCG (seed 0x12345678),
/// randomizing pos_x within level bounds and vel_x with random sign. The
/// `render_y` field encodes the cloud's vertical screen position as Fixed16
/// (`level_height/16 + scaled_offset` per cloud index) and is passed to
/// `RQ_DrawSpriteLocal` as the sprite Y at render time.
///
/// Source: Ghidra decompilation of 0x5482E0 (constructor), 0x5484C0 (HandleMessage),
///         0x548430 (WriteReplayState), 0x548370 (ReadReplayState).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskCloud {
    /// 0x00–0x2F: CTask base
    pub base: CTask<*const CTaskCloudVTable>,
    /// 0x30: Parallax scroll layer depth (Fixed; starts at 0x190000 = 25.0,
    /// decrements by 1 each cloud spawned in a batch)
    pub layer_depth: Fixed,
    /// 0x34: Per-frame animation phase counter (Fixed 16.16); incremented by
    /// `phase_speed` each FrameFinish. Passed to `blit_sprite` as the
    /// `palette` arg (cmd[5]) — acts as a per-cloud animation/palette index.
    /// Constructor initializes it to `(pos_x + render_y) & 0xFFFF`, which is
    /// always 0 since both addends have zero low halves.
    pub anim_phase: Fixed,
    /// 0x38: Animation phase speed (Fixed 16.16; set by cloud type: large=0x200, medium=0x166, small=0xCC)
    pub phase_speed: Fixed,
    /// 0x3C: Sprite ID passed to DrawSpriteLocal (0x268=large, 0x269=medium, 0x26A=small)
    pub sprite_id: u32,
    /// 0x40: X position (Fixed 16.16); wraps at landscape bounds each frame
    pub pos_x: Fixed,
    /// 0x44: Rendered Y position (Fixed 16.16, integer part in upper 16 bits).
    /// EDI register from constructor call site. In CreateWeatherFilter this is
    /// `(level_height/16 + level_height/10 * i/cloud_count + weather_mod) << 16`,
    /// placing each cloud at a small Y near the top of the level.
    ///
    /// Despite contributing to the initial `anim_phase` computation
    /// `(pos_x + render_y) & 0xFFFF` in the constructor, that mask discards the
    /// integer part and only ever yields 0 (both `pos_x` and `render_y` have
    /// their lower 16 bits zero). The integer Y is read here in HandleMessage
    /// and passed to `blit_sprite` as the sprite's screen Y.
    pub render_y: Fixed,
    /// 0x48: X velocity base (Fixed 16.16)
    pub vel_x: Fixed,
    /// 0x4C: Current wind acceleration (Fixed); converges toward wind_target each frame
    pub wind_accel: Fixed,
    /// 0x50: Target wind speed (Fixed); set by message 0x54 (wind-change event)
    pub wind_target: Fixed,
    /// 0x54–0x73: Unused padding. Allocation is 0x74 bytes but only 0x54 bytes are
    /// initialized (memset in CreateWeatherFilter). Not touched by constructor,
    /// HandleMessage, WriteReplayState, or ReadReplayState.
    pub _unknown_54: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<CTaskCloud>() == 0x74);

// Generate typed vtable method wrappers: write_replay_state(), handle_message().
bind_CTaskCloudVTable!(CTaskCloud, base.vtable);

use crate::game::TaskMessage;
use crate::render::message::RenderMessage;
use crate::render::sprite::sprite_op::SpriteOp;

/// CTaskCloud::HandleMessage replacement — pure game logic.
///
/// Handles three message types:
/// - FrameFinish: per-frame position update (parallax scroll with wind drift)
/// - RenderScene: draw sprite at computed position
/// - SetWind: set wind target from message data
///
/// Always calls base CTask::HandleMessage at the end (broadcast to children).
pub unsafe extern "thiscall" fn cloud_handle_message(
    this: *mut CTaskCloud,
    sender: *mut CTask,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    unsafe {
        let msg = TaskMessage::try_from(msg_type);

        match msg {
            Ok(TaskMessage::FrameFinish) => {
                // Advance Y position
                (*this).anim_phase = Fixed((*this).anim_phase.0 + (*this).phase_speed.0);

                // Advance X position: base velocity + wind * 10
                let wind = (*this).wind_accel.0;
                (*this).pos_x = Fixed((*this).pos_x.0 + (*this).vel_x.0 + wind * 10);

                // Wrap X at landscape bounds (with 128.0 Fixed padding)
                let ddgame = CTask::ddgame_raw(this as *const CTask);
                let padding = Fixed::from_int(128);
                let level_left = (*ddgame).level_bound_min_x - padding;
                let level_right = (*ddgame).level_bound_max_x + padding;

                if (*this).pos_x < level_left {
                    (*this).pos_x = level_right;
                } else if (*this).pos_x > level_right {
                    (*this).pos_x = level_left;
                }

                // Converge wind_accel toward wind_target (clamp step to ±0x147)
                let target = (*this).wind_target.0;
                let current = (*this).wind_accel.0;
                let diff = target - current;
                if diff.abs() < 0x147 {
                    (*this).wind_accel = Fixed(target);
                } else if current < target {
                    (*this).wind_accel = Fixed(current + 0x147);
                } else {
                    (*this).wind_accel = Fixed(current - 0x147);
                }
            }

            Ok(TaskMessage::RenderScene) => {
                let ddgame = CTask::ddgame_raw(this as *const CTask);

                // Only render when rendering phase == 5 (in-game rendering active)
                if (*ddgame).render_phase == 5 {
                    // Sub-frame parallax X offset: interpolate this frame's scroll
                    // using the render_interp_a ratio so clouds scroll smoothly
                    // between the 50Hz simulation steps.
                    let scroll_speed = (*this).vel_x.0 + (*this).wind_accel.0 * 10;
                    let parallax_x =
                        ((scroll_speed as i64 * (*ddgame).render_interp_a as i64) >> 16) as i32;
                    let x = parallax_x + (*this).pos_x.0;

                    let rq = &mut *(*ddgame).render_queue;
                    // Original (0x548527..0x54852f) loads `[ESI+0x44]` (render_y)
                    // into EAX as the usercall Y register, and pushes `anim_phase`
                    // (`[ESI+0x34]`) as the trailing stack arg that becomes
                    // `blit_sprite`'s `palette` parameter.
                    let _ = rq.push_typed(
                        (*this).layer_depth.0 as u32,
                        RenderMessage::Sprite {
                            local: true,
                            x: Fixed(x).floor(),
                            y: (*this).render_y.floor(),
                            sprite: SpriteOp((*this).sprite_id),
                            palette: (*this).anim_phase.0 as u32,
                        },
                    );
                }
            }

            Ok(TaskMessage::SetWind) if !data.is_null() => {
                (*this).wind_target = Fixed(*(data as *const i32));
            }

            _ => {}
        }

        // Broadcast to children — raw-pointer version avoids noalias UB
        CTask::broadcast_message_raw(this as *mut CTask, sender, msg_type, size, data);
    }
}

/// Cloud type determines sprite and vertical velocity.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudType {
    /// Large cloud: sprite 0x268, phase_speed 0x200
    Large = 0,
    /// Medium cloud: sprite 0x269, phase_speed 0x166
    Medium = 1,
    /// Small cloud: sprite 0x26A, phase_speed 0xCC
    Small = 2,
}

impl CTaskCloud {
    /// Initialize CTaskCloud fields on an already-constructed CTask base.
    ///
    /// Pure Rust equivalent of the original constructor at 0x5482E0.
    /// The caller must have already:
    /// 1. Allocated 0x74 bytes (e.g. via `wa_malloc(0x74)`)
    /// 2. Zeroed the first 0x54 bytes (matches original `_memset(ptr, 0, 0x54)`)
    /// 3. Called `CTask::Constructor` (0x5625A0) to set up the base task
    ///
    /// This function then sets the vtable, class type, and all cloud-specific fields.
    ///
    /// # Safety
    /// `this` must point to a valid, allocated CTaskCloud with CTask base initialized.
    pub unsafe fn init(
        this: *mut CTaskCloud,
        cloud_type: CloudType,
        layer_depth: Fixed,
        pos_x: Fixed,
        vel_x: Fixed,
        render_y: Fixed,
    ) {
        unsafe {
            use crate::rebase::rb;

            // Set vtable pointer to the CTaskCloud vtable (rebased for ASLR)
            (*this).base.vtable = rb(CTASK_CLOUD_VTABLE) as *const CTaskCloudVTable;
            (*this).base.class_type = ClassType::Cloud;

            // Position: x is the initial horizontal position. The original computes
            // `anim_phase = (pos_x + render_y) & 0xFFFF`, but both pos_x and render_y
            // have their lower 16 bits zero, so the result is always 0.
            (*this).pos_x = pos_x;
            (*this).anim_phase = Fixed((pos_x.0.wrapping_add(render_y.0)) & 0xFFFF);
            (*this).layer_depth = layer_depth;
            (*this).render_y = render_y;
            (*this).vel_x = vel_x;
            (*this).wind_accel = Fixed(0);
            (*this).wind_target = Fixed(0);

            // Set type-dependent velocity and sprite
            match cloud_type {
                CloudType::Large => {
                    (*this).phase_speed = Fixed(0x200);
                    (*this).sprite_id = 0x268;
                }
                CloudType::Medium => {
                    (*this).phase_speed = Fixed(0x166);
                    (*this).sprite_id = 0x269;
                }
                CloudType::Small => {
                    (*this).phase_speed = Fixed(0xCC);
                    (*this).sprite_id = 0x26A;
                }
            }
        }
    }
}
