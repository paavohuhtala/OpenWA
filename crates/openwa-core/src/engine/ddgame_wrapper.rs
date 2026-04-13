use crate::asset::gfx_dir::GfxDir;
use crate::audio::dssound::DSSound;
use crate::engine::ddgame::DDGame;
use crate::engine::net_bridge::NetBridge;
use crate::render::display::gfx::DisplayGfx;
use crate::render::landscape::PCLandscape;
use crate::render::palette::PaletteContext;
use crate::FieldRegistry;

/// Speech name table entry size (0x40 = 64 bytes, null-terminated C string).
pub const SPEECH_NAME_ENTRY_SIZE: usize = 0x40;
/// Maximum number of speech name entries.
pub const SPEECH_NAME_TABLE_LEN: usize = 360;

/// DDGameWrapper — large wrapper around DDGame.
///
/// Created by DDGameWrapper__Constructor (0x56DEF0).
/// Holds the DDGame pointer, graphics handlers, landscape, and display state.
///
/// Vtable: 0x66A30C
///
/// Note: Ghidra shows DWORD-indexed offsets (param_2[0x122] etc.).
/// Byte offset = dword_index * 4.
///
/// PARTIAL: Only confirmed fields are defined. Many fields in the
/// `_unknown_*` blobs are accessed via pointer arithmetic in game_state_init.rs
/// (team arrays at 0x054/0x128/0x22C, alliance data at 0x350/0x3AC, etc.).
#[derive(FieldRegistry)]
#[repr(C)]
pub struct DDGameWrapper {
    // ===== 0x000: Vtable =====
    /// 0x000: Vtable pointer (0x66A30C)
    pub vtable: *mut u8,
    /// 0x004: Unknown (4 bytes gap)
    pub _unknown_004: u32,

    // ===== 0x008-0x050: Sub-object pointers (allocated by InitGameState) =====
    /// 0x008: CTaskTurnGame or CTaskGameState pointer
    pub task_turn_game: *mut u8,
    /// 0x00C: Main serialization BufferObject
    pub main_buffer: *mut u8,
    /// 0x010: Unknown pointer
    pub _field_010: u32,
    /// 0x014: Render BufferObject B (capacity 0x10000)
    pub render_buffer_b: *mut u8,
    /// 0x018: DisplayGfx layer A
    pub display_gfx_a: *mut u8,
    /// 0x01C: DisplayGfx layer B
    pub display_gfx_b: *mut u8,
    /// 0x020: DisplayGfx layer C
    pub display_gfx_c: *mut u8,
    /// 0x024: DisplayGfx main (ConstructFull result)
    pub display_gfx_main: *mut u8,
    /// 0x028: RingBuffer B (capacity 0x2000)
    pub ring_buffer_b: *mut u8,
    /// 0x02C: DisplayGfx D
    pub display_gfx_d: *mut u8,
    /// 0x030: Camera object A (0x3D4 bytes)
    pub camera_a: *mut u8,
    /// 0x034: DisplayGfx E
    pub display_gfx_e: *mut u8,
    /// 0x038: Camera object B (0x3D4 bytes)
    pub camera_b: *mut u8,
    /// 0x03C: RingBuffer A (capacity 0x2000)
    pub ring_buffer_a: *mut u8,
    /// 0x040: Render BufferObject A (capacity 0x10000)
    pub render_buffer_a: *mut u8,
    /// 0x044: Network RingBuffer (conditional on online mode)
    pub network_ring_buffer: *mut u8,
    /// 0x048: State serialization BufferObject
    pub state_buffer: *mut u8,
    /// 0x04C: Statistics object (0xB94 bytes)
    pub statistics: *mut u8,
    /// 0x050: RingBuffer C (capacity 0x1000)
    pub ring_buffer_c: *mut u8,

    // ===== 0x054-0x087: Per-team CTask pointers =====
    /// 0x054: Per-team CTask pointers (13 slots). Zeroed by InitGameState.
    /// Sub-fields +0x08..+0x18 cleared if non-null during InitTeamScoring.
    pub team_task_ptrs: [*mut u8; 13],

    // ===== 0x088-0x090: PaletteContext pointers =====
    /// 0x088: PaletteContext A (allocated 0x72C bytes)
    pub palette_ctx_a: *mut PaletteContext,
    /// 0x08C: PaletteContext B (allocated 0x72C bytes)
    pub palette_ctx_b: *mut PaletteContext,
    /// 0x090: PaletteContext C (allocated 0x72C bytes)
    pub palette_ctx_c: *mut PaletteContext,

    // ===== 0x094-0x0F3: State fields =====
    /// 0x094: Unknown gap
    pub _unknown_094: u32,
    /// 0x098-0x0CF: State block (14 u32s zeroed by InitGameState)
    pub _zeroed_098: [u32; 14],
    /// 0x0D0-0x0DF: Unknown
    pub _unknown_0d0: [u8; 0x10],
    /// 0x0E0: State flag (zeroed by InitGameState)
    pub _field_0e0: u32,
    /// 0x0E4-0x0EB: Unknown
    pub _unknown_0e4: [u8; 8],
    /// 0x0EC: State flag
    pub _field_0ec: u32,
    /// 0x0F0: Init flag (set to 1 by InitGameState early)
    pub init_flag: u32,

    // ===== 0x0F4-0x25F: Team scoring arrays (7 parallel arrays of 13 u32s) =====
    /// 0x0F4: Team score array 0 — zeroed by InitTeamScoring.
    pub team_score_array_0: [u32; 13],
    /// 0x128: Starting team marker — 1 for starting team, 0 for others.
    pub team_starting_marker: [u32; 13],
    /// 0x15C: Team scoring A — initialized to scoring_param_a × 50.
    pub team_scoring_a: [u32; 13],
    /// 0x190: Team score array 3 — zeroed by InitTeamScoring.
    pub team_score_array_3: [u32; 13],
    /// 0x1C4: Team scoring B — initialized to scoring_param_b × 50.
    pub team_scoring_b: [u32; 13],
    /// 0x1F8: Team scoring C — initialized to scoring_param_b × 50.
    pub team_scoring_c: [u32; 13],
    /// 0x22C: Team activity flags — -1 (normal), -2 (training), 0 (inactive), 1 (starting).
    pub team_activity_flags: [u32; 13],

    // ===== 0x260-0x267: Game config =====
    /// 0x260: Health display precision (initialized to 500)
    pub health_precision: i32,
    /// 0x264: Zeroed by InitGameState
    pub _field_264: u32,

    // ===== 0x268-0x273: Sync checksums (already defined) =====
    /// 0x268: Network sync checksum A (written by GameFrameChecksumProcessor)
    pub sync_checksum_a: u32,
    /// 0x26C: Network sync checksum B (written by GameFrameChecksumProcessor)
    pub sync_checksum_b: u32,
    /// 0x270: Checksum validity flag (set to 1 after computation)
    pub checksum_valid: u32,

    // ===== 0x274-0x487: Game state fields =====
    /// 0x274: Initial state checksum (computed at end of InitGameState)
    pub initial_checksum: u32,
    /// 0x278-0x27F: Zeroed
    pub _field_278: u32,
    pub _field_27c: u32,
    /// 0x280-0x29C: Team render order indices (resolution-dependent, 8 entries)
    pub team_render_indices: [i32; 8],
    /// 0x2A0: Worm selection count
    pub worm_select_count: i32,
    /// 0x2A4: Worm selection count (alternate)
    pub worm_select_count_alt: i32,
    /// 0x2A8: Minimum number of active teams
    pub min_active_teams: i32,
    /// 0x2AC: Team count configuration (7 or 10)
    pub team_count_config: i32,
    /// 0x2B0: Maximum team render index (0xC or 0x10)
    pub max_team_render_index: i32,
    /// 0x2B4-0x2BB: Unknown.
    pub _unknown_2b4: [u8; 8],
    /// 0x2BC: Team score array 6 — set to 1 by InitTeamScoring.
    pub team_score_array_6: [u32; 13],
    /// 0x2F0-0x34F: Unknown.
    pub _unknown_2f0: [u8; 0x350 - 0x2F0],
    /// 0x350-0x383: Alliance bitmask arrays (13 × u32, accessed by init_alliance_data)
    pub _alliance_bitmasks: [u32; 13],
    /// 0x384: Screen offset (computed from screen height and team count)
    pub screen_offset: i32,
    /// 0x388: Screen height for HUD
    pub screen_height_hud: i32,
    /// 0x38C-0x3AB: Team-to-slot mapping (first half, 8 entries, filled with team indices)
    pub team_to_slot_a: [i32; 8],
    /// 0x3AC-0x3EB: Slot-to-team reverse mapping (16 entries, sentinel -1 init)
    pub slot_to_team: [i32; 16],
    /// 0x3EC: Last entry of sentinel block
    pub _field_3ec: i32,
    /// 0x3F0: Game mode flag (initialized to 1)
    pub game_mode_flag: i32,
    /// 0x3F4: Sentinel -1
    pub _field_3f4: i32,
    /// 0x3F8: Sentinel -1
    pub _field_3f8: i32,
    /// 0x3FC: Zeroed
    pub _field_3fc: i32,
    /// 0x400: Game turn timer (max_team_render_index << 5)
    pub turn_timer_max: i32,
    /// 0x404: Zeroed
    pub _field_404: i32,
    /// 0x408: Game turn timer copy
    pub turn_timer_current: i32,
    /// 0x40C-0x413: Zeroed
    pub _field_40c: i32,
    pub _field_410: i32,
    /// 0x414: Config bool (from game_info)
    pub _field_414: u32,
    /// 0x418: Zeroed
    pub _field_418: i32,
    /// 0x41C: Unknown
    pub _unknown_41c: u32,
    /// 0x420: Turn percentage (from game_info, fixed-point)
    pub turn_percentage: i32,
    /// 0x424-0x44F: State fields (mix of zeroes and values)
    pub _field_424: i32,
    pub _field_428: i32,
    pub _field_42c: i32,
    pub _field_430: i32,
    /// 0x434: Zeroed
    pub _field_434: i32,
    /// 0x438: Sentinel -1
    pub _field_438: i32,
    /// 0x43C: Sentinel -1
    pub _field_43c: i32,
    /// 0x440: Zeroed
    pub _field_440: i32,
    /// 0x444: Zeroed
    pub _field_444: i32,
    /// 0x448: Zeroed
    pub _field_448: i32,
    /// 0x44C: Zeroed
    pub _field_44c: i32,
    /// 0x450-0x45B: Turn state fields (written by init_turn_state)
    pub _field_450: u32,
    pub _field_454: u32,
    pub _field_458: u32,
    /// 0x45C: Zeroed
    pub _field_45c: i32,
    /// 0x460: Zeroed
    pub _field_460: i32,
    /// 0x464: Zeroed
    pub _field_464: i32,
    /// 0x468: Sentinel -1
    pub _field_468: i32,
    /// 0x46C: Sentinel -1
    pub _field_46c: i32,
    /// 0x470: Sentinel -1
    pub _field_470: i32,
    /// 0x474: Zeroed
    pub _field_474: u32,
    /// 0x478: Zeroed
    pub _field_478: u32,
    /// 0x47C: Unknown
    pub _unknown_47c: u32,
    /// 0x480: Zeroed
    pub _field_480: u32,
    /// 0x484: State initialized flag (set to 1 at end of InitGameState)
    pub state_initialized: u32,

    // ===== 0x488-0x4BF: Core pointers and flags =====
    /// 0x488: Pointer to DDGame allocation (DWORD index 0x122)
    pub ddgame: *mut DDGame,
    /// 0x48C: Network bridge object (0x2C bytes). Only set for online games (game_version == -2).
    pub net_bridge: *mut NetBridge,
    /// 0x490: Replay flag A (from game_info)
    pub replay_flag_a: u8,
    /// 0x491: Replay flag B (from game_info)
    pub replay_flag_b: u8,
    /// 0x492: Unknown byte
    pub _unknown_492: u8,
    /// 0x493: Zeroed by InitGameState
    pub _field_493: u8,
    /// 0x494: Zeroed by InitGameState
    pub _field_494: u32,
    /// 0x498: Zeroed by InitGameState
    pub _field_498: u32,
    /// 0x49C: Version counter (read by InitGameState, not written)
    pub _field_49c: u32,
    /// 0x4A0-0x4AB: Unknown
    pub _unknown_4a0: [u8; 0x0C],
    /// 0x4AC: State field
    pub _field_4ac: u32,
    /// 0x4B0: Sentinel -1
    pub _field_4b0: u32,
    /// 0x4B4-0x4BF: Unknown
    pub _unknown_4b4: [u8; 0x0C],

    // ===== 0x4C0-0x4E3: Graphics/display/sound pointers =====
    /// 0x4C0: Primary GfxDir — main sprite archive (Gfx.dir / Gfx0.dir / Gfx1.dir).
    pub primary_gfx_dir: *mut GfxDir,
    /// 0x4C4: Secondary GfxDir — supplemental sprites (GfxC_3_0.dir), conditional on game version.
    pub secondary_gfx_dir: *mut GfxDir,
    /// 0x4C8: Graphics mode flag (DWORD index 0x132)
    pub gfx_mode: u32,
    /// 0x4CC: PCLandscape object pointer (DWORD index 0x133)
    pub landscape: *mut PCLandscape,
    /// 0x4D0: DisplayGfx pointer (param2 of constructor)
    pub display: *mut DisplayGfx,
    /// 0x4D4: DSSound pointer (param3 of constructor)
    pub sound: *mut DSSound,
    /// 0x4D8: Loading progress counter (incremented per loading tick).
    pub loading_progress: u32,
    /// 0x4DC: Loading progress total (base 0x2AD + 0x38 per team + 0x7E overhead).
    pub loading_total: u32,
    /// 0x4E0: Last displayed loading percentage (init -100 to force first update).
    pub loading_last_pct: u32,

    // ===== 0x4E4-0x6F0F: Remaining fields =====
    /// 0x4E4-0x14E7: Unknown fields
    pub _unknown_4e4: [u8; 0x14E8 - 0x4E4],
    /// 0x14E8: Speech name table — 360 entries of 0x40-byte C strings.
    /// Used by DDGameWrapper__LoadSpeechWAV to deduplicate loaded WAVs.
    pub speech_name_table: [[u8; SPEECH_NAME_ENTRY_SIZE]; SPEECH_NAME_TABLE_LEN],
    /// 0x6EE8: Number of entries used in speech_name_table.
    pub speech_name_count: u32,
    /// 0x6EEC: Init 0 (DWORD index 0x1BBA)
    pub _field_6eec: u32,
    /// 0x6EF0-0x6F0F: Unknown trailing fields (not zeroed by constructor memset).
    pub _unknown_6ef0: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<DDGameWrapper>() == 0x6F10);
