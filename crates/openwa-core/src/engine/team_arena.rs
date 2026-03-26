// ============================================================
// Team arena state — sub-struct at DDGame + 0x4628
// ============================================================
//
// Extracted from ddgame.rs: team/worm data structures, TeamArenaRef,
// and related helper structs used as DDGame fields.

use super::ddgame::offsets;

/// Coordinate entry used in DDGame screen coordinate tables (stride 0x10).
///
/// InitFields zeroes x and y; at runtime they contain fixed-point screen
/// coordinates used for camera tracking and rendering regions.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoordEntry {
    pub x: i32,
    pub y: i32,
    pub _unknown: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<CoordEntry>() == 0x10);

/// Team index permutation map (0x64 = 100 bytes).
///
/// Used for mapping team indices to rendering/turn slots. Three instances
/// live in DDGame at offsets 0x7650, 0x76B4, 0x7718 (stride 0x64).
/// Initialized as identity permutations [0,1,2,...,15] with count=16.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TeamIndexMap {
    /// 16 team index entries (identity permutation on init).
    pub entries: [i16; 16],
    /// Number of active entries (initialized to 16).
    pub count: u16,
    /// Gap — unknown purpose, not initialized by constructor.
    pub _gap: [u8; 64],
    /// Terminator (initialized to 0).
    pub terminator: u16,
}
const _: () = assert!(core::mem::size_of::<TeamIndexMap>() == 0x64);

/// Render table entry (0x14 = 20 bytes).
///
/// 14 entries live at DDGame+0x73B0 (stride 0x14). Only the first u32
/// is zeroed during construction; the rest is uninitialized/unknown.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderEntry {
    /// Active/state flag (zeroed on init).
    pub active: u32,
    /// Unknown data.
    pub _unknown: [u8; 16],
}
const _: () = assert!(core::mem::size_of::<RenderEntry>() == 0x14);

// ============================================================
// CoordList — dynamic array of packed terrain coordinates
// ============================================================

/// Packed terrain coordinate entry (8 bytes).
///
/// `coord` packs x and y as `x * 0x10000 + y` (fixed-point).
/// `flag` is always 1 for populated entries.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CoordListEntry {
    pub coord: u32,
    pub flag: u32,
}
const _: () = assert!(core::mem::size_of::<CoordListEntry>() == 8);

/// Dynamic coordinate array header (12 bytes).
///
/// Allocated at DDGame+0x50C during `init_graphics_and_resources`.
/// Data buffer is a separate allocation of `capacity * 8` bytes.
/// Used for terrain coordinate lookups (spawning, aiming, collision).
#[repr(C)]
pub struct CoordList {
    /// Number of entries currently stored.
    pub count: u32,
    /// Maximum number of entries (600).
    pub capacity: u32,
    /// Pointer to the data buffer (`capacity * sizeof(CoordListEntry)` bytes).
    pub data: *mut CoordListEntry,
}
const _: () = assert!(core::mem::size_of::<CoordList>() == 12);

// ============================================================
// Sound queue entry — 16 entries at DDGame + 0x7F00
// ============================================================

/// Sound queue entry (0x24 = 36 bytes, stride between consecutive entries).
///
/// DDGame maintains a 16-slot sound queue at offset 0x7F00. PlaySoundGlobal
/// appends entries; PlaySoundLocal additionally marks entries as local and
/// stores position via the task's secondary vtable.
#[repr(C)]
pub struct SoundQueueEntry {
    /// Sound effect ID (SoundId enum value).
    pub sound_id: u32,
    /// Flags / priority (e.g. 3=weapon, 7=explosion).
    pub flags: u32,
    /// Volume (Fixed-point, 0x10000 = 1.0).
    pub volume: u32,
    /// Pitch (Fixed-point, 0x10000 = 1.0).
    pub pitch: u32,
    /// Reserved, always 0.
    pub reserved: u32,
    /// 0 = global, 1 = local (has position tracking).
    pub is_local: u8,
    pub _pad: [u8; 3],
    /// Position X (filled by secondary vtable GetPosition for local sounds).
    pub pos_x: u32,
    /// Position Y (filled by secondary vtable GetPosition for local sounds).
    pub pos_y: u32,
    /// Pointer to task's secondary vtable (at task+0xE8) for position updates.
    pub secondary_vtable: u32,
}

const _: () = assert!(core::mem::size_of::<SoundQueueEntry>() == 0x24);

// ============================================================
// Per-worm and per-team block structs
// ============================================================

/// Per-worm data entry (0x9C bytes, stride between consecutive worms).
///
/// WA supports up to 8 playable worms per team. The original code accesses
/// worms via raw pointer arithmetic from the team entry pointer, using
/// stride 0x9C. This means the 8th worm crosses the TeamBlock boundary
/// into the next block's header slot — see `TeamArenaRef::team_worm()`.
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
    /// 0x90-0x9B: Unknown. Used transiently by GetWormPosition (+0x90=x, +0x94=y).
    /// Also observed as nonzero (1) on one poisoned worm but not another —
    /// not reliably correlated with poison state. Needs further investigation.
    pub _unknown_90: [u8; 0x0C],
}

const _: () = assert!(core::mem::size_of::<WormEntry>() == 0x9C);

/// Team-level metadata stored at slot 0 of each TeamBlock (0x9C bytes).
///
/// This struct overlays the same memory as a WormEntry but interprets the
/// high offsets (0x6C+) as team metadata rather than worm data. The low
/// offsets (0x00-0x5F) may contain data from the previous team's 8th worm
/// when that team has 8 worms — they are treated as opaque padding.
///
/// Accessed via `TeamArenaRef::team_header()` and `team_header_b()`.
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

/// Full per-team data block (0x51C bytes, 7 blocks in DDGame).
///
/// Each block starts with a `TeamSlot0` union (0x9C bytes) that serves
/// dual purpose: its high offsets hold team metadata (`TeamHeader`),
/// while its low offsets may contain the 8th worm of the previous team.
/// The remaining 7 worm slots follow, then 0x3C bytes of metadata.
///
/// **Block indexing**: Block 0 is unused (preamble, all zeros). Actual team
/// data starts at block 1. Worms are accessed via `TeamArenaRef::team_worm()`
/// which uses raw pointer arithmetic and naturally crosses block boundaries.
///
/// **Header access**: Use `TeamArenaRef::team_header(idx)` to get the
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

/// Team arena state area within DDGame (at DDGame + 0x4628).
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
pub struct TeamArenaState {
    /// 0x0000-0x1EAF: Per-team data region (opaque).
    ///
    /// This region contains 7 team entries at stride 0x51C (1-indexed, index 0
    /// is preamble). The original WA.exe accesses team data via raw pointer
    /// arithmetic relative to the arena base. In our Rust code, team data is
    /// accessed through `TeamArenaRef`, `TeamBlock`, and `TeamHeader`,
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

const _: () = assert!(core::mem::size_of::<TeamArenaState>() == 0x2C48);

/// Worm state constants and helpers.
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

/// Game phase thresholds (stored in TeamArenaState::game_phase).
pub const GAME_PHASE_SUDDEN_DEATH: i32 = 0x1E4; // 484
pub const GAME_PHASE_NORMAL_MIN: i32 = -2;

impl TeamArenaState {
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
}

/// Typed handle to a TeamArenaState pointer received from WA.exe.
///
/// Wraps the raw `base: u32` from trampoline register captures and provides
/// accessor methods that encapsulate the backward pointer arithmetic to reach
/// TeamBlock worm data. The TeamBlock array lives 0x598 bytes before
/// the TeamArenaState in DDGame memory.
///
/// # Safety
/// Must only be constructed from a valid DDGame + TEAM_ARENA_STATE pointer.
/// `repr(transparent)` ensures identical ABI to `*const u8` / `u32` on i686,
/// so it can be received directly from usercall trampoline register captures.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct TeamArenaRef {
    base: *mut u8,
}

impl TeamArenaRef {
    /// Wrap a raw integer pointer (for usercall trampoline register captures).
    ///
    /// # Safety
    /// `base` must point to a valid TeamArenaState (DDGame + 0x4628).
    #[inline]
    pub unsafe fn from_raw(base: u32) -> Self {
        Self {
            base: base as *mut u8,
        }
    }

    /// Wrap a typed pointer to TeamArenaState.
    ///
    /// # Safety
    /// `arena` must point to a valid TeamArenaState within a live DDGame.
    #[inline]
    pub unsafe fn from_ptr(arena: *mut TeamArenaState) -> Self {
        Self {
            base: arena as *mut u8,
        }
    }

    /// Access the TeamArenaState fields (read-only).
    #[inline]
    pub unsafe fn state(&self) -> &TeamArenaState {
        &*(self.base as *const TeamArenaState)
    }

    /// Access the TeamArenaState fields (mutable).
    #[inline]
    pub unsafe fn state_mut(&self) -> &mut TeamArenaState {
        &mut *(self.base as *mut TeamArenaState)
    }

    /// Get pointer to the TeamBlock array base.
    #[inline]
    pub unsafe fn blocks(&self) -> *mut TeamBlock {
        self.base.sub(offsets::ARENA_TO_BLOCKS) as *mut TeamBlock
    }

    /// Get the team header (metadata) for a team.
    ///
    /// Returns `&block[team_idx+1].header.team`, which holds team metadata:
    /// worm_count, eliminated flag, weapon_alliance, team_name.
    #[inline]
    pub unsafe fn team_header(&self, team_idx: usize) -> &TeamHeader {
        &(*self.blocks().add(team_idx + 1)).header.team
    }

    /// Get a mutable team header for a team.
    #[inline]
    pub unsafe fn team_header_mut(&self, team_idx: usize) -> &mut TeamHeader {
        &mut (*self.blocks().add(team_idx + 1)).header.team
    }

    /// Get a playable worm entry by 1-indexed worm number (1..=8).
    ///
    /// Uses raw pointer arithmetic matching the original WA code:
    /// `base + team_idx * 0x51C + worm_num * 0x9C - 0x598`.
    /// This naturally crosses TeamBlock boundaries when worm_num = 8,
    /// since the 8th worm's early fields (state, health) spill into the
    /// next block's header slot. The header metadata lives at high offsets
    /// (0x6C+) that don't conflict with worm data (0x00-0x5F).
    #[inline]
    pub unsafe fn team_worm(&self, team_idx: usize, worm_num: usize) -> &WormEntry {
        let ptr = self
            .base
            .add(team_idx * 0x51C)
            .add(worm_num * 0x9C)
            .sub(0x598);
        &*(ptr as *const WormEntry)
    }

    /// Get a mutable reference to a specific worm entry.
    pub unsafe fn team_worm_mut(&self, team_idx: usize, worm_num: usize) -> &mut WormEntry {
        let ptr = self
            .base
            .add(team_idx * 0x51C)
            .add(worm_num * 0x9C)
            .sub(0x598);
        &mut *(ptr as *mut WormEntry)
    }

    /// Get a team's block and its header in one call.
    ///
    /// Returns `(block[team_idx], &block[team_idx+1].header.team)`.
    /// The block contains worm data, and the header (slot 0 of the next block)
    /// holds team metadata (worm_count, eliminated flag).
    ///
    /// **Note**: For accessing worms, prefer `team_worm()` which handles
    /// 8-worm teams correctly via cross-boundary pointer arithmetic.
    #[inline]
    pub unsafe fn team_and_header(&self, team_idx: usize) -> (&TeamBlock, &TeamHeader) {
        let blocks = self.blocks();
        let block = &*blocks.add(team_idx);
        let header = &(*blocks.add(team_idx + 1)).header.team;
        (block, header)
    }

    /// Get team header for Pattern B access (alliance/active_worm at +0x70/+0x74).
    ///
    /// Pattern B indexes from block[i+2] for 0-indexed team `i`:
    /// `base + 0x510 + i*0x51C` = `blocks[i+2].header.team + 0x70`.
    #[inline]
    pub unsafe fn team_header_b(&self, team_idx: usize) -> &TeamHeader {
        &(*self.blocks().add(team_idx + 2)).header.team
    }

    /// Get mutable team header for Pattern B access.
    #[inline]
    pub unsafe fn team_header_b_mut(&self, team_idx: usize) -> &mut TeamHeader {
        &mut (*(self.blocks() as *mut TeamBlock).add(team_idx + 2))
            .header
            .team
    }

    /// Compute the flat index for ammo/delay table access.
    ///
    /// Reads the weapon alliance ID from the team header for the given
    /// 1-indexed team, then computes `alliance_id * 142 + weapon_id`.
    ///
    /// The weapon_slots array is interleaved: per alliance, 71 ammo slots
    /// then 71 delay slots (stride 142 per alliance).
    /// Ammo: `weapon_slots[alliance_id * 142 + weapon_id]`
    /// Delay: `weapon_slots[alliance_id * 142 + 71 + weapon_id]`
    /// Returns (alliance_id, weapon_id) for accessing weapon slots.
    #[inline]
    pub unsafe fn weapon_slot_key(&self, team_index: usize, weapon_id: u32) -> (usize, usize) {
        let alliance_id = self.team_header(team_index).weapon_alliance as usize;
        (alliance_id, weapon_id as usize)
    }
}

// ── Snapshot impl ──────────────────────────────────────────

impl crate::snapshot::Snapshot for WormEntry {
    unsafe fn write_snapshot(&self, w: &mut dyn core::fmt::Write, indent: usize) -> core::fmt::Result {
        use crate::snapshot::write_indent;
        let i = indent;
        let name = core::ffi::CStr::from_ptr(self.name.as_ptr() as *const core::ffi::c_char)
            .to_string_lossy();
        write_indent(w, i)?;
        writeln!(w, "state=0x{:02X} active={} hp={}/{} name=\"{}\"",
            self.state, self.active_flag, self.health, self.max_health, name)?;
        Ok(())
    }
}
