//! Pure Rust implementations of GameWorld__InitGameState sub-functions,
//! and the top-level InitGameState itself.
//!
//! Each sub-function is hooked individually so it works regardless of whether
//! InitGameState itself is Rust or the original WA code.

use crate::audio::dssound::DSSound;
use crate::bitgrid::DisplayBitGrid;
use crate::engine::EntityActivityQueue;
use crate::engine::dual_buffer_object::allocate_dual_buffer_object;
use crate::engine::game_info::GameInfo;
use crate::engine::game_state;
use crate::engine::game_state_stream::game_state_stream_init;
use crate::engine::menu_panel::MenuPanel;
use crate::engine::ring_buffer::{allocate_ring_buffer_init, allocate_ring_buffer_raw};
use crate::engine::runtime::GameRuntime;
use crate::engine::team_init::{init_alliance_data, init_team_color_from_names, init_team_scoring};
use crate::engine::world::GameWorld;
use crate::game::weapon::check_weapon_avail;
use crate::render::display::gfx::DisplayGfx;
use crate::render::landscape::{Landscape, init_landscape_borders};
use crate::render::palette::allocate_palette_context;
use crate::wa_call::{call_ctor_with_ecx, call_usercall_esi_stack1};
use openwa_core::fixed::Fixed;
use openwa_core::weapon::WeaponId;

/// Helper: read the GameWorld pointer from a GameRuntime pointer.
#[inline]
unsafe fn world_from_runtime(runtime: *mut GameRuntime) -> *mut GameWorld {
    unsafe { (*runtime).world }
}

/// Bridge to GameWorld__InitFeatureFlags (0x524700): stdcall(runtime), RET 0x4.
unsafe fn wa_init_feature_flags(runtime: *mut GameRuntime) {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut GameRuntime) = core::mem::transmute(
            crate::rebase::rb(crate::address::va::GAME_WORLD_INIT_FEATURE_FLAGS) as usize,
        );
        f(runtime);
    }
}

/// Pure Rust implementation of WorldEntity__InitTurnState (0x528690).
///
/// Convention: usercall(EAX=wrapper), plain RET.
///
/// Initializes turn-related state fields in both the GameRuntime and GameWorld
/// structs. Zeroes camera state, timing fields, per-team flags, and calls
/// GameWorld__InitFeatureFlags. Also dispatches a vtable call on the landscape
/// object.
pub unsafe fn init_turn_state(runtime: *mut GameRuntime) {
    unsafe {
        let world = world_from_runtime(runtime);
        let game_info = (*world).game_info;

        (*runtime)._field_458 = 0xFFFFFFFF;
        (*runtime).message_indicator_timer = Fixed::ZERO;
        (*runtime).message_indicator_anim = Fixed::ZERO;

        // GameWorld+0x72E0/E4 = -1, GameWorld+0x72E8 = 0
        (*world)._unknown_72e0 = 0xFFFFFFFF;
        (*world).render_slot_count = 0xFFFFFFFF;
        (*world)._unknown_72e8 = 0;

        // Zero render entry table first u32 at GameWorld+0x73B0, stride 0x14, while offset < 0x118
        {
            let base = core::ptr::addr_of_mut!((*world).render_entries) as *mut u8;
            let mut off = 0u32;
            while off < 0x118 {
                *(base.add(off as usize) as *mut u32) = 0;
                off += 0x14;
            }
        }

        // More GameWorld field zeroing
        (*world).render_state_flag = 0;
        (*world)._field_72f4 = 0;
        (*world)._field_72f8 = 0;
        (*world)._field_72fc = 0;
        (*world)._field_7300 = 0;
        (*world)._field_7304 = 0;

        // GameWorld.rng_state_1/2 = game_info.rng_seed (RNG seed from scheme)
        let rng_seed = (*game_info).rng_seed;
        (*world).rng_state_1 = rng_seed;
        (*world).rng_state_2 = rng_seed; // duplicate

        (*world)._field_7378 = 0;
        (*world)._field_7374 = 0;
        (*world)._field_737c = 0;
        (*world)._field_77dc = 0;
        (*world)._field_77e0 = 0;
        (*world)._field_7784 = 0;

        // GameWorld._field_7788 = game_info._field_f362 (byte → u32)
        (*world)._field_7788 = (*game_info)._field_f362 as u32;
        (*world)._field_778c = Fixed::ONE;
        (*world)._field_7790 = 0;

        // Camera center: (level_width << 16) / 2, (level_height << 16) / 2
        let level_width = (*world).level_width as i32;
        let level_height = (*world).level_height as i32;
        let cx = (level_width << 16) / 2;
        let cy = (level_height << 16) / 2;
        (*world).viewport_width = cx;
        (*world).viewport_width_2 = cx; // duplicate
        (*world).viewport_height = cy;
        (*world).viewport_height_2 = cy; // duplicate

        (*world)._field_7d84 = 0;
        (*world)._field_7e4c = 0;
        (*world).frame = 0;
        (*world).scaled_frame_accum = Fixed::ZERO;

        // Per-team loop
        let num_teams = (*game_info).num_teams as i32;
        if num_teams > 0 {
            for i in 0..num_teams as usize {
                (*world)._field_7d88[i] = 0;
                (*world)._field_7dbc[i] = 1;
                (*world)._field_7dc9[i] = 1;
                (*world)._field_7dd6[i] = 0;
                (*world).net_peer_ready_flags[i] = 0;
                (*world)._field_7df0[i] = 0;
            }
        }

        (*world)._field_7e03 = 0;
        (*world)._field_7e04 = 0;

        // Call GameWorld__InitFeatureFlags (600-line feature flag init, bridged)
        wa_init_feature_flags(runtime);

        // Post-feature-flag field writes
        (*world)._field_7e41 = 0;
        (*world)._fields_7e50 = [0u32; 5];
        (*world)._field_7e64 = 0;
        (*world).render_skip_gate = 0;
        (*world)._field_7e6c = 0;
        (*world)._fields_7e88 = [0u32; 5];
        (*world).field_7ea0 = 0;
        (*world).field_7ea4 = 0;

        (*world)._field_8148 = 1;

        let world2 = world_from_runtime(runtime);
        (*world2).replay_speed_accum = openwa_core::fixed::Fixed64::ZERO;
        let world3 = world_from_runtime(runtime);
        (*world3).replay_frame_accum = openwa_core::fixed::Fixed64::ZERO;

        // Landscape vtable slot 1: set control flag (donkey_disabled)
        let world4 = world_from_runtime(runtime);
        let landscape = (*world4).landscape;
        if !landscape.is_null() {
            let param = (*(*world4).game_info).donkey_disabled as u32;
            Landscape::set_control_flag_raw(landscape, param);
        }
    }
}

// =============================================================================
// Top-level GameWorld__InitGameState (0x526500) — Rust port
// =============================================================================

/// Pure Rust implementation of GameWorld__InitGameState (0x526500).
///
/// Convention: stdcall(this=GameRuntime*), RET 0x4.
///
/// Called once per game session from the GameRuntime constructor to initialize
/// all game state: sub-objects, display layers, team configuration, weapon tables,
/// turn logic, and the initial state serialization/checksum.
pub unsafe fn init_game_state(runtime: *mut GameRuntime) {
    unsafe {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

        let world = (*runtime).world;

        // ===== Copy replay mode flags from GameInfo =====
        let game_info = (*world).game_info;
        (*runtime).replay_flag_a = (*game_info).invisibility_mode as u8;
        (*runtime).replay_flag_b = ((*game_info).invisibility_mode >> 8) as u8;

        // ===== EntityActivityQueue::Init =====
        {
            let game_version = (*game_info).game_version;
            let capacity: u32 = if game_version >= 0x3C { 0x400 } else { 0x100 };
            EntityActivityQueue::init(&raw mut (*world).entity_activity_queue, capacity);
        }

        // ===== Allocate HudPanel (0x940 bytes) =====
        {
            let mem = wa_malloc_zeroed(0x940);
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                    core::mem::transmute(rb(va::HUD_PANEL_CONSTRUCTOR) as usize);
                ctor(mem)
            };
            (*world).hud_panel = result;
        }

        // ===== Allocate weapon table buffer (0x80D0 bytes) =====
        {
            let mem = wa_malloc_zeroed(0x80D0);
            (*world).weapon_table = mem as *mut crate::game::weapon::WeaponTable;
        }

        // ===== Allocate unknown 0x2C object =====
        {
            let mem = wa_malloc(0x2C);
            if !mem.is_null() {
                *(mem.add(0x08) as *mut u32) = 0;
                *(mem.add(0x0C) as *mut u32) = 0;
                *(mem.add(0x10) as *mut u32) = 0;
                *(mem.add(0x1C) as *mut u32) = 0;
                *(mem.add(0x20) as *mut u32) = 0;
                *(mem.add(0x24) as *mut u32) = 0;
            }
            (*world)._unknown_51c = mem;
        }

        // ===== Allocate RenderQueue (0x12028 bytes) =====
        {
            let mem = wa_malloc_zeroed(0x12028) as *mut u32;
            if !mem.is_null() {
                *mem.add(0x4001) = 0; // mem[0x10004] = 0
                *mem = 0x10000; // mem[0] = 0x10000
            }
            (*world).render_queue = mem as *mut crate::render::queue::RenderQueue;
        }

        // ===== Allocate GameStateStream (0x264 bytes) =====
        {
            let mem = wa_malloc_zeroed(0x264) as *mut u32;
            if !mem.is_null() {
                *mem = rb(0x664194); // GameStateStream vtable
                game_state_stream_init(mem.add(1));
                *mem.add(1) = 0;
                *mem.add(0x90) = world as u32; // GameWorld ptr backref
                *mem.add(0x8B) = 0;
                *mem.add(0x8C) = 0;
            }
            (*world).game_state_stream = mem as *mut u8;
        }

        // ===== Allocate unknown 0x4A74 object =====
        {
            let mem = wa_malloc_zeroed(0x4A74);
            if !mem.is_null() {
                *(mem.add(0x4A50) as *mut u32) = world as u32;
            }
            (*world)._unknown_52c = mem;
        }

        // Get display dimensions
        DisplayGfx::get_dimensions_raw(
            (*world).display,
            core::ptr::addr_of_mut!((*world).level_width_sound) as *mut u32,
            core::ptr::addr_of_mut!((*world).screen_height_pixels) as *mut u32,
        );
        (*world).viewport_pixel_height = 0;

        // ===== Allocate DualBufferObject (main_buffer at wrapper+0x0C) =====
        (*runtime).main_buffer = allocate_dual_buffer_object(world, game_info);

        // ===== Allocate RingBuffer A (wrapper+0x3C, capacity 0x2000) =====
        (*runtime).ring_buffer_a = allocate_ring_buffer_raw(0x3C, 0x2000);

        // ===== Allocate RingBuffer B (wrapper+0x28, capacity 0x2000) =====
        (*runtime).ring_buffer_b = allocate_ring_buffer_raw(0x3C, 0x2000);

        // ===== Allocate render buffer A (wrapper+0x40, capacity 0x10000) =====
        (*runtime).render_buffer_a = allocate_ring_buffer_raw(0x48, 0x10000);

        // ===== Allocate render buffer B (wrapper+0x14, capacity 0x10000) =====
        (*runtime).render_buffer_b = allocate_ring_buffer_raw(0x48, 0x10000);

        // ===== wrapper+0x44: network ring buffer (conditional) =====
        (*runtime).network_ring_buffer = core::ptr::null_mut();
        if !(*world).net_session.is_null() {
            (*runtime).network_ring_buffer = allocate_ring_buffer_init();
        }

        // ===== Allocate state buffer (wrapper+0x48) =====
        (*runtime).state_buffer = allocate_dual_buffer_object(world, game_info);

        // ===== Allocate statistics object (0xB94 bytes, wrapper+0x4C) =====
        {
            let mem = wa_malloc(0xB94);
            if !mem.is_null() {
                *(mem.add(0xB54) as *mut u32) = 0;
                *(mem.add(0xB58) as *mut u32) = 0;
                *(mem.add(0xB64) as *mut u32) = 0;
                *(mem.add(0xB68) as *mut u32) = 0;
                *(mem.add(0xB74) as *mut u32) = 0;
                *(mem.add(0xB78) as *mut u32) = 0;
                *(mem.add(0xB84) as *mut u32) = 0;
                *(mem.add(0xB88) as *mut u32) = 0;
            }
            (*runtime).statistics = mem;
        }

        // ===== Allocate ring buffer C (wrapper+0x50, capacity 0x1000) =====
        (*runtime).ring_buffer_c = allocate_ring_buffer_raw(0x3C, 0x1000);

        // ===== Zero out scalar fields =====
        (*runtime)._field_494 = 0;
        (*runtime)._field_498 = 0;
        (*runtime)._field_493 = 0;

        // Zero team task pointers (13 entries)
        (*runtime).team_task_ptrs = [core::ptr::null_mut(); 13];

        // ===== Allocate 3 PaletteContexts (0x72C bytes each) =====
        (*runtime).palette_ctx_a = allocate_palette_context();
        (*runtime).palette_ctx_b = allocate_palette_context();
        (*runtime).palette_ctx_c = allocate_palette_context();

        // ===== Select vector normalize function based on game version =====
        {
            let game_version = (*game_info).game_version;
            (*world).vector_normalize_fn = if game_version < 0x99 {
                rb(va::VECTOR_NORMALIZE_SIMPLE)
            } else {
                rb(va::VECTOR_NORMALIZE_OVERFLOW)
            };
        }

        // ===== Initialize sentinel fields =====
        (*runtime)._field_468 = -1;
        (*runtime)._field_46c = -1;
        (*runtime)._field_470 = -1;
        (*runtime).game_state = game_state::RUNNING;
        (*runtime).game_end_speed = Fixed::ZERO;
        (*runtime)._field_264 = 0;
        (*runtime).sync_checksum_a = 0;
        (*runtime).checksum_valid = 0;
        (*runtime).connection_issue_threshold = 0;
        (*runtime).connection_issue_anim = Fixed::ZERO;
        (*runtime).chat_box_anim = Fixed::ZERO;
        (*runtime)._field_3f4 = -1;
        (*runtime)._field_3f8 = -1;

        // ===== Resolution-dependent team render indices =====
        {
            let screen_height = (*world).screen_height_pixels;
            if screen_height < 0x2D0 {
                // < 720px: smaller layout
                (*runtime).max_team_render_index = 0xC;
                (*runtime).team_render_indices = [8, 7, 5, 2, 4, 3, 1, 6];
            } else {
                // >= 720px: larger layout
                (*runtime).max_team_render_index = 0x10;
                (*runtime).team_render_indices = [0x10, 0xF, 0xD, 10, 0xC, 0xB, 9, 0xE];
            }

            (*runtime).team_count_config = if screen_height < 600 { 7 } else { 10 };
        }

        // ===== Game mode and sentinel arrays =====
        (*runtime).game_mode_flag = 1;

        // Fill team slot mapping with -1 sentinels
        (*runtime).team_to_slot_a = [-1i32; 8];
        (*runtime).slot_to_team = [-1i32; 16];
        (*runtime)._field_3ec = -1;

        // ===== Team-to-slot mapping (conditional on network/replay mode) =====
        if !(*world).net_session.is_null() || (*runtime).replay_flag_a != 0 {
            let team_count = (*game_info).num_teams as i32;
            let exclude_team = (*game_info).starting_team_index as i32;
            let mut slot_idx = 0usize;
            // slot_to_team[0..12] overlaps the reverse mapping area (0x3BC = slot_to_team[4])
            let pivar7_base = core::ptr::addr_of_mut!((*runtime).slot_to_team[4]) as *mut i32;
            for team_id in 0..team_count {
                if team_id != exclude_team {
                    *pivar7_base.add(team_id as usize) = slot_idx as i32;
                    (*runtime).team_to_slot_a[slot_idx] = team_id;
                    slot_idx += 1;
                } else {
                    *pivar7_base.add(team_id as usize) = -1;
                }
            }
        }

        // ===== Game logic initialization =====
        (*runtime).game_end_phase = 0;
        (*runtime).init_flag = 1;

        // Already-ported sub-functions
        init_team_scoring(runtime);
        init_alliance_data(runtime);

        // ===== Game speed/timing fields =====
        (*runtime).net_end_countdown = 500;
        (*runtime).timing_jitter_state = 0;
        (*runtime).render_scale_fade_request = 0;
        (*runtime)._field_460 = 0;
        (*runtime)._field_478 = 0;
        (*runtime).ui_volume = Fixed::ZERO;

        // Initial sound volume: percent (0..100) → Fixed (0..1.0).
        let pct = (*game_info).sound_volume_percent;
        (*runtime).sound_volume = Fixed::from_raw((pct << 16) / 100);

        // ===== Display setup (headful only) =====
        let is_headful = (*world).is_headful != 0;

        if is_headful {
            init_game_state_display(runtime, world, game_info);
        }

        // ===== Worm selection count and terrain config =====
        {
            let mut val = (*game_info).worm_select_cfg_a;
            let min_teams = (*runtime).min_active_teams;
            let team_count_cfg = (*runtime).team_count_config;

            if val == 0 {
                val = team_count_cfg;
            } else if val < min_teams {
                val = min_teams;
            } else if val > 0x20 {
                val = 0x20;
            }
            (*runtime).worm_select_count = val;

            val = (*game_info).worm_select_cfg_b;
            if val == -1 {
                val = 7;
            } else if val < min_teams {
                val = min_teams;
            } else if val > 0x20 {
                val = 0x20;
            }
            (*runtime).worm_select_count_alt = val;

            (*runtime).hud_team_bar_extended = ((*game_info)._field_f365 != 0) as u32;

            if is_headful {
                // GameWorld+0x000 = keyboard ptr; keyboard+0x10 = config field
                let keyboard = (*world).keyboard as *mut u8;
                *(keyboard.add(0x10) as *mut u32) = ((*game_info)._field_f370 != 0) as u32;
            }
        }

        // Ensure worm_select_count >= worm_select_count_alt
        if (*runtime).worm_select_count < (*runtime).worm_select_count_alt {
            (*runtime).worm_select_count = (*runtime).worm_select_count_alt;
        }

        // ===== Zero state fields =====
        (*runtime).esc_menu_state = 0;
        (*runtime)._field_438 = -1;
        (*runtime)._field_43c = -1;
        (*runtime).menu_panel_width = 0;
        (*runtime).menu_panel_height = 0;
        (*runtime).esc_menu_anim = Fixed::ZERO;
        (*runtime).esc_menu_anim_target = Fixed::ZERO;
        (*runtime).confirm_panel_width = 0;
        (*runtime).confirm_panel_height = 0;
        (*runtime).confirm_anim = Fixed::ZERO;
        (*runtime).confirm_anim_target = Fixed::ZERO;
        (*runtime).chat_box_open = 0;
        (*runtime)._field_410 = 0;

        // ===== Landscape borders (cavern flag) =====
        init_landscape_borders(runtime);

        // ===== Level bounds and camera initialization =====
        init_game_state_level_bounds(world, game_info);

        // ===== Display objects for HUD (headful only) =====
        if is_headful {
            init_game_state_hud_objects(world, game_info);
        }

        // ===== Team name validation =====
        init_team_color_from_names(world, game_info);

        // ===== Weapon and team initialization =====
        (*world).hud_status_code = 0;
        (*world).hud_status_text = core::ptr::null();

        // InitWeaponTable
        {
            let f: unsafe extern "stdcall" fn(*mut GameRuntime) =
                core::mem::transmute(rb(va::INIT_WEAPON_TABLE) as usize);
            f(runtime);
        }

        // GameWorld__InitTeamsFromSetup
        {
            let team_arena = core::ptr::addr_of_mut!((*world).team_arena) as u32;
            let gi_ptr = (*world).game_info as u32;
            let f: unsafe extern "stdcall" fn(u32, u32) =
                core::mem::transmute(rb(va::INIT_TEAMS_FROM_SETUP) as usize);
            f(team_arena, gi_ptr);
        }

        // ===== Version-dependent stack/render config =====
        init_game_state_version_config(world, game_info);

        // ===== Water level and wind =====
        init_game_state_water_level(world, game_info);

        // ===== Random bag initialization =====
        init_game_state_random_bags(world, game_info);

        // ===== Turn state =====
        init_turn_state(runtime);

        // ===== Weapon availability loop =====
        init_game_state_weapon_avail(world, game_info);

        // ===== Team/object tracking arrays =====
        init_game_state_tracking_arrays(world, game_info);

        // ===== Statistics counters =====
        {
            let base = core::ptr::addr_of_mut!((*world)._unknown_9890) as *mut u32;
            for i in 0..10usize {
                *base.add(i) = 0;
            }
        }

        // ===== TeamManager constructor =====
        {
            let mem = wa_malloc_zeroed(0x6C);
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                let ctor: unsafe extern "stdcall" fn(*mut u8, u32) -> *mut u8 =
                    core::mem::transmute(rb(va::TEAM_MANAGER_CONSTRUCTOR) as usize);
                ctor(mem, world as u32)
            };
            (*world).turn_order_widget = result as *mut _;
        }

        // ===== Network sync callback =====
        let sound = (*world).sound;
        if !sound.is_null() {
            DSSound::update_channels_raw(sound);
        }

        // ===== WorldRootEntity constructor =====
        {
            let game_version = (*game_info).game_version;
            let gi_ptr = (*world).game_info as u32;
            if game_version == -2 {
                // Online game: ECX = *net_bridge (dereferenced!) for the constructor
                let mem = wa_malloc_zeroed(0x324);
                if mem.is_null() {
                    (*runtime).world_root = core::ptr::null_mut::<crate::task::WorldRootEntity>();
                } else {
                    let net_bridge = (*runtime).net_bridge;
                    let ecx_val = *(net_bridge as *const u32); // deref net_bridge to get ECX
                    call_ctor_with_ecx(mem, gi_ptr, ecx_val, rb(va::WORLD_ROOT_ENTITY_CTOR));
                    *(mem.add(0x300) as *mut u32) = net_bridge as u32;
                    // Override vtables for online mode
                    *(mem as *mut u32) = rb(0x669C28);
                    *(mem.add(0x30) as *mut u32) = rb(0x669C44);
                    (*runtime).world_root = mem as *mut crate::task::WorldRootEntity;
                }
            } else {
                // Normal game: ECX must = GameWorld for the constructor
                let mem = wa_malloc_zeroed(0x320);
                let result = if mem.is_null() {
                    core::ptr::null_mut()
                } else {
                    call_ctor_with_ecx(mem, gi_ptr, world as u32, rb(va::WORLD_ROOT_ENTITY_CTOR))
                };
                (*runtime).world_root = result as *mut crate::task::WorldRootEntity;
            }
        }

        // ===== Zero frame timing state (0x98-0xD4) =====
        (*runtime).timing_ref = 0;
        (*runtime).last_frame_time = 0;
        (*runtime).frame_accum_a = 0;
        (*runtime).frame_accum_b = 0;
        (*runtime).frame_accum_c = 0;
        (*runtime).initial_ref = 0;
        (*runtime).pause_detect = 0;

        // ===== Replay/network mode flag =====
        {
            let replay_a = (*runtime).replay_flag_a;
            let result = if replay_a == 0
                || ((*game_info).replay_config_flag != 0 && (*runtime)._field_49c > 0xC)
            {
                0u32
            } else {
                0xFFFFFFFFu32
            };
            (*world).recorded_frame_counter = result as i32;
        }

        (*runtime)._field_4b0 = 0xFFFFFFFF;
        (*runtime)._field_4ac = 0;
        (*runtime)._field_0e0 = 0;

        // 0xEC: (game_info.f340 != 0) - 1 (i.e., 0 if nonzero, 0xFFFFFFFF if zero)
        (*runtime).frame_delay_counter = (((*game_info)._field_f340 != 0) as i32).wrapping_sub(1);

        (*game_info)._field_f34c = -1;
        (*runtime).game_state = game_state::INITIALIZED;

        // ===== Serialize initial game state =====
        {
            let serialize: unsafe extern "stdcall" fn(*mut GameRuntime, *mut u8) =
                core::mem::transmute(rb(va::SERIALIZE_GAME_STATE) as usize);
            serialize(runtime, (*runtime).state_buffer);
        }

        // ===== Copy game state to statistics buffer =====
        {
            let src = core::ptr::addr_of!((*world)._unknown_8168) as *const u32;
            let dst = (*runtime).statistics as *mut u32;
            core::ptr::copy_nonoverlapping(src, dst, 0x2E5);
        }

        // ===== Compute initial checksum =====
        {
            let buf_ptr = *((*runtime).state_buffer as *const *const u8);
            let buf_len = *((*runtime).state_buffer.add(0x0C) as *const u32); // field[3]
            let mut hash = 0u32;
            for i in 0..buf_len as usize {
                hash = hash.rotate_left(3).wrapping_add(*buf_ptr.add(i) as u32);
            }

            // Get frame counter via landscape vtable
            let landscape = (*world).landscape;
            let frame = Landscape::get_frame_checksum_raw(landscape);

            (*runtime).initial_checksum = frame.wrapping_add(hash);
        }

        // ===== Weapon panel (headful only) =====
        if is_headful {
            let mem = wa_malloc_zeroed(0x208);
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                // usercall: ESI=this(mem), stack=(GameWorld), RET 0x4
                call_usercall_esi_stack1(mem, world as u32, rb(va::INIT_WEAPON_PANEL))
            };
            (*world).weapon_panel = result;
        }
    }
}

// =============================================================================
// Helper functions for init_game_state
// =============================================================================

/// Display setup phase of InitGameState (headful only).
///
/// Creates DisplayGfx layers, camera objects, and the main display surface.
unsafe fn init_game_state_display(
    runtime: *mut GameRuntime,
    world: *mut GameWorld,
    game_info: *const GameInfo,
) {
    unsafe {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::wa_malloc_zeroed;

        let max_team_render = (*runtime).max_team_render_index;

        let mut display_width: u32 = 0;
        let mut display_height: u32 = 0;
        DisplayGfx::get_dimensions_raw((*world).display, &mut display_width, &mut display_height);

        // Screen height for HUD: 0x12C (300) if wide layout, 0x8C (140) if narrow
        let screen_height: i32 = if max_team_render != 0xC {
            0xA0 + 0x8C // 300
        } else {
            0x8C // 140
        };
        (*runtime).screen_height_hud = screen_height;

        // Count active teams from slot_to_team array
        let team_count = (*game_info).num_teams as u32;
        let count_active = |runtime: *mut GameRuntime, base: u32| -> u32 {
            let mut n = base;
            if (*runtime).slot_to_team[0] >= 0 {
                let mut idx = 1;
                loop {
                    n += 1;
                    if (*runtime).slot_to_team[idx] < 0 {
                        break;
                    }
                    idx += 1;
                }
            }
            n
        };
        let active_count = count_active(runtime, team_count);

        // min_teams: (int)(active_count - 1) < 2 ? 1 : active_count - 1
        let min_teams: i32 = if (active_count.wrapping_sub(1) as i32) < 2 {
            1
        } else {
            count_active(runtime, team_count) as i32 - 1
        };
        (*runtime).min_active_teams = min_teams;

        // screen_offset = display_width - screen_height
        (*runtime).screen_offset = display_width as i32 - screen_height;
        let screen_offset = display_width as i32 - screen_height;

        // ===== Layer 1 (wrapper+0x18): BitGrid(8, max_team*64+2, screen_offset-7) =====
        {
            let height = max_team_render * 64 + 2;
            let width = screen_offset - 7;
            (*runtime).display_gfx_a = create_display_gfx_layer_sized(height as u32, width as u32);
        }

        // ===== Layer 2 (wrapper+0x1C): BitGrid(8, max_team*33+6, display_width) =====
        {
            let height = max_team_render * 33 + 6;
            (*runtime).display_gfx_b = create_display_gfx_layer_sized(height as u32, display_width);
        }

        // ===== Layer 3 (wrapper+0x20): BitGrid(8, (min_teams+1)*max_team+1, screen_height) =====
        {
            let height = (min_teams + 1) * max_team_render + 1;
            (*runtime).display_gfx_c =
                create_display_gfx_layer_sized(height as u32, screen_height as u32);
        }

        // ===== ConstructFull for main display (wrapper+0x24) =====
        // usercall: ECX=gfx_color_table[7], EDX=max_team_render_index
        // stdcall stack: (this, display, team_idx, screen_offset-3, render_param)
        {
            let mem = wa_malloc_zeroed(0x468);
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                {
                    let p_display = (*world).display as u32;
                    let p_team = (*runtime).team_render_indices[0] as u32;
                    let p_offset = screen_offset - 3;
                    let p_render = (*world).gfx_color_table[8]; // [8] = 0x732C
                    let p_ecx = (*world).gfx_color_table[7]; // [7] = 0x7328
                    let p_edx = max_team_render as u32;
                    let target = rb(va::DISPLAYGFX_CONSTRUCT_FULL);
                    let f: unsafe extern "fastcall" fn(
                        u32,
                        u32,
                        *mut u8,
                        u32,
                        u32,
                        i32,
                        u32,
                    ) -> *mut u8 = core::mem::transmute(target as usize);
                    f(p_ecx, p_edx, mem, p_display, p_team, p_offset, p_render)
                }
            };
            (*runtime).display_gfx_main = result;
        }

        // Turn timer
        {
            let timer_val = max_team_render << 5;
            (*runtime).turn_timer_max = timer_val;
            (*runtime).turn_timer_current = timer_val;
            (*runtime)._field_404 = 0;
        }

        // Dirty rect / clipping calls on layers A, B, C
        fill_display_layer((*runtime).display_gfx_a as *mut DisplayBitGrid, world);
        fill_display_layer((*runtime).display_gfx_b as *mut DisplayBitGrid, world);
        fill_display_layer((*runtime).display_gfx_c as *mut DisplayBitGrid, world);

        // ===== Layer 4 (wrapper+0x2C): BitGrid(8, 0x100, 0x154) — constant =====
        (*runtime).display_gfx_d =
            create_display_gfx_layer_sized(0x100, 0x154) as *mut DisplayBitGrid;

        // ===== Layer 5 (wrapper+0x34): BitGrid(8, 0x30, 0xC0) — constant =====
        (*runtime).display_gfx_e =
            create_display_gfx_layer_sized(0x30, 0xC0) as *mut DisplayBitGrid;

        // Create 2 menu/viewport widgets (wrapper+0x30, +0x38), each paired
        // with the BitGrid layer immediately preceding it.
        (*runtime).menu_panel_a = create_menu_panel(runtime, world, 0x2C);
        (*runtime).menu_panel_b = create_menu_panel(runtime, world, 0x34);
    }
}

/// Create a DisplayGfx layer (0x4C bytes): malloc + memset + BitGrid__Init + vtable.
/// Each layer has specific height/width for its BitGrid.
unsafe fn create_display_gfx_layer_sized(height: u32, width: u32) -> *mut u8 {
    unsafe {
        use crate::bitgrid::{BIT_GRID_DISPLAY_VTABLE, BitGrid};
        use crate::rebase::rb;
        use crate::wa_alloc::wa_malloc_zeroed;

        let mem = wa_malloc_zeroed(0x4C) as *mut u32;
        if mem.is_null() {
            return core::ptr::null_mut();
        }
        // BitGrid::init at the start of the buffer (this IS a BitGrid, not a wrapper)
        BitGrid::init(mem as *mut BitGrid, 8, width, height);
        // Override base vtable (0x6640EC) with DisplayBitGrid vtable (0x664144)
        *mem = rb(BIT_GRID_DISPLAY_VTABLE);
        mem as *mut u8
    }
}

/// Fill a DisplayBitGrid layer with its background color, respecting clip bounds.
unsafe fn fill_display_layer(gfx: *mut DisplayBitGrid, world: *mut GameWorld) {
    unsafe {
        if gfx.is_null() {
            return;
        }
        let width = (*gfx).width as i32;
        let height = (*gfx).height as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let clip_right = (*gfx).clip_right as i32;
        let clip_bottom = (*gfx).clip_bottom as i32;
        if clip_right <= 0 || clip_bottom <= 0 {
            return;
        }
        let clip_left = (*gfx).clip_left as i32;
        let clip_top = (*gfx).clip_top as i32;
        if clip_left >= width || clip_top >= height {
            return;
        }

        let x1 = clip_left.max(0);
        let y1 = clip_top.max(0);
        let x2 = clip_right.min(width);
        let y2 = clip_bottom.min(height);

        let color = (*world).gfx_color_table[7] as u8;
        DisplayBitGrid::fill_rect_raw(gfx, x1, y1, x2, y2, color);
    }
}

/// Create a [`MenuPanel`] (0x3D4 bytes) bound to the BitGrid layer at
/// `runtime + display_gfx_offset`.
unsafe fn create_menu_panel(
    runtime: *mut GameRuntime,
    world: *mut GameWorld,
    display_gfx_offset: usize,
) -> *mut MenuPanel {
    unsafe {
        use crate::engine::menu_panel::MenuPanel;
        use crate::wa_alloc::wa_malloc_zeroed;

        let panel = wa_malloc_zeroed(0x3D4) as *mut MenuPanel;
        if panel.is_null() {
            return core::ptr::null_mut();
        }

        let display_bitgrid =
            *((runtime as *const u8).add(display_gfx_offset) as *const *mut DisplayBitGrid);

        (*panel).display_a = display_bitgrid;
        (*panel).display_b = (*world).display;
        (*panel).color_low = (*world).gfx_color_table[7] as i32;
        (*panel).color_high = (*world).gfx_color_table[0] as i32;

        let w = (*display_bitgrid).width as i32;
        let h = (*display_bitgrid).height as i32;

        (*panel).cursor_x = w / 2;
        (*panel).cursor_y = h / 2;
        (*panel).clip_right = w;
        (*panel).clip_bottom = h;

        panel
    }
}

/// Initialize level bounds and camera center positions.
unsafe fn init_game_state_level_bounds(world: *mut GameWorld, game_info: *const GameInfo) {
    unsafe {
        (*world).shake_intensity_x = Fixed::ZERO;
        (*world).shake_intensity_y = Fixed::ZERO;

        let is_cavern = (*world).is_cavern as i32;
        if is_cavern == 0 {
            // Open-air level
            (*world).level_bound_min_x = Fixed(0xF8020000u32 as i32);
            (*world).level_bound_min_y = Fixed(0xF8020000u32 as i32);
            let level_width = (*world).level_width as i32;
            (*world).level_bound_max_x = Fixed((level_width + 0x7FE) * 0x10000);
        } else {
            // Cavern level
            let game_version = (*game_info).game_version;
            let bound = if game_version >= -1 { 0x20000i32 } else { 0i32 };
            (*world).level_bound_min_x = Fixed(bound);
            (*world).level_bound_min_y = Fixed(0x20000);
            let level_width = (*world).level_width;
            (*world).level_bound_max_x =
                Fixed((level_width.wrapping_mul(0x10000)).wrapping_sub(bound as u32) as i32);
        }

        // Camera center initialization (4 viewports)
        // Each iteration writes to viewport_coords[i+1] (entries 1..5).
        let level_w = (*world).level_width as i32;
        let level_h = (*world).level_height as i32;
        let cx = Fixed((level_w << 16) / 2);
        let cy = Fixed((level_h << 16) / 2);
        for i in 0..4 {
            let entry = &mut (*world).viewport_coords[i + 1];
            entry.center_x = cx;
            entry.center_y = cy;
            entry.center_x_target = cx;
            entry.center_y_target = cy;
        }

        // Map dimension fields
        (*world).map_boundary_width = 0x30D4;
        (*world).map_boundary_height = (*world).level_height;

        let game_version = (*game_info).game_version;
        if game_version > 0x32 {
            let level_w = (*world).level_width as i32;
            (*world).map_boundary_width = (level_w + 0x2954) as u32;
            (*world).map_boundary_height = 0x2B8;
        }
    }
}

/// Create HUD display objects (headful only, conditional on is_headful).
unsafe fn init_game_state_hud_objects(world: *mut GameWorld, _game_info: *const GameInfo) {
    unsafe {
        use crate::address::va;
        use crate::rebase::rb;
        use crate::wa_alloc::wa_malloc_zeroed;

        // DisplayObject for HUD background
        // usercall: ECX=gfx_color_table[6], EDX=gfx_color_table[7], stdcall stack=(this, GameWorld+0x33C)
        {
            let mem = wa_malloc_zeroed(0x58) as *mut u32;
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                let ctor: unsafe extern "fastcall" fn(u32, u32, *mut u32, u32) -> *mut u8 =
                    core::mem::transmute(rb(va::DISPLAY_OBJECT_CONSTRUCTOR) as usize);
                ctor(
                    (*world).gfx_color_table[6], // ECX
                    (*world).gfx_color_table[7], // EDX
                    mem,
                    core::ptr::addr_of!((*world).sprite_cache[128]) as u32,
                )
            };
            (*world)._unknown_540 = result;
        }

        // DisplayGfx textbox for HUD text
        // thiscall: ECX=GameWorld.display, stack=(this, 0x13, 2)
        {
            let mem = wa_malloc_zeroed(0x158) as *mut u32;
            let result = if mem.is_null() {
                core::ptr::null_mut()
            } else {
                let ctor: unsafe extern "thiscall" fn(
                    *mut DisplayGfx,
                    *mut u32,
                    u32,
                    u32,
                ) -> *mut u8 = core::mem::transmute(rb(va::CONSTRUCT_TEXTBOX) as usize);
                ctor((*world).display, mem, 0x13, 2)
            };
            (*world).textbox = result;
        }
    }
}

/// Version-dependent configuration (stack size, rendering constants).
unsafe fn init_game_state_version_config(world: *mut GameWorld, game_info: *const GameInfo) {
    unsafe {
        let game_version = (*game_info).game_version;

        // Stack height: game_version < -1 → 0x700, >= -1 → 0x800
        (*world).stack_height = if game_version < -1 { 0x700 } else { 0x800 };

        // Copy RNG seed
        let rng_seed = (*game_info).rng_seed;
        (*world).game_rng = rng_seed;
        (*world).team_health_ratio[0] = rng_seed as i32;

        // Zero various fields
        (*world).frame_counter = 0;
        (*world)._field_5d4 = 0;
        (*world)._field_5d8 = 0;
        (*world)._field_5dc = 0;
        (*world)._field_7e4c = 0;
        (*world)._field_5d0 = 0;

        // Zero counter arrays (6 entries each × 3 blocks starting at turn_time_limit)
        {
            let base = core::ptr::addr_of_mut!((*world).turn_time_limit) as *mut u32;
            for i in 0..18usize {
                *base.add(i) = 0;
            }
        }

        // Zero more fields
        (*world)._field_45e0 = 0;
        (*world)._field_45e4 = 0;
        (*world)._field_45e8 = 0;
    }
}

/// Water level and boundary calculations.
unsafe fn init_game_state_water_level(world: *mut GameWorld, game_info: *mut GameInfo) {
    unsafe {
        // Water level: (100 - level_height_raw) * level_height / 100
        // level_height_raw appears to encode a water height percentage here
        let height_raw = (*world).level_height_raw as i32;
        let level_height = (*world).level_height as i32;
        let water_level = ((100 - height_raw) * level_height) / 100;
        (*world).water_level = water_level;

        // Clamp to 0 for newer versions
        let game_version = (*game_info).game_version;
        if water_level < 0 && game_version > 0x179 {
            (*world).water_level = 0;
        }

        // Max Y boundary
        let water_val = (*world).water_level;
        let min_y = (*world).level_bound_min_y.0;
        let max_y_candidate = (water_val + 0xA0) * 0x10000;
        let min_y_plus = min_y + 0x300_0000;
        let max_y = if max_y_candidate > min_y_plus {
            max_y_candidate
        } else {
            min_y_plus
        };
        (*world).level_bound_max_y = Fixed(max_y);

        // Clamp for newer versions
        if game_version > 0x11E && (max_y >> 16) + 0x28 > 0x7FFF {
            (*world).level_bound_max_y = Fixed(0x7FD70000u32 as i32);
        }

        // Derived water fields
        (*world).water_kill_y = (*world).level_bound_max_y.to_int() + 0x28;
        (*world)._field_5f8 = 0;
        (*world).water_level_initial = (*world).water_level;
        (*world)._field_5ec = 0;

        // Terrain type percentages
        (*world).terrain_pct_a = (*game_info).terrain_cfg_a as u32;
        (*world).terrain_pct_b = (*game_info).terrain_cfg_b as u32;
        (*world).terrain_pct_c = (*game_info).terrain_cfg_c as u32;

        // Game state flags
        (*world)._field_5f0 = 1;
        (*world)._field_5f4 = 100;
        (*world)._field_5fc = 0;
    }
}

/// Random bag initialization (land, mine, barrel, weapon).
unsafe fn init_game_state_random_bags(world: *mut GameWorld, game_info: *mut GameInfo) {
    unsafe {
        // TODO: Get rid of this
        let world_raw = world as *mut u8;
        // Random bag at GameWorld+0x360C (5 zeroes + 1 one)
        {
            let write_idx_ptr = world_raw.add(0x379C) as *mut i32;
            let mut write_idx = *write_idx_ptr;
            if write_idx + 5 < 0x65 {
                for _ in 0..5 {
                    *(world_raw.add(0x360C) as *mut u32).add(write_idx as usize) = 0;
                    write_idx += 1;
                }
                *write_idx_ptr = write_idx;
            }
            if write_idx + 1 < 0x65 {
                *(world_raw.add(0x360C) as *mut u32).add(write_idx as usize) = 1;
                *write_idx_ptr = write_idx + 1;
            }
        }

        // Terrain type percentages validation and clamping
        {
            let pct_land = (*game_info).drop_pct_land as i32;
            let pct_mine = (*game_info).drop_pct_mine as i32;
            let pct_barrel = (*game_info).drop_pct_barrel as i32;

            let mut remaining = 100 - pct_land;
            if remaining < 0 {
                // Land > 100%, clamp all
                (*game_info).drop_pct_land = 100;
                (*game_info).drop_pct_mine = 0;
                (*game_info).drop_pct_barrel = 0;
                remaining = 0;
            } else {
                remaining -= pct_mine;
                if remaining < 0 {
                    (*game_info).drop_pct_mine = (pct_mine + remaining) as u8;
                    (*game_info).drop_pct_barrel = 0;
                    remaining = 0;
                } else {
                    remaining -= pct_barrel;
                    if remaining < 0 {
                        (*game_info).drop_pct_barrel = (pct_barrel + remaining) as u8;
                    }
                }
            }

            // Extended terrain type from game_info
            let ext_type = (*game_info).ext_terrain_type;
            let ext_pct = (*game_info).ext_terrain_pct as i32;
            if ext_type == 0 {
                (*game_info).ext_terrain_pct = remaining as u8;
            } else if remaining < ext_pct {
                (*game_info).ext_terrain_pct = 0;
            }
        }

        // Fill terrain type random bags at GameWorld+0x3F84
        {
            let write_idx_ptr = world_raw.add(0x4114) as *mut i32;

            // Land entries (type 1)
            let count = (*game_info).drop_pct_land as u32;
            let mut idx = *write_idx_ptr;
            if (idx as u32 + count) < 0x65 {
                for _ in 0..count {
                    *(world_raw.add(0x3F84) as *mut u32).add(idx as usize) = 1;
                    idx += 1;
                }
                *write_idx_ptr = idx;
            }

            // Mine entries (type 2)
            let count = (*game_info).drop_pct_mine as u32;
            idx = *write_idx_ptr;
            if (idx as u32 + count) < 0x65 {
                for _ in 0..count {
                    *(world_raw.add(0x3F84) as *mut u32).add(idx as usize) = 2;
                    idx += 1;
                }
                *write_idx_ptr = idx;
            }

            // Barrel entries (type 4)
            let count = (*game_info).drop_pct_barrel as u32;
            idx = *write_idx_ptr;
            if (idx as u32 + count) < 0x65 {
                for _ in 0..count {
                    *(world_raw.add(0x3F84) as *mut u32).add(idx as usize) = 4;
                    idx += 1;
                }
                *write_idx_ptr = idx;
            }

            // Extended entries (type 0)
            let count = (*game_info).ext_terrain_pct as u32;
            idx = *write_idx_ptr;
            if (idx as u32 + count) < 0x65 {
                for _ in 0..count {
                    *(world_raw.add(0x3F84) as *mut u32).add(idx as usize) = 0;
                    idx += 1;
                }
                *write_idx_ptr = idx;
            }
        }

        // Random bag at GameWorld+0x3934 (entries 0, 1)
        {
            let write_idx_ptr = world_raw.add(0x3AC4) as *mut i32;
            let mut idx = *write_idx_ptr;
            if idx + 1 < 0x65 {
                *(world_raw.add(0x3934) as *mut u32).add(idx as usize) = 0;
                idx += 1;
                *write_idx_ptr = idx;
            }
            if idx + 1 < 0x65 {
                *(world_raw.add(0x3934) as *mut u32).add(idx as usize) = 1;
                *write_idx_ptr = idx + 1;
            }
        }

        // Random bag at GameWorld+0x42AC (entries 0..3)
        {
            let write_idx_ptr = world_raw.add(0x443C) as *mut i32;
            for val in 0..4i32 {
                let idx = *write_idx_ptr;
                if idx + 1 < 0x65 {
                    *(world_raw.add(0x42AC) as *mut i32).add(idx as usize) = val;
                    *write_idx_ptr = idx + 1;
                }
            }
        }
    }
}

/// Weapon availability loop using already-ported check_weapon_avail.
unsafe fn init_game_state_weapon_avail(world: *mut GameWorld, game_info: *const GameInfo) {
    unsafe {
        let world_raw = world as *mut u8;
        let game_version = (*game_info).game_version;

        for weapon_id in WeaponId::iter_known() {
            let avail = check_weapon_avail(world, weapon_id);
            let is_avail = if game_version < 0x4D {
                avail != 0
            } else {
                avail > 0
            };

            if is_avail {
                let write_idx_ptr = world_raw.add(0x3DEC) as *mut i32;
                let idx = *write_idx_ptr;
                if idx + 1 < 0x65 {
                    *(world_raw.add(0x3C5C) as *mut u32).add(idx as usize) = weapon_id.0;
                    *write_idx_ptr = idx + 1;
                }
            }
        }
    }
}

/// Allocate team/object tracking arrays and initialize the 0x51C object.
unsafe fn init_game_state_tracking_arrays(world: *mut GameWorld, game_info: *const GameInfo) {
    unsafe {
        use crate::wa_alloc::wa_malloc;

        // Game speed
        (*world).game_speed = Fixed::ONE;

        let is_headful = (*world).is_headful != 0;
        if !is_headful {
            (*world).game_speed_target = Fixed(0x10000000);
        } else {
            (*world).game_speed_target = Fixed((*game_info).game_speed_config);
        }

        // Network speed callback
        let sound = (*world).sound;
        if !sound.is_null() {
            DSSound::set_frequency_scale_raw(
                sound,
                (*world).game_speed.0 as u32,
                (*world).game_speed_target.0,
            );
        }

        // Misc fields
        (*world).sound_queue_count = 0;
        (*world).render_phase = (*game_info).render_phase_cfg as i32;
        (*world)._field_7640 = 0;
        (*world)._field_7644 = (*game_info)._field_f363 as u32;
        (*world)._field_7648 = (*game_info)._field_f364 as u32;

        // Render state
        (*world)._field_7390 = 0;
        (*world).render_scale = Fixed::ONE;
        (*world)._field_7398 = 0;

        // Allocate team tracking arrays (GameWorld+0x514, GameWorld+0x518)
        {
            let count_a = (*game_info).team_slot_count;
            let size = count_a.wrapping_mul(4);
            let arr = wa_malloc(size);
            (*world)._unknown_514 = arr;
            for i in 0..count_a as usize {
                *(arr as *mut u32).add(i) = 0;
            }
        }
        {
            let count_b = (*game_info).object_slot_count;
            let size = count_b.wrapping_mul(4);
            let arr = wa_malloc(size);
            (*world)._unknown_518 = arr;
            for i in 0..count_b as usize {
                *(arr as *mut u32).add(i) = 0;
            }
        }

        // Initialize the 0x51C object (vector-like structure)
        {
            let obj = (*world)._unknown_51c;
            if !obj.is_null() {
                *(obj as *mut u32) = 0; // first field = 0
                // The vector operations (FUN_005370c0, etc.) are complex.
                // Bridge to original via setting fields directly.
                let obj32 = obj as *mut u32;
                // Reset the internal vector state
                *(obj32.add(5)) = 0; // +0x14

                // Reset the second vector (at +0x1C..+0x24)
                let start = *(obj32.add(7)) as *mut u32; // +0x1C
                let end = *(obj32.add(8)) as *mut u32; // +0x20
                if start != end {
                    // Move elements: dest = start + (capacity - end offset)
                    // This is a std::vector erase-to-end operation.
                    // For init, just set write pointer = start
                    *(obj32.add(8)) = *(obj32.add(7)); // end = start
                }

                *(obj32.add(10)) = 0xFFFFFFFF; // +0x28 = -1
            }
        }
    }
}
