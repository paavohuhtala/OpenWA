use openwa_core::fixed::Fixed;

use crate::FieldRegistry;
use crate::asset::gfx_dir::GfxDir;
use crate::audio::dssound::DSSound;
use crate::bitgrid::DisplayBitGrid;
use crate::engine::menu_panel::MenuPanel;
use crate::engine::net_bridge::NetBridge;
use crate::engine::world::GameWorld;
use crate::render::display::gfx::DisplayGfx;
use crate::render::landscape::Landscape;
use crate::render::palette::PaletteContext;
use crate::task::WorldRootEntity;

/// Speech name table entry size (0x40 = 64 bytes, null-terminated C string).
pub const SPEECH_NAME_ENTRY_SIZE: usize = 0x40;
/// Maximum number of speech name entries.
pub const SPEECH_NAME_TABLE_LEN: usize = 360;

#[openwa_game::vtable(size = 10, va = 0x0066A30C, class = "GameRuntime")]
pub struct GameRuntimeVtable {
    /// Scalar deleting destructor (0x5713C0).
    #[slot(0)]
    pub destructor: fn(this: *mut GameRuntime, flags: u32) -> *mut GameRuntime,
    /// Send-game-state hook (0x56FAF0 base = `DDNetGameWrapper__SendGameState`).
    /// Called by StepFrame's end-of-game headless log before draining the
    /// input queue: thiscall(this=wrapper, buf=render_buffer_a, 0, 0).
    /// In non-network wrappers the base impl is a no-op path; the net
    /// subclass overrides to flush queued network state.
    #[slot(2)]
    pub send_game_state: fn(this: *mut GameRuntime, buf: *mut u8, a: u32, b: u32),
    /// Render frame (0x56E040) — called each frame from `GameSession__ProcessFrame`.
    #[slot(7)]
    pub render_frame: fn(this: *mut GameRuntime),
    /// Get game state (0x528A20) — returns `self.game_state` (+0x484).
    /// See [`crate::engine::game_state`] for known return values.
    #[slot(9)]
    pub get_game_state: fn(this: *mut GameRuntime) -> u32,
}

bind_GameRuntimeVtable!(GameRuntime, vtable);

/// GameRuntime — large wrapper around GameWorld.
///
/// Created by GameRuntime__Constructor (0x56DEF0).
/// Holds the GameWorld pointer, graphics handlers, landscape, and display state.
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
pub struct GameRuntime {
    // ===== 0x000: Vtable =====
    /// 0x000: Vtable pointer (0x66A30C)
    pub vtable: *const GameRuntimeVtable,
    /// 0x004: Unknown (4 bytes gap)
    pub _unknown_004: u32,

    // ===== 0x008-0x050: Sub-object pointers (allocated by InitGameState) =====
    /// 0x008: WorldRootEntity (or GameStateEntity for online games — same vtable shape at slot 2).
    /// DispatchFrame/StepFrame broadcast game-end messages through `handle_message` (vtable slot 2).
    pub world_root: *mut WorldRootEntity,
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
    /// 0x02C: BitGrid layer D (8bpp 256x340) — render target for
    /// `menu_panel_a`. Doubles as the ESC-menu drawing canvas: vtable slots
    /// 1/2/5 (`fill_hline`/`fill_vline`/`put_pixel_clipped`) are dispatched
    /// against this layer during menu paint.
    pub display_gfx_d: *mut DisplayBitGrid,
    /// 0x030: Menu/viewport widget A (0x3D4 bytes). Allocated by
    /// `create_camera_object` paired with [`Self::display_gfx_d`]. Acts as
    /// the in-round game viewport (cursor = camera target) and, when the
    /// ESC menu is open, as the menu's item-list panel (cursor = active
    /// selection, items array at +0x30).
    pub menu_panel_a: *mut MenuPanel,
    /// 0x034: BitGrid layer E (8bpp 48x192) — render target for
    /// `menu_panel_b`.
    pub display_gfx_e: *mut DisplayBitGrid,
    /// 0x038: Menu/viewport widget B (0x3D4 bytes). Paired with
    /// [`Self::display_gfx_e`].
    pub menu_panel_b: *mut MenuPanel,
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

    // ===== 0x054-0x087: Per-team BaseEntity pointers =====
    /// 0x054: Per-team BaseEntity pointers (13 slots). Zeroed by InitGameState.
    /// Sub-fields +0x08..+0x18 cleared if non-null during InitTeamScoring.
    pub team_task_ptrs: [*mut u8; 13],

    // ===== 0x088-0x090: PaletteContext pointers =====
    /// 0x088: PaletteContext A (allocated 0x72C bytes)
    pub palette_ctx_a: *mut PaletteContext,
    /// 0x08C: PaletteContext B (allocated 0x72C bytes)
    pub palette_ctx_b: *mut PaletteContext,
    /// 0x090: PaletteContext C (allocated 0x72C bytes)
    pub palette_ctx_c: *mut PaletteContext,

    // ===== 0x094-0x0F3: Frame timing state (used by DispatchFrame) =====
    /// 0x094: Unknown gap
    pub _unknown_094: u32,
    /// 0x098: Reference timestamp (QPC ticks) — used for delta calculation.
    pub timing_ref: u64,
    /// 0x0A0: Last-frame timestamp (QPC ticks) — stored at end of DispatchFrame.
    pub last_frame_time: u64,
    /// 0x0A8: Frame accumulator A — paused frame time (QPC ticks).
    pub frame_accum_a: u64,
    /// 0x0B0: Frame accumulator B — running frame time (QPC ticks).
    pub frame_accum_b: u64,
    /// 0x0B8: Frame accumulator C — sub-frame remainder (QPC ticks).
    pub frame_accum_c: u64,
    /// 0x0C0: Initial reference timestamp (QPC ticks).
    pub initial_ref: u64,
    /// 0x0C8: Pause detection timestamp (QPC ticks).
    pub pause_detect: u64,
    /// 0x0D0: Secondary pause timestamp (QPC ticks).
    pub pause_secondary: u64,
    /// 0x0D8-0x0DF: Unknown
    pub _unknown_0d8: [u8; 8],
    /// 0x0E0: State flag (zeroed by InitGameState)
    pub _field_0e0: u32,
    /// 0x0E4: Unknown
    pub _field_0e4: u32,
    /// 0x0E8: Running step-count accumulator — DispatchFrame adds
    /// `(step_count - 1)` here each frame when StepFrame ran.
    pub step_count_accum: i32,
    /// 0x0EC: Frame delay counter — counts down during speed transitions, -1 = inactive
    pub frame_delay_counter: i32,
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
    /// 0x260: Network end-game handshake countdown — initialised to 500 by
    /// `init_game_state`; decremented every frame by the state-2/3 handlers
    /// (`OnNetworkEndAwaitPeers`, `OnGameState3`) while waiting for peers to
    /// converge on round-end. When it reaches zero the transition to
    /// `ROUND_ENDING` is forced regardless of peer state.
    pub net_end_countdown: i32,
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
    /// 0x278: Threshold value read by [`advance_frame_counters`] — slot C
    /// slews `_field_27c` toward `Fixed::ONE` when this exceeds 100, else
    /// toward `Fixed::ZERO`.
    pub _field_278: u32,
    /// 0x27C: Slew state C (Fixed 16.16). See [`advance_frame_counters`].
    pub _field_27c: Fixed,
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
    /// 0x3FC: Slew state A (Fixed 16.16). See [`advance_frame_counters`].
    pub _field_3fc: Fixed,
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
    pub ui_volume: Fixed,
    /// 0x41C: Unknown
    pub _unknown_41c: u32,
    /// 0x420: Live sound volume — the slider's "desired" value (Fixed,
    /// 0..1.0). Initialized at `init_game_state` from
    /// [`GameInfo::sound_volume_percent`] via the percent→Fixed
    /// conversion `(percent << 16) / 100`. While the ESC menu is open
    /// the volume slider's `render_ctx` points here, so dragging the
    /// slider writes a fresh `0..0x10000` value each frame.
    /// `ApplyVolumeSettings` (0x00534B40) reads this and propagates it
    /// downstream: copies it to [`Self::ui_volume`] (the "last applied"
    /// snapshot used for click/miss SFX), clamps it by the game-end
    /// fade and the engine-suspended flag, then pushes the result to
    /// `DSSound::SetMasterVolume` and `Music::SetVolume`.
    pub sound_volume: Fixed,
    /// 0x424-0x44F: State fields (mix of zeroes and values)
    pub esc_menu_anim: Fixed,
    pub esc_menu_anim_target: Fixed,
    pub confirm_anim: Fixed,
    pub confirm_anim_target: Fixed,
    /// 0x434: ESC-menu state machine.
    /// 0 = closed (bare HUD); 1 = open (built by `OpenEscMenu`, awaiting
    /// nav input); 2 = confirm / network-end-of-game flow (calls
    /// `BeginNetworkGameEnd`). Driven by `setup_frame_params` each
    /// headful frame.
    pub esc_menu_state: i32,
    /// 0x438: Sentinel -1
    pub _field_438: i32,
    /// 0x43C: Sentinel -1
    pub _field_43c: i32,
    /// 0x440: ESC-menu panel content width — written near the end of
    /// `GameRuntime::OpenEscMenu` from the hud_data_query (msg 0x7D3)
    /// response. Zero-init.
    pub menu_panel_width: i32,
    /// 0x444: ESC-menu panel content height — total y-offset accumulated
    /// during item layout in `GameRuntime::OpenEscMenu`, plus 2. Zero-init.
    pub menu_panel_height: i32,
    /// 0x448: Confirm-dialog panel width — copied from
    /// `display_gfx_e.width` by `OpenEscMenuConfirmDialog`. The
    /// `menu_panel_b` analogue of [`Self::menu_panel_width`]. Zero-init.
    pub confirm_panel_width: i32,
    /// 0x44C: Confirm-dialog panel height — `2 * line_height + 12` (two
    /// line_height rows for title + button row, plus 12 px of padding
    /// and border). Set by `OpenEscMenuConfirmDialog`. The `menu_panel_b`
    /// analogue of [`Self::menu_panel_height`]. Zero-init.
    pub confirm_panel_height: i32,
    /// 0x450-0x45B: Turn state fields (written by init_turn_state).
    ///
    /// `_field_450` is a Fixed countdown that [`advance_frame_counters`]
    /// decays by `advance_ratio` each frame (clamped at zero); `_field_454`
    /// is slew state B driven by the same fn.
    pub _field_450: Fixed,
    pub _field_454: Fixed,
    pub _field_458: u32,
    /// 0x45C: Timing jitter state — values 0/1/2, used by DispatchFrame pause detection
    pub timing_jitter_state: i32,
    /// 0x460: Zeroed
    pub _field_460: i32,
    /// 0x464: Render-scale fade request/direction. Tri-state i32 driving
    /// `step_render_scale_fade`: `< 0` fades `GameWorld::render_scale` toward
    /// `Fixed::ONE` (fade-in), `> 0` fades toward `Fixed::ZERO` (fade-out),
    /// `0` is idle. Latches back to 0 when the target is reached.
    pub render_scale_fade_request: i32,
    /// 0x468: Sentinel -1
    pub _field_468: i32,
    /// 0x46C: Sentinel -1
    pub _field_46c: i32,
    /// 0x470: Sentinel -1
    pub _field_470: i32,
    /// 0x474: Game end phase — 0=running, 1=ending. Set by DispatchFrame on game-over.
    pub game_end_phase: u32,
    /// 0x478: Zeroed
    pub _field_478: u32,
    /// 0x47C: Cleared on game end
    pub game_end_clear: u32,
    /// 0x480: Set to 0x10000 on game end
    pub game_end_speed: u32,
    /// 0x484: Main game-loop state. Read via vtable slot 9 each frame.
    /// See [`crate::engine::game_state`] for the constants and the
    /// transition graph (RUNNING → INITIALIZED → round-end path → EXIT).
    pub game_state: u32,

    // ===== 0x488-0x4BF: Core pointers and flags =====
    /// 0x488: Pointer to GameWorld allocation (DWORD index 0x122)
    pub world: *mut GameWorld,
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
    /// 0x4CC: Landscape object pointer (DWORD index 0x133)
    pub landscape: *mut Landscape,
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
    /// Used by GameRuntime__LoadSpeechWAV to deduplicate loaded WAVs.
    pub speech_name_table: [[u8; SPEECH_NAME_ENTRY_SIZE]; SPEECH_NAME_TABLE_LEN],
    /// 0x6EE8: Number of entries used in speech_name_table.
    pub speech_name_count: u32,
    /// 0x6EEC: Init 0 (DWORD index 0x1BBA)
    pub _field_6eec: u32,
    /// 0x6EF0-0x6F0F: Unknown trailing fields (not zeroed by constructor memset).
    pub _unknown_6ef0: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<GameRuntime>() == 0x6F10);
