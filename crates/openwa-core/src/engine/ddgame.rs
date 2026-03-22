use crate::address::va;
use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::audio::speech::SpeechSlotTable;
use crate::display::dd_display::DDDisplay;
use crate::display::gradient::compute_complex_gradient;
use crate::display::palette::Palette;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_info::GameInfo;
use crate::engine::net_bridge::NetBridge;
use crate::game::weapon::WeaponTable;
use crate::input::keyboard::DDKeyboard;
use crate::rebase::rb;
// Re-export public GfxHandler functions so existing `engine::ddgame::*` imports keep working.
use crate::render::gfx_dir::{
    call_gfx_find_and_load, call_gfx_load_and_wrap, call_gfx_load_dir, GfxDir,
};
pub use crate::render::gfx_dir::{gfx_dir_find_entry, gfx_dir_load_dir, gfx_resource_create};
use crate::render::landscape::PCLandscape;
use crate::render::queue::RenderQueue;
use crate::render::turn_order::TurnOrderWidget;
use crate::task::bit_grid::BitGrid;
use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

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
    /// 0x138: DisplayGfx object pointer (vtable 0x664144)
    pub display_gfx: *mut u8,
    /// 0x13C-0x37F: Sprite/image object cache (145 pointer slots).
    /// All populated entries have vtable 0x664144 (same class as `display_gfx`).
    /// Not initialized in DDGame__Constructor — filled during gameplay with
    /// weapon sprites, effect images, cursor graphics, etc.
    pub sprite_cache: [*mut u8; 145],
    /// 0x380: BitGrid pointer (vtable 0x664118, 0x2C bytes)
    pub bit_grid: *mut u8,
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
    /// 0x4620-0x4627: Unknown
    pub _unknown_4620: [u8; 0x4628 - 0x4620],
    /// 0x4628: Team arena state — per-team data, ammo, delays, alliance tracking.
    /// Note: fields previously named init_field_64d8 (= team_count at arena+0x1EB0)
    /// and init_field_72a4 (= weapon_slots entry at arena+0x2A7C) are inside this struct.
    pub team_arena: TeamArenaState,
    /// 0x7270-0x72D7: Unknown
    pub _unknown_7270: [u8; 0x72D8 - 0x7270],

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
    /// 0x72F4-0x7307: Unknown
    pub _unknown_72f4: [u8; 0x7308 - 0x72F4],
    /// 0x7308: Sprite/gfx dimension data (passed to GFX_DIR_LOAD_SPRITES).
    pub gfx_sprite_data: [u8; 0x730C - 0x7308],
    /// 0x730C-0x732F: GfxDir color table (9 entries).
    /// Populated from colours.img pixel row: `color_table[i] = get_pixel(sprite, i, 0)`.
    /// Known entries:
    /// - [6] (0x7324): Crosshair line color (DrawPolygon param_2)
    /// - [8] (0x732C): Crosshair line style (DrawPolygon param_1)
    pub gfx_color_table: [u32; 9],
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
    /// 0x74C8-0x764F: Unknown
    pub _unknown_74c8: [u8; 0x7650 - 0x74C8],

    /// 0x7650: Team index permutation maps (3 × 0x64 bytes).
    /// Used for team-to-slot mapping (render order, turn order, display order).
    /// Initialized as identity permutations [0..15] with count=16.
    pub team_index_maps: [TeamIndexMap; 3],
    /// 0x777C: Level width output (written by PCLandscape constructor param 10).
    pub level_width_raw: u32,
    /// 0x7780: Level height output (written by PCLandscape constructor param 11).
    pub level_height_raw: u32,
    /// 0x7784-0x779B: Unknown
    pub _unknown_7784: [u8; 0x779C - 0x7784],

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
    /// Cleared by DSSound_LoadAllSpeechBanks (0x571A70), filled by DDGameWrapper__LoadSpeechWAV (0x571530).
    pub speech_slot_table: SpeechSlotTable,

    /// 0x7D84-0x7E24: Unknown
    pub _unknown_7d84: [u8; 0x7E25 - 0x7D84],
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
    /// 0x7E40: Fast-forward hurry flag (byte).
    pub hurry_flag: u8,
    /// 0x7E41-0x7E9F: Unknown
    pub _unknown_7e41: [u8; 0x7EA0 - 0x7E41],

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
    /// 0x7EF4: Unknown.
    pub field_7ef4: u32,
    /// 0x7EF8: Sound available flag (1 when game_info+0xF914 == 0, i.e. not headless).
    pub sound_available: u32,
    /// 0x7EFC: Always initialized to 1 in constructor.
    pub field_7efc: u32,

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

    /// 0x98B4-0x98B7: Unknown
    pub _unknown_98b4: [u8; 0x98B8 - 0x98B4],
}

const _: () = assert!(core::mem::size_of::<DDGame>() == 0x98B8);

// ============================================================
// DDGame constructor — replaces DDGame__Constructor (0x56E220)
// ============================================================
//
// Despite being named DDGame__Constructor, the original function
// receives DDGameWrapper* as `this` and creates DDGame internally.
// It populates fields on BOTH the wrapper and the inner DDGame.
// The Rust entry point is DDGameWrapper::create_game() in
// ddgame_wrapper.rs; the bulk of the logic lives here because
// it primarily initializes DDGame fields.

// ============================================================
// Pure Rust implementations of DDGame sub-functions
// ============================================================
// These are called both by create_ddgame() and by MinHook
// trampolines in openwa-wormkit/replacements/ddgame_init.rs.

/// Pure Rust implementation of DDGame__InitFields (0x526120).
///
/// Zeroes stride-0x194 table entries, calls init_render_indices,
/// then zeroes coordination/sound entries at 0x8Cxx and 0x98xx.
///
/// # Safety
/// `ddgame` must point to a valid, zero-filled DDGame allocation (0x98B8 bytes).
pub unsafe fn ddgame_init_fields(ddgame: *mut DDGame) {
    let base = ddgame as *mut u8;

    // Zero the stride-0x194 table (10 entries starting at 0x379C).
    // These offsets are deep in the unknown 0x2E00-0x45EB region and don't
    // have named fields yet — keep as raw offsets for now.
    for &off in &[
        0x379Cusize, 0x3930, 0x3AC4, 0x3C58, 0x3DEC,
        0x3F80, 0x4114, 0x42A8, 0x443C, 0x45D0,
    ] {
        *(base.add(off) as *mut u32) = 0;
    }

    // init_field_64d8 = TeamArenaState.team_count (arena+0x1EB0)
    (*ddgame).team_arena.team_count = 0;
    // init_field_72a4 = weapon_slots flat[754] = alliance 5, ammo[44]
    (*ddgame).team_arena.weapon_slots.teams[5].ammo[44] = 0;

    // InitRenderIndices — original sets ESI = ddgame + 0x72D8, now uses typed DDGame ptr
    ddgame_init_render_indices(ddgame);

    // Zero x and y of each screen coordinate entry (4 entries each)
    for entry in &mut (*ddgame).screen_coords {
        entry.x = 0;
        entry.y = 0;
    }
    for entry in &mut (*ddgame).screen_coords_2 {
        entry.x = 0;
        entry.y = 0;
    }
}

/// Pure Rust implementation of DDGame__InitRenderIndices (0x526080).
///
/// Convention: usercall(ESI=base_ptr), plain RET.
/// Initialize render state flag, render entry table, and team index maps.
///
/// Original: called with ESI = ddgame + 0x72D8, but now uses typed DDGame fields.
///
/// # Safety
/// `ddgame` must point to a valid DDGame allocation.
pub unsafe fn ddgame_init_render_indices(ddgame: *mut DDGame) {
    (*ddgame).render_state_flag = 0;

    // Zero the active flag of each render entry (eh_vector_constructor_iterator).
    for entry in &mut (*ddgame).render_entries {
        entry.active = 0;
    }

    // Initialize all three team index maps as identity permutations.
    for map in &mut (*ddgame).team_index_maps {
        for i in 0..16i16 {
            map.entries[i as usize] = i;
        }
        map.count = 0x10;
        map.terminator = 0;
    }
}

// BitGrid__Init moved to crate::task::bit_grid
pub use crate::task::bit_grid::bit_grid_init;

/// Pure Rust implementation of FUN_570E20 (display layer color init).
///
/// Convention: usercall(ESI=wrapper), plain RET.
///
/// Sets display layer color parameters via display->vtable[4](layer, color).
/// Layer 1 color depends on gfx_mode and game_version.
/// Layer 2 = 0x20, Layer 3 = 0x70.
///
/// # Safety
/// `wrapper` must be a valid DDGameWrapper with initialized display and ddgame.
#[cfg(target_arch = "x86")]
pub unsafe fn display_layer_color_init(wrapper: *mut DDGameWrapper) {
    let ddgame = (*wrapper).ddgame;
    let game_info = (*ddgame).game_info;
    let game_version = (*game_info).game_version;

    // wrapper+0x4C8 is gfx_mode (at DDGameWrapper.gfx_mode)
    let layer1_color = if (*wrapper).gfx_mode == 0 {
        // (game_version > -2) - 1: yields 0 if true, -1 if false
        // Then + 0x69: yields 0x69 or 0x68
        if game_version > -2 {
            0x69i32
        } else {
            0x68i32
        }
    } else {
        5 + 0x69 // = 0x6E
    };

    let display = (*wrapper).display;
    DDDisplay::set_layer_color(display, 1, layer1_color);
    DDDisplay::set_layer_color(display, 2, 0x20);
    DDDisplay::set_layer_color(display, 3, 0x70);
}

/// Initialize runtime addresses for the constructor bridges.
/// Must be called once at DLL startup (from lib.rs or similar).
pub fn init_constructor_addrs() {
    unsafe {
        SPRITE_REGION_CTOR_ADDR = rb(va::SPRITE_REGION_CONSTRUCTOR);
        FUN_570A90_ADDR = rb(va::FUN_570A90);
        FUN_570F30_ADDR = rb(va::DDGAME_INIT_SOUND_PATHS);
        LOAD_SPEECH_BANKS_ADDR = rb(va::DSSOUND_LOAD_ALL_SPEECH_BANKS);
        LOADING_PROGRESS_TICK_ADDR = rb(va::DDGAME_WRAPPER_LOADING_PROGRESS_TICK);
        GFX_LOAD_SPRITES_ADDR = rb(va::GFX_DIR_LOAD_SPRITES);
    }
    crate::render::gfx_dir::init_addrs();
}

// ── Typed wrappers for WA stdcall functions ────────────────────────────────
// Each wraps a single WA function with a typed Rust signature.
// Replace with pure Rust implementations as functions are ported.

/// DDGame__InitVersionFlags (0x525BE0): sets DDGame+0x7E2E/0x7E2F/0x7E3F.
#[cfg(target_arch = "x86")]
unsafe fn wa_init_version_flags(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_VERSION_FLAGS) as usize);
    f(wrapper);
}

/// GfxHandler__LoadSprites (0x570B50): usercall(ESI=layer_ctx) + stdcall(4 params).
///
/// ESI must hold the display layer context (from DDDisplay::set_active_layer).
/// The function uses ESI for LoadSpriteFromVfs and GfxResource__Create_Maybe
/// when param4 (gfx_dir) is non-null.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn wa_load_sprites(
    _wrapper: *mut DDGameWrapper,
    _sprite_data: *mut u8,
    _display_flags: u32,
    _param4: u32,
    _layer_ctx: *mut u8, // → ESI
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, [esp+24]",  // layer_ctx (5th param: 4 saved + 4 ret + 4*4 params + 4 = 24)
        "push [esp+20]",      // param4
        "push [esp+20]",      // display_flags
        "push [esp+20]",      // sprite_data
        "push [esp+20]",      // wrapper
        "call [{addr}]",
        "pop esi",
        "ret",
        addr = sym GFX_LOAD_SPRITES_ADDR,
    );
}

static mut GFX_LOAD_SPRITES_ADDR: u32 = 0;

/// DSSound_LoadEffectWAVs (0x571660): load sound effect WAVs.
#[cfg(target_arch = "x86")]
unsafe fn wa_load_effect_wavs(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DSSOUND_LOAD_EFFECT_WAVS) as usize);
    f(wrapper);
}

/// PCLandscape__Constructor (0x57ACB0): construct landscape object (0xB44 bytes, 11 params).
///
/// param_5 is `game_info + 0xDAAC` (landscape data path region within GameInfo).
/// The SoundEmitter sub-constructor reads paths, water settings, etc. from this
/// pointer with negative offsets back into GameInfo.
#[cfg(target_arch = "x86")]
unsafe fn wa_pc_landscape_ctor(
    this: *mut u8,
    ddgame: *mut DDGame,
    gfx_resource: *mut u8,
    display: *mut DDDisplay,
    landscape_data: *const u8,
    byte_output: *mut u8,
    gfx_mode: u32,
    temp_buf: *mut u32,
    coord_output: *mut u32,
    width_output: *mut u32,
    height_output: *mut u32,
) -> *mut u8 {
    let f: unsafe extern "stdcall" fn(
        *mut u8,
        *mut DDGame,
        *mut u8,
        *mut DDDisplay,
        *const u8,
        *mut u8,
        u32,
        *mut u32,
        *mut u32,
        *mut u32,
        *mut u32,
    ) -> *mut u8 = core::mem::transmute(rb(va::PC_LANDSCAPE_CONSTRUCTOR) as usize);
    f(
        this,
        ddgame,
        gfx_resource,
        display,
        landscape_data,
        byte_output,
        gfx_mode,
        temp_buf,
        coord_output,
        width_output,
        height_output,
    )
}

/// DisplayGfx__Constructor (0x4F5E80): wrap raw image data in DisplayGfx object.
#[cfg(target_arch = "x86")]
unsafe fn wa_displaygfx_ctor(raw_image: *mut u8) -> *mut u8 {
    let f: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
        core::mem::transmute(rb(va::DISPLAYGFX_CONSTRUCTOR) as usize);
    f(raw_image)
}

/// DDGame__InitDisplayFinal (0x56A830): finalize display for non-headless mode.
#[cfg(target_arch = "x86")]
unsafe fn wa_init_display_final(display: *mut DDDisplay) {
    let f: unsafe extern "stdcall" fn(*mut DDDisplay) =
        core::mem::transmute(rb(va::DDGAME_INIT_DISPLAY_FINAL) as usize);
    f(display);
}

/// DDGame__LoadHudAndWeaponSprites (0x53D0E0): load weapon icons and HUD sprites.
/// thiscall(ECX=gfx_dir) + 2 stack(ddgame, secondary_gfx_dir), RET 0x8.
#[cfg(target_arch = "x86")]
unsafe fn wa_load_hud_sprites(gfx_dir: *mut u8, ddgame: *mut DDGame, secondary: *mut u8) {
    let f: unsafe extern "thiscall" fn(*mut u8, *mut DDGame, *mut u8) =
        core::mem::transmute(rb(va::DDGAME_LOAD_HUD_AND_WEAPON_SPRITES) as usize);
    f(gfx_dir, ddgame, secondary);
}

/// DDGame__InitPaletteGradientSprites (0x5706D0): creates DisplayGfx palette
/// gradient objects for each team. stdcall(wrapper), RET 0x4.
#[cfg(target_arch = "x86")]
unsafe fn wa_init_palette_gradient_sprites(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_PALETTE_GRADIENT_SPRITES) as usize);
    f(wrapper);
}

/// Optional callback invoked right after DDGame allocation (before any field
/// initialization). Used by the hardware watchpoint debugger to arm DR0–DR3.
pub static mut ON_DDGAME_ALLOC: Option<unsafe fn(*mut u8)> = None;

/// Create and initialize DDGame, matching DDGame__Constructor (0x56E220).
///
/// Allocates 0x98B8 bytes from WA's heap, initializes all fields, and creates
/// sub-objects. Populates fields on both `wrapper` (DDGameWrapper) and the
/// returned DDGame.
///
/// # Safety
/// All pointer params must be valid WA objects. `wrapper` must be a
/// partially-initialized DDGameWrapper (vtable, display, sound set).
#[cfg(target_arch = "x86")]
pub unsafe fn create_ddgame(
    wrapper: *mut DDGameWrapper,
    keyboard: *mut DDKeyboard,
    display: *mut DDDisplay,
    sound: *mut DSSound,
    palette: *mut Palette,
    music: *mut Music,
    param7: *mut u8,   // timer object (0x1F4 observed)
    net_game: *mut u8, // from GameSession
    game_info: *mut GameInfo,
    network_ecx: u32, // implicit ECX from caller
) -> *mut DDGame {
    // ── 1. Allocate and zero-fill (matches: memset(piVar3, 0, 0x98B8)) ──
    let ddgame = wa_malloc_zeroed(0x98B8) as *mut DDGame;
    if ddgame.is_null() {
        return core::ptr::null_mut();
    }

    // Notify watchpoint debugger (if active) so it can arm DR0–DR3.
    if let Some(cb) = ON_DDGAME_ALLOC {
        cb(ddgame as *mut u8);
    }

    // ── 2. InitFields — pure Rust (replaces usercall bridge) ──
    ddgame_init_fields(ddgame);

    // ── 3-4. Store params BEFORE exposing DDGame via wrapper ──
    // Critical: game_info must be set before wrapper->ddgame, because the
    // message pump (triggered by audio loading) can cause game tasks to
    // read ddgame->game_info. If game_info is null, they crash.
    (*ddgame).display = display;
    (*ddgame).sound = sound;
    (*ddgame).keyboard = keyboard;
    (*ddgame).palette = palette;
    (*ddgame).music = music;
    (*ddgame).timer_obj = param7;
    (*ddgame).network_ecx = network_ecx;
    (*ddgame).game_info = game_info;
    (*ddgame).net_game = net_game;

    // Now safe to expose — all fields that concurrent readers check are set.
    (*wrapper).ddgame = ddgame;

    // ── 5. Set g_GameInfo global ──
    *(rb(va::G_GAME_INFO) as *mut *mut GameInfo) = game_info;

    // ── 6. Sound available + always-1 flags ──
    let is_headless = (*game_info).headless_mode != 0;
    // sound_available enables loading progress bar, message pump, and sound during construction.
    (*ddgame).sound_available = if is_headless { 0 } else { 1 };
    (*ddgame).field_7efc = 1;

    // ── 7. Network bridge (online games only) ──
    (*wrapper).net_bridge = core::ptr::null_mut();

    if (*game_info).game_version == -2 {
        let bridge = wa_malloc_zeroed(core::mem::size_of::<NetBridge>() as u32) as *mut NetBridge;
        (*bridge).ddgame = ddgame;
        (*bridge).net_config_1 = (*game_info).net_config_1;
        (*bridge).net_config_2 = (*game_info).net_config_2;
        (*wrapper).net_bridge = bridge;
        if network_ecx != 0 {
            *((network_ecx as *mut u8).add(0x18) as *mut *mut NetBridge) = bridge;
        }
    }

    // ── 9. InitVersionFlags — sets DDGame+0x7E2E/0x7E2F/0x7E3F ──
    wa_init_version_flags(wrapper);

    // ── 10. GfxHandler, landscape, sprites, audio, resources ──
    init_graphics_and_resources(wrapper, game_info, net_game, display, is_headless);

    let _ = crate::log::log_line("[DDGame] create_ddgame complete");
    ddgame
}

/// Second half of the constructor: GfxHandler, landscape, sprites, audio, resources.
///
/// Second half of the constructor — initializes graphics, audio, landscape, and sprites.
#[cfg(target_arch = "x86")]
unsafe fn init_graphics_and_resources(
    wrapper: *mut DDGameWrapper,
    game_info: *mut GameInfo,
    _net_game: *mut u8,
    _display: *mut DDDisplay,
    is_headless: bool,
) {
    let ddgame = (*wrapper).ddgame;
    use core::ffi::c_char;
    let fopen: unsafe extern "cdecl" fn(*const c_char, *const c_char) -> *mut u8 =
        core::mem::transmute(rb(va::WA_FOPEN) as usize);
    let gfx_dir_vtable = rb(va::GFX_DIR_VTABLE) as u32;

    // ── GfxDir #1 (primary) ──
    let gfx1 = GfxDir::alloc(gfx_dir_vtable);
    (*wrapper).primary_gfx_dir = gfx1 as *mut u8;
    (*wrapper).secondary_gfx_dir = core::ptr::null_mut();

    // Build path list (order depends on headless + display_flags)
    let headless = (*game_info).headless_mode as i32;
    let display_flags = (*game_info).display_flags as i32;
    let paths: [&core::ffi::CStr; 3] = if headless != 0 {
        if display_flags == 0 {
            [
                c"data\\Gfx\\Gfx.dir",
                c"data\\Gfx\\Gfx0.dir",
                c"data\\Gfx\\Gfx1.dir",
            ]
        } else {
            [
                c"data\\Gfx\\Gfx.dir",
                c"data\\Gfx\\Gfx1.dir",
                c"data\\Gfx\\Gfx0.dir",
            ]
        }
    } else if display_flags == 0 {
        [
            c"data\\Gfx\\Gfx0.dir",
            c"data\\Gfx\\Gfx1.dir",
            c"data\\Gfx\\Gfx.dir",
        ]
    } else {
        [
            c"data\\Gfx\\Gfx1.dir",
            c"data\\Gfx\\Gfx0.dir",
            c"data\\Gfx\\Gfx.dir",
        ]
    };

    let mut gfx_loaded_idx = 0u32;
    for (i, path) in paths.iter().enumerate() {
        let fp = fopen(path.as_ptr(), c"rb".as_ptr());
        (*gfx1).file_handle = fp;
        if !fp.is_null()
            && call_gfx_load_dir(gfx1 as *mut u8, crate::render::gfx_dir::gfx_load_dir_addr()) != 0
        {
            gfx_loaded_idx = i as u32;
            break;
        }
        if i == 2 {
            panic!("DDGame: couldn't open any Gfx.dir");
        }
    }

    let headless_offset = if headless != 0 { 1u32 } else { 0u32 };
    (*wrapper).gfx_mode = if gfx_loaded_idx.wrapping_sub(headless_offset) < 2 {
        1
    } else {
        0
    };

    // ── GfxDir #2 (conditional — supplemental sprites for certain game versions) ──
    let game_version = (*game_info).game_version;
    let threshold = if (*wrapper).gfx_mode != 0 { 33 } else { -2i32 };
    if game_version < threshold {
        let c_digit = if game_version > -3 { b'2' } else { b'1' };
        let mut gfx_c_path = *b"data\\Gfx\\GfxC_3_0.dir\0";
        gfx_c_path[14] = c_digit;

        let gfx2 = GfxDir::alloc(gfx_dir_vtable);
        (*wrapper).secondary_gfx_dir = gfx2 as *mut u8;

        let fp = fopen(gfx_c_path.as_ptr().cast(), c"rb".as_ptr());
        (*gfx2).file_handle = fp;
        let load_addr = crate::render::gfx_dir::gfx_load_dir_addr();
        if fp.is_null() || call_gfx_load_dir(gfx2 as *mut u8, load_addr) == 0 {
            let fp2 = fopen(c"data\\Gfx\\Gfx.dir".as_ptr(), c"rb".as_ptr());
            (*gfx2).file_handle = fp2;
            if fp2.is_null() || call_gfx_load_dir(gfx2 as *mut u8, load_addr) == 0 {
                panic!("DDGame: couldn't open secondary Gfx.dir");
            }
        }
    }

    // ── Display palette setup (non-headless) ──
    if !is_headless {
        if *(rb(va::G_DISPLAY_MODE_FLAG) as *const c_char) == 0 {
            call_usercall_eax(wrapper, FUN_570A90_ADDR);
        }
        let disp = (*wrapper).display;
        let gfx_dir = (*wrapper).primary_gfx_dir;
        DDDisplay::set_layer_color(disp, 1, 0xFE);
        DDDisplay::load_sprite(
            disp,
            1,
            1,
            0,
            gfx_dir,
            rb(va::STR_CDROM_SPR) as *const c_char,
        );
        DDDisplay::set_layer_visibility(disp, 1, -100);

        // Palette slot range init (raw byte offsets into DisplayBase)
        let disp_raw = disp as *mut u8;
        let palette_range_ptr = *(disp_raw.add(0x3120) as *const *mut i16);
        if !palette_range_ptr.is_null() && *(disp_raw.add(0x3534) as *const i32) == 0 {
            let start = *palette_range_ptr as u32;
            let end = (*palette_range_ptr.add(1) as u32) + 1;
            if start < end {
                for i in start..end {
                    *(disp_raw.add(0x312C + i as usize * 4) as *mut u32) = 1;
                }
            }
            crate::wa_alloc::wa_free(*(disp_raw.add(0x3120) as *const *mut u8));
            *(disp_raw.add(0x3120) as *mut u32) = 0;
        }
    }

    // ── FUN_00570E20: usercall(ESI=wrapper), plain RET ──
    // Runs for all modes — headless vtable[4] is 0x5231E0 (same as headful).
    display_layer_color_init(wrapper);

    // ── Display vtable slot 5 (offset 0x14) ──
    // Original: CALL EAX (vtable[5]), saves return value in ESI for use as
    // the `output` parameter in the color-entries GfxResource__Create call below.
    let layer_ctx = DDDisplay::set_active_layer((*ddgame).display, 1);

    // ── GfxDir color entries DDGame+0x730C..0x732C ──
    // Original logic: if gfx_mode!=0, try GfxResource__Create for colours.img.
    // If gfx_mode==0 OR resource creation fails, fall back to LoadSprites.
    // The fallback's 4th param is primary_gfx_dir when gfx_mode==0, or 0 on resource fail.
    if (*wrapper).gfx_mode != 0 {
        let res = gfx_resource_create(
            (*wrapper).primary_gfx_dir,
            rb(va::STR_COLOURS_IMG) as *const c_char,
            layer_ctx,
        );
        if !res.is_null() {
            let rvt = *(res as *const *const u32);
            let get_color: unsafe extern "thiscall" fn(*mut u8, u32, u32) -> u32 =
                core::mem::transmute(*rvt.add(4));
            for i in 0..9u32 {
                (*ddgame).gfx_color_table[i as usize] = get_color(res, i, 0);
            }
            let release: unsafe extern "thiscall" fn(*mut u8, u8) =
                core::mem::transmute(*rvt.add(3));
            release(res, 1);
        } else {
            // Resource creation failed — fallback with param4=0
            wa_load_sprites(
                wrapper,
                (*ddgame).gfx_sprite_data.as_mut_ptr(),
                (*game_info).display_flags,
                0,
                layer_ctx,
            );
        }
    } else {
        // gfx_mode==0 (headless): fallback LoadSprites with param4=primary_gfx_dir
        wa_load_sprites(
            wrapper,
            (*ddgame).gfx_sprite_data.as_mut_ptr(),
            (*game_info).display_flags,
            (*wrapper).primary_gfx_dir as u32,
            layer_ctx,
        );
    }

    // ── Secondary GfxDir object (DDGame+0x2C, conditional) ──
    if !(*wrapper).secondary_gfx_dir.is_null() {
        let gfxdir2 = wa_malloc_zeroed(0x70C);
        *(gfxdir2 as *mut u16) = 1;
        *(gfxdir2.add(2) as *mut u16) = 0x5A;
        // FUN_5411A0: usercall(EAX=gfxdir2), plain RET
        call_usercall_eax(gfxdir2 as *mut DDGameWrapper, rb(va::PALETTE_CONTEXT_INIT));
        *(gfxdir2.add(0x708) as *mut u16) = 0;
        (*ddgame).secondary_gfxdir = gfxdir2;
        // param4=0 so the ESI-dependent block is skipped; layer_ctx doesn't matter
        wa_load_sprites(
            wrapper,
            (*ddgame).gfx_sprite_data.as_mut_ptr(),
            (*game_info).display_flags,
            0,
            core::ptr::null_mut(),
        );
    }

    // ── DDGameWrapper field inits ──
    (*wrapper).loading_progress = 0;
    if is_headless {
        (*wrapper).loading_total = 0x2AD;
    } else {
        let team_count = (*game_info).speech_team_count as u32;
        (*wrapper).loading_total = team_count * 0x38 + 0x7E + 0x2AD;
    }
    (*wrapper).loading_last_pct = 0xFFFFFF9C; // -100: forces first progress bar update
    (*wrapper).speech_name_count = 0;

    // ── Audio init (non-headless + sound available) ──
    if !is_headless {
        // FUN_570F30: usercall(ESI=wrapper)
        call_usercall_esi(wrapper, FUN_570F30_ADDR);
        if !(*ddgame).sound.is_null() {
            wa_load_effect_wavs(wrapper);
            // DSSound_LoadAllSpeechBanks: the original is hooked to our Rust
            // replacement (speech.rs), so the usercall bridge calls our code.
            call_usercall_esi(wrapper, LOAD_SPEECH_BANKS_ADDR);
            // Allocate ActiveSoundTable (0x608 bytes)
            let ast = wa_malloc(0x608) as *mut ActiveSoundTable;
            core::ptr::write_bytes(ast as *mut u8, 0, 0x600);
            (*ast).ddgame = ddgame;
            (*ast).counter = 0;
            (*ddgame).active_sounds = ast;
        }
    }

    // ── GfxResource for masks.img ──
    // The original constructor uses a stack-local PaletteContext buffer (ESP+0x1A4)
    // initialized at the start with (word=1, word=0xFE, PaletteContext__Init).
    // This same buffer is reused as the output param for GfxResource__Create(masks.img).
    // We replicate this initialization here. Size 0x900 matches the stack allocation.
    let gfx_resource: *mut u8;
    {
        let gfx_dir = (*wrapper).primary_gfx_dir;
        // Create PaletteContext the same way the original DDGame constructor does:
        // word[0]=1, word[1]=0xFE, then PaletteContext__Init (0x5411A0)
        let palette_ctx = wa_malloc_zeroed(0x900);
        *(palette_ctx as *mut u16) = 1;
        *(palette_ctx.add(2) as *mut u16) = 0xFE;
        call_usercall_eax(palette_ctx as *mut DDGameWrapper, rb(va::PALETTE_CONTEXT_INIT));
        gfx_resource =
            gfx_resource_create(gfx_dir, rb(va::STR_MASKS_IMG) as *const c_char, palette_ctx);
    }

    // ── Dump GfxResource for A/B comparison ──
    if !gfx_resource.is_null() {
        let use_orig = std::env::var("OPENWA_USE_ORIG_CTOR").is_ok();
        let tag = if use_orig { "orig" } else { "rust" };
        // Dump GfxResource object + first sub-object
        let gr_data = core::slice::from_raw_parts(gfx_resource, 0x100);
        let _ = std::fs::write(format!("gfx_resource_{}.bin", tag), gr_data);
    }

    // ── PCLandscape (alloc 0xB44, stdcall 11 params) ──
    // Temporary output buffers for landscape coordinate data (used later for coord_list).
    // These were stack locals in the original code (aiStack_978, iStack_11f9).
    let mut landscape_coords_buf = [0u32; 0x400]; // coord output: pairs of (x, y)
    let mut landscape_byte_buf = [0u8; 0x100]; // byte output
    let mut landscape_temp = [0u32; 0x400]; // [0] = coord count, rest = temp data

    let landscape = {
        let alloc = wa_malloc_zeroed(0xB44);
        if !alloc.is_null() {
            let result = wa_pc_landscape_ctor(
                alloc,
                ddgame,
                gfx_resource,
                (*wrapper).display,
                (*game_info).landscape_data_path.as_ptr(),
                landscape_byte_buf.as_mut_ptr(),
                (*wrapper).gfx_mode,
                landscape_temp.as_mut_ptr(),
                landscape_coords_buf.as_mut_ptr(),
                &raw mut (*ddgame).level_width_raw,
                &raw mut (*ddgame).level_height_raw,
            );
            (*wrapper).landscape = result as *mut PCLandscape;
            (*ddgame).landscape = result as *mut PCLandscape;
            result
        } else {
            (*wrapper).landscape = core::ptr::null_mut();
            (*ddgame).landscape = core::ptr::null_mut();
            core::ptr::null_mut()
        }
    };

    // ── BitGrid at DDGame+0x380 (alloc 0x4C, memset 0x2C) ──
    // Pure Rust: allocate object, call bit_grid_init, override vtable.
    {
        let bit_grid = wa_malloc(0x4C);
        core::ptr::write_bytes(bit_grid, 0, 0x2C);
        if !bit_grid.is_null() {
            let width = (*ddgame).level_width;
            let height = (*ddgame).level_height;
            bit_grid_init(bit_grid, 1, width, height);
            *(bit_grid as *mut u32) = rb(va::BIT_GRID_VARIANT_VTABLE);
        }
        (*ddgame).bit_grid = bit_grid;
    }

    // ── 8× SpriteRegion at DDGame+0x46C..0x488 ──
    // SpriteRegion__Constructor: fastcall(ECX, EDX) + 6 stack(this, p2, p3, p4, p5, p6), RET 0x18
    {
        // (array_index, ECX, EDX, p2, p3, p4, p5, p6=gfx_resource)
        // p6 is the GfxResource pointer returned by GfxResource__Create_Maybe.
        let gr = gfx_resource as u32;
        let regions: [(usize, u32, u32, u32, u32, u32, u32, u32); 8] = [
            (2, 0x37, 0x36, 0x2E, 0x24, 0x41, 0x2D, gr),
            (0, 0x30, 0x0C, 0x2D, 0x07, 0x34, 0x09, gr),
            (1, 0x11, 0x1A, 0x0D, 0x0A, 0x16, 0x13, gr),
            (3, 0x0C, 0x3D, 0x00, 0x20, 0x18, 0x33, gr),
            (4, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, gr),
            (6, 0x1A2, 0x1B, 0x173, 0x09, 0x1D8, 0x03, gr),
            (7, 0x1EF, 0x26, 0x1E5, 0x07, 0x1F9, 0x16, gr),
            (5, 0x2D, 0x08, 0x2D, 0x07, 0x2E, 0x07, gr),
        ];

        for &(idx, ecx, edx, p2, p3, p4, p5, p6) in &regions {
            let alloc = wa_malloc_zeroed(0x9C);
            let result = if !alloc.is_null() {
                call_sprite_region_ctor(alloc, ecx, edx, p2, p3, p4, p5, p6)
            } else {
                core::ptr::null_mut()
            };
            (*ddgame).sprite_regions[idx] = result;
        }
    }

    // ── Landscape property at DDGame+0x468 (PCLandscape vtable[0xB]) ──
    if !landscape.is_null() {
        let land_vt = *(landscape as *const *const u32);
        let get_val: unsafe extern "thiscall" fn(*mut u8) -> u32 =
            core::mem::transmute(*land_vt.add(0xB));
        (*ddgame).landscape_property = get_val(landscape);
    }

    // NOTE: gfx_resource is NOT released here — arrow SpriteRegions need it.
    // It's released after the arrow loop below.

    // ── Arrow sprites + collision regions (32 iterations) ──
    {
        let gfx_dir = (*wrapper).primary_gfx_dir;

        for i in 0u32..32 {
            // Format "arrow%02u.img\0" into stack buffer
            let mut name_buf = *b"arrow00.img\0\0\0\0\0";
            name_buf[5] = b'0' + (i / 10) as u8;
            name_buf[6] = b'0' + (i % 10) as u8;

            let layer_ctx = DDDisplay::set_active_layer((*ddgame).display, 1);

            let entry = gfx_dir_find_entry(name_buf.as_ptr().cast(), gfx_dir);

            let sprite: *mut u8;
            if !entry.is_null() {
                // Try gfx_dir->vtable[2](entry->field_4)
                let gfx_vt = *(gfx_dir as *const *const u32);
                let load_cached: unsafe extern "thiscall" fn(*mut u8, u32) -> *mut u8 =
                    core::mem::transmute(*gfx_vt.add(2));
                let entry_val = *(entry.add(4) as *const u32);
                let cached = load_cached(gfx_dir, entry_val);
                if !cached.is_null() {
                    sprite = wa_displaygfx_ctor(cached);
                } else {
                    // Fallback: load from file via GfxDir__LoadImage + IMG_Decode
                    sprite = call_gfx_load_and_wrap(gfx_dir, name_buf.as_ptr().cast(), layer_ctx);
                }
            } else {
                // Entry not found — try direct file load
                sprite = call_gfx_load_and_wrap(gfx_dir, name_buf.as_ptr().cast(), layer_ctx);
            }

            // Store arrow sprite at DDGame+0x38+i*4
            (*ddgame).arrow_sprites[i as usize] = sprite;

            // Calculate collision region dimensions from sprite
            if !sprite.is_null() {
                let tsm = &*(sprite as *const BitGrid);
                let sprite_w = tsm.width as i32;
                let sprite_h = tsm.height as i32;
                let half_w = (sprite_w / 2 - 10).max(0);
                let half_h = (sprite_h / 2 - 10).max(0);

                // Create SpriteRegion for collision
                let alloc = wa_malloc_zeroed(0x9C);
                let region = if !alloc.is_null() {
                    // SpriteRegion params: ECX, EDX, this, p2, p3, p4, p5, p6
                    // this[3] = p4 - p2 (width), this[4] = EDX - p3 (height)
                    // For arrows: region from (0,0) to (half_w, half_h)
                    call_sprite_region_ctor(
                        alloc,
                        0,                   // ECX (x_max for this[5])
                        half_h as u32,       // EDX (y_max → this[4] = EDX - p3)
                        0,                   // p2 (x_offset)
                        0,                   // p3 (y_offset)
                        half_w as u32,       // p4 (x_limit → this[3] = p4 - p2)
                        half_h as u32,       // p5 (y_limit for this[6])
                        gfx_resource as u32, // p6 (gfx_resource)
                    )
                } else {
                    core::ptr::null_mut()
                };
                (*ddgame).arrow_collision_regions[i as usize] = region;
            }

            // Arrow GfxDir (conditional on secondary gfxdir)
            if !(*ddgame).secondary_gfxdir.is_null() {
                let gfx_resource_create: unsafe extern "thiscall" fn(*mut u8, *mut u8) -> *mut u8 =
                    core::mem::transmute(rb(va::GFX_RESOURCE_CREATE) as usize);
                (*ddgame).arrow_gfxdirs[i as usize] =
                    gfx_resource_create(gfx_dir, core::ptr::null_mut());
            }
        }
    }

    // Release gfx_resource AFTER arrow loop (arrows need it for SpriteRegions)
    // vtable[3] = DisplayGfx__vmethod_3: thiscall(this, byte param_2), RET 4.
    if !gfx_resource.is_null() {
        let rvt = *(gfx_resource as *const *const u32);
        let release: unsafe extern "thiscall" fn(*mut u8, u8) = core::mem::transmute(*rvt.add(3));
        release(gfx_resource, 1);
    }

    // ── DisplayGfx at DDGame+0x138 ──
    {
        let tsm = wa_malloc(0x4C);
        core::ptr::write_bytes(tsm, 0, 0x2C);
        if !tsm.is_null() {
            bit_grid_init(tsm, 8, 0x100, 0x1E0);
            *(tsm as *mut u32) = rb(va::DISPLAY_GFX_VTABLE);
        }
        (*ddgame).display_gfx = tsm;
    }

    // ── CoordList at DDGame+0x50C (capacity 600, 0x12C0 buffer) ──
    {
        let cl = wa_malloc(core::mem::size_of::<CoordList>() as u32) as *mut CoordList;
        (*cl).count = 0;
        (*cl).capacity = 600;
        let data = wa_malloc_zeroed(600 * core::mem::size_of::<CoordListEntry>() as u32)
            as *mut CoordListEntry;
        (*cl).data = data;
        (*ddgame).coord_list = cl;

        // Populate coord_list from PCLandscape's coordinate output.
        // landscape_temp[0] = coordinate count, landscape_coords_buf = pairs of (x, y).
        // Original packs as: coord = x * 0x10000 + y (Fixed-point).
        // Duplicates are skipped.
        let coord_count = landscape_temp[0];
        for j in 0..coord_count as usize {
            let x = landscape_coords_buf[j * 2];
            let y = landscape_coords_buf[j * 2 + 1];
            let coord_val = x.wrapping_mul(0x10000).wrapping_add(y);
            let cur_count = (*cl).count as usize;
            if cur_count >= 600 {
                break;
            }
            // Check for duplicates
            let mut dup = false;
            for k in 0..cur_count {
                if (*data.add(k)).coord == coord_val {
                    dup = true;
                    break;
                }
            }
            if !dup {
                (*data.add(cur_count)).coord = coord_val;
                (*data.add(cur_count)).flag = 1;
                (*cl).count = (cur_count + 1) as u32;
            }
        }
    }

    // Temporary landscape buffers (stack arrays) are dropped automatically here.

    // ── Loading progress ticks (2 of 4 — before load_resource_list) ──
    call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);
    call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);

    // ── Sprite resource loading via DDGameWrapper vtable[0] ──
    // DDNetGameWrapper__LoadResourceList: thiscall(ECX=wrapper) +
    // 5 stack params (layer, gfx_dir, base_path, data_table, table_size)
    {
        let landscape_ptr = (*wrapper).landscape;
        let water_layer = (*landscape_ptr).water_gfx_dir;
        let land_layer = (*landscape_ptr).level_gfx_dir;
        let gfx_dir = (*wrapper).primary_gfx_dir;

        let wrapper_vt = *(wrapper as *const *const u32);
        let load_resource_list: unsafe extern "thiscall" fn(
            *mut DDGameWrapper,
            u32,
            *mut u8,
            *const u8,
            *const u8,
            u32,
        ) = core::mem::transmute(*wrapper_vt);
        // Load resources for layer 1 (main sprites)
        load_resource_list(
            wrapper,
            1,
            gfx_dir,
            rb(va::SPRITE_RESOURCE_BASE_PATH) as *const u8, // base path
            rb(va::SPRITE_RESOURCE_TABLE_1) as *const u8,
            0x1D88, // table size
        );
        // Set global flag based on game version
        let gv = (*(*ddgame).game_info).game_version;
        *(rb(va::G_SPRITE_VERSION_FLAG) as *mut u32) = if gv < 8 { 0 } else { 0x10 };

        // Load resources for layer 1 with different table
        load_resource_list(
            wrapper,
            1,
            gfx_dir,
            rb(va::SPRITE_RESOURCE_BASE_PATH) as *const u8,
            rb(va::SPRITE_RESOURCE_TABLE_2) as *const u8,
            0x18,
        );

        // Load resources for layer 2 (water)
        load_resource_list(
            wrapper,
            2,
            water_layer,
            rb(va::SPRITE_RESOURCE_BASE_PATH) as *const u8,
            rb(va::WATER_RESOURCE_TABLE) as *const u8,
            0x2F4,
        );

        let disp = (*wrapper).display;
        DDDisplay::set_active_layer(disp, 3);

        // back.spr and debris.spr must be loaded unconditionally — they're used by
        // GenerateDebrisParticles (0x546F70) for particle effects, which affects
        // the game RNG (DDGame+0x45EC). The original constructor loads them even
        // in headless mode. Skipping them causes replay desync.
        DDDisplay::load_sprite_by_layer(
            disp,
            3,
            0x26D,
            land_layer,
            c"back.spr".as_ptr().cast(),
        );
        // debris.spr must be loaded unconditionally — it's used by
        // GenerateDebrisParticles (0x546F70) for particle effects, which
        // affects the game RNG (DDGame+0x45EC). Skipping it in headless
        // mode causes desync (longbow replay checksum mismatch at frame 1350).
        DDDisplay::load_sprite(disp, 3, 0x26E, 0, land_layer, c"debris.spr".as_ptr());

        DDDisplay::load_sprite_by_layer(
            disp,
            2,
            0x26C,
            water_layer,
            c"layer\\layer.spr|layer.spr".as_ptr().cast(),
        );

        (*ddgame).gradient_image_2 = core::ptr::null_mut();

        // ── Gradient image (0x030) ──
        let level_height = (*ddgame).level_height as i32;
        let layer3_ctx = DDDisplay::set_active_layer(disp, 3);
        let s_var1 = *(layer3_ctx.add(0x606) as *const i16);

        if s_var1 < 0x61 && level_height == 0x2B8 {
            // Simple gradient: load gradient.img directly
            let gradient = call_gfx_find_and_load(land_layer, c"gradient.img", layer3_ctx);
            (*ddgame).gradient_image = gradient;
        } else {
            compute_complex_gradient(ddgame, land_layer, layer3_ctx, s_var1);
        }

        // ── Fill image → fill_pixel (0x7338) ──
        {
            let layer2_ctx = DDDisplay::set_active_layer((*ddgame).display, 2);
            // In the original, fill.img uses piStack_126c which the decompiler
            // shows was set from piVar3 (water_layer from landscape+0xB38).
            let fill_sprite = call_gfx_find_and_load(water_layer, c"fill.img", layer2_ctx);
            if !fill_sprite.is_null() {
                // Get pixel value: fill_sprite->vtable[4](0, 0)
                let fill_vt = *(fill_sprite as *const *const u32);
                let get_pixel: unsafe extern "thiscall" fn(*mut u8, i32, i32) -> u32 =
                    core::mem::transmute(*fill_vt.add(4));
                (*ddgame).fill_pixel = get_pixel(fill_sprite, 0, 0);
                // Release fill sprite: vtable[3] = DisplayGfx__vmethod_3(this, param_2=1)
                let release: unsafe extern "thiscall" fn(*mut u8, u8) =
                    core::mem::transmute(*fill_vt.add(3));
                release(fill_sprite, 1);
            }
        }

        // ── DDGame__LoadHudAndWeaponSprites (0x53D0E0) ──
        wa_load_hud_sprites(gfx_dir, ddgame, (*wrapper).secondary_gfx_dir);

        // ── DDGame__InitPaletteGradientSprites (0x5706D0) ──
        // Creates DisplayGfx objects at DDGame+0x41C.. for each team's palette
        // gradient data from GameInfo. stdcall(wrapper), RET 0x4.
        wa_init_palette_gradient_sprites(wrapper);

        // ── Loading progress tick (1 of 2 — after InitPaletteGradientSprites) ──
        call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);
    }
    // ── Gradient image stub (DDGame+0x30) ──
    // Minimal stub: [6]=0 (zero-width) so CTaskLand skips the gradient column loop.
    if (*ddgame).gradient_image.is_null() {
        let obj = wa_malloc_zeroed(core::mem::size_of::<BitGrid>() as u32) as *mut BitGrid;
        if !obj.is_null() {
            (*obj).vtable = rb(va::BIT_GRID_VTABLE);
            // height = 0 → CTaskLand skips the gradient column loop
            (*ddgame).gradient_image = obj as *mut u8;
        }
    }

    // ── Release primary GfxHandler (vtable[3] = release, param 1 = free) ──
    let gfx_dir_4c0 = (*wrapper).primary_gfx_dir;
    if !gfx_dir_4c0.is_null() {
        let gfx_vt = *(gfx_dir_4c0 as *const *const u32);
        let release: unsafe extern "thiscall" fn(*mut u8, u32) =
            core::mem::transmute(*gfx_vt.add(3));
        release(gfx_dir_4c0, 1);
    }

    if !is_headless {
        wa_init_display_final((*wrapper).display);
    }

    // ── FUN_00570A90 (second call, conditional) ──
    if *(rb(va::G_DISPLAY_MODE_FLAG) as *const c_char) == 0 {
        call_usercall_eax(wrapper, FUN_570A90_ADDR);
    }

    // ── Final display layer visibility ──
    {
        let disp = (*wrapper).display;
        DDDisplay::set_layer_visibility(disp, 1, 0);
        DDDisplay::set_layer_visibility(disp, 2, 0);
        DDDisplay::set_layer_visibility(disp, 3, 1);
    }

    let _ = crate::log::log_line("[DDGame] init_graphics_and_resources DONE");
}

// Statics for usercall bridge addresses
static mut FUN_570A90_ADDR: u32 = 0;
static mut FUN_570F30_ADDR: u32 = 0;
static mut LOAD_SPEECH_BANKS_ADDR: u32 = 0;
static mut LOADING_PROGRESS_TICK_ADDR: u32 = 0;

/// Bridge: usercall(ESI=wrapper), plain RET. Used by FUN_570E20, FUN_570F30, LoadSpeechBanks.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_usercall_esi(_wrapper: *mut DDGameWrapper, _addr: u32) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",  // ESI = wrapper
        "movl 12(%esp), %eax", // EAX = target address
        "calll *%eax",
        "popl %esi",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(EAX=wrapper), plain RET. Used by FUN_570A90.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_usercall_eax(_wrapper: *mut DDGameWrapper, _addr: u32) {
    core::arch::naked_asm!(
        "movl 4(%esp), %eax", // EAX = wrapper
        "movl 8(%esp), %ecx", // ECX = target address (temp)
        "calll *%ecx",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(ECX=wrapper), plain RET. Used by FUN_5717A0.
#[cfg(target_arch = "x86")]
unsafe fn call_usercall_ecx(wrapper: *mut DDGameWrapper, addr: u32) {
    let f: unsafe extern "thiscall" fn(*mut DDGameWrapper) = core::mem::transmute(addr as usize);
    f(wrapper);
}

/// Bridge to SpriteRegion__Constructor (0x57DB20).
/// Convention: fastcall(ECX, EDX) + 6 stack params, RET 0x18.
#[cfg(target_arch = "x86")]
static mut SPRITE_REGION_CTOR_ADDR: u32 = 0;

#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_sprite_region_ctor(
    _this: *mut u8,
    _ecx: u32,
    _edx: u32,
    _p2: u32,
    _p3: u32,
    _p4: u32,
    _p5: u32,
    _p6: u32,
) -> *mut u8 {
    // Stack on entry: [ret] [this] [ecx] [edx] [p2] [p3] [p4] [p5] [p6]
    // Need: ECX=ecx, EDX=edx, push p6 p5 p4 p3 p2 this, call
    // SpriteRegion__Constructor: fastcall(ECX, EDX) + 6 stack, RET 0x18
    // Params: (this, ecx, edx, p2, p3, p4, p5, p6)
    core::arch::naked_asm!(
        "pushl %ebp",
        "pushl %ebx",
        // Load ECX and EDX (shifted by 2 pushes = 8 bytes)
        "movl 16(%esp), %ecx",   // ecx param (offset 4+8=12? No: 0=ebx,4=ebp,8=ret,12=this,16=ecx)
        "movl 20(%esp), %edx",   // edx param
        // Push 6 stack params in reverse: p6, p5, p4, p3, p2, this
        "pushl 40(%esp)",         // p6 (0=ebx,4=ebp,8=ret,...,40=p6)
        "pushl 40(%esp)",         // p5 (shifted by 4)
        "pushl 40(%esp)",         // p4 (shifted by 8)
        "pushl 40(%esp)",         // p3 (shifted by 12)
        "pushl 40(%esp)",         // p2 (shifted by 16)
        "pushl 32(%esp)",         // this (shifted by 20)
        "calll *({addr})",        // fastcall, callee cleans 6×4=24 bytes
        "popl %ebx",
        "popl %ebp",
        "retl",
        addr = sym SPRITE_REGION_CTOR_ADDR,
        options(att_syntax),
    );
}

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
// Team arena state — sub-struct at DDGame + 0x4628
// ============================================================

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

// ── Snapshot impls ──────────────────────────────────────────

#[cfg(target_arch = "x86")]
impl crate::snapshot::Snapshot for DDGame {
    unsafe fn write_snapshot(&self, w: &mut dyn core::fmt::Write, indent: usize) -> core::fmt::Result {
        use crate::fixed::Fixed;
        use crate::snapshot::{write_indent, fmt_ptr};
        let i = indent;

        write_indent(w, i)?; writeln!(w, "frame_counter = {}", self.frame_counter)?;
        write_indent(w, i)?; writeln!(w, "game_speed = {}", Fixed(self.game_speed))?;
        write_indent(w, i)?; writeln!(w, "game_speed_target = {}", Fixed(self.game_speed_target))?;
        write_indent(w, i)?; writeln!(w, "rng_state = 0x{:08X} 0x{:08X}", self.rng_state_1, self.rng_state_2)?;
        write_indent(w, i)?; writeln!(w, "camera = ({}, {})", Fixed(self.camera_x), Fixed(self.camera_y))?;
        write_indent(w, i)?; writeln!(w, "camera_target = ({}, {})", Fixed(self.camera_target_x), Fixed(self.camera_target_y))?;
        write_indent(w, i)?; writeln!(w, "level_size = {}x{}", self.level_width, self.level_height)?;
        write_indent(w, i)?; writeln!(w, "level_size_raw = {}x{}", self.level_width_raw, self.level_height_raw)?;
        write_indent(w, i)?; writeln!(w, "landscape_property = {}", fmt_ptr(self.landscape_property as *const u8))?;
        write_indent(w, i)?; writeln!(w, "gfx_color_table = {:?}", self.gfx_color_table)?;
        write_indent(w, i)?; writeln!(w, "fast_forward = req={} active={}", self.fast_forward_request, self.fast_forward_active)?;
        write_indent(w, i)?; writeln!(w, "keyboard = {}", fmt_ptr(self.keyboard as *const u8))?;
        write_indent(w, i)?; writeln!(w, "display = {}", fmt_ptr(self.display as *const u8))?;
        write_indent(w, i)?; writeln!(w, "sound = {}", fmt_ptr(self.sound as *const u8))?;
        write_indent(w, i)?; writeln!(w, "game_info = {}", fmt_ptr(self.game_info as *const u8))?;
        write_indent(w, i)?; writeln!(w, "landscape = {}", fmt_ptr(self.landscape as *const u8))?;
        write_indent(w, i)?; writeln!(w, "task_land = {}", fmt_ptr(self.task_land))?;

        // TeamArenaState summary
        write_indent(w, i)?; writeln!(w, "team_arena.team_count = {}", self.team_arena.team_count)?;
        write_indent(w, i)?; writeln!(w, "team_arena.game_phase = {}", self.team_arena.game_phase)?;
        write_indent(w, i)?; writeln!(w, "team_arena.game_mode_flag = {}", self.team_arena.game_mode_flag)?;

        // Dump weapon slots (ammo only, skip zeros)
        write_indent(w, i)?; writeln!(w, "weapon_slots:")?;
        for team in 0..6usize {
            let slots = &self.team_arena.weapon_slots.teams[team];
            let mut has_any = false;
            for wpn in 0..71usize {
                let ammo = slots.ammo[wpn];
                if ammo != 0 {
                    if !has_any {
                        write_indent(w, i + 1)?; write!(w, "team[{}] ammo:", team)?;
                        has_any = true;
                    }
                    if ammo == -1 {
                        write!(w, " {}=inf", wpn)?;
                    } else {
                        write!(w, " {}={}", wpn, ammo)?;
                    }
                }
            }
            if has_any { writeln!(w)?; }
        }

        Ok(())
    }
}

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
