use super::base::CTask;
use crate::FieldRegistry;
use openwa_core::fixed::Fixed;

crate::define_addresses! {
    class "CTaskTeam" {
        /// CTaskTeam vtable - per-team task
        vtable CTASK_TEAM_VTABLE = 0x0066_9EE4;
        ctor CTASK_TEAM_CTOR = 0x0055_5BB0;
    }
}

/// CTaskTeam vtable — 12 slots. Extends CTask base (8 slots) with team-specific behavior.
///
/// Vtable at Ghidra 0x669EE4. Slot 2 (HandleMessage) is the main team message
/// dispatcher (1701 instructions, handles weapon fire, surrender, worm selection, etc.)
#[openwa_game::vtable(size = 12, va = 0x0066_9EE4, class = "CTaskTeam")]
pub struct CTaskTeamVTable {
    /// WriteReplayState — serializes team state to replay stream.
    /// thiscall + 1 stack param (stream), RET 0x4.
    #[slot(0)]
    pub write_replay_state: fn(this: *mut CTaskTeam, stream: *mut u8),
    /// Free — destructor. thiscall + 1 stack param (flags), RET 0x4.
    #[slot(1)]
    pub free: fn(this: *mut CTaskTeam, flags: u8) -> *mut CTaskTeam,
    /// HandleMessage — processes messages sent to this team (weapon fire,
    /// surrender, worm selection, napalm strike, etc.)
    /// thiscall + 4 stack params, RET 0x10.
    #[slot(2)]
    pub handle_message:
        fn(this: *mut CTaskTeam, sender: *mut CTask, msg_type: u32, size: u32, data: *const u8),
    /// GetEntityData — returns team data by query code.
    /// thiscall + 3 stack params, RET 0xC.
    #[slot(3)]
    pub get_entity_data: fn(this: *mut CTaskTeam, query: u32, param: u32, out: *mut u32) -> u32,
    // Slots 4-6: inherited CTask methods
    /// ProcessFrame — per-frame team update.
    /// thiscall + 1 stack param (flags), RET 0x4.
    #[slot(7)]
    pub process_frame: fn(this: *mut CTaskTeam, flags: u32),
}

/// Per-team state-tracker task — one instance per team, child of CTaskTurnGame.
///
/// Tracks per-team data: which team number it represents, how many worms were
/// spawned, and a weapon/item slot table.  Registered in the SharedData hash
/// table with type code 0x15 (21).
///
/// Inheritance: extends CTask directly (CTask base at +0x00).
/// class_type = 10.  Allocation: 0x460 bytes via `operator new` (0x5C0AB8).
/// Constructor: `CTaskTeam__Constructor` (0x555BB0).
/// Primary vtable: `CTaskTeam__vtable2` (0x00669EE4).
/// Secondary interface vtable: 0x00669F00 (at object +0x30).
///
/// Key constructor logic (0x555BB0):
///   - `team_index` at +0x38: 1-based team number (passed as param_3)
///   - `_item_slots[0..99]` at +0x88: up to 100 weapon/item IDs loaded from scheme
///   - `worm_count` at +0x218: number of CTaskWorm children constructed (1-indexed)
///   - SharedData node (0x30 bytes) registered with key = (team_index, type=0x15)
#[derive(FieldRegistry)]
#[repr(C)]
pub struct CTaskTeam {
    /// 0x00–0x2F: CTask base with typed CTaskTeamVTable vtable pointer.
    pub base: CTask<*const CTaskTeamVTable>,
    /// 0x30: Secondary interface vtable pointer (Ghidra 0x00669F00)
    pub _secondary_vtable: u32,
    /// 0x34: Unknown — observed to hold the same value as `team_index` in all runs.
    pub _unknown_34: u32,
    /// 0x38: Team number, **1-based**.  Matches TeamArena slot index.
    pub team_index: u32,
    /// 0x3C: Unknown — observed 0 for team 1, 1 for team 2.
    pub _unknown_3c: u32,
    /// 0x40: Unknown signed integer — observed −40 for team 1, −20 for team 2.
    pub _unknown_40: i32,
    /// 0x44: Unknown (always 0 in observed runs).
    pub _unknown_44: u32,
    /// 0x48: Number of living worms remaining on this team.
    /// Decrements as worms are eliminated; equals `worm_count` at game start.
    pub alive_worm_count: u32,
    /// 0x4C–0x5B: Unknown.
    pub _unknown_4c: [u8; 16],
    /// 0x5C: Unknown — consistently 11 across teams and runs.
    pub _unknown_5c: u32,
    /// 0x60: Index of the last weapon launched by this team (0 = none launched yet).
    /// Field name confirmed by wkJellyWorm: `lastLaunchedWeapon_dword60`.
    pub last_launched_weapon: u32,
    /// 0x64–0x87: Unknown.
    pub _unknown_64: [u8; 36],
    /// 0x88–0x217: Unknown region (100 DWORDs).  Observed non-zero values at the start
    /// (+0x88 = team_index, +0x8C/+0x90 = 3) in a 2-worm bot game; purpose unclear.
    pub _unknown_88: [u32; 100],
    /// 0x218: Number of CTaskWorm children constructed for this team.
    pub worm_count: u32,
    /// 0x21C–0x3EB: Unknown.
    pub _unknown_21c: [u8; 0x1D0],
    /// 0x3EC–0x3F3: Unknown flags (observed: 1, 1 then 0xFFFFFFFF×3 at +0x3F4).
    pub _unknown_3ec: [u32; 2],
    /// 0x3F4–0x3FF: Sentinel values (observed: 0xFFFFFFFF in all runs).
    pub _sentinels_3f4: [u32; 3],
    /// 0x400–0x403: Unknown.
    pub _unknown_400: u32,
    /// 0x404: X position (Fixed16.16) — per-team, likely spawn or last-worm position.
    pub pos_x: Fixed,
    /// 0x408: Y position (Fixed16.16) — per-team, likely spawn or last-worm position.
    pub pos_y: Fixed,
    /// 0x40C: Unknown Fixed16.16 value — same for all teams (observed ≈ 666.89).
    pub _unknown_40c: Fixed,
    /// 0x410: Unknown (observed 1 in all runs).
    pub _unknown_410: u32,
    /// 0x414–0x45F: Unknown.
    pub _unknown_414: [u8; 0x4C],
}

const _: () = assert!(core::mem::size_of::<CTaskTeam>() == 0x460);

// Generate typed vtable method wrappers: handle_message(), write_replay_state(), etc.
bind_CTaskTeamVTable!(CTaskTeam, base.vtable);

// ── Typed message handlers ──────────────────────────────────

impl CTaskTeam {
    /// Handle message 0x2B (Surrender) at the CTaskTeam level — ported from
    /// CTaskTeam::HandleMessage (0x557310) case 0x2B.
    ///
    /// **Important:** This is only the CTaskTeam layer. The entity at SharedData
    /// key (0, 0x14) is actually a CTaskTurnGame (inherits CTaskTeam).
    /// CTaskTurnGame::HandleMessage (0x55DC00) wraps this with:
    ///   - End-turn logic (FUN_0055C300) if the active team surrenders
    ///   - Surrender sound playback
    ///
    /// To port message 0x2B fully, CTaskTurnGame::HandleMessage case 0x2B must
    /// also be ported. Until then, use vtable dispatch (handle_message_raw) on the
    /// CTaskTurnGame to hit the TurnGame override.
    ///
    /// # Safety
    /// `this` must be a valid CTaskTeam pointer with valid ddgame.
    pub unsafe fn on_surrender_fire(this: *mut Self, sender: *mut CTask, msg_team_index: u32) {
        unsafe {
            use crate::game::TaskMessage;

            let team_index = (*this).team_index;

            // 1. Team ownership check — only process messages for our team
            if msg_team_index != team_index {
                return;
            }

            // Serialize the message to raw bytes for broadcast to WA children
            let mut buf = [0u8; 8];
            buf[0..4].copy_from_slice(&msg_team_index.to_ne_bytes());
            let task_ptr = this as *mut CTask;

            // 2. If game_version > 0xF4: broadcast DetonateWeapon (0x2A) to children first
            let ddgame = CTask::ddgame_raw(this as *const CTask);
            let game_version = (*(*ddgame).game_info).game_version;
            if game_version > 0xF4 {
                CTask::broadcast_message_raw(
                    task_ptr,
                    sender,
                    TaskMessage::DetonateWeapon as u32,
                    4,
                    buf.as_ptr(),
                );
            }

            // 3. Set per-team napalm flag
            // Original: *(ddgame + team_index * 0x51C + 0x4618) = 1
            let flag_ptr =
                (ddgame as *mut u8).add(team_index as usize * 0x51C + 0x4618) as *mut u32;
            *flag_ptr = 1;

            // 4. Broadcast original message (0x2B) to children
            CTask::broadcast_message_raw(
                task_ptr,
                sender,
                TaskMessage::Surrender as u32,
                4,
                buf.as_ptr(),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// CTaskTeam__CreateWeatherFilter (0x552960) — cloud spawning factory
// ---------------------------------------------------------------------------

use crate::address::va;
use crate::engine::ddgame::DDGame;
use crate::rebase::rb;
use crate::task::cloud::{CTaskCloud, CloudType};
use crate::task::filter::CTaskFilter;
use crate::wa_alloc::wa_malloc_struct_zeroed;
use openwa_core::rng::wa_lcg;

/// Bridge: call CTaskFilter constructor (usercall: ECX=0x1B, stdcall params: this, parent).
/// Returns constructed pointer in EAX.
///
/// # Safety
/// `this` must point to a zeroed 0xB4-byte allocation. `parent` must be a valid CTask.
#[unsafe(naked)]
unsafe extern "cdecl" fn call_filter_ctor(
    _this: *mut u8,
    _parent: *mut u8,
    _addr: u32,
) -> *mut CTaskFilter {
    core::arch::naked_asm!(
        "mov ecx, 0x1b",     // init_val_1c register param
        "mov eax, [esp+12]", // addr (3rd cdecl param)
        "push [esp+8]",      // push parent (2nd cdecl param)
        "push [esp+8]",      // push this (1st cdecl param, shifted by 1 push)
        "call eax",          // stdcall, callee cleans 2 params (RET 8)
        "ret",               // cdecl return; caller cleans our 3 params
    )
}

/// Pure Rust port of CTaskTeam__CreateWeatherFilter (0x552960).
///
/// Creates a CTaskFilter subscribed to messages {1, 2, 3, 0x54}, then spawns
/// CTaskCloud children with deterministic LCG randomization.
///
/// Cloud count: 32 if `level_width_raw == 0`, else 10.
///
/// Called from CTaskTeam constructor. stdcall with 1 param (parent task).
///
/// # Safety
/// `parent` must be a valid CTask pointer (typically a CTaskTeam child).
pub unsafe extern "stdcall" fn create_weather_filter(parent: *mut CTask) {
    unsafe {
        // 1. Allocate and construct CTaskFilter
        let filter_ptr = wa_malloc_struct_zeroed::<CTaskFilter>();
        let filter = call_filter_ctor(
            filter_ptr as *mut u8,
            parent as *mut u8,
            rb(va::CTASK_FILTER_CTOR),
        );
        // ownership transfers to the task tree

        // 2. Subscribe to messages: FrameStart(1), FrameFinish(2), RenderScene(3), SetWind(0x54)
        (*filter).subscription_table[1] = 1;
        (*filter).subscription_table[2] = 1;
        (*filter).subscription_table[3] = 1;
        (*filter).subscription_table[0x54] = 1;

        // 3. Read level geometry from DDGame
        let ddgame = (*parent).ddgame;
        let level_right = (*ddgame).level_bound_max_x.0;
        let level_left = (*ddgame).level_bound_min_x.0;
        let level_width_int = (level_right - level_left) >> 16; // integer pixels
        let level_min_x_int = (*ddgame).level_bound_min_x.0 >> 16; // integer part (signed)
        let level_height = (*ddgame).level_height as i32;

        // 4. Determine cloud count and weather modifier
        let (cloud_count, weather_mod): (i32, i32) = if (*ddgame).level_width_raw != 0 {
            (10, 0)
        } else {
            (32, -256)
        };

        if cloud_count <= 0 || level_width_int <= 0 {
            return;
        }

        // 5. LCG random loop — spawn clouds as children of the filter
        let mut rng_state: u32 = 0x12345678;
        let mut layer_depth = 0x19_0000i32; // Fixed 25.0
        let mut accum_3i = 0i32;

        // Height contribution: (level_height + rounding) / 16
        // Matches MSVC signed-division rounding: (val + (val >> 31 & 15)) >> 4
        let height_round = if level_height < 0 {
            level_height & 0xF
        } else {
            0
        };
        let height_contrib = (level_height + height_round) >> 4;

        // Height tenth: level_height / 10 with MSVC magic-number divide
        let height_tenth = {
            let h = level_height as i64;
            let raw = ((h * 0x6666_6667) >> 34) as i32;
            raw + ((raw as u32 >> 31) as i32) // round toward zero
        };

        let ctask_ctor: unsafe extern "stdcall" fn(*mut u8, *mut u8, *mut DDGame) =
            core::mem::transmute(rb(va::CTASK_CONSTRUCTOR) as usize);

        for i in 0..cloud_count {
            // X position: random within level bounds
            let rand_frac = (rng_state & 0xFFFF) as i32;
            rng_state = wa_lcg(rng_state);

            let pos_x_int = rand_frac % level_width_int + level_min_x_int;

            // Render Y: height/16 + scaled offset + weather modifier, in Fixed16.16
            // (integer pixel value placed in the upper 16 bits).
            let y_offset = height_tenth * i / cloud_count;
            let render_y = Fixed((height_contrib + y_offset + weather_mod) << 16);

            // X velocity: random magnitude + 0x8000, random sign
            let vel_x_base = (rng_state & 0xFFFF) as i32 + 0x8000;
            rng_state = wa_lcg(rng_state);
            let vel_x = if rng_state & 0x10000 != 0 {
                Fixed(-vel_x_base)
            } else {
                Fixed(vel_x_base)
            };
            rng_state = wa_lcg(rng_state); // advance a third time per iteration

            // Cloud type: distributes large→medium→small across the batch
            let cloud_type = match 2 - accum_3i / cloud_count {
                0 => CloudType::Large,
                1 => CloudType::Medium,
                _ => CloudType::Small,
            };

            // Allocate cloud, construct CTask base, then init cloud fields
            let cloud_ptr = wa_malloc_struct_zeroed::<CTaskCloud>();
            ctask_ctor(cloud_ptr as *mut u8, filter as *mut u8, ddgame);
            CTaskCloud::init(
                cloud_ptr,
                cloud_type,
                Fixed(layer_depth),
                Fixed(pos_x_int << 16),
                vel_x,
                render_y,
            );
            // ownership transfers to the task tree

            layer_depth -= 1;
            accum_3i += 3;
        }
    }
}
