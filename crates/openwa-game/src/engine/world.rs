use crate::FieldRegistry;
use crate::audio::SoundQueueEntry;
use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::audio::speech::SpeechSlotTable;
use crate::bitgrid::{BitGrid, CollisionBitGrid, DisplayBitGrid};
use crate::engine::game_info::GameInfo;
use crate::engine::{CoordEntry, CoordList, EntityActivityQueue, TeamArena, TeamIndexMap};
use crate::game::weapon::WeaponTable;
use crate::input::keyboard::Keyboard;
use crate::input::mouse::MouseInput;
use crate::render::RenderEntry;
use crate::render::display::gfx::DisplayGfx;
use crate::render::landscape::Landscape;
use crate::render::queue::RenderQueue;
use crate::render::turn_order::TurnOrderWidget;
use crate::wa::localized_template::LocalizedTemplate;
use openwa_core::fixed::{Fixed, Fixed64};

/// GameWorld — the main game engine object.
///
/// This is a massive ~39KB struct (0x98D8 bytes) that owns all major subsystems:
/// display, landscape, sound, graphics handlers, and task state machines.
///
/// Allocated in GameWorld__Constructor (0x56E220).
/// The GameWorld pointer is stored at GameRuntime+0x488 (DWORD index 0x122).
///
/// PARTIAL: Fields up to 0x54C are densely mapped from the constructor.
/// Beyond that, only scattered fields are known — use the `offsets` module.
///
/// Note on offsets: The constructor accesses GameWorld fields via
/// `*(param_2[0x122] + byte_offset)` — these are byte offsets, NOT DWORD-indexed.
/// DWORD indexing only applies to param_2 (GameRuntime) itself.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct GameWorld {
    /// 0x000: Keyboard pointer (vtable 0x66AEC8). Constructor param "keyboard".
    pub keyboard: *mut Keyboard,
    /// 0x004: DisplayGfx pointer (vtable 0x66A218). Constructor param "display".
    pub display: *mut DisplayGfx,
    /// 0x008: DSSound pointer (vtable 0x66AF20). Constructor param "sound".
    /// Null means sound is disabled (checked by PlaySoundGlobal).
    pub sound: *mut DSSound,
    /// 0x00C: Active sound position table (0x608 bytes, conditional on sound available).
    /// Tracks up to 64 positional sounds with world coordinates for 3D audio mixing.
    /// NULL when `game_info+0xF914 != 0` (headless/server) or `sound == NULL`.
    pub active_sounds: *mut ActiveSoundTable,
    /// 0x010: [`MouseInput`] adapter (vtable 0x66A2E4). Forwarded from
    /// `GameSession.mouse_input`; despite the historical "palette" name on
    /// its vtable + GameRuntime constructor parameter, this is a small
    /// mouse-input wrapper that reads `g_GameSession.mouse_delta_*`/
    /// `mouse_button_state` and debounces button presses through a
    /// per-instance [`button_armed_latch`](MouseInput::button_armed_latch).
    /// The actual graphics palette lives in [`PaletteContext`](crate::render::palette::PaletteContext).
    pub mouse_input: *mut MouseInput,
    /// 0x014: Music object pointer (vtable 0x66B3E0). Constructor param "music".
    pub music: *mut Music,
    /// 0x018: Localized-template resolver (from GameSession+0xBC). Wraps
    /// WA's localization tables with a per-token cache + escape-code
    /// post-processor. See [`LocalizedTemplate`](crate::wa::localized_template::LocalizedTemplate).
    pub localized_template: *mut LocalizedTemplate,
    /// 0x01C: Per-game network session object. NULL for offline play.
    /// When non-null, drives end-of-round peer synchronisation via its
    /// vtable (see `engine::net_session`).
    pub net_session: *mut crate::engine::net_session::NetSession,
    /// 0x020: Landscape pointer (copied from GameRuntime[0x133])
    pub landscape: *mut Landscape,
    /// 0x024: GameInfo pointer (passed as param_10 to constructor).
    pub game_info: *mut GameInfo,
    /// 0x028: Network game object (param_9 to GameRuntime constructor).
    pub net_game: *mut u8,
    /// 0x02C: Secondary PaletteContext (0x70C bytes, conditional on secondary GfxDir)
    pub secondary_palette_ctx: *mut crate::render::palette::PaletteContext,
    /// 0x030: Gradient image (decoded from gradient.img via IMG_Decode).
    pub gradient_image: *mut BitGrid,
    /// 0x034: Gradient image 2 pointer.
    pub gradient_image_2: *mut BitGrid,
    /// 0x038-0x0B4: Arrow sprite BitGrid pointers (32 entries, decoded from arrow*.img).
    pub arrow_sprites: [*mut BitGrid; 32],
    /// 0x0B8-0x134: Arrow GfxDir pointers (32 entries, conditional)
    pub arrow_gfxdirs: [*mut u8; 32],
    /// 0x138: Primary display BitGrid (vtable 0x664144, 8bpp pixel buffer).
    /// Allocated as 0x4C bytes, initialized with BitGrid::init(8, 0x100, 0x1E0).
    pub display_bitgrid: *mut DisplayBitGrid,
    /// 0x13C-0x37F: Sprite/image BitGrid cache (145 pointer slots).
    /// All populated entries have vtable 0x664144 (same class as `display_bitgrid`).
    /// Not initialized in GameWorld__Constructor — filled during gameplay with
    /// weapon sprites, effect images, cursor graphics, etc.
    pub sprite_cache: [*mut u8; 145],
    /// 0x380: Collision BitGrid pointer (vtable 0x664118, 0x2C bytes).
    /// Used for terrain collision/spatial queries.
    pub collision_grid: *mut CollisionBitGrid,
    /// 0x384-0x467: Additional sprite/image object slots.
    /// Same vtable 0x664144 as sprite_cache. ~20 entries populated at runtime.
    pub sprite_cache_2: [*mut u8; 57],
    /// 0x468: Landscape property from Landscape vtable[0xB] (thiscall getter).
    /// Set during GameWorld construction if landscape is non-null.
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
    /// Created in GameWorld_InitGameState_Maybe (0x526690), constructor 0x4FB5F0.
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
    /// 0x544: Textbox object pointer, details unknown
    pub textbox: *mut u8,
    /// 0x548: Weapon panel pointer
    pub weapon_panel: *mut u8,
    /// 0x54C: LandEntity pointer (set by LandEntity__InitLandscape at 0x5056F0).
    /// The landscape/terrain task. Vtable at 0x664388.
    pub task_land: *mut u8,
    /// 0x550-0x5C3: Unknown.
    pub _unknown_550: [u8; 0x5C4 - 0x550],
    /// 0x5C4: Vector normalize function pointer. Selected by game version in InitGameState:
    /// < 0x99 → VECTOR_NORMALIZE_SIMPLE (0x5755D0), >= 0x99 → VECTOR_NORMALIZE_OVERFLOW.
    pub vector_normalize_fn: u32,
    /// 0x5C8: Stack height config. Version-dependent: < -1 → 0x700, >= -1 → 0x800.
    pub stack_height: u32,

    /// 0x5CC: Game frame counter. Incremented at end of each frame in
    /// GameFrameDispatcher. Compared against GameInfo+0xF344 (sound start
    /// threshold) by IsSoundSuppressed and DispatchGlobalSound.
    pub frame_counter: i32,

    /// 0x5D0: Frame processing flag (set 0/1 by GameFrameEndProcessor, zeroed by InitGameState).
    pub _field_5d0: u32,
    /// 0x5D4: Unknown (zeroed by InitGameState).
    pub _field_5d4: u32,
    /// 0x5D8: Unknown (zeroed by InitGameState).
    pub _field_5d8: u32,
    /// 0x5DC: Unknown (zeroed by InitGameState).
    pub _field_5dc: u32,
    /// 0x5E0: Water level in pixels. Computed as `(100 - water_pct) * level_height / 100`.
    pub water_level: i32,
    /// 0x5E4: Water kill Y boundary (pixels). Derived from level_bound_max_y integer part + 0x28.
    pub water_kill_y: i32,
    /// 0x5E8: Initial water level (copy of water_level at init time).
    pub water_level_initial: i32,
    /// 0x5EC: Unknown (zeroed by InitGameState).
    pub _field_5ec: u32,
    /// 0x5F0: Unknown (set to 1 by InitGameState).
    pub _field_5f0: u32,
    /// 0x5F4: Unknown (set to 100 by InitGameState).
    pub _field_5f4: u32,
    /// 0x5F8: Unknown (zeroed by InitGameState).
    pub _field_5f8: u32,
    /// 0x5FC: Unknown (zeroed by InitGameState).
    pub _field_5fc: u32,
    /// 0x600: EntityActivityQueue — most-recent-activity rank table over
    /// every `WorldEntity` (Worm, OldWorm, Crate, Mine, OilDrum, etc.).
    /// `init` (formerly misnamed `SpriteGfxTable__Init`) sets `free_pool`
    /// to the identity permutation and `ages` to all-`0xFFFFFFFF`. Each
    /// entity holds one slot for its lifetime; the rank gets reset to
    /// "newest" at activity edges. Only known reader: `WormEntity::
    /// BehaviorTick` water-death path (forwards `ages[slot]` as a stagger
    /// delay to `ScoreBubbleEntity`'s ctor). Total size 0x300C.
    pub entity_activity_queue: EntityActivityQueue,

    /// 0x360C-0x45D3: Unknown (mostly zero at runtime).
    ///
    /// Contains FUN_00526120 zeroed offsets at stride 0x194:
    /// 0x379C, 0x3930, 0x3AC4, 0x3C58, 0x3DEC, 0x3F80, 0x4114, 0x42A8, 0x443C, 0x45D0
    pub _unknown_360c: [u8; 0x45D4 - 0x360C],
    /// 0x45D4: Terrain drop percentage A (from GameInfo+0xD955).
    pub terrain_pct_a: u32,
    /// 0x45D8: Terrain drop percentage B (from GameInfo+0xD958).
    pub terrain_pct_b: u32,
    /// 0x45DC: Terrain drop percentage C (from GameInfo+0xD957).
    pub terrain_pct_c: u32,
    /// 0x45E0: Unknown (zeroed by InitGameState version config).
    pub _field_45e0: u32,
    /// 0x45E4: Unknown (zeroed by InitGameState version config).
    pub _field_45e4: u32,
    /// 0x45E8: Unknown (zeroed by InitGameState version config).
    pub _field_45e8: u32,

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
    pub team_arena: TeamArena,
    /// 0x7270-0x72A3: Unknown
    pub _unknown_7270: [u8; 0x72A4 - 0x7270],
    /// 0x72A4: Object pool counter. Incremented by +7 per MissileEntity, +2 per ArrowEntity.
    /// Checked before spawning: `pool_count + N <= 700` or overflow warning shown.
    /// Written by ArrowEntity ctor at GameWorld+0x4628+0x2C7C.
    pub object_pool_count: i32,
    /// 0x72A8-0x72D7: Unknown
    pub _unknown_72a8: [u8; 0x72D8 - 0x72A8],

    /// 0x72D8: Game speed multiplier (Fixed16.16, 1.0 = normal speed).
    pub game_speed: Fixed,
    /// 0x72DC: Game speed target (Fixed16.16, 1.0 = normal speed).
    pub game_speed_target: Fixed,
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
    /// 0x7340: Border shader-layer color A (sel=0 in the diagonal stripe).
    /// Paired with `gfx_color_table[0]` (terrain layer) when `Landscape::init_borders`
    /// (0x57D7F0) draws the indestructible-borders pattern. The shader-layer slot 5
    /// dispatch is a no-op for the LandscapeShader-vtable variant of the layer at
    /// `Landscape+0x91C`, so this value only ends up on screen in the
    /// DisplayGfx/BitGrid path.
    pub border_shader_color_a: u32,
    /// 0x7344-0x734B: Unknown
    pub _unknown_7344: [u8; 8],
    /// 0x734C: Border shader-layer color B (sel=1 in the diagonal stripe).
    /// Paired with `gfx_color_table[3]` (terrain layer) when `Landscape::init_borders`
    /// draws borders. See `border_shader_color_a` for the dispatch caveat.
    pub border_shader_color_b: u32,
    /// 0x7350-0x736F: Unknown
    pub _unknown_7350: [u8; 0x7370 - 0x7350],
    /// 0x7370: Current team color index. Set from GameInfo+0xD924;
    /// overridden to -1 (0xFFFFFFFF) if any CPU team is detected.
    pub team_color: u32,
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
    /// 0x7390: Unknown (zeroed by InitGameState).
    pub _field_7390: u32,
    /// 0x7394: Render scale factor (Fixed-point, 0x10000 = 1.0).
    pub render_scale: Fixed,
    /// 0x7398: Unknown (zeroed by InitGameState).
    pub _field_7398: u32,
    /// 0x739C: Render state flag (zeroed by InitRenderIndices).
    pub render_state_flag: u32,

    /// 0x73A0: Effect-event point — written by [`GameWorld::register_event_point_raw`]
    /// (WA 0x547E70). Despite the original "min" / "max" / "expand bbox" code shape,
    /// the entry path always resets `+0x73A0..+0x73AC` to a single `(x, y)` point.
    pub camera_x: Fixed,
    /// 0x73A4: see [`Self::camera_x`].
    pub camera_y: Fixed,
    /// 0x73A8: see [`Self::camera_x`].
    pub camera_target_x: Fixed,
    /// 0x73AC: see [`Self::camera_x`].
    pub camera_target_y: Fixed,

    /// 0x73B0: Render entry table (14 entries × 0x14 bytes).
    /// First u32 of each entry zeroed by InitRenderIndices.
    pub render_entries: [RenderEntry; 14],
    /// 0x74C8: 0x178-byte backup region populated by [`init_weapon_table`].
    /// Holds a verbatim copy of `weapon_table+0x29EC` (a slice within
    /// entry 23's `fire_params` block). Stored as 0x5E dwords because
    /// WA's memcpy uses dword stride and two later writes (indices 3 and
    /// 0x19) overwrite specific dwords in this region.
    ///
    /// [`init_weapon_table`]: crate::game::init_weapon_table::init_weapon_table
    pub weapon_table_backup: [u32; 0x5E],
    /// 0x7640: Unknown (zeroed by InitGameState).
    pub _field_7640: u32,
    /// 0x7644: Unknown config (from GameInfo+0xF363).
    pub _field_7644: u32,
    /// 0x7648: Unknown config (from GameInfo+0xF364).
    pub _field_7648: u32,
    /// 0x764C: Rendering phase. Checked by CloudEntity::HandleMessage:
    /// clouds only render when this == 5 (in-game rendering active).
    pub render_phase: i32,

    /// 0x7650: Team index permutation maps (3 × 0x64 bytes).
    /// Used for team-to-slot mapping (render order, turn order, display order).
    /// Initialized as identity permutations [0..15] with count=16.
    pub team_index_maps: [TeamIndexMap; 3],
    /// 0x777C: Cavern / indestructible-borders flag.
    /// Written by the Landscape constructor (param 10) and toggled by
    /// `init_landscape_borders` (0x528480) when the scheme's cavern byte changes.
    /// Used as a boolean elsewhere: nonzero means cavern level (fewer clouds,
    /// different level bounds, alternate super-weapon rules).
    pub is_cavern: u32,
    /// 0x7780: Level height output (written by Landscape constructor param 11).
    pub level_height_raw: u32,
    /// 0x7784: Unknown (zeroed by InitTurnState).
    pub _field_7784: u32,
    /// 0x7788: Unknown (set from GameInfo+0xF362 during InitTurnState).
    pub _field_7788: u32,
    /// 0x778C: Unknown (set to Fixed 1.0 by InitTurnState).
    pub _field_778c: Fixed,
    /// 0x7790: Unknown (zeroed by InitTurnState).
    pub _field_7790: u32,
    /// 0x7794: Shake intensity X (Fixed-point, zeroed by InitGameState level bounds).
    pub shake_intensity_x: Fixed,
    /// 0x7798: Shake intensity Y (Fixed-point, zeroed by InitGameState level bounds).
    pub shake_intensity_y: Fixed,

    /// 0x779C: Level bound min X (Fixed16.16, negative = off-screen left).
    pub level_bound_min_x: Fixed,
    /// 0x77A0: Level bound max X (Fixed16.16).
    pub level_bound_max_x: Fixed,
    /// 0x77A4: Level bound min Y (Fixed16.16).
    pub level_bound_min_y: Fixed,
    /// 0x77A8: Level bound max Y (Fixed16.16).
    pub level_bound_max_y: Fixed,
    /// 0x77AC: Viewport pixel width — derived each frame in
    /// `GameRender_Maybe` (0x533DC0) as `(level_bound_max_x - min_x) >> 16`,
    /// clamped to the display dimensions. Read by
    /// `GameRuntime::RenderEscMenuOverlay` (0x00535000) to center the ESC
    /// menu against the rendered viewport.
    pub viewport_pixel_width: i32,
    /// 0x77B0: Viewport pixel height — derived each frame in
    /// `GameRender_Maybe` as `(level_bound_max_y - min_y) >> 16` (rounded
    /// down to an even number), clamped to the display dimensions.
    pub viewport_pixel_height: i32,
    /// 0x77B4: Previous frame's [`Self::viewport_pixel_height`]. Saved at
    /// the top of `GameRender_Maybe` (0x00533DC0) before the new height is
    /// computed, then read by downstream draw code that needs to compare
    /// last frame's bar height against this frame's. Initialized to zero;
    /// the first frame copies `viewport_pixel_height` into here so the
    /// second frame sees a non-zero "previous" value.
    pub viewport_pixel_height_prev: i32,
    /// 0x77B8: Level width for 3D sound distance computation (pixels, not fixed-point).
    /// Read by ComputeDistanceParams, shifted left 16 before passing to Distance3D_Attenuation.
    pub level_width_sound: i32,
    /// 0x77BC: Screen height in pixels (output from display vtable[1]).
    /// Read by InitGameState for resolution-dependent layout.
    pub screen_height_pixels: i32,

    /// 0x77C0: Level width in pixels (set by Landscape constructor).
    pub level_width: u32,
    /// 0x77C4: Level height in pixels (set by Landscape constructor).
    pub level_height: u32,
    /// 0x77C8: Total pixels (width × height).
    pub level_total_pixels: u32,

    /// 0x77CC: Map boundary width. Default 0x30D4; updated to `level_w + 0x2954` for version > 0x32.
    pub map_boundary_width: u32,
    /// 0x77D0: Map boundary height. Default = level_height; updated to 0x2B8 for version > 0x32.
    pub map_boundary_height: u32,
    /// 0x77D4: Frame counter
    pub frame: u32,
    /// 0x77D8: Fixed accumulator of scaled-frame time — companion to the
    /// integer frame counter at [`Self::frame`], with sub-frame precision.
    /// [`crate::engine::main_loop::dispatch_frame::update_network_hud_animations`]
    /// adds `advance_ratio` each frame (= 1.0 when one frame of wall-clock
    /// time has elapsed at the target game speed) — that bump is the
    /// only frame-counter side effect of an otherwise HUD-animation
    /// function. Zeroed by InitTurnState.
    pub scaled_frame_accum: Fixed,
    /// 0x77DC: Unknown (zeroed by InitTurnState).
    pub _field_77dc: u32,
    /// 0x77E0: Unknown (zeroed by InitTurnState).
    pub _field_77e0: u32,

    /// 0x77E4: Speech slot table. Maps (team, speech_line_id) → DSSound buffer index.
    /// Cleared by DSSound_LoadAllSpeechBanks (0x571A70), filled by GameRuntime__LoadSpeechWAV (0x571530).
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
    pub net_peer_ready_flags: [u8; 13],
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
    /// Controls whether Select Worm is considered a super weapon (in is_super_weapon).
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
    /// 0x7E50-0x7E63: Unknown fields (all zeroed by InitTurnState).
    pub _fields_7e50: [u32; 5],
    /// 0x7E64: Unknown (zeroed by InitTurnState).
    pub _field_7e64: u32,
    /// 0x7E68: "Skip render" gate. When non-zero, `GameRender_Maybe`
    /// (0x00533DC0) early-exits at the very top — entire per-frame render
    /// pipeline (queue dispatch + tail funcs) is bypassed. Writers TBD;
    /// candidates include the dialog/end-of-round freeze and the
    /// host-side network-game-end "wait for peers" state.
    pub render_skip_gate: u32,
    /// 0x7E6C: Unknown (zeroed by InitTurnState).
    pub _field_7e6c: u32,
    /// 0x7E70: Per-team scoring flags (6 entries, written by InitAllianceData).
    pub team_scoring_flags: [u32; 6],
    /// 0x7E88-0x7E9B: Unknown fields (all zeroed by InitTurnState).
    pub _fields_7e88: [u32; 5],
    /// 0x7E9C: Last keyboard poll result. Written each frame by DispatchFrame
    /// from `Keyboard::vtable[1](0xd)` when `wrapper._field_410 == 0`.
    pub kb_poll_result: u32,

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
    /// 0x7EF0: Recording-side frame counter — `-1` during replay playback.
    ///
    /// `GameWorld::InitGameState` sets this to `-1` when a replay is being
    /// played back (nothing is being recorded), otherwise `0`. Outside
    /// replay playback, `DDNetGameWrapper::SendGameState` (0x0056FAF0)
    /// increments it once per game frame whose state was handed to the
    /// network / recording stream. So it tracks how many frames have been
    /// recorded, parallel to `frame_counter` (0x5CC) which tracks how
    /// many frames have been simulated.
    ///
    /// The end-of-round log prefix emits `[recorded_t] [sim_t]` when
    /// this is `>= 0`, and just `[sim_t]` during replay playback — the
    /// two-column form makes recording-vs-sim drift visible at a glance.
    pub recorded_frame_counter: i32,
    /// 0x7EF4: HUD status message string pointer. Set when object pool overflows
    /// (loaded via string resource LOG_CRASH_TOO_MANY_OBJECTS). Read by HUD rendering for warning display.
    pub hud_status_text: *const core::ffi::c_char,
    /// 0x7EF8: Headful/interactive mode flag (u32, 0 = headless, 1 = headful).
    /// Computed in the GameWorld constructor as `game_info.headless_mode == 0`.
    /// Read throughout the engine to gate interactive-mode-only work:
    /// loading progress bar, message pumps, sound, input keyboard/palette
    /// frame polls, rendering-related init. Not strictly "is sound enabled"
    /// — headless tests suppress more than just sound.
    pub is_headful: u32,
    /// 0x7EFC: Always initialized to 1 in constructor.
    pub field_7efc: u32,

    // === Sound queue (0x7F00-0x8143) ===
    /// 0x7F00: Sound queue (16 entries, stride 0x24). Appended by PlaySoundGlobal.
    pub sound_queue: [SoundQueueEntry; 16],
    /// 0x8140: Number of entries currently in the sound queue (0–16).
    pub sound_queue_count: i32,

    /// 0x8144: Round-wide total jetpack fuel used. When nonzero at
    /// end-of-round headless-log time, StepFrame emits the
    /// `"LOG_JETPACK_FUEL_TOTAL %d\n\n"` footer (English: "Total Jet Pack
    /// fuel used: N") after the per-team stats block. Printed via `%d`
    /// (signed format) but the stored value is used as a u32 counter.
    pub round_jetpack_fuel_total: u32,
    /// 0x8148: Unknown (set to 1 by InitTurnState).
    pub _field_8148: u32,
    /// 0x814C-0x814F: Unknown
    pub _unknown_814c: [u8; 4],
    /// 0x8150: Render interpolation factor A (16.16 fixed, 0..0x10000).
    ///
    /// Written each frame by `DispatchFrame` as the fraction of the current
    /// simulation tick elapsed in wall time — effectively a sub-frame
    /// progress ratio. Consumers multiply it by per-object velocities to
    /// interpolate smooth render positions between the 50Hz simulation
    /// ticks (`CloudEntity`'s parallax scroll, crosshair aim-range animation,
    /// worm render-position offset, etc.).
    ///
    /// Clamped to 0 while the game is paused. In replay mode holds a
    /// speed ratio where `>= 0x10000` triggers one simulation step.
    pub render_interp_a: Fixed,

    /// 0x8154: Render interpolation factor B — parallels `render_interp_a`
    /// but is driven by `frame_accum_b` (running frame accumulator) instead
    /// of `frame_accum_a` (paused accumulator). Written in the same block
    /// by `DispatchFrame` and rescaled on speed changes.
    pub render_interp_b: Fixed,
    /// 0x8158: Replay-speed accumulator — 16.16 fixed, 64-bit storage.
    ///
    /// Advanced by `Fixed64::from_raw(0x32_0000)` (= `50 * Fixed::ONE`) per
    /// replay-dispatch frame in `DispatchFrame`. Divided by
    /// `GameInfo::replay_ticks` to produce a running replay-time clock in
    /// Fixed; the current frame's `render_interp` is that clock minus
    /// `replay_frame_accum`. Scaling by 50 lets the division reproduce
    /// the target tick rate for the default 50 replay-ticks-per-second.
    ///
    /// 64-bit width is needed so the clock doesn't wrap during long
    /// replays; the ceremonial `.to_fixed_wrapping()` on the read side
    /// mirrors the original's i32 low-half narrowing.
    pub replay_speed_accum: Fixed64,
    /// 0x8160: Replay-progress accumulator (16.16 fixed, 48 integer bits).
    ///
    /// In replay mode, `StepFrame` adds `Fixed::ONE` per tick; the running
    /// sum is used by `DispatchFrame`'s replay-speed computation as the
    /// "replay time" reference (via low-32-bit projection). 64-bit
    /// storage lets it accumulate without overflowing Fixed's ±32k
    /// integer range (~18 min at 50 fps).
    pub replay_frame_accum: Fixed64,
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

    /// 0x8CBC-0x8D0B: 5 viewport coordinate entries (0x10-byte stride).
    /// InitGameState writes camera center to entries 1..5 (indexed from base 0..4).
    /// Entry 0 is not written by init (used for other purposes).
    pub viewport_coords: [CoordEntry; 5],

    /// 0x8D0C-0x984F: Unknown
    pub _unknown_8d0c: [u8; 0x9850 - 0x8D0C],

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

const _: () = assert!(core::mem::size_of::<GameWorld>() == 0x98D8);

// ============================================================
// GameWorld methods
// ============================================================

impl GameWorld {
    /// Advance the game RNG and return the new state.
    ///
    /// Formula: `rng = (frame_counter + rng) * 0x19660D + 0x3C6EF35F`
    ///
    /// This is the same LCG used by `ADVANCE_GAME_RNG` (0x53F320). There is a single
    /// shared RNG — both gameplay and visual effects advance it. Any difference in
    /// RNG call count between Rust and original code causes replay desync.
    pub fn advance_rng(&mut self) -> u32 {
        let rng = openwa_core::rng::wa_lcg((self.frame_counter as u32).wrapping_add(self.game_rng));
        self.game_rng = rng;
        rng
    }

    /// Record a landing-event point in the per-kind bbox slot at
    /// [`render_entries`](Self::render_entries)`[idx]`. Pure Rust port of WA
    /// 0x00547D10 (`fastcall(ECX=x, EDX=y, [ESP+4]=task, [ESP+8]=idx)`,
    /// RET 0x8).
    ///
    /// Behavior:
    /// - Gated on `_field_45e8 == 0` — no-op when the gate is set.
    /// - On first hit (`active == 0`): initializes the slot to a single-point
    ///   bbox `(x, y)` and arms `active = 1`.
    /// - On subsequent hits: expands `min_x/_y` / `max_x/_y` to include `(x, y)`.
    ///
    /// Slot index is the event kind (1, 2, 3, 4, 9, 11 — see
    /// [`WormEntity::landing_check_raw`](crate::task::WormEntity::landing_check_raw)).
    pub unsafe fn record_landing_event_raw(this: *mut GameWorld, idx: u32, x: i32, y: i32) {
        unsafe {
            if (*this)._field_45e8 != 0 {
                return;
            }
            let entry = (*this).render_entries.as_mut_ptr().add(idx as usize);
            if (*entry).active == 0 {
                (*entry).active = 1;
                (*entry).min_x = x;
                (*entry).max_x = x;
                (*entry).min_y = y;
                (*entry).max_y = y;
                return;
            }
            if x < (*entry).min_x {
                (*entry).min_x = x;
            }
            if x > (*entry).max_x {
                (*entry).max_x = x;
            }
            if y < (*entry).min_y {
                (*entry).min_y = y;
            }
            if y > (*entry).max_y {
                (*entry).max_y = y;
            }
        }
    }

    /// Reset the effect-event point at +0x73A0..+0x73AC to `(x, y)` and arm the
    /// gate flag at +0x739C. Pure Rust port of WA 0x547E70 (was bridged as
    /// `set_gravity_center`; the misnamed "gravity" interpretation is wrong).
    ///
    /// The WA function disassembles into a "reset bbox / expand bbox" pattern,
    /// but the entry path unconditionally zeros the gate before checking it,
    /// so the JNZ at 0x00547E8E is never taken — the "expand" branch is dead
    /// code. Faithful port: always reset to single point + arm gate.
    ///
    /// `_raw` form per the project's noalias rule: callers commonly pass a
    /// pointer obtained from a WA bridge call, so we can't claim `&mut self`.
    pub unsafe fn register_event_point_raw(this: *mut GameWorld, x: Fixed, y: Fixed) {
        unsafe {
            (*this).camera_x = x;
            (*this).camera_target_x = x;
            (*this).camera_y = y;
            (*this).camera_target_y = y;
            (*this).render_state_flag = 1;
        }
    }

    /// Advance the secondary effect RNG at GameWorld+0x45F0 and return the new state.
    ///
    /// Formula: `rng = rng * 0x19660D + 0x3C6EF35F` (simpler than [`advance_rng`] — no
    /// frame_counter). Uses `team_health_ratio[0]`, the unused index-0 slot of the
    /// 1-indexed health ratio array, repurposed by WA as a secondary RNG for weapon
    /// release visual effects.
    pub fn advance_effect_rng(&mut self) -> u32 {
        let rng = openwa_core::rng::wa_lcg(self.team_health_ratio[0] as u32);
        self.team_health_ratio[0] = rng as i32;
        rng
    }

    /// Read a per-team sound ID from the team sound table at GameWorld+0x7768.
    ///
    /// The table has stride 0xF0 per team (240-byte per-team config blocks).
    /// The u32 at the base of each block is a sound ID used for type-2 (rope)
    /// weapon release sounds.
    ///
    /// # Safety
    /// `team_id` must be a valid team index (0–5).
    pub unsafe fn team_sound_id(&self, team_id: u32) -> u32 {
        unsafe {
            let base = (self as *const GameWorld as *const u8).add(0x7768);
            *(base.add((team_id as usize) * 0xF0) as *const u32)
        }
    }

    /// Read a per-team damage-grunt sound ID. Each team has three consecutive
    /// sound IDs starting at offset 0x24 inside its per-team config block
    /// (so absolute offset `0x778C + team * 0xF0 + slot * 4`). The damage
    /// paths in `WormEntity::HandleMessage` (cases 0x1C/0x76 and 0x4B) pick
    /// `slot = AdvanceGameRNG() % 3` for variation.
    ///
    /// # Safety
    /// `team_id` must be a valid team index (0–5) and `slot` ∈ `0..=2`.
    pub unsafe fn team_damage_grunt_id(&self, team_id: u32, slot: u32) -> u32 {
        unsafe {
            let base = (self as *const GameWorld as *const u8).add(0x778C);
            *(base.add((team_id as usize) * 0xF0 + (slot as usize) * 4) as *const u32)
        }
    }

    /// Get a mutable pointer to a per-team/per-worm weapon stat counter.
    ///
    /// Four counters exist at GameWorld base offsets 0x40CC, 0x40D0, 0x40D4, 0x40D8,
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
        unsafe {
            (self as *mut GameWorld as *mut u8)
                .add(base_offset)
                .add((team_id as usize) * 0x51C)
                .add((worm_id as usize) * 0x9C) as *mut i32
        }
    }

    /// Show the "too many objects" warning on the HUD.
    ///
    /// Sets `hud_status_code = 6` and loads `LOG_CRASH_TOO_MANY_OBJECTS` into
    /// `hud_status_text`. Only writes if `game_info.game_version < 60`.
    ///
    /// # Safety
    /// `self.game_info` must be valid.
    pub unsafe fn show_pool_overflow_warning(&mut self) {
        unsafe {
            use crate::wa::string_resource::{res, wa_load_string};

            let game_version = (*self.game_info).game_version;
            if game_version < 0x3C {
                self.hud_status_code = 6;
                self.hud_status_text = wa_load_string(res::LOG_CRASH_TOO_MANY_OBJECTS);
            }
        }
    }

    /// Listener position for 3D audio — stored at GameWorld+0x8CEC (viewport_coords[3]).
    ///
    /// Returns `(x, y)` as raw i32 values (fixed-point 16.16).
    /// Used by ComputeDistanceParams / Distance3D_Attenuation.
    pub fn listener_pos(&self) -> (i32, i32) {
        (
            self.viewport_coords[3].center_x.0,
            self.viewport_coords[3].center_y.0,
        )
    }
}

/// Well-known byte offsets into GameWorld, for use with raw pointer access.
///
/// The GameWorld pointer is at GameRuntime+0x488 (DWORD index 0x122).
pub mod offsets {
    /// Byte offset from TeamArena base back to TeamBlock array start.
    /// `blocks_ptr = (tws_base as *const c_char).sub(ARENA_TO_BLOCKS) as *const TeamBlock`
    ///
    /// entry_ptr(0) = GameWorld+0x4628 = TEAM_BLOCKS + 0x598.
    /// 0x598 = sizeof(TeamBlock) + 0x7C = one block + offset into TeamHeader.
    pub const ARENA_TO_BLOCKS: usize = 0x598;
}

// ── Snapshot impls ──────────────────────────────────────────

impl crate::snapshot::Snapshot for GameWorld {
    unsafe fn write_snapshot(
        &self,
        w: &mut dyn core::fmt::Write,
        indent: usize,
    ) -> core::fmt::Result {
        use crate::snapshot::{fmt_ptr, write_indent};
        let i = indent;

        write_indent(w, i)?;
        writeln!(w, "frame_counter = {}", self.frame_counter)?;
        write_indent(w, i)?;
        writeln!(w, "game_speed = {}", self.game_speed)?;
        write_indent(w, i)?;
        writeln!(w, "game_speed_target = {}", self.game_speed_target)?;
        write_indent(w, i)?;
        writeln!(
            w,
            "rng_state = 0x{:08X} 0x{:08X}",
            self.rng_state_1, self.rng_state_2
        )?;
        write_indent(w, i)?;
        writeln!(w, "camera = ({}, {})", self.camera_x, self.camera_y)?;
        write_indent(w, i)?;
        writeln!(
            w,
            "camera_target = ({}, {})",
            self.camera_target_x, self.camera_target_y
        )?;
        write_indent(w, i)?;
        writeln!(w, "level_size = {}x{}", self.level_width, self.level_height)?;
        write_indent(w, i)?;
        writeln!(
            w,
            "is_cavern = {}, level_height_raw = {}",
            self.is_cavern, self.level_height_raw
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

        // TeamArena summary
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
