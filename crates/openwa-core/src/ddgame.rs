use crate::active_sound::ActiveSoundTable;
use crate::dddisplay::DDDisplay;
use crate::ddkeyboard::DDKeyboard;
use crate::dssound::DSSound;
use crate::game_info::GameInfo;
use crate::landscape::PCLandscape;
use crate::music::Music;
use crate::palette::Palette;
use crate::render::RenderQueue;
use crate::speech::SpeechSlotTable;
use crate::turn_order::TurnOrderWidget;

/// DDGame — the main game engine object.
///
/// This is a massive ~39KB struct (0x98B8 bytes) that owns all major subsystems:
/// display, landscape, sound, graphics handlers, and task state machines.
///
/// Allocated in DDGame__Constructor (0x56E220).
/// The DDGame pointer is stored at DDGameWrapper+0x488 (DWORD index 0x122).
///
/// PARTIAL: Fields up to 0x54C are densely mapped from the constructor.
/// Beyond that, only scattered fields are known — use the `offsets` module.
///
/// Note on offsets: The constructor accesses DDGame fields via
/// `*(param_2[0x122] + byte_offset)` — these are byte offsets, NOT DWORD-indexed.
/// DWORD indexing only applies to param_2 (DDGameWrapper) itself.
#[repr(C)]
pub struct DDGame {
    /// 0x000: DDKeyboard pointer (vtable 0x66AEC8). Constructor param "keyboard".
    pub keyboard: *mut DDKeyboard,
    /// 0x004: DDDisplay pointer (vtable 0x66A218). Constructor param "display".
    pub display: *mut DDDisplay,
    /// 0x008: DSSound pointer (vtable 0x66AF20). Constructor param "sound".
    /// Null means sound is disabled (checked by PlaySoundGlobal).
    pub sound: *mut DSSound,
    /// 0x00C: Active sound position table (0x608 bytes, conditional on sound available).
    /// Tracks up to 64 positional sounds with world coordinates for 3D audio mixing.
    /// NULL when `game_info+0xF914 != 0` (headless/server) or `sound == NULL`.
    pub active_sounds: *mut ActiveSoundTable,
    /// 0x010: Palette object pointer (vtable 0x66A2E4). Constructor param "palette".
    pub palette: *mut Palette,
    /// 0x014: Music object pointer (vtable 0x66B3E0). Constructor param "music".
    pub music: *mut Music,
    /// 0x018: Constructor param7 (unknown purpose, contains 0x1F4 at runtime).
    pub _param_018: *mut u8,
    /// 0x01C: Caller/parent pointer (ECX from constructor, often NULL).
    pub _caller: *mut u8,
    /// 0x020: PCLandscape pointer (copied from DDGameWrapper[0x133])
    pub landscape: *mut PCLandscape,
    /// 0x024: GameInfo pointer (passed as param_10 to constructor).
    pub game_info: *mut GameInfo,
    /// 0x028: param_9
    pub _param_028: *mut u8,
    /// 0x02C: Secondary GfxDir object (0x70C bytes, conditional on GfxHandler 1)
    pub secondary_gfxdir: *mut u8,
    /// 0x030: Gradient image pointer
    pub gradient_image: *mut u8,
    /// 0x034: Gradient image 2 pointer
    pub gradient_image_2: *mut u8,
    /// 0x038-0x0B4: Arrow sprite object pointers (32 entries)
    pub arrow_sprites: [*mut u8; 32],
    /// 0x0B8-0x134: Arrow GfxDir pointers (32 entries, conditional)
    pub arrow_gfxdirs: [*mut u8; 32],
    /// 0x138: DisplayGfx object pointer (vtable 0x664144)
    pub display_gfx: *mut u8,
    /// 0x13C-0x37F: Sprite/image object cache (145 pointer slots).
    /// All populated entries have vtable 0x664144 (same class as `display_gfx`).
    /// Not initialized in DDGame__Constructor — filled during gameplay with
    /// weapon sprites, effect images, cursor graphics, etc.
    pub sprite_cache: [*mut u8; 145],
    /// 0x380: TaskStateMachine pointer (vtable 0x664118, 0x2C bytes)
    pub task_state_machine: *mut u8,
    /// 0x384-0x467: Additional sprite/image object slots.
    /// Same vtable 0x664144 as sprite_cache. ~20 entries populated at runtime.
    pub sprite_cache_2: [*mut u8; 57],
    /// 0x468: Landscape-derived value
    pub _landscape_val: *mut u8,
    /// 0x46C-0x488: 8 SpriteRegion pointers (0x9C bytes each, vtable 0x66B268)
    /// Created by SpriteRegion__Constructor (0x57DB20).
    /// Each contains 32 TaskStateMachine sub-objects.
    pub sprite_regions: [*mut u8; 8],
    /// 0x48C-0x508: Arrow collision region pointers (32 entries)
    pub arrow_collision_regions: [*mut u8; 32],
    /// 0x50C: Coordinate list object (capacity 600, 0x12C0 data buffer)
    pub coord_list: *mut u8,
    /// 0x510: Weapon table pointer
    pub weapon_table: *mut u8,
    /// 0x514: Unknown pointer (populated at runtime)
    pub _unknown_514: *mut u8,
    /// 0x518: Unknown pointer (populated at runtime)
    pub _unknown_518: *mut u8,
    /// 0x51C: Unknown pointer (populated at runtime)
    pub _unknown_51c: *mut u8,
    /// 0x520: Unknown (zero in runtime dump)
    pub _unknown_520: u32,
    /// 0x524: RenderQueue pointer (passed as `this` to all Draw* functions)
    pub render_queue: *mut RenderQueue,
    /// 0x528: Game state stream object (vtable 0x664194, vt[0]=0x4FB5C0).
    /// Created in DDGame_InitGameState_Maybe (0x526690), constructor 0x4FB5F0.
    /// Reads from replay/packet data stream.
    pub game_state_stream: *mut u8,
    /// 0x52C: Unknown pointer
    pub _unknown_52c: *mut u8,
    /// 0x530: Turn order widget (vtable 0x66A088, vt[0]=0x563E90).
    /// Constructor 0x563D40. UI component that renders team banners with
    /// animated sliding transitions (sin-table interpolation). Groups teams
    /// by alliance, creates per-team entries with textbox + DisplayGfx.
    pub turn_order_widget: *mut TurnOrderWidget,
    /// 0x534: HUD panel object (vtable 0x664698, vt[0]=0x5241F0).
    /// Constructor 0x524070. 104×28 px, 3 DisplayGfx layers, 2296-byte LUT.
    pub hud_panel: *mut u8,
    /// 0x538-0x53F: Unknown (zero in runtime dump)
    pub _unknown_538: [u8; 8],
    /// 0x540: Unknown pointer
    pub _unknown_540: *mut u8,
    /// 0x544: Unknown pointer (*=0x1BC at runtime)
    pub _unknown_544: *mut u8,
    /// 0x548: Weapon panel pointer
    pub weapon_panel: *mut u8,
    /// 0x54C: CTaskLand pointer (set by CTaskLand__InitLandscape at 0x5056F0).
    /// The landscape/terrain task. Vtable at 0x664388.
    pub task_land: *mut u8,
    /// 0x550-0x25FF: Large unverified region.
    ///
    /// Runtime observations (not yet linked to code):
    /// - 0x5C4: value matches code address 0x5755D0 (fixed-point normalize fn)
    /// - 0x5C8-0x5FF: small config-like values (2048, 150, 3000, 696, 896, 100, 300)
    /// - 0x600-0x25FF: identity permutation [0,1,2,...,~2048] — purpose unknown
    pub _unknown_550: [u8; 0x2600 - 0x550],

    /// 0x2600-0x2DFF: Block of 0xFFFFFFFF values at runtime (512 i32 entries).
    /// May be unused slots in a parallel table to the 0x600 permutation.
    pub _unknown_2600: [u8; 0x2E00 - 0x2600],

    /// 0x2E00-0x45EB: Unknown (mostly zero at runtime)
    ///
    /// Contains FUN_00526120 zeroed offsets at stride 0x194:
    /// 0x379C, 0x3930, 0x3AC4, 0x3C58, 0x3DEC, 0x3F80, 0x4114, 0x42A8, 0x443C, 0x45D0
    pub _unknown_2e00: [u8; 0x45EC - 0x2E00],

    /// 0x45EC: Unknown (0xA307A169 at runtime — not part of team scale arrays).
    pub _unknown_45ec: u32,
    /// 0x45F0-0x4607: Per-team health ratio (Fixed-point, 6 entries, 1-indexed).
    /// 0x10000 = 100% health. Rendered as bar width: `value * 100 >> 16 + 4` pixels.
    /// Read by TurnOrderTeamEntry render method (0x563620).
    /// Initialized to 0x10000 (1.0) by TurnOrderTeamEntry constructor (0x5630B0).
    pub team_health_ratio: [i32; 6],
    /// 0x4608-0x461F: Per-team health ratio 2 (Fixed-point, 6 entries, 1-indexed).
    /// Initialized to 0x10000 (1.0). Not read by the render method — may be
    /// target/previous value for interpolation, or used by update logic.
    pub team_health_ratio_2: [i32; 6],
    /// 0x4620-0x64D7: Unknown
    ///
    /// Known landmarks:
    /// - 0x64D8: cleared by init
    pub _unknown_4620: [u8; 0x64D8 - 0x4620],
    /// 0x64D8: Cleared by init.
    pub _field_64d8: u32,
    /// 0x64DC-0x72A3: Unknown
    pub _unknown_64dc: [u8; 0x72A4 - 0x64DC],
    /// 0x72A4: Cleared by init.
    pub _field_72a4: u32,
    /// 0x72A8-0x72D7: Unknown
    pub _unknown_72a8: [u8; 0x72D8 - 0x72A8],

    /// 0x72D8: Game speed multiplier (Fixed-point, 0x10000 = 1.0x).
    pub game_speed: i32,
    /// 0x72DC: Game speed target (Fixed-point, 0x10000 = 1.0x).
    pub game_speed_target: i32,
    /// 0x72E0-0x72EB: Unknown
    pub _unknown_72e0: [u8; 0x72EC - 0x72E0],
    /// 0x72EC: RNG state word 1 (changes every frame).
    pub rng_state_1: u32,
    /// 0x72F0: RNG state word 2 (changes every frame).
    pub rng_state_2: u32,
    /// 0x72F4-0x730B: Unknown
    pub _unknown_72f4: [u8; 0x730C - 0x72F4],
    /// 0x730C-0x731C: 5 GfxDir color entries
    pub _gfx_color_entries: [u8; 0x7324 - 0x730C],

    /// 0x7324: Crosshair line color param (DrawPolygon param_2).
    /// Part of GfxDir color entries at 0x730C.
    pub crosshair_line_color: u32,
    /// 0x7328: Unknown (between crosshair params)
    pub _unknown_7328: [u8; 4],
    /// 0x732C: Crosshair line style param (DrawPolygon param_1).
    pub crosshair_line_style: u32,
    /// 0x7330-0x7337: Unknown
    pub _unknown_7330: [u8; 8],
    /// 0x7338: Fill pixel value
    pub fill_pixel: u32,
    /// 0x733C-0x737F: Unknown
    pub _unknown_733c: [u8; 0x7380 - 0x733C],

    /// 0x7380: Viewport width (Fixed-point, e.g. 960.0 = 0x03C00000).
    pub viewport_width: i32,
    /// 0x7384: Viewport height (Fixed-point, e.g. 348.0 = 0x015C0000).
    pub viewport_height: i32,
    /// 0x7388: Viewport width max/duplicate (Fixed-point).
    pub viewport_width_2: i32,
    /// 0x738C: Viewport height max/duplicate (Fixed-point).
    pub viewport_height_2: i32,
    /// 0x7390-0x739F: Unknown
    pub _unknown_7390: [u8; 0x73A0 - 0x7390],

    /// 0x73A0: Camera X position (Fixed-point, e.g. 393.0).
    pub camera_x: i32,
    /// 0x73A4: Camera Y position (Fixed-point, e.g. 532.0).
    pub camera_y: i32,
    /// 0x73A8: Camera target X (Fixed-point, duplicate/interpolation target).
    pub camera_target_x: i32,
    /// 0x73AC: Camera target Y (Fixed-point).
    pub camera_target_y: i32,

    /// 0x73B0-0x764F: Unknown
    pub _unknown_73b0: [u8; 0x7650 - 0x73B0],

    /// 0x7650-0x768F: Team index mapping table 1.
    /// Packed u16 pairs: [0,1], [2,3], ..., [14,15]. Team-to-slot or turn order.
    pub team_index_map_1: [u8; 0x7690 - 0x7650],
    /// 0x7690-0x76AF: Unknown
    pub _unknown_7690: [u8; 0x76B0 - 0x7690],
    /// 0x76B0-0x76EF: Team index mapping table 2 (same pattern).
    pub team_index_map_2: [u8; 0x76F0 - 0x76B0],
    /// 0x76F0-0x7717: Unknown
    pub _unknown_76f0: [u8; 0x7718 - 0x76F0],
    /// 0x7718-0x7757: Team index mapping table 3 (similar pattern, slightly different end).
    pub team_index_map_3: [u8; 0x7758 - 0x7718],
    /// 0x7758-0x779B: Unknown
    pub _unknown_7758: [u8; 0x779C - 0x7758],

    /// 0x779C: Level bound min X (Fixed-point, negative = off-screen left).
    pub level_bound_min_x: i32,
    /// 0x77A0: Level bound max X (Fixed-point).
    pub level_bound_max_x: i32,
    /// 0x77A4: Level bound min Y (Fixed-point, same as min_x typically).
    pub level_bound_min_y: i32,
    /// 0x77A8: Level bound max Y (Fixed-point).
    pub level_bound_max_y: i32,
    /// 0x77AC-0x77BF: Unknown
    pub _unknown_77ac: [u8; 0x77C0 - 0x77AC],

    /// 0x77C0: Level width in pixels (set by PCLandscape constructor).
    pub level_width: u32,
    /// 0x77C4: Level height in pixels (set by PCLandscape constructor).
    pub level_height: u32,
    /// 0x77C8: Total pixels (width × height).
    pub level_total_pixels: u32,

    /// 0x77CC-0x77E3: Unknown
    pub _unknown_77cc: [u8; 0x77E4 - 0x77CC],

    /// 0x77E4: Speech slot table. Maps (team, speech_line_id) → DSSound buffer index.
    /// Cleared by DSSound_LoadAllSpeechBanks (0x571A70), filled by DSSound_LoadSpeechWAV (0x571530).
    pub speech_slot_table: SpeechSlotTable,

    /// 0x7D84-0x7E9F: Unknown
    pub _unknown_7d84: [u8; 0x7EA0 - 0x7D84],

    /// 0x7EA0: Unknown flag/counter (value 4 at runtime — team count?)
    pub _field_7ea0: u32,
    /// 0x7EA4: Unknown
    pub _unknown_7ea4: u32,
    /// 0x7EA8: Turn time limit in seconds (150 = 2:30 default).
    pub turn_time_limit: u32,
    /// 0x7EAC-0x7ECF: Unknown
    pub _unknown_7eac: [u8; 0x7ED0 - 0x7EAC],
    /// 0x7ED0-0x7EEF: Unknown
    pub _unknown_7ed0: [u8; 0x7EF0 - 0x7ED0],
    /// 0x7EF0: Unknown flag (-1 = 0xFFFFFFFF at runtime)
    pub _field_7ef0: i32,
    /// 0x7EF4: Unknown
    pub _unknown_7ef4: u32,
    /// 0x7EF8: Sound available flag (1 when game_info+0xF914 == 0, i.e. not headless).
    pub sound_available: u32,
    /// 0x7EFC: Always initialized to 1 in constructor.
    pub _field_7efc: u32,

    // === Sound queue (0x7F00-0x8143) ===
    /// 0x7F00: Sound queue (16 entries, stride 0x24). Appended by PlaySoundGlobal.
    pub sound_queue: [SoundQueueEntry; 16],
    /// 0x8140: Number of entries currently in the sound queue (0–16).
    pub sound_queue_count: i32,

    /// 0x8144-0x814F: Unknown
    pub _unknown_8144: [u8; 0x8150 - 0x8144],
    /// 0x8150: Scale factor used by DrawCrosshairLine (multiplied by 0x140000).
    pub crosshair_scale: i32,

    /// 0x8154-0x818B: Unknown
    pub _unknown_8154: [u8; 0x818C - 0x8154],

    /// 0x818C: Turn status text buffer (null-terminated ASCII).
    /// Shows on screen during gameplay, e.g. "Cheesy harkitseee siirtoaan"
    /// ("Cheesy is considering their move" in Finnish).
    pub turn_status_text: [u8; 64],

    /// 0x81CC-0x8CBB: Unknown
    ///
    /// Known landmarks:
    /// - 0x8174: value 0x3FC (1020) at runtime
    pub _unknown_81cc: [u8; 0x8CBC - 0x81CC],

    /// 0x8CBC-0x8CF8: 4× coordinate entries (0x10-byte stride).
    /// InitFields zeroes +0 and +4 of each. At runtime contains fixed-point
    /// screen coordinates (e.g. 393.0, 532.0, 960.0, 348.0).
    pub coord_entries_8cbc: [u8; 0x8CFC - 0x8CBC],

    /// 0x8CFC-0x984F: Unknown
    pub _unknown_8cfc: [u8; 0x9850 - 0x8CFC],

    /// 0x9850-0x988F: 4× coordinate entries (0x10-byte stride, zeroed by InitFields).
    pub coord_entries_9850: [u8; 0x9890 - 0x9850],

    /// 0x9890-0x98A3: Flags
    pub _unknown_9890: [u8; 0x98A4 - 0x9890],
    /// 0x98A4: Checkpoint active flag.
    pub checkpoint_active: u32,
    /// 0x98A8: Unknown
    pub _unknown_98a8: u32,
    /// 0x98AC: Fast-forward request flag.
    pub fast_forward_request: u32,

    /// 0x98B0: Fast-forward active flag.
    /// When set to 1, FUN_005307A0 processes up to 50 game frames per render
    /// cycle. Sound is suppressed and rendering is skipped. Cleared at turn
    /// boundaries (FUN_00534540, FUN_0055BDD0), so must be re-set continuously.
    /// This is the same flag toggled by spacebar (key 0x35) during replay.
    pub fast_forward_active: u32,

    /// 0x98B4-0x98B7: Unknown
    pub _unknown_98b4: [u8; 0x98B8 - 0x98B4],
}

const _: () = assert!(core::mem::size_of::<DDGame>() == 0x98B8);

/// Well-known byte offsets into DDGame, for use with raw pointer access.
///
/// The DDGame pointer is at DDGameWrapper+0x488 (DWORD index 0x122).
pub mod offsets {
    // === Header / init params (0x000-0x02C) ===
    pub const LANDSCAPE: usize = 0x020;
    pub const GAME_INFO: usize = 0x024;
    pub const SECONDARY_GFXDIR: usize = 0x02C;
    pub const GRADIENT_IMAGE: usize = 0x030;

    // === Sprite arrays (0x038-0x138) ===
    pub const ARROW_SPRITES: usize = 0x038;
    pub const ARROW_GFXDIRS: usize = 0x0B8;
    pub const DISPLAY_GFX: usize = 0x138;

    // === Task/state machines (0x380-0x488) ===
    pub const TASK_STATE_MACHINE: usize = 0x380;
    pub const SPRITE_REGIONS: usize = 0x46C;

    // === Arrow collision (0x48C-0x50C) ===
    pub const ARROW_COLLISION_REGIONS: usize = 0x48C;
    pub const COORD_LIST: usize = 0x50C;

    // === WormKit-documented offsets ===
    pub const WEAPON_TABLE: usize = 0x510;
    pub const WEAPON_PANEL: usize = 0x548;

    // === Team weapon state (DDGame + 0x4628) ===
    /// Base of TeamArenaState sub-struct within DDGame.
    /// Callers pass DDGame + TEAM_ARENA_STATE as base pointer to
    /// GetAmmo/AddAmmo/SubtractAmmo.
    pub const TEAM_ARENA_STATE: usize = 0x4628;

    // === Team block array (7 × FullTeamBlock, stride 0x51C) ===
    /// Start of team block array within DDGame (7 blocks, stride 0x51C).
    /// Derived: entry_ptr(team=0) - 0x598 = 0x4628 - 0x598 = 0x4090.
    /// Runtime-confirmed: block[0] is zeroed preamble, blocks[1-6] hold team data.
    pub const TEAM_BLOCKS: usize = 0x4090;

    /// Byte offset from TeamArenaState base back to FullTeamBlock array start.
    /// `blocks_ptr = (tws_base as *const u8).sub(ARENA_TO_BLOCKS) as *const FullTeamBlock`
    ///
    /// entry_ptr(0) = DDGame+0x4628 = TEAM_BLOCKS + 0x598.
    /// 0x598 = sizeof(FullTeamBlock) + 0x7C = one block + offset into TeamBlockHeader.
    pub const ARENA_TO_BLOCKS: usize = 0x598;

    // === FUN_00526120 init offsets (stride 0x194, 10 entries) ===
    pub const INIT_TABLE_BASE: usize = 0x379C;
    pub const INIT_TABLE_STRIDE: usize = 0x194;

    // === Game objects (0x528-0x54C) ===
    pub const GAME_STATE_STREAM: usize = 0x528;
    pub const TURN_ORDER_WIDGET: usize = 0x530;
    pub const HUD_PANEL: usize = 0x534;
    /// CTaskLand pointer (landscape/terrain task, vtable 0x664388).
    pub const TASK_LAND: usize = 0x54C;

    // === Per-team health ratio (turn order health bar) ===
    /// Per-team health ratio array (6 × i32, 1-indexed by team).
    /// 0x10000 = 100%. Rendered as `value * 100 >> 16` pixel width.
    pub const TEAM_HEALTH_RATIO: usize = 0x45F0;
    /// Per-team health ratio 2 (6 × i32, 1-indexed by team).
    pub const TEAM_HEALTH_RATIO_2: usize = 0x4608;

    // === RNG (0x72EC) ===
    pub const RNG_STATE_1: usize = 0x72EC;
    pub const RNG_STATE_2: usize = 0x72F0;

    // === Sparse fields in upper region ===
    pub const GFX_COLOR_ENTRIES: usize = 0x730C;

    // === Camera/viewport (0x7380-0x73AC) ===
    pub const VIEWPORT_WIDTH: usize = 0x7380;
    pub const VIEWPORT_HEIGHT: usize = 0x7384;
    pub const CAMERA_X: usize = 0x73A0;
    pub const CAMERA_Y: usize = 0x73A4;
    pub const CAMERA_TARGET_X: usize = 0x73A8;
    pub const CAMERA_TARGET_Y: usize = 0x73AC;

    // === Game speed (0x72D8) ===
    pub const GAME_SPEED: usize = 0x72D8;
    pub const GAME_SPEED_TARGET: usize = 0x72DC;

    // === Level bounds (0x779C-0x77A8) ===
    pub const LEVEL_BOUND_MIN_X: usize = 0x779C;
    pub const LEVEL_BOUND_MAX_X: usize = 0x77A0;
    pub const LEVEL_BOUND_MIN_Y: usize = 0x77A4;
    pub const LEVEL_BOUND_MAX_Y: usize = 0x77A8;
    pub const TURN_TIME_LIMIT: usize = 0x7EA8;
    pub const SOUND_AVAILABLE: usize = 0x7EF8;
    /// Scale factor used by DrawCrosshairLine (multiplied by 0x140000).
    pub const CROSSHAIR_SCALE: usize = 0x8150;
    /// Turn status text (null-terminated ASCII, shown during gameplay).
    pub const TURN_STATUS_TEXT: usize = 0x818C;
    /// Checkpoint active flag.
    pub const CHECKPOINT_ACTIVE: usize = 0x98A4;
    /// Fast-forward request flag.
    pub const FAST_FORWARD_REQUEST: usize = 0x98AC;

    // === Speech slot table (DDGame + 0x77E4) ===
    /// Speech slot table: maps (team, speech_line_id) → DSSound buffer index.
    pub const SPEECH_SLOT_TABLE: usize = 0x77E4;

    // === Fast-forward (DDGame + 0x98B0) ===
    /// Fast-forward active flag (u32, 1 = active).
    pub const FAST_FORWARD_ACTIVE: usize = 0x98B0;

    // === Sound queue (DDGame + 0x7F00) ===
    /// DSSound pointer (null = sound disabled).
    pub const SOUND: usize = 0x0008;
    /// Sound queue base (16 × SoundQueueEntry, stride 0x24).
    pub const SOUND_QUEUE: usize = 0x7F00;
    /// Sound queue count (i32, 0–16).
    pub const SOUND_QUEUE_COUNT: usize = 0x8140;
}

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
// Team arena state — sub-struct at DDGame + 0x4628
// ============================================================


// ============================================================
// Per-worm and per-team block structs
// ============================================================

/// Per-worm data entry (0x9C bytes, stride between consecutive worms).
///
/// WA supports up to 8 playable worms per team. The original code accesses
/// worms via raw pointer arithmetic from the team entry pointer, using
/// stride 0x9C. This means the 8th worm crosses the FullTeamBlock boundary
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
    /// 0x04-0x0B: Unknown
    pub _unknown_04: [u8; 8],
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
    /// 0x90-0x9B: Unknown (zeroed in runtime dump for playable worms).
    /// GetWormPosition reads pos_x/pos_y from +0x90/+0x94 via negative entry_ptr
    /// arithmetic, but values appear transient — not populated at rest.
    /// Actual worm positions live in CGameTask objects (+0x84/+0x88).
    pub _unknown_90: [u8; 0x0C],
}

const _: () = assert!(core::mem::size_of::<WormEntry>() == 0x9C);

/// Team-level metadata stored at slot 0 of each FullTeamBlock (0x9C bytes).
///
/// This struct overlays the same memory as a WormEntry but interprets the
/// high offsets (0x6C+) as team metadata rather than worm data. The low
/// offsets (0x00-0x5F) may contain data from the previous team's 8th worm
/// when that team has 8 worms — they are treated as opaque padding.
///
/// Accessed via `TeamArenaRef::team_header()` and `team_header_b()`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TeamBlockHeader {
    /// 0x00-0x5F: Opaque — may hold 8th worm data from previous team.
    pub _worm_overlap: [u8; 0x60],
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
    /// 0x7C: Unknown.
    pub _unknown_7c: [u8; 4],
    /// 0x80: Alliance ID for ammo/delay table indexing (GetAmmo/AddAmmo/SubtractAmmo).
    /// Teams with the same weapon_alliance share ammo pools. Distinct from
    /// `alliance` at 0x70 which is used by CountTeamsByAlliance.
    pub weapon_alliance: i32,
    /// 0x84: Team name, null-terminated ASCII string.
    pub team_name: [u8; 0x14],
    /// 0x98: Unknown trailing bytes.
    pub _unknown_98: [u8; 4],
}

const _: () = assert!(core::mem::size_of::<TeamBlockHeader>() == 0x9C);

/// Union for slot 0 of a FullTeamBlock.
///
/// This slot is dual-purpose: its high offsets store team metadata
/// (`TeamBlockHeader`), while its low offsets may contain the 8th worm
/// of the previous team (`WormEntry`). The two uses don't conflict
/// because worm data occupies 0x00-0x5F and header data starts at 0x6C.
#[repr(C)]
#[derive(Clone, Copy)]
pub union TeamBlockSlot0 {
    /// View as worm data (used when the previous team has 8 worms).
    pub worm: core::mem::ManuallyDrop<WormEntry>,
    /// View as team metadata (eliminated, alliance, worm_count, etc.).
    pub team: core::mem::ManuallyDrop<TeamBlockHeader>,
}

/// Full per-team data block (0x51C bytes, 7 blocks in DDGame).
///
/// Each block starts with a `TeamBlockSlot0` union (0x9C bytes) that serves
/// dual purpose: its high offsets hold team metadata (`TeamBlockHeader`),
/// while its low offsets may contain the 8th worm of the previous team.
/// The remaining 7 worm slots follow, then 0x3C bytes of metadata.
///
/// **Block indexing**: Block 0 is unused (preamble, all zeros). Actual team
/// data starts at block 1. Worms are accessed via `TeamArenaRef::team_worm()`
/// which uses raw pointer arithmetic and naturally crosses block boundaries.
///
/// **Header access**: Use `TeamArenaRef::team_header(idx)` to get the
/// `TeamBlockHeader` for a team (reads from `blocks[idx+1].header.team`).
#[repr(C)]
pub struct FullTeamBlock {
    /// 0x000-0x09B: Header slot (union of TeamBlockHeader and WormEntry).
    /// Team metadata at high offsets; may hold 8th worm data at low offsets.
    pub header: TeamBlockSlot0,
    /// 0x09C-0x4DF: 7 worm entries (slots 1-7, stride 0x9C)
    pub worms: [WormEntry; 7],
    /// 0x4E0-0x51B: Team metadata (0x3C bytes)
    pub _metadata: [u8; 0x3C],
}

const _: () = assert!(core::mem::size_of::<FullTeamBlock>() == 0x51C);

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
/// `weapon_slots[alliance * 142 + weapon]` for ammo and
/// `weapon_slots[alliance * 142 + 71 + weapon]` for delay.
#[repr(C)]
pub struct TeamArenaState {
    /// 0x0000-0x1EAF: Per-team data region (opaque).
    ///
    /// This region contains 7 team entries at stride 0x51C (1-indexed, index 0
    /// is preamble). The original WA.exe accesses team data via raw pointer
    /// arithmetic relative to the arena base. In our Rust code, team data is
    /// accessed through `TeamArenaRef`, `FullTeamBlock`, and `TeamBlockHeader`,
    /// so this region is treated as opaque padding.
    ///
    /// The 7th entry (team 6, 1-indexed) only has 8 bytes before team_count;
    /// the rest overlaps with weapon_slots.
    pub _teams_region: [u8; 0x1EB0],
    /// 0x1EB0: Number of teams in the game (used by team iteration loops)
    pub team_count: i32,
    /// 0x1EB4: Interleaved ammo/delay slots.
    /// Per alliance: [ammo_0..ammo_70, delay_0..delay_70] = 142 i32 entries.
    /// 6 alliances * 142 = 852 total entries.
    /// Ammo: -1 = unlimited, 0 = none, >0 = count.
    /// Delay: nonzero = weapon on delay.
    pub weapon_slots: [i32; 852],
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
    /// Get ammo count for a weapon slot (by flat index).
    pub fn get_ammo(&self, index: usize) -> i32 {
        self.weapon_slots[index]
    }

    /// Get mutable reference to ammo count for a weapon slot.
    pub fn ammo_mut(&mut self, index: usize) -> &mut i32 {
        &mut self.weapon_slots[index]
    }

    /// Get delay flag for a weapon slot (by flat index).
    /// Delay is at +71 offset from the ammo index within the same alliance block.
    pub fn get_delay(&self, index: usize) -> i32 {
        self.weapon_slots[index + 71]
    }
}

/// Typed handle to a TeamArenaState pointer received from WA.exe.
///
/// Wraps the raw `base: u32` from trampoline register captures and provides
/// accessor methods that encapsulate the backward pointer arithmetic to reach
/// FullTeamBlock worm data. The FullTeamBlock array lives 0x598 bytes before
/// the TeamArenaState in DDGame memory.
///
/// # Safety
/// Must only be constructed from a valid DDGame + TEAM_ARENA_STATE pointer.
/// `repr(transparent)` ensures identical ABI to `*const u8` / `u32` on i686,
/// so it can be received directly from usercall trampoline register captures.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct TeamArenaRef {
    base: *const u8,
}

impl TeamArenaRef {
    /// Wrap a raw base pointer (for non-trampoline contexts like validation).
    ///
    /// # Safety
    /// `base` must point to DDGame + TEAM_ARENA_STATE (0x4628).
    #[inline]
    pub unsafe fn from_raw(base: u32) -> Self {
        Self { base: base as *const u8 }
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

    /// Get pointer to the FullTeamBlock array base.
    #[inline]
    pub unsafe fn blocks(&self) -> *const FullTeamBlock {
        self.base.sub(offsets::ARENA_TO_BLOCKS) as *const FullTeamBlock
    }

    /// Get the team header (metadata) for a team.
    ///
    /// Returns `&block[team_idx+1].header.team`, which holds team metadata:
    /// worm_count, eliminated flag, weapon_alliance, team_name.
    #[inline]
    pub unsafe fn team_header(&self, team_idx: usize) -> &TeamBlockHeader {
        &(*self.blocks().add(team_idx + 1)).header.team
    }

    /// Get a playable worm entry by 1-indexed worm number (1..=8).
    ///
    /// Uses raw pointer arithmetic matching the original WA code:
    /// `base + team_idx * 0x51C + worm_num * 0x9C - 0x598`.
    /// This naturally crosses FullTeamBlock boundaries when worm_num = 8,
    /// since the 8th worm's early fields (state, health) spill into the
    /// next block's header slot. The header metadata lives at high offsets
    /// (0x6C+) that don't conflict with worm data (0x00-0x5F).
    #[inline]
    pub unsafe fn team_worm(&self, team_idx: usize, worm_num: usize) -> &WormEntry {
        let ptr = self.base
            .add(team_idx * 0x51C)
            .add(worm_num * 0x9C)
            .sub(0x598);
        &*(ptr as *const WormEntry)
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
    pub unsafe fn team_and_header(&self, team_idx: usize) -> (&FullTeamBlock, &TeamBlockHeader) {
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
    pub unsafe fn team_header_b(&self, team_idx: usize) -> &TeamBlockHeader {
        &(*self.blocks().add(team_idx + 2)).header.team
    }

    /// Get mutable team header for Pattern B access.
    #[inline]
    pub unsafe fn team_header_b_mut(&self, team_idx: usize) -> &mut TeamBlockHeader {
        &mut (*(self.blocks() as *mut FullTeamBlock).add(team_idx + 2)).header.team
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
    #[inline]
    pub unsafe fn ammo_index(&self, team_index: usize, weapon_id: u32) -> usize {
        let alliance_id = self.team_header(team_index).weapon_alliance as usize;
        alliance_id * 142 + weapon_id as usize
    }
}
