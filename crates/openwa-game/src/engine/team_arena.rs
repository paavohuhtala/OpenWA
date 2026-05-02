// ============================================================
// Team arena state — sub-struct at GameWorld + 0x4628
// ============================================================
//
// Extracted from world.rs: team/worm data structures, TeamArena,
// and related helper structs used as GameWorld fields.

use core::ffi::CStr;

use crate::snapshot::Snapshot;

use super::world::offsets;

/// Team index permutation map (0x64 = 100 bytes).
///
/// Used for mapping team indices to rendering/turn slots. Three instances
/// live in GameWorld at offsets 0x7650, 0x76B4, 0x7718 (stride 0x64).
///
/// Layout: a free-pool stack of 16 `i16` indices plus a pair of parallel
/// arrays storing the currently-active handles. `RemoveHandle` (0x00526000,
/// see [`Self::remove_handle`]) searches `active_list`, compacts both
/// parallel arrays leftward, pushes the freed handle onto the `entries`
/// stack, and decrements `active_count`. The constructor at [`world_constructor`]
/// fills `entries[i] = i` (identity), `count = 16`, `active_count = 0`; the
/// active list and its companion array start uninitialised (heap-zeroed
/// from `wa_malloc_struct_zeroed`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TeamIndexMap {
    /// 0x00: Free-pool stack — 16 `i16` slots holding currently-unused
    /// indices. `RemoveHandle` pushes the freed handle here at `[count]`.
    /// Constructor fills with identity `[0..15]`.
    pub entries: [i16; 16],
    /// 0x20: Free-pool top-of-stack pointer. Initial: `16` (full pool).
    /// Increments on each successful `RemoveHandle`; decremented elsewhere
    /// by the (still-bridged) `AddHandle` companion when a handle is taken.
    pub count: u16,
    /// 0x22: Parallel companion array shifted in lockstep with
    /// `active_list`. Purpose unconfirmed (likely a metadata slot tied to
    /// each active handle). Heap-zeroed initially; populated on add.
    pub parallel_array: [i16; 16],
    /// 0x42: Active handle list — searched by `RemoveHandle` for the
    /// caller's `*handle_ptr` value. Compacted leftward when an entry is
    /// removed.
    pub active_list: [i16; 16],
    /// 0x62: Active count — number of currently-valid entries in
    /// `active_list` / `parallel_array`. Initial: `0`. Decrements on each
    /// successful `RemoveHandle`.
    pub active_count: u16,
}
const _: () = assert!(core::mem::size_of::<TeamIndexMap>() == 0x64);
const _: () = assert!(core::mem::offset_of!(TeamIndexMap, count) == 0x20);
const _: () = assert!(core::mem::offset_of!(TeamIndexMap, parallel_array) == 0x22);
const _: () = assert!(core::mem::offset_of!(TeamIndexMap, active_list) == 0x42);
const _: () = assert!(core::mem::offset_of!(TeamIndexMap, active_count) == 0x62);

impl TeamIndexMap {
    /// Rust port of `TeamIndexMap__RemoveHandle_Maybe` (0x00526000).
    ///
    /// Convention: usercall `EAX = *mut TeamIndexMap, EDI = *mut i32`,
    /// plain `RET`. Removes `*handle_ptr` from `active_list` if present;
    /// when found:
    ///
    /// 1. Push the freed handle onto `entries[count]`, increment `count`.
    /// 2. Compact `active_list` and `parallel_array` leftward starting at
    ///    the found index (closing the gap; trailing slot left stale).
    /// 3. Set `*handle_ptr = -1`.
    /// 4. Decrement `active_count`.
    ///
    /// No-op if `active_count == 0` or the handle is not present in the
    /// active list. The shift uses `i16` reads as in WA — the value at
    /// `*handle_ptr` is sign-extended to `i32` before the comparison, so
    /// negative handle values match correctly.
    ///
    /// # Safety
    /// `handle_ptr` must point to a valid `i32`. The map must be a valid
    /// `TeamIndexMap` instance.
    pub unsafe fn remove_handle(this: *mut TeamIndexMap, handle_ptr: *mut i32) {
        unsafe {
            let active_count = (*this).active_count as i32;
            if active_count <= 0 {
                return;
            }

            let handle = *handle_ptr;
            // Search active_list[0..active_count] for the handle.
            let mut found_idx: Option<usize> = None;
            for i in 0..active_count as usize {
                if (*this).active_list[i] as i32 == handle {
                    found_idx = Some(i);
                    break;
                }
            }
            let Some(found_idx) = found_idx else {
                return;
            };

            // Push handle onto the free pool at entries[count].
            let count = (*this).count as usize;
            (*this).entries[count] = handle as i16;
            (*this).count = (count as u16).wrapping_add(1);

            // Compact both parallel arrays leftward from found_idx.
            let last = (active_count - 1) as usize;
            for i in found_idx..last {
                (*this).parallel_array[i] = (*this).parallel_array[i + 1];
                (*this).active_list[i] = (*this).active_list[i + 1];
            }

            *handle_ptr = -1;
            (*this).active_count = (*this).active_count.wrapping_sub(1);
        }
    }

    /// Rust port of `TeamIndexMap__PopHandle_Maybe` (0x00525F50).
    ///
    /// Convention: thiscall `ECX = *mut TeamIndexMap`, 1 stack arg `key: i32`,
    /// `RET 0x4`, returns the popped handle in EAX (or `-1` if the free pool
    /// is empty).
    ///
    /// Pops one index off the free-pool stack (`entries[--count]`) and
    /// inserts it into the active set, keeping `parallel_array` sorted in
    /// non-increasing order of `key`. The new entry is placed at the first
    /// position `i` where `parallel_array[i] <= key` (or appended if no
    /// such position exists), shifting everything after rightward in both
    /// `parallel_array` and `active_list`.
    ///
    /// Returns `-1` when the pool is exhausted (`count == 0`); otherwise
    /// returns the freshly popped handle (the same value now stored in
    /// `active_list[i]`).
    ///
    /// # Safety
    /// `this` must point to a valid `TeamIndexMap`.
    pub unsafe fn pop_handle(this: *mut TeamIndexMap, key: i32) -> i32 {
        unsafe {
            let count = (*this).count;
            if count == 0 {
                return -1;
            }

            // Pop from the free pool.
            let new_count = count - 1;
            (*this).count = new_count;
            let handle = (*this).entries[new_count as usize] as i32;

            // Find insertion position: first slot where parallel_array[i] <= key.
            let active_count = (*this).active_count as i32;
            let mut insert_at: i32 = 0;
            if active_count > 0 {
                while ((*this).parallel_array[insert_at as usize] as i32) > key {
                    insert_at += 1;
                    if insert_at >= active_count {
                        break;
                    }
                }
            }

            // Shift entries [insert_at..active_count] one slot to the right.
            if active_count > insert_at {
                let mut i = active_count as usize;
                while i > insert_at as usize {
                    (*this).parallel_array[i] = (*this).parallel_array[i - 1];
                    (*this).active_list[i] = (*this).active_list[i - 1];
                    i -= 1;
                }
            }

            // Insert.
            (*this).parallel_array[insert_at as usize] = key as i16;
            (*this).active_list[insert_at as usize] = handle as i16;
            (*this).active_count = (*this).active_count.wrapping_add(1);

            handle
        }
    }
}

// ============================================================
// Per-worm and per-team block structs
// ============================================================

/// Per-worm data entry (0x9C bytes, stride between consecutive worms).
///
/// WA supports up to 8 playable worms per team. The original code accesses
/// worms via raw pointer arithmetic from the team entry pointer, using
/// stride 0x9C. This means the 8th worm crosses the TeamBlock boundary
/// into the next block's header slot — see `TeamArena::team_worm()`.
///
/// Field offsets confirmed by runtime memory dump (validator DLL):
/// - state at 0x00: 0x67 = active/selected, 0x65 = idle, 0x80+ = special
/// - active_flag at 0x0C: 1 for alive worms
/// - max_health at 0x58: initial health value (100)
/// - health at 0x5C: current health (100 = full)
/// - name at 0x78: null-terminated worm name string (~20 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WormEntry {
    /// 0x00: Worm state machine state.
    /// Values: 0x65=idle, 0x67=active/selected, 0x68=active variant.
    /// Special states {0x80..0x85, 0x89} = dying/drowning/special animation.
    /// TODO: Update this to use WormState enum
    pub state: u32,
    /// 0x04: Unknown counter. Incremented by Freeze (+10).
    pub effect_counter_04_Maybe: i32,
    /// 0x08: Turn action counter. Incremented by weapon handlers:
    /// Surrender (+14), Mail/Mine/Mole (+7), Freeze (+3).
    pub turn_action_counter_Maybe: i32,
    /// 0x0C: Active/alive flag (1 for alive worms in game, 0 otherwise).
    pub active_flag: i32,
    /// 0x10-0x57: Unknown
    pub _unknown_10: [u8; 0x48],
    /// 0x58: Max health (initial health value, typically 100).
    pub max_health: i32,
    /// 0x5C: Current health. Used by GetTeamTotalHealth.
    pub health: i32,
    /// 0x60-0x77: Unknown.
    pub _unknown_60: [u8; 0x18],
    /// 0x78: Worm name, null-terminated ASCII string (~20 bytes).
    pub name: [u8; 0x18],
    /// 0x90-0x97: Used transiently by GetWormPosition (+0x90=x, +0x94=y).
    pub _unknown_90: [u8; 0x08],
    /// 0x98: Per-turn action-pending flag. Read by `WormEntity::HandleMessage`
    /// cases 0x2 (BehaviorTick — clears it after acting), 0x1C/0x76 (damage
    /// — checked on the *sender's* worm), and 0x2B (Surrender — gates the
    /// drop-to-Idle). Set somewhere upstream when a worm performs an action
    /// that needs a one-frame ack; semantics TBD.
    pub _field_98: u32,
}

const _: () = assert!(core::mem::size_of::<WormEntry>() == 0x9C);

/// Team-level metadata stored at slot 0 of each TeamBlock (0x9C bytes).
///
/// This struct overlays the same memory as a WormEntry but interprets the
/// high offsets (0x6C+) as team metadata rather than worm data. The low
/// offsets (0x00-0x5F) may contain data from the previous team's 8th worm
/// when that team has 8 worms — they are treated as opaque padding.
///
/// Accessed via `TeamArena::team_header()` and `team_header_b()`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TeamHeader {
    /// 0x00-0x5F: Opaque — may hold 8th worm data from previous team.
    pub worm_overlap: [u8; 0x60],
    /// 0x60-0x6B: Unknown padding.
    pub _unknown_60: [u8; 0x0C],
    /// 0x6C: Team eliminated flag (nonzero = eliminated).
    pub eliminated: i32,
    /// 0x70: Alliance ID used by CountTeamsByAlliance and SetActiveWorm_Maybe.
    pub alliance: i32,
    /// 0x74: Active worm index (0 = none, N = worm N is active).
    pub active_worm: i32,
    /// 0x78: Number of worms on this team.
    pub worm_count: i32,
    /// 0x7C: Per-team turn action flags (bitfield).
    /// Skip Go (weapon 57) toggles a bit here based on the weapon's fire_params.
    /// Bit is set to mark the team should skip; in game_version > 0x1C, toggling
    /// again clears it.
    pub turn_action_flags: u32,
    /// 0x80: Alliance ID for ammo/delay table indexing (GetAmmo/AddAmmo/SubtractAmmo).
    /// Teams with the same weapon_alliance share ammo pools. Distinct from
    /// `alliance` at 0x70 which is used by CountTeamsByAlliance.
    pub weapon_alliance: i32,
    /// 0x84: Team name, null-terminated ASCII string.
    pub team_name: [u8; 0x14],
    /// 0x98: Unknown trailing bytes.
    pub _unknown_98: [u8; 4],
}

const _: () = assert!(core::mem::size_of::<TeamHeader>() == 0x9C);

/// Union for slot 0 of a TeamBlock.
///
/// This slot is dual-purpose: its high offsets store team metadata
/// (`TeamHeader`), while its low offsets may contain the 8th worm
/// of the previous team (`WormEntry`). The two uses don't conflict
/// because worm data occupies 0x00-0x5F and header data starts at 0x6C.
#[repr(C)]
#[derive(Clone, Copy)]
pub union TeamSlot0 {
    /// View as worm data (used when the previous team has 8 worms).
    pub worm: core::mem::ManuallyDrop<WormEntry>,
    /// View as team metadata (eliminated, alliance, worm_count, etc.).
    pub team: core::mem::ManuallyDrop<TeamHeader>,
}

/// Full per-team data block (0x51C bytes, 7 blocks in GameWorld).
///
/// Each block starts with a `TeamSlot0` union (0x9C bytes) that serves
/// dual purpose: its high offsets hold team metadata (`TeamHeader`),
/// while its low offsets may contain the 8th worm of the previous team.
/// The remaining 7 worm slots follow, then 0x3C bytes of metadata.
///
/// **Block indexing**: Block 0 is unused (preamble, all zeros). Actual team
/// data starts at block 1. Worms are accessed via `TeamArena::team_worm()`
/// which uses raw pointer arithmetic and naturally crosses block boundaries.
///
/// **Header access**: Use `TeamArena::team_header(idx)` to get the
/// `TeamHeader` for a team (reads from `blocks[idx+1].header.team`).
#[repr(C)]
pub struct TeamBlock {
    /// 0x000-0x09B: Header slot (union of TeamHeader and WormEntry).
    /// Team metadata at high offsets; may hold 8th worm data at low offsets.
    pub header: TeamSlot0,
    /// 0x09C-0x4DF: 7 worm entries (slots 1-7, stride 0x9C)
    pub worms: [WormEntry; 7],
    /// 0x4E0-0x51B: Team metadata (0x3C bytes)
    pub trailer: [u8; 0x3C],
}

const _: () = assert!(core::mem::size_of::<TeamBlock>() == 0x51C);

/// Per-alliance weapon ammo and delay data (0x238 = 568 bytes per alliance).
#[repr(C)]
pub struct TeamWeaponSlots {
    /// Ammo counts per weapon (71 entries). -1 = unlimited, 0 = none, >0 = count.
    pub ammo: [i32; 71],
    /// Delay flags per weapon (71 entries). Nonzero = weapon on cooldown.
    pub delay: [i32; 71],
}
const _: () = assert!(core::mem::size_of::<TeamWeaponSlots>() == 0x238);

/// Weapon slot data for all 6 alliances (0xD50 = 3408 bytes total).
#[repr(C)]
pub struct WeaponSlots {
    pub teams: [TeamWeaponSlots; 6],
}
const _: () = assert!(core::mem::size_of::<WeaponSlots>() == 852 * 4);

/// Team arena state area within GameWorld (at GameWorld + 0x4628).
///
/// Contains per-team entries and an interleaved ammo/delay table.
/// Used by GetAmmo (0x5225E0), AddAmmo (0x522640), SubtractAmmo (0x522680).
///
/// The ammo/delay table uses stride 142 (= 71 weapons * 2) per alliance.
/// Within each alliance block of 142 entries:
/// - Entries 0..70 are ammo counts (accessed at base + 0x1EB4 + index * 4)
/// - Entries 71..141 are delay flags (accessed at base + 0x1FD0 + index * 4)
///
/// The original code accesses these as two arrays at different base offsets
/// (0x1EB4 and 0x1FD0) using the same index `alliance_id * 142 + weapon_id`.
/// Since 0x1FD0 - 0x1EB4 = 71 * 4, this is equivalent to accessing
/// `weapon_slots.teams[alliance].ammo[weapon]` and
/// `weapon_slots.teams[alliance].delay[weapon]`.
#[repr(C)]
pub struct TeamArena {
    /// 0x0000-0x1EAF: Per-team data region (opaque).
    ///
    /// This region contains 7 team entries at stride 0x51C (1-indexed, index 0
    /// is preamble). The original WA.exe accesses team data via raw pointer
    /// arithmetic relative to the arena base. In our Rust code, team data is
    /// accessed through `TeamArena`, `TeamBlock`, and `TeamHeader`,
    /// so this region is treated as opaque padding.
    ///
    /// The 7th entry (team 6, 1-indexed) only has 8 bytes before team_count;
    /// the rest overlaps with weapon_slots.
    pub team_blocks_region: [u8; 0x1EB0],
    /// 0x1EB0: Number of teams in the game (used by team iteration loops)
    pub team_count: i32,
    /// 0x1EB4: Per-alliance weapon ammo and delay data.
    /// 6 alliances, each with 71 ammo + 71 delay i32 entries.
    /// Ammo: -1 = unlimited, 0 = none, >0 = count.
    /// Delay: nonzero = weapon on delay.
    pub weapon_slots: WeaponSlots,
    /// 0x2C04: Padding between weapon_slots end and game_mode_flag.
    /// weapon_slots: 852 * 4 = 3408 = 0xD50. 0x1EB4 + 0xD50 = 0x2C04.
    pub _pad_2c04: [u8; 8],
    /// 0x2C0C: Game mode flag (nonzero = override weapon delays in certain conditions)
    pub game_mode_flag: i32,
    /// 0x2C10: Unknown padding
    pub _pad_2c10: [u8; 0x18],
    /// 0x2C28: Game phase counter (>=484 = sudden death, >=-2 = normal game)
    /// TODO: Convert this to GamePhase enum
    pub game_phase: i32,

    // === Alliance tracking (set by CountTeamsByAlliance + SetActiveWorm_Maybe) ===
    /// 0x2C2C: Current alliance being evaluated
    pub current_alliance: i32,
    /// 0x2C30: Count of teams with an active worm (set by SetActiveWorm_Maybe)
    pub active_worm_count: i32,
    /// 0x2C34: Count of active teams with valid alliance (>=0)
    pub active_team_count: i32,
    /// 0x2C38: Count of teams matching current_alliance
    pub same_alliance_count: i32,
    /// 0x2C3C: Count of teams not matching current_alliance
    pub enemy_team_count: i32,
    /// 0x2C40: Last team index set active (written by SetActiveWorm_Maybe)
    pub last_active_team: i32,
    /// 0x2C44: Alliance of last activated team (written by SetActiveWorm_Maybe)
    pub last_active_alliance: i32,
}

const _: () = assert!(core::mem::size_of::<TeamArena>() == 0x2C48);

/// Worm state constants and helpers.
/// TODO: Replace this module with the WormState enum
pub mod worm {
    // --- Known state values (runtime-validated via validator dumps) ---

    /// Transitional state checked by CheckWormState0x64 (0x5228D0). 11 xrefs.
    /// Appears briefly during turn transitions.
    pub const STATE_TRANSITIONAL: u32 = 0x64;
    /// Idle — worm is waiting, not currently active.
    pub const STATE_IDLE: u32 = 0x65;
    /// Active — worm is currently selected and taking its turn.
    pub const STATE_ACTIVE: u32 = 0x67;
    /// Dead — worm has been killed (hp=0). Persists across turns.
    pub const STATE_DEAD: u32 = 0x87;

    /// Special worm states — worm is dying/drowning/in special animation.
    /// Checked by IsWormInSpecialState (0x5226B0).
    /// All values in the 0x80+ range. 0x87 (dead) is also in this set.
    pub const SPECIAL_STATES: [u32; 6] = [0x80, 0x81, 0x82, 0x83, 0x85, 0x89];

    /// Check if a worm state value is a "special" state.
    pub fn is_special_state(state: u32) -> bool {
        SPECIAL_STATES.contains(&state)
    }
}

/// Game phase thresholds (stored in TeamArena::game_phase).
pub const GAME_PHASE_SUDDEN_DEATH: i32 = 0x1E4; // 484
pub const GAME_PHASE_NORMAL_MIN: i32 = -2;

impl TeamArena {
    /// Get ammo count for a weapon by alliance and weapon ID.
    pub fn get_ammo(&self, alliance: usize, weapon_id: usize) -> i32 {
        self.weapon_slots.teams[alliance].ammo[weapon_id]
    }

    /// Get mutable reference to ammo count.
    pub fn ammo_mut(&mut self, alliance: usize, weapon_id: usize) -> &mut i32 {
        &mut self.weapon_slots.teams[alliance].ammo[weapon_id]
    }

    /// Get delay flag for a weapon by alliance and weapon ID.
    pub fn get_delay(&self, alliance: usize, weapon_id: usize) -> i32 {
        self.weapon_slots.teams[alliance].delay[weapon_id]
    }

    // ── Raw-pointer accessors (safe from noalias miscompilation) ──

    /// Get pointer to the TeamBlock array base.
    ///
    /// The TeamBlock array lives 0x598 bytes before TeamArena in GameWorld memory.
    #[inline]
    pub unsafe fn blocks_mut(this: *mut Self) -> *mut TeamBlock {
        unsafe { (this as *mut u8).sub(offsets::ARENA_TO_BLOCKS) as *mut TeamBlock }
    }

    /// Get pointer to the TeamBlock array base (read-only).
    ///
    /// The TeamBlock array lives 0x598 bytes before TeamArena in GameWorld memory.
    #[inline]
    pub unsafe fn blocks(this: *const Self) -> *const TeamBlock {
        unsafe { (this as *const u8).sub(offsets::ARENA_TO_BLOCKS) as *const TeamBlock }
    }

    /// Get pointer to team header (metadata) for a team.
    ///
    /// Returns `blocks[team_idx+1].header.team`, which holds team metadata:
    /// worm_count, eliminated flag, weapon_alliance, team_name.
    #[inline]
    pub unsafe fn team_header_mut(this: *mut Self, team_idx: usize) -> *mut TeamHeader {
        unsafe {
            &raw mut (*Self::blocks_mut(this).add(team_idx + 1)).header.team as *mut TeamHeader
        }
    }

    /// Get pointer to team header (metadata) for a team.
    ///
    /// Returns `blocks[team_idx+1].header.team`, which holds team metadata:
    /// worm_count, eliminated flag, weapon_alliance, team_name.
    #[inline]
    pub unsafe fn team_header(this: *const Self, team_idx: usize) -> *const TeamHeader {
        unsafe {
            &raw const (*Self::blocks(this).add(team_idx + 1)).header.team as *const TeamHeader
        }
    }

    /// Get pointer to a playable worm entry by 1-indexed worm number (1..=8).
    ///
    /// Uses raw pointer arithmetic matching the original WA code:
    /// `base + team_idx * 0x51C + worm_num * 0x9C - 0x598`.
    /// This naturally crosses TeamBlock boundaries when worm_num = 8,
    /// since the 8th worm's early fields (state, health) spill into the
    /// next block's header slot. The header metadata lives at high offsets
    /// (0x6C+) that don't conflict with worm data (0x00-0x5F).
    #[inline]
    pub unsafe fn team_worm_mut(
        this: *mut Self,
        team_idx: usize,
        worm_num: usize,
    ) -> *mut WormEntry {
        unsafe {
            let base = this as *mut u8;
            base.add(team_idx * 0x51C).add(worm_num * 0x9C).sub(0x598) as *mut WormEntry
        }
    }

    #[inline]
    pub unsafe fn team_worm(
        this: *const Self,
        team_idx: usize,
        worm_num: usize,
    ) -> *const WormEntry {
        unsafe {
            let base = this as *const u8;
            base.add(team_idx * 0x51C).add(worm_num * 0x9C).sub(0x598) as *const WormEntry
        }
    }

    /// Get a team's block pointer and its header pointer in one call.
    ///
    /// Returns `(blocks[team_idx], blocks[team_idx+1].header.team)`.
    /// The block contains worm data, and the header (slot 0 of the next block)
    /// holds team metadata (worm_count, eliminated flag).
    ///
    /// **Note**: For accessing worms, prefer `team_worm()` which handles
    /// 8-worm teams correctly via cross-boundary pointer arithmetic.
    #[inline]
    pub unsafe fn team_and_header(
        this: *mut Self,
        team_idx: usize,
    ) -> (*mut TeamBlock, *mut TeamHeader) {
        unsafe {
            let blocks = Self::blocks_mut(this);
            let block = blocks.add(team_idx);
            let header = &raw mut (*blocks.add(team_idx + 1)).header.team as *mut TeamHeader;
            (block, header)
        }
    }

    /// Get team header pointer for Pattern B access (alliance/active_worm at +0x70/+0x74).
    ///
    /// Pattern B indexes from block[i+2] for 0-indexed team `i`:
    /// `base + 0x510 + i*0x51C` = `blocks[i+2].header.team + 0x70`.
    #[inline]
    pub unsafe fn team_header_b_mut(this: *mut Self, team_idx: usize) -> *mut TeamHeader {
        unsafe {
            &raw mut (*Self::blocks_mut(this).add(team_idx + 2)).header.team as *mut TeamHeader
        }
    }

    /// Get team header pointer for Pattern B access (alliance/active_worm at +0x70/+0x74).
    ///
    /// Pattern B indexes from block[i+2] for 0-indexed team `i`:
    /// `base + 0x510 + i*0x51C` = `blocks[i+2].header.team + 0x70`.
    #[inline]
    pub unsafe fn team_header_b(this: *const Self, team_idx: usize) -> *const TeamHeader {
        unsafe {
            &raw const (*Self::blocks(this).add(team_idx + 2)).header.team as *const TeamHeader
        }
    }

    /// Compute the flat index for ammo/delay table access.
    ///
    /// Reads the weapon alliance ID from the team header for the given
    /// 1-indexed team, then computes `alliance_id * 142 + weapon_id`.
    ///
    /// Returns (alliance_id, weapon_id) for accessing weapon_slots.
    #[inline]
    pub unsafe fn weapon_slot_key(
        this: *const Self,
        team_index: usize,
        weapon_id: u32,
    ) -> (usize, usize) {
        unsafe {
            let alliance_id = (*Self::team_header(this, team_index)).weapon_alliance as usize;
            (alliance_id, weapon_id as usize)
        }
    }
}

// ── Snapshot impl ──────────────────────────────────────────

impl Snapshot for WormEntry {
    unsafe fn write_snapshot(
        &self,
        w: &mut dyn core::fmt::Write,
        indent: usize,
    ) -> core::fmt::Result {
        unsafe {
            use crate::snapshot::write_indent;
            let i = indent;
            let name =
                CStr::from_ptr(self.name.as_ptr() as *const core::ffi::c_char).to_string_lossy();
            write_indent(w, i)?;
            writeln!(
                w,
                "state=0x{:02X} active={} hp={}/{} name=\"{}\"",
                self.state, self.active_flag, self.health, self.max_health, name
            )?;
            Ok(())
        }
    }
}
