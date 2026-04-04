use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::audio::speech::SpeechSlotTable;
use crate::display::dd_display::DDDisplay;
use crate::display::palette::Palette;
use crate::display::{CollisionBitGrid, DisplayBitGrid};
use crate::engine::game_info::GameInfo;
use crate::engine::{
    CoordEntry, CoordList, RenderEntry, SoundQueueEntry, TeamArenaState, TeamIndexMap,
};
use crate::fixed::Fixed;
use crate::game::weapon::WeaponTable;
use crate::input::keyboard::DDKeyboard;
use crate::render::landscape::PCLandscape;
use crate::render::queue::RenderQueue;
use crate::render::turn_order::TurnOrderWidget;
use crate::FieldRegistry;

/// DDGame — the main game engine object.
///
/// This is a massive ~39KB struct (0x98D8 bytes) that owns all major subsystems:
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
#[derive(FieldRegistry)]
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
    /// 0x018: Timer object pointer (0x30 bytes, from GameSession+0xBC).
    pub timer_obj: *mut u8,
    /// 0x01C: Network/caller ECX value from constructor (often NULL for offline).
    pub network_ecx: u32,
    /// 0x020: PCLandscape pointer (copied from DDGameWrapper[0x133])
    pub landscape: *mut PCLandscape,
    /// 0x024: GameInfo pointer (passed as param_10 to constructor).
    pub game_info: *mut GameInfo,
    /// 0x028: Network game object (param_9 to DDGameWrapper constructor).
    pub net_game: *mut u8,
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
    /// 0x138: Primary display BitGrid (vtable 0x664144, 8bpp pixel buffer).
    /// Allocated as 0x4C bytes, initialized with BitGrid::init(8, 0x100, 0x1E0).
    pub display_bitgrid: *mut DisplayBitGrid,
    /// 0x13C-0x37F: Sprite/image BitGrid cache (145 pointer slots).
    /// All populated entries have vtable 0x664144 (same class as `display_bitgrid`).
    /// Not initialized in DDGame__Constructor — filled during gameplay with
    /// weapon sprites, effect images, cursor graphics, etc.
    pub sprite_cache: [*mut u8; 145],
    /// 0x380: Collision BitGrid pointer (vtable 0x664118, 0x2C bytes).
    /// Used for terrain collision/spatial queries.
    pub collision_grid: *mut CollisionBitGrid,
    /// 0x384-0x467: Additional sprite/image object slots.
    /// Same vtable 0x664144 as sprite_cache. ~20 entries populated at runtime.
    pub sprite_cache_2: [*mut u8; 57],
    /// 0x468: Landscape property from PCLandscape vtable[0xB] (thiscall getter).
    /// Set during DDGame construction if landscape is non-null.
    pub landscape_property: u32,
    /// 0x46C-0x488: 8 SpriteRegion pointers (0x9C bytes each, vtable 0x66B268)
    /// Created by SpriteRegion__Constructor (0x57DB20).
    /// Each contains 32 BitGrid sub-objects.
    pub sprite_regions: [*mut u8; 8],
    /// 0x48C-0x508: Arrow collision region pointers (32 entries)
    pub arrow_collision_regions: [*mut u8; 32],
    /// 0x50C: Coordinate list — dynamic array of packed (x,y) terrain coords.
    pub coord_list: *mut CoordList,
    /// 0x510: Weapon table pointer (0x10 header + 71 × WeaponEntry).
    pub weapon_table: *mut WeaponTable,
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
    /// 0x550-0x5CB: Unknown.
    ///
    /// Runtime observations (not yet linked to code):
    /// - 0x5C4: value matches code address 0x5755D0 (fixed-point normalize fn)
    /// - 0x5C8-0x5CB: config-like values
    pub _unknown_550: [u8; 0x5CC - 0x550],

    /// 0x5CC: Game frame counter. Incremented at end of each frame in
    /// GameFrameDispatcher. Compared against GameInfo+0xF344 (sound start
    /// threshold) by IsSoundSuppressed and DispatchGlobalSound.
    pub frame_counter: i32,

    /// 0x5D0-0x25FF: Large unverified region.
    ///
    /// Runtime observations:
    /// - 0x5D0: frame processing flag (set 0/1 by GameFrameEndProcessor)
    /// - 0x5D8-0x5FF: small config-like values (2048, 150, 3000, 696, 896, 100, 300)
    /// - 0x600-0x25FF: identity permutation [0,1,2,...,~2048] — purpose unknown
    pub _unknown_5d0: [u8; 0x2600 - 0x5D0],

    /// 0x2600-0x2DFF: Block of 0xFFFFFFFF values at runtime (512 i32 entries).
    /// May be unused slots in a parallel table to the 0x600 permutation.
    pub _unknown_2600: [u8; 0x2E00 - 0x2600],

    /// 0x2E00-0x45EB: Unknown (mostly zero at runtime)
    ///
    /// Contains FUN_00526120 zeroed offsets at stride 0x194:
    /// 0x379C, 0x3930, 0x3AC4, 0x3C58, 0x3DEC, 0x3F80, 0x4114, 0x42A8, 0x443C, 0x45D0
    pub _unknown_2e00: [u8; 0x45EC - 0x2E00],

    /// 0x45EC: Game RNG state. Advanced each frame and by weapon spread:
    /// `rng = (frame_counter + rng) * 0x19660D + 0x3C6EF35F`.
    /// See also `ADVANCE_GAME_RNG` (0x53F320) which is the WA.exe function.
    /// Single shared RNG — affects both gameplay AND visual effects.
    pub game_rng: u32,
    /// 0x45F0-0x4607: Per-team health ratio (Fixed-point, 6 entries, 1-indexed).
    /// 0x10000 = 100% health. Rendered as bar width: `value * 100 >> 16 + 4` pixels.
    /// Read by TurnOrderTeamEntry render method (0x563620).
    /// Initialized to 0x10000 (1.0) by TurnOrderTeamEntry constructor (0x5630B0).
    pub team_health_ratio: [i32; 6],
    /// 0x4608-0x461F: Per-team health ratio 2 (Fixed-point, 6 entries, 1-indexed).
    /// Initialized to 0x10000 (1.0). Not read by the render method — may be
    /// target/previous value for interpolation, or used by update logic.
    pub team_health_ratio_2: [i32; 6],
    /// 0x4620-0x4623: Unknown
    pub _unknown_4620: u32,
    /// 0x4624: HUD status message code. Set to 6 when object pool is full
    /// (too many projectiles). Read by HUD rendering to display warning text.
    pub hud_status_code: i32,
    /// 0x4628: Team arena state — per-team data, ammo, delays, alliance tracking.
    /// Note: fields previously named init_field_64d8 (= team_count at arena+0x1EB0)
    /// and init_field_72a4 (= weapon_slots entry at arena+0x2A7C) are inside this struct.
    pub team_arena: TeamArenaState,
    /// 0x7270-0x72A3: Unknown
    pub _unknown_7270: [u8; 0x72A4 - 0x7270],
    /// 0x72A4: Object pool counter. Incremented by +7 per CTaskMissile, +2 per CTaskArrow.
    /// Checked before spawning: `pool_count + N <= 700` or overflow warning shown.
    /// Written by CTaskArrow ctor at DDGame+0x4628+0x2C7C.
    pub object_pool_count: i32,
    /// 0x72A8-0x72D7: Unknown
    pub _unknown_72a8: [u8; 0x72D8 - 0x72A8],

    /// 0x72D8: Game speed multiplier (Fixed-point, 0x10000 = 1.0x).
    pub game_speed: i32,
    /// 0x72DC: Game speed target (Fixed-point, 0x10000 = 1.0x).
    pub game_speed_target: i32,
    /// 0x72E0-0x72E3: Unknown
    pub _unknown_72e0: u32,
    /// 0x72E4: Active render/weapon slot count. Reset to 14 (0x0E) by WeaponRelease
    /// sync check, matching the render_entries array size.
    pub render_slot_count: u32,
    /// 0x72E8-0x72EB: Unknown
    pub _unknown_72e8: u32,
    /// 0x72EC: RNG state word 1 (changes every frame).
    pub rng_state_1: u32,
    /// 0x72F0: RNG state word 2 (changes every frame).
    pub rng_state_2: u32,
    /// 0x72F4: Unknown (zeroed by InitTurnState).
    pub _field_72f4: u32,
    /// 0x72F8: Unknown (zeroed by InitTurnState).
    pub _field_72f8: u32,
    /// 0x72FC: Unknown (zeroed by InitTurnState).
    pub _field_72fc: u32,
    /// 0x7300: Unknown (zeroed by InitTurnState).
    pub _field_7300: u32,
    /// 0x7304: Unknown (zeroed by InitTurnState).
    pub _field_7304: u32,
    /// 0x7308: Sprite/gfx dimension data (passed to GFX_DIR_LOAD_SPRITES).
    pub gfx_sprite_data: [u8; 0x730C - 0x7308],
    /// 0x730C-0x7337: GfxDir color table (11 entries).
    /// Populated from colours.img pixel row: `color_table[i] = get_pixel(sprite, i, 0)`.
    /// Known entries:
    /// - [6] (0x7324): Crosshair line color (DrawPolygon param_2)
    /// - [8] (0x732C): Crosshair line style (DrawPolygon param_1)
    /// - [9]-[10] (0x7330-0x7334): Font palette entries (used by LoadFontExtension)
    pub gfx_color_table: [u32; 11],
    /// 0x7338: Fill pixel value
    pub fill_pixel: u32,
    /// 0x733C-0x733F: Unknown
    pub _unknown_733c: [u8; 4],
    /// 0x7340: Landscape dimension param (passed to PCLandscape vtable slot 6).
    pub _field_7340: u32,
    /// 0x7344-0x734B: Unknown
    pub _unknown_7344: [u8; 8],
    /// 0x734C: Landscape dimension param (passed to PCLandscape vtable slot 6).
    pub _field_734c: u32,
    /// 0x7350-0x7373: Unknown
    pub _unknown_7350: [u8; 36],
    /// 0x7374: Unknown (zeroed by InitTurnState).
    pub _field_7374: u32,
    /// 0x7378: Unknown (zeroed by InitTurnState).
    pub _field_7378: u32,
    /// 0x737C: Unknown (zeroed by InitTurnState).
    pub _field_737c: u32,

    /// 0x7380: Viewport width (Fixed-point, e.g. 960.0 = 0x03C00000).
    pub viewport_width: i32,
    /// 0x7384: Viewport height (Fixed-point, e.g. 348.0 = 0x015C0000).
    pub viewport_height: i32,
    /// 0x7388: Viewport width max/duplicate (Fixed-point).
    pub viewport_width_2: i32,
    /// 0x738C: Viewport height max/duplicate (Fixed-point).
    pub viewport_height_2: i32,
    /// 0x7390-0x739B: Unknown
    pub _unknown_7390: [u8; 0x739C - 0x7390],
    /// 0x739C: Render state flag (zeroed by InitRenderIndices).
    pub render_state_flag: u32,

    /// 0x73A0: Camera X position (Fixed-point, e.g. 393.0).
    pub camera_x: i32,
    /// 0x73A4: Camera Y position (Fixed-point, e.g. 532.0).
    pub camera_y: i32,
    /// 0x73A8: Camera target X (Fixed-point, duplicate/interpolation target).
    pub camera_target_x: i32,
    /// 0x73AC: Camera target Y (Fixed-point).
    pub camera_target_y: i32,

    /// 0x73B0: Render entry table (14 entries × 0x14 bytes).
    /// First u32 of each entry zeroed by InitRenderIndices.
    pub render_entries: [RenderEntry; 14],
    /// 0x74C8-0x764B: Unknown
    pub _unknown_74c8: [u8; 0x764C - 0x74C8],
    /// 0x764C: Rendering phase. Checked by CTaskCloud::HandleMessage:
    /// clouds only render when this == 5 (in-game rendering active).
    pub render_phase: i32,

    /// 0x7650: Team index permutation maps (3 × 0x64 bytes).
    /// Used for team-to-slot mapping (render order, turn order, display order).
    /// Initialized as identity permutations [0..15] with count=16.
    pub team_index_maps: [TeamIndexMap; 3],
    /// 0x777C: Level width output (written by PCLandscape constructor param 10).
    pub level_width_raw: u32,
    /// 0x7780: Level height output (written by PCLandscape constructor param 11).
    pub level_height_raw: u32,
    /// 0x7784: Unknown (zeroed by InitTurnState).
    pub _field_7784: u32,
    /// 0x7788: Unknown (set from GameInfo+0xF362 during InitTurnState).
    pub _field_7788: u32,
    /// 0x778C: Unknown (set to Fixed 1.0 = 0x10000 by InitTurnState).
    pub _field_778c: u32,
    /// 0x7790: Unknown (zeroed by InitTurnState).
    pub _field_7790: u32,
    /// 0x7794-0x779B: Unknown
    pub _unknown_7794: [u8; 8],

    /// 0x779C: Level bound min X (Fixed16.16, negative = off-screen left).
    pub level_bound_min_x: Fixed,
    /// 0x77A0: Level bound max X (Fixed16.16).
    pub level_bound_max_x: Fixed,
    /// 0x77A4: Level bound min Y (Fixed-point, same as min_x typically).
    pub level_bound_min_y: i32,
    /// 0x77A8: Level bound max Y (Fixed-point).
    pub level_bound_max_y: i32,
    /// 0x77AC-0x77B7: Unknown
    pub _unknown_77ac: [u8; 0x77B8 - 0x77AC],
    /// 0x77B8: Level width for 3D sound distance computation (pixels, not fixed-point).
    /// Read by ComputeDistanceParams, shifted left 16 before passing to Distance3D_Attenuation.
    pub level_width_sound: i32,
    /// 0x77BC-0x77BF: Unknown
    pub _unknown_77bc: [u8; 0x77C0 - 0x77BC],

    /// 0x77C0: Level width in pixels (set by PCLandscape constructor).
    pub level_width: u32,
    /// 0x77C4: Level height in pixels (set by PCLandscape constructor).
    pub level_height: u32,
    /// 0x77C8: Total pixels (width × height).
    pub level_total_pixels: u32,

    /// 0x77CC-0x77D3: Unknown
    pub _unknown_77cc: [u8; 8],
    /// 0x77D4: Unknown (zeroed by InitTurnState).
    pub _field_77d4: u32,
    /// 0x77D8: Unknown (zeroed by InitTurnState).
    pub _field_77d8: u32,
    /// 0x77DC: Unknown (zeroed by InitTurnState).
    pub _field_77dc: u32,
    /// 0x77E0: Unknown (zeroed by InitTurnState).
    pub _field_77e0: u32,

    /// 0x77E4: Speech slot table. Maps (team, speech_line_id) → DSSound buffer index.
    /// Cleared by DSSound_LoadAllSpeechBanks (0x571A70), filled by DDGameWrapper__LoadSpeechWAV (0x571530).
    pub speech_slot_table: SpeechSlotTable,

    /// 0x7D84: Unknown (zeroed by InitTurnState).
    pub _field_7d84: u32,
    /// 0x7D88: Per-team u32 flags (13 entries, zeroed per team by InitTurnState).
    pub _field_7d88: [u32; 13],
    /// 0x7DBC: Per-team byte flags array 1 (13 entries, set to 1 by InitTurnState).
    pub _field_7dbc: [u8; 13],
    /// 0x7DC9: Per-team byte flags array 2 (13 entries, set to 1 by InitTurnState).
    pub _field_7dc9: [u8; 13],
    /// 0x7DD6: Per-team byte flags array 3 (13 entries, zeroed by InitTurnState).
    pub _field_7dd6: [u8; 13],
    /// 0x7DE3: Per-team byte flags array 4 (13 entries, zeroed by InitTurnState).
    pub _field_7de3: [u8; 13],
    /// 0x7DF0: Per-team byte flags array 5 (13 entries, zeroed by InitTurnState).
    pub _field_7df0: [u8; 13],
    /// 0x7DFD-0x7E02: Unknown
    pub _unknown_7dfd: [u8; 6],
    /// 0x7E03: Unknown byte (zeroed by InitTurnState).
    pub _field_7e03: u8,
    /// 0x7E04: Unknown byte (zeroed by InitTurnState).
    pub _field_7e04: u8,
    /// 0x7E05-0x7E24: Unknown
    pub _unknown_7e05: [u8; 32],
    /// 0x7E25: SuperSheep/AquaSheep restriction active flag (byte).
    /// When nonzero, the SuperSheep/AquaSheep weapon check is applied.
    pub supersheep_restricted: u8,
    /// 0x7E26-0x7E2D: Unknown
    pub _unknown_7e26: [u8; 0x7E2E - 0x7E26],
    /// 0x7E2E: Version flag byte 1 (set by InitVersionFlags).
    pub version_flag_1: u8,
    /// 0x7E2F: Version flag byte 2 (set by InitVersionFlags).
    pub version_flag_2: u8,
    /// 0x7E30-0x7E3E: Unknown
    pub _unknown_7e30: [u8; 0x7E3F - 0x7E30],
    /// 0x7E3F: Version flag byte 3 (set by InitVersionFlags).
    /// Passed to is_super_weapon as the version/mode parameter.
    pub version_flag_3: u8,
    /// 0x7E40: Game logic version byte 4 (set by InitVersionFlags).
    /// Used in FireWeapon__MailMineMole to gate vtable call behavior by version.
    pub version_flag_4: u8,
    /// 0x7E41: Unknown byte (zeroed by InitTurnState).
    pub _field_7e41: u8,
    /// 0x7E42-0x7E4B: Unknown
    pub _unknown_7e42: [u8; 10],
    /// 0x7E4C: Unknown (zeroed by InitTurnState).
    pub _field_7e4c: u32,
    /// 0x7E50-0x7E6F: Unknown fields (all zeroed by InitTurnState).
    pub _fields_7e50: [u32; 8],
    /// 0x7E70: Per-team scoring flags (6 entries, written by InitAllianceData).
    pub team_scoring_flags: [u32; 6],
    /// 0x7E88-0x7E9B: Unknown fields (all zeroed by InitTurnState).
    pub _fields_7e88: [u32; 5],
    /// 0x7E9C-0x7E9F: Unknown
    pub _unknown_7e9c: [u8; 4],

    /// 0x7EA0: Flag/counter (value 4 at runtime — likely team count).
    pub field_7ea0: u32,
    /// 0x7EA4: Unknown.
    pub field_7ea4: u32,
    /// 0x7EA8: Turn time limit in seconds (150 = 2:30 default).
    pub turn_time_limit: u32,
    /// 0x7EAC-0x7ECF: Unknown
    pub _unknown_7eac: [u8; 0x7ED0 - 0x7EAC],
    /// 0x7ED0-0x7EEF: Unknown
    pub _unknown_7ed0: [u8; 0x7EF0 - 0x7ED0],
    /// 0x7EF0: Flag (-1 = 0xFFFFFFFF at runtime).
    pub field_7ef0: i32,
    /// 0x7EF4: HUD status message string pointer. Set when object pool overflows
    /// (loaded via string resource 0x70F). Read by HUD rendering for warning display.
    pub hud_status_text: *const core::ffi::c_char,
    /// 0x7EF8: Sound available flag (1 when game_info+0xF914 == 0, i.e. not headless).
    pub sound_available: u32,
    /// 0x7EFC: Always initialized to 1 in constructor.
    pub field_7efc: u32,

    // === Sound queue (0x7F00-0x8143) ===
    /// 0x7F00: Sound queue (16 entries, stride 0x24). Appended by PlaySoundGlobal.
    pub sound_queue: [SoundQueueEntry; 16],
    /// 0x8140: Number of entries currently in the sound queue (0–16).
    pub sound_queue_count: i32,

    /// 0x8144-0x8147: Unknown
    pub _unknown_8144: [u8; 4],
    /// 0x8148: Unknown (set to 1 by InitTurnState).
    pub _field_8148: u32,
    /// 0x814C-0x814F: Unknown
    pub _unknown_814c: [u8; 4],
    /// 0x8150: Parallax/camera scale factor (Fixed-point multiplier).
    /// Used by DrawCrosshairLine (multiplied by 0x140000) and
    /// CTaskCloud render (parallax X offset = wind_speed * this >> 16).
    pub parallax_scale: i32,

    /// 0x8154-0x8157: Unknown
    pub _unknown_8154: [u8; 4],
    /// 0x8158: Unknown (zeroed by InitTurnState).
    pub _field_8158: u32,
    /// 0x815C: Unknown (zeroed by InitTurnState).
    pub _field_815c: u32,
    /// 0x8160: Unknown (zeroed by InitTurnState).
    pub _field_8160: u32,
    /// 0x8164: Unknown (zeroed by InitTurnState).
    pub _field_8164: u32,
    /// 0x8168-0x818B: Unknown
    pub _unknown_8168: [u8; 36],

    /// 0x818C: Turn status text buffer (null-terminated ASCII).
    /// Shows on screen during gameplay, e.g. "Cheesy harkitseee siirtoaan"
    /// ("Cheesy is considering their move" in Finnish).
    pub turn_status_text: [u8; 64],

    /// 0x81CC-0x8CBB: Unknown
    ///
    /// Known landmarks:
    /// - 0x8174: value 0x3FC (1020) at runtime
    pub _unknown_81cc: [u8; 0x8CBC - 0x81CC],

    /// 0x8CBC-0x8CFB: 4 coordinate entries (0x10-byte stride).
    /// InitFields zeroes x and y of each. At runtime contains fixed-point
    /// screen coordinates (e.g. 393.0, 532.0, 960.0, 348.0).
    pub screen_coords: [CoordEntry; 4],

    /// 0x8CFC-0x984F: Unknown
    pub _unknown_8cfc: [u8; 0x9850 - 0x8CFC],

    /// 0x9850-0x988F: 4 coordinate entries (0x10-byte stride, zeroed by InitFields).
    pub screen_coords_2: [CoordEntry; 4],

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

    /// 0x98B4-0x98D7: Unknown tail region.
    /// The original constructor allocates 0x98D8 bytes but memsets only 0x98B8.
    /// The remaining 0x20 bytes are not zeroed by the initial memset but may be
    /// initialized by subsequent constructor steps or game code.
    pub _unknown_98b4: [u8; 0x98D8 - 0x98B4],
}

const _: () = assert!(core::mem::size_of::<DDGame>() == 0x98D8);

// ============================================================
// DDGame methods
// ============================================================

impl DDGame {
    /// Advance the game RNG and return the new state.
    ///
    /// Formula: `rng = (frame_counter + rng) * 0x19660D + 0x3C6EF35F`
    ///
    /// This is the same LCG used by `ADVANCE_GAME_RNG` (0x53F320). There is a single
    /// shared RNG — both gameplay and visual effects advance it. Any difference in
    /// RNG call count between Rust and original code causes replay desync.
    pub fn advance_rng(&mut self) -> u32 {
        let rng = crate::rng::wa_lcg((self.frame_counter as u32).wrapping_add(self.game_rng));
        self.game_rng = rng;
        rng
    }

    /// Advance the secondary effect RNG at DDGame+0x45F0 and return the new state.
    ///
    /// Formula: `rng = rng * 0x19660D + 0x3C6EF35F` (simpler than [`advance_rng`] — no
    /// frame_counter). Uses `team_health_ratio[0]`, the unused index-0 slot of the
    /// 1-indexed health ratio array, repurposed by WA as a secondary RNG for weapon
    /// release visual effects.
    pub fn advance_effect_rng(&mut self) -> u32 {
        let rng = crate::rng::wa_lcg(self.team_health_ratio[0] as u32);
        self.team_health_ratio[0] = rng as i32;
        rng
    }

    /// Read a per-team sound ID from the team sound table at DDGame+0x7768.
    ///
    /// The table has stride 0xF0 per team (240-byte per-team config blocks).
    /// The u32 at the base of each block is a sound ID used for type-2 (rope)
    /// weapon release sounds.
    ///
    /// # Safety
    /// `team_id` must be a valid team index (0–5).
    pub unsafe fn team_sound_id(&self, team_id: u32) -> u32 {
        let base = (self as *const DDGame as *const u8).add(0x7768);
        *(base.add((team_id as usize) * 0xF0) as *const u32)
    }

    /// Get a mutable pointer to a per-team/per-worm weapon stat counter.
    ///
    /// Four counters exist at DDGame base offsets 0x40CC, 0x40D0, 0x40D4, 0x40D8,
    /// indexed by `team_id * 0x51C + worm_id * 0x9C` (same stride as FullTeamBlock
    /// and WormEntry). These track weapon usage stats during gameplay.
    ///
    /// # Safety
    /// `team_id` and `worm_id` must be valid indices.
    pub unsafe fn weapon_stat_counter(
        &mut self,
        team_id: u32,
        worm_id: u32,
        base_offset: usize,
    ) -> *mut i32 {
        (self as *mut DDGame as *mut u8)
            .add(base_offset)
            .add((team_id as usize) * 0x51C)
            .add((worm_id as usize) * 0x9C) as *mut i32
    }

    /// Show the "too many objects" warning on the HUD.
    ///
    /// Sets `hud_status_code = 6` and loads string resource 0x70F into
    /// `hud_status_text`. Only writes if `game_info.game_version < 60`.
    ///
    /// # Safety
    /// `self.game_info` must be valid.
    pub unsafe fn show_pool_overflow_warning(&mut self) {
        use crate::address::va;
        use crate::rebase::rb;

        let game_version = (*self.game_info).game_version;
        if game_version < 0x3C {
            self.hud_status_code = 6;
            let load_string: unsafe extern "cdecl" fn(u32) -> *const core::ffi::c_char =
                core::mem::transmute(rb(va::LOAD_STRING_RESOURCE));
            self.hud_status_text = load_string(0x70F);
        }
    }

    /// Listener position for 3D audio — stored at DDGame+0x8CEC (screen_coords[3]).
    ///
    /// Returns `(x, y)` as raw i32 values (fixed-point 16.16).
    /// Used by ComputeDistanceParams / Distance3D_Attenuation.
    pub fn listener_pos(&self) -> (i32, i32) {
        (self.screen_coords[3].x, self.screen_coords[3].y)
    }
}

// DDGame constructor code (create_ddgame, init helpers, usercall bridges)
// has been moved to ddgame_constructor.rs.

// BitGrid__Init lives in crate::display::bitgrid
// Re-exported via ddgame_constructor.rs
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

    // === Team block array (7 × TeamBlock, stride 0x51C) ===
    /// Start of team block array within DDGame (7 blocks, stride 0x51C).
    /// Derived: entry_ptr(team=0) - 0x598 = 0x4628 - 0x598 = 0x4090.
    /// Runtime-confirmed: block[0] is zeroed preamble, blocks[1-6] hold team data.
    pub const TEAM_BLOCKS: usize = 0x4090;

    /// Byte offset from TeamArenaState base back to TeamBlock array start.
    /// `blocks_ptr = (tws_base as *const c_char).sub(ARENA_TO_BLOCKS) as *const TeamBlock`
    ///
    /// entry_ptr(0) = DDGame+0x4628 = TEAM_BLOCKS + 0x598.
    /// 0x598 = sizeof(TeamBlock) + 0x7C = one block + offset into TeamHeader.
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
    pub const PARALLAX_SCALE: usize = 0x8150;
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

// ── Snapshot impls ──────────────────────────────────────────

#[cfg(target_arch = "x86")]
impl crate::snapshot::Snapshot for DDGame {
    unsafe fn write_snapshot(
        &self,
        w: &mut dyn core::fmt::Write,
        indent: usize,
    ) -> core::fmt::Result {
        use crate::fixed::Fixed;
        use crate::snapshot::{fmt_ptr, write_indent};
        let i = indent;

        write_indent(w, i)?;
        writeln!(w, "frame_counter = {}", self.frame_counter)?;
        write_indent(w, i)?;
        writeln!(w, "game_speed = {}", Fixed(self.game_speed))?;
        write_indent(w, i)?;
        writeln!(w, "game_speed_target = {}", Fixed(self.game_speed_target))?;
        write_indent(w, i)?;
        writeln!(
            w,
            "rng_state = 0x{:08X} 0x{:08X}",
            self.rng_state_1, self.rng_state_2
        )?;
        write_indent(w, i)?;
        writeln!(
            w,
            "camera = ({}, {})",
            Fixed(self.camera_x),
            Fixed(self.camera_y)
        )?;
        write_indent(w, i)?;
        writeln!(
            w,
            "camera_target = ({}, {})",
            Fixed(self.camera_target_x),
            Fixed(self.camera_target_y)
        )?;
        write_indent(w, i)?;
        writeln!(w, "level_size = {}x{}", self.level_width, self.level_height)?;
        write_indent(w, i)?;
        writeln!(
            w,
            "level_size_raw = {}x{}",
            self.level_width_raw, self.level_height_raw
        )?;
        write_indent(w, i)?;
        writeln!(
            w,
            "landscape_property = {}",
            fmt_ptr(self.landscape_property as *const u8)
        )?;
        write_indent(w, i)?;
        writeln!(w, "gfx_color_table = {:?}", self.gfx_color_table)?;
        write_indent(w, i)?;
        writeln!(
            w,
            "fast_forward = req={} active={}",
            self.fast_forward_request, self.fast_forward_active
        )?;
        write_indent(w, i)?;
        writeln!(w, "keyboard = {}", fmt_ptr(self.keyboard as *const u8))?;
        write_indent(w, i)?;
        writeln!(w, "display = {}", fmt_ptr(self.display as *const u8))?;
        write_indent(w, i)?;
        writeln!(w, "sound = {}", fmt_ptr(self.sound as *const u8))?;
        write_indent(w, i)?;
        writeln!(w, "game_info = {}", fmt_ptr(self.game_info as *const u8))?;
        write_indent(w, i)?;
        writeln!(w, "landscape = {}", fmt_ptr(self.landscape as *const u8))?;
        write_indent(w, i)?;
        writeln!(w, "task_land = {}", fmt_ptr(self.task_land))?;

        // TeamArenaState summary
        write_indent(w, i)?;
        writeln!(w, "team_arena.team_count = {}", self.team_arena.team_count)?;
        write_indent(w, i)?;
        writeln!(w, "team_arena.game_phase = {}", self.team_arena.game_phase)?;
        write_indent(w, i)?;
        writeln!(
            w,
            "team_arena.game_mode_flag = {}",
            self.team_arena.game_mode_flag
        )?;

        // Dump weapon slots (ammo only, skip zeros)
        write_indent(w, i)?;
        writeln!(w, "weapon_slots:")?;
        for team in 0..6usize {
            let slots = &self.team_arena.weapon_slots.teams[team];
            let mut has_any = false;
            for wpn in 0..71usize {
                let ammo = slots.ammo[wpn];
                if ammo != 0 {
                    if !has_any {
                        write_indent(w, i + 1)?;
                        write!(w, "team[{}] ammo:", team)?;
                        has_any = true;
                    }
                    if ammo == -1 {
                        write!(w, " {}=inf", wpn)?;
                    } else {
                        write!(w, " {}={}", wpn, ammo)?;
                    }
                }
            }
            if has_any {
                writeln!(w)?;
            }
        }

        Ok(())
    }
}
