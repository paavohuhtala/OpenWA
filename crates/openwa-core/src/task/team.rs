use crate::fixed::Fixed;
use super::base::CTask;

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
#[repr(C)]
pub struct CTaskTeam {
    /// 0x00–0x2F: CTask base (vtable, parent, children, shared_data, ddgame, …)
    pub base: CTask,
    /// 0x30: Secondary interface vtable pointer (Ghidra 0x00669F00)
    pub _secondary_vtable: u32,
    /// 0x34: Unknown — observed to hold the same value as `team_index` in all runs.
    pub _unknown_34: u32,
    /// 0x38: Team number, **1-based**.  Matches TeamArenaState slot index.
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
