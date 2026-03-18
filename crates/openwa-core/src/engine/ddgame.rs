use crate::address::va;
use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::audio::speech::SpeechSlotTable;
use crate::display::dd_display::DDDisplay;
use crate::display::palette::Palette;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_info::GameInfo;
use crate::input::keyboard::DDKeyboard;
use crate::log::log_line;
use crate::rebase::rb;
use crate::render::landscape::PCLandscape;
use crate::render::queue::RenderQueue;
use crate::render::turn_order::TurnOrderWidget;
use crate::wa_alloc::wa_malloc;

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
    /// Cleared by DSSound_LoadAllSpeechBanks (0x571A70), filled by DDGameWrapper__LoadSpeechWAV (0x571530).
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

    // Zero the stride-0x194 table (10 entries)
    for &off in &[
        0x379Cusize,
        0x3930,
        0x3AC4,
        0x3C58,
        0x3DEC,
        0x3F80,
        0x4114,
        0x42A8,
        0x443C,
        0x45D0,
    ] {
        *(base.add(off as usize) as *mut u32) = 0;
    }

    *(base.add(0x64D8) as *mut u32) = 0;
    *(base.add(0x72A4) as *mut u32) = 0;

    // InitRenderIndices — original sets ESI = ddgame + 0x72D8 before calling
    ddgame_init_render_indices(base.add(0x72D8));

    // Zero 8 fields at 0x8Cxx
    for &off in &[
        0x8CBCusize,
        0x8CC0,
        0x8CCC,
        0x8CD0,
        0x8CDC,
        0x8CE0,
        0x8CEC,
        0x8CF0,
    ] {
        *(base.add(off as usize) as *mut u32) = 0;
    }

    // Zero 8 fields at 0x98xx
    for &off in &[
        0x9850usize,
        0x9854,
        0x9860,
        0x9864,
        0x9870,
        0x9874,
        0x9880,
        0x9884,
    ] {
        *(base.add(off as usize) as *mut u32) = 0;
    }
}

/// Pure Rust implementation of DDGame__InitRenderIndices (0x526080).
///
/// Convention: usercall(ESI=base_ptr), plain RET.
///
/// **Important:** The base pointer is NOT the DDGame pointer!
/// InitFields calls this with ESI = ddgame + 0x72D8.
/// All offsets are relative to whatever ESI points to.
///
/// Absolute DDGame offsets (base = ddgame+0x72D8):
/// - base+0xC4 = ddgame+0x739C
/// - base+0xD8 = ddgame+0x73B0  (eh_vector_constructor_iterator region)
/// - base+0x378 = ddgame+0x7650 (team_index_map_1)
/// - base+0x3DC = ddgame+0x76B4 (team_index_map_2)
/// - base+0x440 = ddgame+0x7718 (team_index_map_3)
///
/// # Safety
/// `base` must point to a valid memory region with at least 0x4A4 bytes.
pub unsafe fn ddgame_init_render_indices(base: *mut u8) {
    // Zero base+0xC4
    *(base.add(0xC4) as *mut u32) = 0;

    // eh_vector_constructor_iterator equivalent:
    // FUN_525F40 is fastcall { *param_1 = 0; }
    // 14 entries at stride 0x14 starting from +0xD8
    for i in 0..14usize {
        *(base.add(0xD8 + i * 0x14) as *mut u32) = 0;
    }

    // Identity permutation 1: base+0x378 (= ddgame+0x7650), 16 entries (i16)
    for i in 0..16i16 {
        *(base.add(0x378 + i as usize * 2) as *mut i16) = i;
    }
    *(base.add(0x398) as *mut u16) = 0x10;
    *(base.add(0x3DA) as *mut u16) = 0;

    // Identity permutation 2: base+0x3DC (= ddgame+0x76B4), 16 entries (i16)
    for i in 0..16i16 {
        *(base.add(0x3DC + i as usize * 2) as *mut i16) = i;
    }
    *(base.add(0x3FC) as *mut u16) = 0x10;
    *(base.add(0x43E) as *mut u16) = 0;

    // Identity permutation 3: base+0x440 (= ddgame+0x7718), 16 entries (i16)
    for i in 0..16i16 {
        *(base.add(0x440 + i as usize * 2) as *mut i16) = i;
    }
    *(base.add(0x4A2) as *mut u16) = 0;
    *(base.add(0x460) as *mut u16) = 0x10;
}

/// Pure Rust implementation of TaskStateMachine__Init (0x4F6370).
///
/// Convention: usercall(ESI=object, ECX=param1, EDI=height) + 1 stack(width), RET 0x4.
///
/// Allocates a bit-per-cell grid buffer. `param1` is typically 1 (cells per unit).
/// `width` and `height` are pixel dimensions. The buffer is a row-major bitfield
/// with rows aligned to 4 bytes.
///
/// # Safety
/// `object` must point to a zero-filled allocation of at least 0x2C bytes.
pub unsafe fn task_state_machine_init(object: *mut u8, param1: u32, width: u32, height: u32) {
    // Row stride: bits-to-bytes rounded up, then aligned to 4
    let bits = param1.wrapping_mul(width).wrapping_add(7) as i32;
    let row_stride = ((bits >> 3) + 3) & !3;
    let total_size = row_stride as u32 * height;

    // Allocate data buffer with 0x20-byte header
    let alloc_size = ((total_size + 3) & !3) + 0x20;
    let buffer = wa_malloc(alloc_size);

    let _ = crate::log::log_line(&format!(
        "[TSM] init: obj=0x{:08X} p1={} w={} h={} stride={} total={} alloc={} buf=0x{:08X}",
        object as u32, param1, width, height, row_stride, total_size, alloc_size, buffer as u32,
    ));

    if buffer.is_null() {
        let _ = crate::log::log_line("[TSM] WARN: wa_malloc returned null, skipping memset");
        return;
    }
    // Guard against integer overflow producing tiny alloc_size with huge total_size
    if total_size as usize > alloc_size as usize {
        let _ = crate::log::log_line(&format!(
            "[TSM] OVERFLOW: total_size {} > alloc_size {}, skipping memset",
            total_size, alloc_size,
        ));
        return;
    }

    // Memset twice (matches original — likely redundant but exact match)
    core::ptr::write_bytes(buffer, 0, total_size as usize);
    core::ptr::write_bytes(buffer, 0, total_size as usize);

    // Set object fields (DWORD array layout)
    let obj = object as *mut u32;
    *obj.add(0) = rb(0x6640EC); // vtable
    *obj.add(1) = 0; // unused
    *obj.add(2) = buffer as u32; // data pointer
    *obj.add(3) = param1; // param1
    *obj.add(4) = row_stride as u32; // row stride
    *obj.add(5) = width; // width
    *obj.add(6) = height; // height
    *obj.add(7) = 0; // unused
    *obj.add(8) = 0; // unused
    *obj.add(9) = width; // width (duplicate)
    *obj.add(10) = height; // height (duplicate)
}

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

    let display = (*wrapper).display as *mut u8;
    let vt = *(display as *const *const u32);
    // vtable[4] (offset 0x10): thiscall(display, layer, color), RET 0x8
    let set_color: unsafe extern "thiscall" fn(*mut u8, i32, i32) =
        core::mem::transmute(*vt.add(4));

    set_color(display, 1, layer1_color);
    set_color(display, 2, 0x20);
    set_color(display, 3, 0x70);
}

/// Hash function for GfxDir entry lookup (FUN_566390).
///
/// 10-bit CRC-like hash over the global name buffer at 0x8ACF58.
/// Operates on the already-lowercased name.
///
/// # Safety
/// `name` must be a valid null-terminated C string pointer.
unsafe fn gfx_dir_hash(name: *const u8) -> u32 {
    let mut hash: u32 = 0;
    let mut p = name;
    while *p != 0 {
        let bit9 = (hash >> 9) & 1;
        hash = ((hash << 1) | bit9) & 0x3FF;
        hash = hash.wrapping_add(*p as u32) & 0x3FF;
        p = p.add(1);
    }
    hash
}

/// Pure Rust implementation of GfxDir__FindEntry (0x566520).
///
/// Convention: usercall(EAX=name) + 1 stack(gfx_handler), RET 0x4.
///
/// Looks up a name in the GfxHandler's hash table. Names are case-insensitive.
/// Supports `|`-separated fallback names (e.g. "path\\file.img|fallback.img").
///
/// Returns entry pointer or null. Entry layout:
/// - entry+0x00: next pointer (linked list)
/// - entry+0x04: value (passed to vtable[2] for cached load)
/// - entry+0x08: unknown
/// - entry+0x0C: name string (null-terminated, lowercase)
///
/// # Safety
/// `gfx_handler` must be a valid GfxHandler with initialized hash table at +0x04.
/// `name` must be a valid null-terminated C string.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_dir_find_entry(name: *const u8, gfx_handler: *mut u8) -> *mut u8 {
    let mut current_name = name;

    loop {
        // Copy name to stack buffer (max 0xFF chars + null)
        let mut buf = [0u8; 0x100];
        let mut i = 0usize;
        let mut src = current_name;
        while *src != 0 && i < 0xFF {
            buf[i] = *src;
            src = src.add(1);
            i += 1;
        }
        buf[i] = 0;

        // Find '|' separator in buffer
        let mut pipe_pos: Option<usize> = None;
        for j in 0..i {
            if buf[j] == b'|' {
                buf[j] = 0; // truncate at pipe
                pipe_pos = Some(j);
                break;
            }
        }

        // Compute next_name: pointer into original string after '|'
        let next_name: *const u8 = if let Some(pos) = pipe_pos {
            // Offset of '|' in buffer = pos
            // Same offset in original string → current_name + pos + 1
            current_name.add(pos + 1)
        } else {
            core::ptr::null()
        };

        // Lowercase A-Z in buffer
        for j in 0..buf.len() {
            if buf[j] == 0 {
                break;
            }
            if buf[j] >= b'A' && buf[j] <= b'Z' {
                buf[j] += 0x20;
            }
        }

        // Hash the lowercased name
        let bucket = gfx_dir_hash(buf.as_ptr());

        // Walk linked list in hash bucket
        // gfx_handler+4 = pointer to bucket array (1024 entries)
        let bucket_array = *(gfx_handler.add(4) as *const *const u32);
        let mut entry = *bucket_array.add(bucket as usize) as *mut u8;

        while !entry.is_null() {
            // Compare entry name at entry+0x0C with our buffer
            let entry_name = entry.add(0x0C);
            let mut match_found = true;
            let mut k = 0usize;
            loop {
                let a = *entry_name.add(k);
                let b = buf[k];
                if a != b {
                    match_found = false;
                    break;
                }
                if a == 0 {
                    break;
                }
                k += 1;
            }

            if match_found {
                return entry;
            }

            // Next in linked list: entry+0x00
            entry = *(entry as *const *mut u8);
        }

        // Not found — try fallback name after '|'
        if next_name.is_null() {
            return core::ptr::null_mut();
        }
        current_name = next_name;
    }
}

/// Pure Rust implementation of FUN_5665F0 (GfxHandler reset).
///
/// Convention: usercall(ESI=handler), plain RET. Called at start of LoadDir.
unsafe fn gfx_handler_reset(handler: *mut u8) {
    *(handler.add(0x04) as *mut u32) = 0;
    *(handler.add(0x08) as *mut u32) = 0;
    *(handler.add(0x194) as *mut u32) = 0;

    for i in 0..16u32 {
        // handler+0x0C + i*0x10 = 0 (16 entries at stride 0x10)
        *(handler.add(0x0C + i as usize * 0x10) as *mut u32) = 0;
        // handler+0x10C + i*4 = i (index array)
        *(handler.add(0x10C + i as usize * 4) as *mut u32) = i;
    }

    *(handler.add(400) as *mut u32) = 0; // 0x190
    *(handler.add(0x18C) as *mut u32) = 0x10;
}

/// Pure Rust implementation of GfxHandler__LoadDir (0x5663E0).
///
/// Convention: usercall(EAX=handler), plain RET. Returns 1 on success, 0 on failure.
///
/// Reads a .dir file through the handler's vtable I/O methods:
/// - vtable[0]: read(buf, size) → thiscall, returns bytes read
/// - vtable[1]: seek/reposition(size) → thiscall
/// - vtable[2]: allocate(size) → thiscall, returns buffer or null
///
/// .dir file format:
/// - 4 bytes: magic "DIR\x1A" (0x1A524944)
/// - 4 bytes: total_file_size
/// - 4 bytes: data_size (hash table + entries)
/// - data: 1024-bucket hash table followed by linked list nodes
///   All pointers are relative offsets from (data_start + 4)
///
/// # Safety
/// `handler` must be a valid GfxHandler with file handle at +0x198.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_handler_load_dir(handler: *mut u8) -> i32 {
    gfx_handler_reset(handler);

    let vt = *(handler as *const *const u32);
    // vtable[0] = read: thiscall(handler, buf, size) -> bytes_read
    let vt_read: unsafe extern "thiscall" fn(*mut u8, *mut u8, u32) -> u32 =
        core::mem::transmute(*vt);
    // vtable[1] = seek: thiscall(handler, size)
    let vt_seek: unsafe extern "thiscall" fn(*mut u8, u32) = core::mem::transmute(*vt.add(1));
    // vtable[2] = allocate: thiscall(handler, size) -> *mut u8
    let vt_alloc: unsafe extern "thiscall" fn(*mut u8, u32) -> *mut u8 =
        core::mem::transmute(*vt.add(2));

    // Read and validate magic
    let mut magic: u32 = 0;
    if vt_read(handler, &mut magic as *mut u32 as *mut u8, 4) != 4 {
        return 0;
    }
    if magic != 0x1A524944 {
        // "DIR\x1A"
        return 0;
    }

    // Read total_file_size and data_size
    let mut total_file_size: u32 = 0;
    if vt_read(handler, &mut total_file_size as *mut u32 as *mut u8, 4) != 4 {
        return 0;
    }
    let mut data_size: u32 = 0;
    if vt_read(handler, &mut data_size as *mut u32 as *mut u8, 4) != 4 {
        return 0;
    }

    let alloc_size = data_size + 4;

    // Try fast path: vtable[2] allocate (memory-maps the data)
    let data = vt_alloc(handler, alloc_size);
    *(handler.add(4) as *mut *mut u8) = data;

    if data.is_null() {
        // Fallback: seek past header, then malloc + read entire data block.
        // Original calls vt_seek(alloc_size) then WA_MallocMemset(EDI=read_size).
        // WA_MallocMemset allocates (size+3)&~3 + 0x20 and memsets size bytes.
        vt_seek(handler, alloc_size);

        let read_size = total_file_size - data_size - 4;
        let malloc_size = ((read_size + 3) & !3) + 0x20;
        let buf = wa_malloc(malloc_size);
        if buf.is_null() {
            return 0;
        }
        core::ptr::write_bytes(buf, 0, read_size as usize);
        *(handler.add(4) as *mut *mut u8) = buf;

        let bytes_read = vt_read(handler, buf, read_size);
        if bytes_read != read_size {
            crate::wa_alloc::wa_free(buf);
            return 0;
        }

        // Mark as "loaded from fallback" at handler+8
        *(handler.add(8) as *mut u32) = 1;
    }

    // Fix up relative pointers in the hash table
    // 1024 buckets at data[0..0x1000], each is a pointer to a linked list node
    let data = *(handler.add(4) as *const *mut u8);
    let base = data as u32;

    for bucket in 0..1024u32 {
        let bucket_ptr = data.add(bucket as usize * 4) as *mut u32;
        let entry_offset = *bucket_ptr;
        if entry_offset == 0 {
            continue;
        }

        // Convert relative offset to absolute: offset + base - 4
        let entry_addr = entry_offset.wrapping_add(base).wrapping_sub(4);
        *bucket_ptr = entry_addr;

        // Walk linked list, fix up each next pointer
        let mut node = entry_addr as *mut u32;
        loop {
            if node.is_null() {
                break;
            }
            let next_offset = *node;
            if next_offset == 0 {
                break;
            }
            let next_addr = next_offset.wrapping_add(base).wrapping_sub(4);
            *node = next_addr;
            node = next_addr as *mut u32;
        }
    }

    // Mark as loaded
    *(handler.add(0x194) as *mut u32) = 1;

    1 // success
}

/// Pure Rust implementation of GfxResource__Create_Maybe (0x4F6300).
///
/// Convention: usercall(ECX=gfx_handler, EAX=name) + 1 stack(output), RET 0x4.
///
/// Looks up `name` in the GfxHandler's directory, tries cached load, wraps
/// as DisplayGfx. Falls back to loading the raw image and decoding it.
///
/// # Safety
/// All pointers must be valid WA objects.
#[cfg(target_arch = "x86")]
pub unsafe fn gfx_resource_create(
    gfx_handler: *mut u8,
    name: *const u8,
    output: *mut u8,
) -> *mut u8 {
    let _ = crate::log::log_line(&format!(
        "[GfxRes] create: handler=0x{:08X} name=0x{:08X} out=0x{:08X}",
        gfx_handler as u32, name as u32, output as u32,
    ));
    // 1. Try FindEntry → cached load → DisplayGfx wrap
    let entry = gfx_dir_find_entry(name, gfx_handler);
    let _ = crate::log::log_line(&format!("[GfxRes] entry=0x{:08X}", entry as u32));
    if !entry.is_null() {
        // gfx_handler->vtable[2](entry->field_4) — cached load
        let vt = *(gfx_handler as *const *const u32);
        let _ = crate::log::log_line(&format!("[GfxRes] vt=0x{:08X}", vt as u32));
        let load_cached: unsafe extern "thiscall" fn(*mut u8, u32) -> *mut u8 =
            core::mem::transmute(*vt.add(2));
        let entry_val = *(entry.add(4) as *const u32);
        let cached = load_cached(gfx_handler, entry_val);
        let _ = crate::log::log_line(&format!("[GfxRes] cached=0x{:08X}", cached as u32));
        if !cached.is_null() {
            // DisplayGfx__Constructor_Maybe: stdcall(raw_image), RET 0x4
            let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                core::mem::transmute(rb(va::DISPLAYGFX_CONSTRUCTOR) as usize);
            return ctor(cached);
        }
    }

    // 2. Fallback: LoadImage → IMG_Decode
    let _ = crate::log::log_line("[GfxRes] fallback: LoadImage");
    let raw_image = call_gfx_load_image(gfx_handler, name);
    let _ = crate::log::log_line(&format!("[GfxRes] raw_image=0x{:08X}", raw_image as u32));
    if raw_image.is_null() {
        return core::ptr::null_mut();
    }

    // IMG_Decode: stdcall(output, raw_image, 1), RET 0xC
    let _ = crate::log::log_line("[GfxRes] calling IMG_Decode");
    let decode: unsafe extern "stdcall" fn(*mut u8, *mut u8, i32) -> *mut u8 =
        core::mem::transmute(rb(va::IMG_DECODE) as usize);
    let result = decode(output, raw_image, 1);
    let _ = crate::log::log_line(&format!(
        "[GfxRes] IMG_Decode result=0x{:08X}",
        result as u32
    ));

    // Release raw image: raw_image->vtable[0](1)
    let img_vt = *(raw_image as *const *const u32);
    let fn_ptr = *img_vt;
    let slot_idx = *(raw_image.add(4) as *const u32);
    let stored_handler = *(raw_image.add(8) as *const u32);
    let _ = crate::log::log_line(&format!(
        "[GfxRes] release: raw=0x{:08X} vt=0x{:08X} fn=0x{:08X} slot={} handler=0x{:08X}",
        raw_image as u32, img_vt as u32, fn_ptr as u32, slot_idx, stored_handler,
    ));
    let _ = crate::log::log_line(&format!(
        "[GfxRes] handler_usedcnt={} handler_freecnt={}",
        *(gfx_handler.add(0x190) as *const u32),
        *(gfx_handler.add(0x18C) as *const u32),
    ));
    let release: unsafe extern "thiscall" fn(*mut u8, i32) = core::mem::transmute(fn_ptr);
    let _ = crate::log::log_line("[GfxRes] calling release...");
    release(raw_image, 1);
    let _ = crate::log::log_line("[GfxRes] release done");
    let _ = crate::log::log_line(&format!(
        "[GfxRes] heap-ok raw=0x{:08X} result=0x{:08X}",
        raw_image as u32, result as u32
    ));
    let _ = crate::log::log_line("[GfxRes] before return");

    result
}

/// Runtime address of DDGame__InitFields (set by init_constructor_addrs()).
static mut INIT_FIELDS_ADDR: u32 = 0;
static mut GFX_LOAD_DIR_ADDR: u32 = 0;

/// Initialize runtime addresses for the constructor bridges.
/// Must be called once at DLL startup (from lib.rs or similar).
pub fn init_constructor_addrs() {
    unsafe {
        INIT_FIELDS_ADDR = rb(va::DDGAME_INIT_FIELDS);
        SPRITE_REGION_CTOR_ADDR = rb(va::SPRITE_REGION_CONSTRUCTOR);
        FUN_570E20_ADDR = rb(va::FUN_570E20);
        FUN_570A90_ADDR = rb(va::FUN_570A90);
        GFX_RESOURCE_CREATE_ADDR = rb(va::GFX_RESOURCE_CREATE);
        TSM_INIT_ADDR = rb(va::TASK_STATE_MACHINE_INIT);
        GFX_FIND_ENTRY_ADDR = rb(va::GFX_DIR_FIND_ENTRY);
        GFX_LOAD_IMAGE_ADDR = rb(va::GFX_DIR_LOAD_IMAGE);
        GFX_LOAD_DIR_ADDR = rb(va::GFX_HANDLER_LOAD_DIR);
        FUN_570F30_ADDR = rb(va::DDGAME_INIT_SOUND_PATHS);
        LOAD_SPEECH_BANKS_ADDR = rb(va::DSSOUND_LOAD_ALL_SPEECH_BANKS);
        LOADING_PROGRESS_TICK_ADDR = rb(va::DDGAME_WRAPPER_LOADING_PROGRESS_TICK);
    }
}

/// Bridge to DDGame__InitFields (0x526120).
/// Convention: usercall(EDI=ddgame), plain RET.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_init_fields(_ddgame: *mut DDGame) {
    core::arch::naked_asm!(
        "pushl %edi",
        "movl 8(%esp), %edi",
        "calll *({addr})",
        "popl %edi",
        "retl",
        addr = sym INIT_FIELDS_ADDR,
        options(att_syntax),
    );
}

/// Bridge to DDGame__InitVersionFlags (0x525BE0).
/// Convention: stdcall(ddgame_wrapper).
#[cfg(target_arch = "x86")]
unsafe fn call_init_version_flags(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_VERSION_FLAGS) as usize);
    f(wrapper);
}

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
    let ddgame = wa_malloc(0x98B8) as *mut DDGame;
    if ddgame.is_null() {
        return core::ptr::null_mut();
    }
    core::ptr::write_bytes(ddgame as *mut u8, 0, 0x98B8);

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
    (*ddgame)._param_018 = param7;
    (*ddgame)._caller = network_ecx as *mut u8;
    (*ddgame).game_info = game_info;
    (*ddgame)._param_028 = net_game;

    // Now safe to expose — all fields that concurrent readers check are set.
    (*wrapper).ddgame = ddgame;

    // Verify game_info is set
    let _ = crate::log::log_line(&format!(
        "[DDGame] exposed: ddgame=0x{:08X}, game_info=0x{:08X}, net_game=0x{:08X}",
        ddgame as u32, game_info as u32, net_game as u32,
    ));

    // ── 5. Set g_GameInfo global ──
    *(rb(va::G_GAME_INFO) as *mut *mut GameInfo) = game_info;

    // ── 6. Sound available + always-1 flags ──
    let is_headless = (*game_info).headless_mode != 0;
    // Original: sound_available = (headless_mode == 0) — set EARLY.
    // This enables the loading progress bar, message pump, and sound during
    // construction. Previously deferred to avoid message pump crashes, but all
    // critical DDGame fields (game_info, display, sound, etc.) are now set.
    (*ddgame).sound_available = if is_headless { 0 } else { 1 };
    (*ddgame)._field_7efc = 1;

    // ── 7. DDGameWrapper+0x48C init ──
    (*wrapper).ddgame_secondary = core::ptr::null_mut();

    // ── 8. Conditional network object (game_version == -2) ──
    if (*game_info).game_version == -2 {
        let net_obj = wa_malloc(0x2C);
        core::ptr::write_bytes(net_obj, 0, 0x2C);
        *(net_obj as *mut *mut DDGame) = ddgame;
        *net_obj.add(0x28) = (*game_info).net_config_1;
        *net_obj.add(0x29) = (*game_info).net_config_2;
        (*wrapper).ddgame_secondary = net_obj;
        if network_ecx != 0 {
            *((network_ecx as *mut u8).add(0x18) as *mut *mut u8) = net_obj;
        }
    }

    // ── 9. InitVersionFlags — sets DDGame+0x7E2E/0x7E2F/0x7E3F ──
    let _ = crate::log::log_line("[DDGame] calling InitVersionFlags");
    call_init_version_flags(wrapper);
    let _ = crate::log::log_line("[DDGame] calling init_graphics_and_resources");

    // ── 10. GfxHandler, landscape, sprites, audio, resources ──
    init_graphics_and_resources(wrapper, game_info, net_game, display, is_headless);

    let _ = crate::log::log_line("[DDGame] create_ddgame complete");
    ddgame
}

/// Second half of the constructor: GfxHandler, landscape, sprites, audio, resources.
///
/// Separated for readability. Will be broken into smaller functions as
/// individual sections are converted to pure Rust.
#[cfg(target_arch = "x86")]
unsafe fn init_graphics_and_resources(
    wrapper: *mut DDGameWrapper,
    game_info: *mut GameInfo,
    net_game: *mut u8,
    display: *mut DDDisplay,
    is_headless: bool,
) {
    let ddgame = (*wrapper).ddgame;
    let fopen: unsafe extern "cdecl" fn(*const u8, *const u8) -> *mut u8 =
        core::mem::transmute(rb(va::WA_FOPEN) as usize);
    // gfx_load_dir_addr removed — using direct Rust gfx_handler_load_dir() now
    let gfx_handler_vtable = rb(0x66B280) as u32; // GfxHandler__vtable (from asm: MOV [ESI], 0x66B280)

    // ── GfxHandler #1 (primary) ──
    let gfx1 = wa_malloc(0x19C);
    core::ptr::write_bytes(gfx1, 0, 0x19C);
    *(gfx1 as *mut u32) = gfx_handler_vtable;
    (*wrapper)._field_4c0 = gfx1;
    (*wrapper)._field_4c4 = core::ptr::null_mut();

    // Build path list (order depends on headless + GameInfo+0xF374)
    let f914 = (*game_info).headless_mode as i32;
    let f374 = (*game_info).display_flags as i32;
    let paths: [&[u8]; 3] = if f914 != 0 {
        if f374 == 0 {
            [
                b"data\\Gfx\\Gfx.dir\0",
                b"data\\Gfx\\Gfx0.dir\0",
                b"data\\Gfx\\Gfx1.dir\0",
            ]
        } else {
            [
                b"data\\Gfx\\Gfx.dir\0",
                b"data\\Gfx\\Gfx1.dir\0",
                b"data\\Gfx\\Gfx0.dir\0",
            ]
        }
    } else if f374 == 0 {
        [
            b"data\\Gfx\\Gfx0.dir\0",
            b"data\\Gfx\\Gfx1.dir\0",
            b"data\\Gfx\\Gfx.dir\0",
        ]
    } else {
        [
            b"data\\Gfx\\Gfx1.dir\0",
            b"data\\Gfx\\Gfx0.dir\0",
            b"data\\Gfx\\Gfx.dir\0",
        ]
    };

    let mut gfx_loaded_idx = 0u32;
    for (i, path) in paths.iter().enumerate() {
        let fp = fopen(path.as_ptr(), b"rb\0".as_ptr());
        *(gfx1.add(0x198) as *mut *mut u8) = fp;
        if !fp.is_null() && call_gfx_load_dir(gfx1, GFX_LOAD_DIR_ADDR) != 0 {
            gfx_loaded_idx = i as u32;
            break;
        }
        if i == 2 {
            panic!("DDGame: couldn't open any Gfx.dir");
        }
    }

    let headless_offset = if f914 != 0 { 1u32 } else { 0u32 };
    (*wrapper).gfx_mode = if gfx_loaded_idx.wrapping_sub(headless_offset) < 2 {
        1
    } else {
        0
    };

    // ── GfxHandler #2 (conditional) ──
    let d778 = (*game_info).game_version;
    // Original: (-(gfx_mode != 0) & 0x23) - 2 → gfx_mode ? 33 : -2
    let threshold = if (*wrapper).gfx_mode != 0 { 33 } else { -2i32 };
    if d778 < threshold {
        let c_digit = if d778 > -3 { b'2' } else { b'1' };
        let mut gfx_c_path = *b"data\\Gfx\\GfxC_3_0.dir\0";
        gfx_c_path[14] = c_digit;

        let gfx2 = wa_malloc(0x19C);
        core::ptr::write_bytes(gfx2, 0, 0x19C);
        *(gfx2 as *mut u32) = gfx_handler_vtable;
        (*wrapper)._field_4c4 = gfx2;

        let fp = fopen(gfx_c_path.as_ptr(), b"rb\0".as_ptr());
        *(gfx2.add(0x198) as *mut *mut u8) = fp;
        if fp.is_null() || call_gfx_load_dir(gfx2, GFX_LOAD_DIR_ADDR) == 0 {
            let fp2 = fopen(b"data\\Gfx\\Gfx.dir\0".as_ptr(), b"rb\0".as_ptr());
            *(gfx2.add(0x198) as *mut *mut u8) = fp2;
            if fp2.is_null() || call_gfx_load_dir(gfx2, GFX_LOAD_DIR_ADDR) == 0 {
                panic!("DDGame: couldn't open secondary Gfx.dir");
            }
        }
    }

    // ── Display palette setup (non-headless) ──
    if !is_headless {
        if *(rb(0x88E485) as *const u8) == 0 {
            call_usercall_eax(wrapper, FUN_570A90_ADDR);
        }
        // DDGameWrapper+0x4D0 points to the display object used for palette calls.
        // BUT: FUN_00570A90 or something else may write to wrapper in ways we
        // don't expect. Read via struct field instead of raw offset.
        let disp = (*wrapper).display as *mut u8;
        let gfx_handler = (*wrapper)._field_4c0;
        let vt = *(disp as *const *const u32);
        // vtable[4] (offset 0x10): thiscall + 2 stack params (1, 0xFE), RET 0x8
        let vt_10: unsafe extern "thiscall" fn(*mut u8, i32, i32) =
            core::mem::transmute(*vt.add(4));
        vt_10(disp, 1, 0xFE);
        // vtable[0x1F] (offset 0x7C): thiscall + 5 stack params, RET 0x14
        let vt_7c: unsafe extern "thiscall" fn(*mut u8, i32, i32, i32, *mut u8, u32) =
            core::mem::transmute(*vt.add(0x1F));
        vt_7c(disp, 1, 1, 0, gfx_handler, rb(0x66A3A8));
        // vtable[0x17] (offset 0x5C): thiscall + 2 stack params, RET 0x8
        let vt_5c: unsafe extern "thiscall" fn(*mut u8, i32, i32) =
            core::mem::transmute(*vt.add(0x17));
        vt_5c(disp, 1, -100);

        // Palette slot range init
        let palette_range_ptr = *(disp.add(0x3120) as *const *mut i16);
        if !palette_range_ptr.is_null() && *(disp.add(0x3534) as *const i32) == 0 {
            let start = *palette_range_ptr as u32;
            let end = (*palette_range_ptr.add(1) as u32) + 1;
            if start < end {
                for i in start..end {
                    *(disp.add(0x312C + i as usize * 4) as *mut u32) = 1;
                }
            }
            crate::wa_alloc::wa_free(*(disp.add(0x3120) as *const *mut u8));
            *(disp.add(0x3120) as *mut u32) = 0;
        }
    }

    // ── FUN_00570E20: usercall(ESI=wrapper), plain RET ──
    // Runs for all modes — headless vtable[4] is 0x5231E0 (same as headful).
    display_layer_color_init(wrapper);

    // ── Display vtable slot 5 (offset 0x14) ──
    // Original: CALL EAX (vtable[5]), saves return value in ESI for use as
    // the `output` parameter in the color-entries GfxResource__Create call below.
    let layer_ctx = {
        let vt = *((*ddgame).display as *const *const u32);
        let f: unsafe extern "thiscall" fn(*mut DDDisplay, i32) -> *mut u8 =
            core::mem::transmute(*vt.add(5));
        f((*ddgame).display, 1)
    };

    let _ = crate::log::log_line(&format!("[DDGame] gfx_mode={}", (*wrapper).gfx_mode));
    // ── GfxDir color entries DDGame+0x730C..0x732C ──
    if (*wrapper).gfx_mode != 0 {
        // Original: PUSH ESI (=layer_ctx from vtable[5] call), MOV EAX=0x66A3B4, CALL 0x4F6300
        // The layer_ctx is used as the output buffer, not a plain stack alloc.
        let res = gfx_resource_create((*wrapper)._field_4c0, rb(0x66A3B4) as *const u8, layer_ctx);
        if !res.is_null() {
            let rvt = *(res as *const *const u32);
            let _ = crate::log::log_line(&format!(
                "[DDGame] color entries: res=0x{:08X} vt=0x{:08X} vt[4]=0x{:08X}",
                res as u32,
                rvt as u32,
                *rvt.add(4),
            ));
            // vtable[4] = get_pixel: thiscall(this, x, y) -> color, RET 0x8.
            let get_color: unsafe extern "thiscall" fn(*mut u8, u32, u32) -> u32 =
                core::mem::transmute(*rvt.add(4));
            let mut off = 0x730Cu32;
            let mut idx = 0u32;
            while off < 0x732Du32 {
                let c = get_color(res, idx, 0);
                *((ddgame as *mut u8).add(off as usize) as *mut u32) = c;
                if idx == 0 {
                    let _ = crate::log::log_line(&format!("[DDGame] color[0] = 0x{:08X}", c,));
                }
                off += 4;
                idx += 1;
            }
            // DisplayGfx__vmethod_3: thiscall(this, byte param_2), RET 4.
            // param_2 & 1 = free the object itself.
            let release: unsafe extern "thiscall" fn(*mut u8, u8) =
                core::mem::transmute(*rvt.add(3));
            release(res, 1);
        }
    } else {
        // GFX_HANDLER_LOAD_SPRITES: SKIP for testing (gfx_mode==0 path)
    }

    let _ = crate::log::log_line("[DDGame] past color entries");
    let _ = crate::log::log_line(&format!(
        "[DDGame] 4c0=0x{:08X} 4c4=0x{:08X}",
        (*wrapper)._field_4c0 as u32,
        (*wrapper)._field_4c4 as u32
    ));
    // ── Secondary GfxDir object (DDGame+0x2C, conditional) ──
    if !(*wrapper)._field_4c4.is_null() {
        let gfxdir2 = wa_malloc(0x70C);
        core::ptr::write_bytes(gfxdir2, 0, 0x70C);
        *(gfxdir2 as *mut u16) = 1;
        *(gfxdir2.add(2) as *mut u16) = 0x5A;
        // FUN_5411A0: usercall(EAX=gfxdir2), plain RET
        call_usercall_eax(gfxdir2 as *mut DDGameWrapper, rb(0x5411A0));
        *(gfxdir2.add(0x708) as *mut u16) = 0;
        (*ddgame).secondary_gfxdir = gfxdir2;
        // GFX_HANDLER_LOAD_SPRITES: stdcall(wrapper, ddgame+0x7308, game_info+0xF374, 0), RET 0x10

        let f: unsafe extern "stdcall" fn(*mut DDGameWrapper, *mut u8, u32, u32) =
            core::mem::transmute(rb(va::GFX_HANDLER_LOAD_SPRITES) as usize);
        f(
            wrapper,
            (ddgame as *mut u8).add(0x7308),
            (*game_info).display_flags,
            0,
        );
    }

    // ── DDGameWrapper field inits ──
    (*wrapper)._field_4d8 = 0;
    // Original: if (display == NULL) iVar4=0 else iVar4=*(byte*)(game_info+0x44C)*0x38+0x7E
    // Ghidra mislabeled game_info as param8 — the original reads game_info, not net_game.
    // Our headless display is non-null, so use is_headless as the condition.
    if is_headless {
        (*wrapper)._field_4dc = 0x2AD;
    } else {
        let byte_val = *(game_info as *const u8).add(0x44C) as u32;
        (*wrapper)._field_4dc = byte_val * 0x38 + 0x7E + 0x2AD;
    }
    (*wrapper)._field_4e0 = 0xFFFFFF9C; // -100
    (*wrapper).speech_name_count = 0;

    // ── Audio init (non-headless + sound available) ──
    let _ = crate::log::log_line("[DDGame] audio: start");
    if !is_headless {
        // FUN_570F30: usercall(ESI=wrapper)
        call_usercall_esi(wrapper, FUN_570F30_ADDR);
        let _ = crate::log::log_line("[DDGame] audio: past 570F30");

        if !(*ddgame).sound.is_null() {
            // DSSound_LoadEffectWAVs: stdcall(wrapper)
            let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
                core::mem::transmute(rb(va::DSSOUND_LOAD_EFFECT_WAVS) as usize);
            f(wrapper);
            // DSSound_LoadAllSpeechBanks: the original is hooked to our Rust
            // replacement (speech.rs), so the usercall bridge calls our code.
            call_usercall_esi(wrapper, LOAD_SPEECH_BANKS_ADDR);
            let _ = crate::log::log_line("[DDGame] audio: done");
            // Allocate ActiveSoundTable (0x608 bytes)
            let ast = wa_malloc(0x608) as *mut ActiveSoundTable;
            (*ast).ddgame = ddgame;
            (*ast).counter = 0;
            core::ptr::write_bytes(ast as *mut u8, 0, 0x600);
            (*ddgame).active_sounds = ast;
        }
    }

    let _ = crate::log::log_line("[DDGame] past ActiveSoundTable");

    // ── Loading progress ticks (original calls DDGameWrapper__LoadingProgressTick twice here) ──
    call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);
    call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);

    // ── GfxResource: thiscall(ECX=gfx_handler) + EAX=name + 1 stack(output), RET 0x4 ──
    // Test: call GfxResource with real params
    let gfx_resource: *mut u8;
    {
        {
            let gfx_handler = (*wrapper)._field_4c0;
            let out_buf = wa_malloc(0x900);
            core::ptr::write_bytes(out_buf, 0, 0x900);
            // return moved to before color entries
            let _ = crate::log::log_line("[DDGame] calling GfxResource_Create");
            gfx_resource = gfx_resource_create(gfx_handler, rb(0x66A3C0) as *const u8, out_buf);
            let _ = crate::log::log_line("[DDGame] after gfx_resource_create");
            let _ = crate::log::log_line("[DDGame] GfxResource created (value elided)");
        }
    }

    // ── PCLandscape (alloc 0xB44, stdcall 11 params) ──
    let landscape = {
        let alloc = wa_malloc(0xB44);
        core::ptr::write_bytes(alloc, 0, 0xB44);
        if !alloc.is_null() {
            // Output locals for landscape data (used later for coord_list)
            // Param 9 output: in original this is aiStack_978 (571 ints = 2284 bytes).
            // Param 6 output: iStack_11f9 (single byte, but adjacent to other locals).
            // Heap-allocate both to avoid stack overflow.
            let landscape_coords_buf = wa_malloc(0x1000); // 4KB for coord output
            core::ptr::write_bytes(landscape_coords_buf, 0, 0x1000);
            let landscape_byte_buf = wa_malloc(0x100); // generous for byte output
            core::ptr::write_bytes(landscape_byte_buf, 0, 0x100);

            let pc_ctor: unsafe extern "stdcall" fn(
                *mut u8,
                *mut DDGame,
                *mut u8, // 1=this, 2=ddgame, 3=gfx_resource
                *mut DDDisplay,
                *const u8, // 4=display, 5=game_info+0xDAAC
                *mut u8,
                u32, // 6=&landscape_byte, 7=gfx_mode
                *mut u8,
                *mut u8, // 8=stack local, 9=coord output
                *mut u8,
                *mut u8, // 10=&ddgame+0x777C, 11=&ddgame+0x7780
            ) -> *mut u8 = core::mem::transmute(rb(va::PC_LANDSCAPE_CONSTRUCTOR) as usize);

            // Param 8 is a stack local in the original's 4KB frame.
            // Heap-allocate a generous buffer since we don't know the exact size.
            let stack_local_8 = wa_malloc(0x1000);
            core::ptr::write_bytes(stack_local_8, 0, 0x1000);
            let result = pc_ctor(
                alloc,
                ddgame,
                gfx_resource,
                (*wrapper).display, // param 4: display (NOT wrapper!)
                (*game_info).landscape_data_path.as_ptr(), // param 5
                landscape_byte_buf, // param 6
                (*wrapper).gfx_mode, // param 7
                stack_local_8,      // param 8
                landscape_coords_buf, // param 9
                (ddgame as *mut u8).add(0x777C), // param 10
                (ddgame as *mut u8).add(0x7780), // param 11
            );
            let _ = crate::log::log_line(&format!(
                "[DDGame] PCLandscape returned: 0x{:08X}",
                result as u32
            ));
            // Store landscape pointer
            (*wrapper).landscape = result as *mut PCLandscape;
            (*ddgame).landscape = result as *mut PCLandscape;
            result
        } else {
            (*wrapper).landscape = core::ptr::null_mut();
            (*ddgame).landscape = core::ptr::null_mut();
            core::ptr::null_mut()
        }
    };

    // ── TaskStateMachine at DDGame+0x380 (alloc 0x4C, memset 0x2C) ──
    // Pure Rust: allocate object, call task_state_machine_init, override vtable.
    {
        let tsm = wa_malloc(0x4C);
        core::ptr::write_bytes(tsm, 0, 0x2C);
        if !tsm.is_null() {
            let width = (*ddgame).level_width;
            let height = (*ddgame).level_height;
            task_state_machine_init(tsm, 1, width, height);
            *(tsm as *mut u32) = rb(0x664118); // Override vtable to TaskStateMachine
        }
        (*ddgame).task_state_machine = tsm;
    }

    // ── 8× SpriteRegion at DDGame+0x46C..0x488 ──
    // SpriteRegion__Constructor: fastcall(ECX, EDX) + 6 stack(this, p2, p3, p4, p5, p6), RET 0x18
    {
        // Params verified by hooking original (see SpriteRegion param logger).
        // (ddgame_offset, ECX, EDX, p2, p3, p4, p5, p6=gfx_resource)
        // p6 is the GfxResource pointer returned by GfxResource__Create_Maybe.
        let gr = gfx_resource as u32;
        let regions: [(u32, u32, u32, u32, u32, u32, u32, u32); 8] = [
            (0x474, 0x37, 0x36, 0x2E, 0x24, 0x41, 0x2D, gr),
            (0x46C, 0x30, 0x0C, 0x2D, 0x07, 0x34, 0x09, gr),
            (0x470, 0x11, 0x1A, 0x0D, 0x0A, 0x16, 0x13, gr),
            (0x478, 0x0C, 0x3D, 0x00, 0x20, 0x18, 0x33, gr),
            (0x47C, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, gr),
            (0x484, 0x1A2, 0x1B, 0x173, 0x09, 0x1D8, 0x03, gr),
            (0x488, 0x1EF, 0x26, 0x1E5, 0x07, 0x1F9, 0x16, gr),
            (0x480, 0x2D, 0x08, 0x2D, 0x07, 0x2E, 0x07, gr),
        ];

        for &(offset, ecx, edx, p2, p3, p4, p5, p6) in &regions {
            let alloc = wa_malloc(0x9C);
            core::ptr::write_bytes(alloc, 0, 0x9C);
            let result = if !alloc.is_null() {
                call_sprite_region_ctor(alloc, ecx, edx, p2, p3, p4, p5, p6)
            } else {
                core::ptr::null_mut()
            };
            *((ddgame as *mut u8).add(offset as usize) as *mut *mut u8) = result;
        }
    }

    // ── Landscape-derived value at DDGame+0x468 ──
    if !landscape.is_null() {
        let land_vt = *(landscape as *const *const u32);
        let get_val: unsafe extern "thiscall" fn(*mut u8) -> u32 =
            core::mem::transmute(*land_vt.add(0xB));
        *((ddgame as *mut u8).add(0x468) as *mut u32) = get_val(landscape);
    }

    // NOTE: gfx_resource is NOT released here — arrow SpriteRegions need it.
    // It's released after the arrow loop below.

    // ── Arrow sprites + collision regions (32 iterations) ──
    {
        let gfx_handler = (*wrapper)._field_4c0;

        for i in 0u32..32 {
            // Format "arrow%02u.img\0" into stack buffer
            let mut name_buf = *b"arrow00.img\0\0\0\0\0";
            name_buf[5] = b'0' + (i / 10) as u8;
            name_buf[6] = b'0' + (i % 10) as u8;

            // Display vtable[5](1) — set active layer, returns context ptr
            let disp_vt = *((*ddgame).display as *const *const u32);
            let set_layer: unsafe extern "thiscall" fn(*mut DDDisplay, i32) -> *mut u8 =
                core::mem::transmute(*disp_vt.add(5));
            let layer_ctx = set_layer((*ddgame).display, 1);
            if i == 0 {
                let _ = crate::log::log_line(&format!(
                    "[DDGame] arrow[0]: layer_ctx=0x{:08X}, gfx=0x{:08X}",
                    layer_ctx as u32, gfx_handler as u32
                ));
            }

            let entry = gfx_dir_find_entry(name_buf.as_ptr(), gfx_handler);
            if i == 0 {
                let _ = crate::log::log_line(&format!(
                    "[DDGame] arrow[0]: entry=0x{:08X}",
                    entry as u32
                ));
            }

            let sprite: *mut u8;
            if !entry.is_null() {
                // Try gfx_handler->vtable[2](entry->field_4)
                let gfx_vt = *(gfx_handler as *const *const u32);
                let load_cached: unsafe extern "thiscall" fn(*mut u8, u32) -> *mut u8 =
                    core::mem::transmute(*gfx_vt.add(2));
                let entry_val = *(entry.add(4) as *const u32);
                let cached = load_cached(gfx_handler, entry_val);
                if i == 0 {
                    let _ = crate::log::log_line(&format!(
                        "[DDGame] arrow[0]: cached=0x{:08X}",
                        cached as u32
                    ));
                }
                if !cached.is_null() {
                    // Wrap with DisplayGfx constructor
                    let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                        core::mem::transmute(rb(0x4F5E80) as usize);
                    sprite = ctor(cached);
                } else {
                    // Fallback: load from file via GfxDir__LoadImage + IMG_Decode
                    sprite = call_gfx_load_and_wrap(gfx_handler, name_buf.as_ptr(), layer_ctx);
                }
            } else {
                // Entry not found — try direct file load
                sprite = call_gfx_load_and_wrap(gfx_handler, name_buf.as_ptr(), layer_ctx);
            }
            if i == 0 {
                let _ = crate::log::log_line(&format!(
                    "[DDGame] arrow[0]: sprite=0x{:08X}",
                    sprite as u32
                ));
            }

            // Store arrow sprite at DDGame+0x38+i*4
            (*ddgame).arrow_sprites[i as usize] = sprite;

            // Calculate collision region dimensions from sprite
            if !sprite.is_null() {
                let sprite_w = *(sprite.add(0x14) as *const i32);
                let sprite_h = *(sprite.add(0x18) as *const i32);
                let half_w = (sprite_w / 2 - 10).max(0);
                let half_h = (sprite_h / 2 - 10).max(0);

                // Create SpriteRegion for collision
                let alloc = wa_malloc(0x9C);
                core::ptr::write_bytes(alloc, 0, 0x9C);
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
                    gfx_resource_create(gfx_handler, core::ptr::null_mut());
            }
        }
    }

    let _ = crate::log::log_line("[DDGame] arrow loop done");

    // Release gfx_resource AFTER arrow loop (arrows need it for SpriteRegions)
    // vtable[3] = DisplayGfx__vmethod_3: thiscall(this, byte param_2), RET 4.
    if !gfx_resource.is_null() {
        let rvt = *(gfx_resource as *const *const u32);
        let release: unsafe extern "thiscall" fn(*mut u8, u8) = core::mem::transmute(*rvt.add(3));
        release(gfx_resource, 1);
    }

    // ── DisplayGfx at DDGame+0x138 ──
    // Original: alloc 0x4C, memset 0x2C, PUSH 0x100, MOV ECX=8, MOV EDI=0x1E0,
    // CALL 0x4F6370, MOV [alloc]=0x664144
    {
        let tsm = wa_malloc(0x4C); // original allocs 0x4C, not 0x2C!
        core::ptr::write_bytes(tsm, 0, 0x2C);
        if !tsm.is_null() {
            // TaskStateMachine__Init_Maybe: fastcall(ECX=8) + 1 stack(0x100), RET 0x4?
            // Also EDI=0x1E0 (implicit). Need naked bridge for EDI.
            // For now, just set vtable directly.
            *(tsm as *mut u32) = rb(0x664144); // DisplayGfx__vtable
        }
        (*ddgame).display_gfx = tsm;
    }

    // ── CoordList at DDGame+0x50C (capacity 600, 0x12C0 buffer) ──
    {
        let cl = wa_malloc(12) as *mut u32;
        *cl = 0; // count
        *cl.add(1) = 600; // capacity
        let data = wa_malloc(0x12C0);
        core::ptr::write_bytes(data, 0, 0x12C0);
        *cl.add(2) = data as u32;
        (*ddgame).coord_list = cl as *mut u8;

        // TODO: populate coord_list from landscape data (complex loop)
    }

    // ── Sprite resource loading via DDGameWrapper vtable[0] ──
    // DDNetGameWrapper__LoadResourceList: thiscall(ECX=wrapper) +
    // 5 stack params (layer, gfx_handler, base_path, data_table, table_size)
    {
        let landscape_ptr = (*wrapper).landscape as *const u8;
        let water_layer = *(landscape_ptr.add(0xB38) as *const *mut u8);
        let land_layer = *(landscape_ptr.add(0xB34) as *const *mut u8);
        let gfx_handler = (*wrapper)._field_4c0;

        let wrapper_vt = *(wrapper as *const *const u32);
        let _ = crate::log::log_line(&format!(
            "[DDGame] sprite resource loading: wrapper_vt=0x{:08X}, vt[0]=0x{:08X}, landscape=0x{:08X}, water=0x{:08X}, land=0x{:08X}",
            wrapper_vt as u32, *wrapper_vt, landscape_ptr as u32, water_layer as u32, land_layer as u32,
        ));
        let load_resource_list: unsafe extern "thiscall" fn(
            *mut DDGameWrapper,
            u32,
            *mut u8,
            *const u8,
            *const u8,
            u32,
        ) = core::mem::transmute(*wrapper_vt);
        // Load resources for layer 1 (main sprites)
        let _ = crate::log::log_line("[DDGame] load_resource_list #1 (main sprites)");
        load_resource_list(
            wrapper,
            1,
            gfx_handler,
            rb(0x643F2B) as *const u8, // base path
            rb(0x6AD2C0) as *const u8, // resource table
            0x1D88,                    // table size
        );
        let _ = crate::log::log_line(&format!(
            "[DDGame] after #1: sprite_cache[144]=0x{:08X}",
            (*ddgame).sprite_cache[144] as u32,
        ));

        // Set global flag based on game version
        let gv = (*(*ddgame).game_info).game_version;
        *(rb(0x6AF050) as *mut u32) = if gv < 8 { 0 } else { 0x10 };

        // Load resources for layer 1 with different table
        let _ = crate::log::log_line("[DDGame] load_resource_list #2");
        load_resource_list(
            wrapper,
            1,
            gfx_handler,
            rb(0x643F2B) as *const u8,
            rb(0x6AF048) as *const u8,
            0x18,
        );

        // Load resources for layer 2 (water)
        let _ = crate::log::log_line("[DDGame] load_resource_list #3 (water)");
        load_resource_list(
            wrapper,
            2,
            water_layer,
            rb(0x643F2B) as *const u8,
            rb(0x6AF060) as *const u8,
            0x2F4,
        );

        // Display vtable[5](3) — set active layer 3, returns context ptr
        let disp_vt = *((*ddgame).display as *const *const u32);
        let set_layer: unsafe extern "thiscall" fn(*mut DDDisplay, i32) -> *mut u8 =
            core::mem::transmute(*disp_vt.add(5));
        let _layer3_ctx = set_layer((*ddgame).display, 3);

        // Load back.spr and debris.spr (conditional)
        let disp_obj = (*wrapper).display as *mut u8;
        let disp_obj_vt = *(disp_obj as *const *const u32);
        // Check if gfx_mode high byte is set (original: uStack_123c._3_1_ != '\0')
        if (*wrapper).gfx_mode != 0 {
            let load_spr_94: unsafe extern "thiscall" fn(*mut u8, u32, u32, *mut u8, *const u8) =
                core::mem::transmute(*disp_obj_vt.add(0x94 / 4));
            load_spr_94(disp_obj, 3, 0x26D, land_layer, b"back.spr\0".as_ptr());

            let load_spr_7c: unsafe extern "thiscall" fn(
                *mut u8,
                u32,
                u32,
                u32,
                *mut u8,
                *const u8,
            ) = core::mem::transmute(*disp_obj_vt.add(0x7C / 4));
            load_spr_7c(disp_obj, 3, 0x26E, 0, land_layer, b"debris.spr\0".as_ptr());
        }

        // Load layer.spr into layer 2
        let load_spr_94: unsafe extern "thiscall" fn(*mut u8, u32, u32, *mut u8, *const u8) =
            core::mem::transmute(*disp_obj_vt.add(0x94 / 4));
        load_spr_94(
            disp_obj,
            2,
            0x26C,
            water_layer,
            b"layer\\layer.spr|layer.spr\0".as_ptr(),
        );

        // gradient_image_2 = 0
        (*ddgame).gradient_image_2 = core::ptr::null_mut();

        // ── Gradient image (0x030) ──
        // The gradient is loaded from "gradient.img" via GfxDir.
        // Simple path: height <= 0x60 AND level_height == 0x2B8
        let level_height = (*ddgame).level_height as i32;
        // Read sVar1 from display layer 3 context
        let layer3_ctx = set_layer((*ddgame).display, 3);
        let s_var1 = *(layer3_ctx.add(0x606) as *const i16);

        if s_var1 < 0x61 && level_height == 0x2B8 {
            // Simple gradient: load gradient.img directly
            let gradient =
                call_gfx_find_and_load(land_layer, b"gradient.img\0".as_ptr(), layer3_ctx);
            (*ddgame).gradient_image = gradient;
        } else {
            // Complex gradient computation — ~200 lines of color interpolation.
            // For now, skip this path (gradient_image remains null/0).
            // This affects the sky background appearance but not gameplay.
            // Set gradient_image to null (already zero from memset)
        }

        // ── Fill image → fill_pixel (0x7338) ──
        {
            let layer2_ctx = set_layer((*ddgame).display, 2);
            // In the original, fill.img uses piStack_126c which the decompiler
            // shows was set from piVar3 (water_layer from landscape+0xB38).
            let fill_sprite =
                call_gfx_find_and_load(water_layer, b"fill.img\0".as_ptr(), layer2_ctx);
            let _ = crate::log::log_line(&format!(
                "[DDGame] fill.img: water=0x{:08X} sprite=0x{:08X}",
                water_layer as u32, fill_sprite as u32,
            ));
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
        // Loads weapon icons (cow.img, pigeon.img, etc.), wind indicators, stop sign,
        // girder sprites, and creates the DDGame+0x37C DisplayGfx object needed by
        // CTaskLand__InitLandscape.
        // Convention: thiscall(ECX=gfx_handler_4c0) + 2 stack(ddgame, wrapper_4c4), RET 0x8.
        {
            let hud_load: unsafe extern "thiscall" fn(*mut u8, *mut DDGame, *mut u8) =
                core::mem::transmute(rb(va::DDGAME_LOAD_HUD_AND_WEAPON_SPRITES) as usize);
            hud_load(gfx_handler, ddgame, (*wrapper)._field_4c4);
            let _ = crate::log::log_line(&format!(
                "[DDGame] after LoadHudAndWeaponSprites: +0x37C=0x{:08X}",
                (*ddgame).sprite_cache[144] as u32,
            ));
        }
    }
    // ── Gradient image stub (DDGame+0x30) ──
    // The original constructor loads "gradient.img" here and stores the result at DDGame+0x30.
    // CTaskLand__InitLandscape reads DDGame+0x30/+0x34 and calls vtable[4](0, i) for each
    // pixel column. vtable[4] = CTaskOilDrum__ProcessFrame_stub (XOR EAX,EAX; RET 0x8) returns 0.
    // A minimal stub with [6]=0 (zero-width) causes the loop to not execute at all.
    if (*ddgame).gradient_image.is_null() {
        let obj = wa_malloc(0x2C);
        core::ptr::write_bytes(obj, 0, 0x2C);
        if !obj.is_null() {
            *(obj as *mut u32) = rb(0x6640EC); // vtable (DisplayGfx vtable, vtable[4]=ProcessFrame_stub)
                                               // [6] = height/width = 0 → CTaskLand loop: `if (0 < 0)` → skip
            (*ddgame).gradient_image = obj;
        }
    }

    // ── Release primary GfxHandler (vtable[3] = release, param 1 = free) ──
    let gfx_handler_4c0 = (*wrapper)._field_4c0;
    if !gfx_handler_4c0.is_null() {
        let gfx_vt = *(gfx_handler_4c0 as *const *const u32);
        let release: unsafe extern "thiscall" fn(*mut u8, u32) =
            core::mem::transmute(*gfx_vt.add(3));
        release(gfx_handler_4c0, 1);
    }

    // ── DDGame__InitDisplayFinal_Maybe (0x56A830): non-headless display finalization ──
    if !is_headless {
        let f: unsafe extern "stdcall" fn(*mut DDDisplay) =
            core::mem::transmute(rb(va::DDGAME_INIT_DISPLAY_FINAL) as usize);
        f((*wrapper).display);
    }

    // ── FUN_00570A90 (second call, conditional) ──
    if *(rb(0x88E485) as *const u8) == 0 {
        call_usercall_eax(wrapper, FUN_570A90_ADDR);
    }

    // ── Final display layer visibility (vtable[0x17], offset 0x5C) ──
    {
        let disp = (*wrapper).display as *mut u8;
        let vt = *(disp as *const *const u32);
        let set_vis: unsafe extern "thiscall" fn(*mut u8, i32, i32) =
            core::mem::transmute(*vt.add(0x17));
        set_vis(disp, 1, 0);
        set_vis(disp, 2, 0);
        set_vis(disp, 3, 1);
    }

    let _ = crate::log::log_line("[DDGame] init_graphics_and_resources DONE");
}

/// Helper: find entry in GfxDir and load image, or load directly.
/// Returns a DisplayGfx/sprite pointer or null.
#[cfg(target_arch = "x86")]
unsafe fn call_gfx_find_and_load(
    gfx_dir: *mut u8,
    name: *const u8,
    display_ctx: *mut u8,
) -> *mut u8 {
    let entry = gfx_dir_find_entry(name, gfx_dir);

    if !entry.is_null() {
        // Try cached load: gfx_dir->vtable[2](entry->field_4)
        let dir_vt = *(gfx_dir as *const *const u32);
        let load_cached: unsafe extern "thiscall" fn(*mut u8, u32) -> *mut u8 =
            core::mem::transmute(*dir_vt.add(2));
        let cached = load_cached(gfx_dir, *(entry.add(4) as *const u32));
        if !cached.is_null() {
            // Wrap with DisplayGfx__Constructor_Maybe (0x4F5E80)
            // This is stdcall(1 param), RET 0x4
            let ctor: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
                core::mem::transmute(rb(0x4F5E80) as usize);
            return ctor(cached);
        }
    }

    // Fallback: load image directly
    call_gfx_load_and_wrap(gfx_dir, name, display_ctx)
}

// Statics for usercall bridge addresses
static mut FUN_570E20_ADDR: u32 = 0;
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

/// Runtime address of TaskStateMachine__Init_Maybe.
static mut TSM_INIT_ADDR: u32 = 0;

/// Bridge to TaskStateMachine__Init_Maybe (0x4F6370).
/// Convention: usercall(ESI=object, ECX=1, EDI=height) + 1 stack(width), RET 0x4.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_tsm_init(_object: *mut u8, _width: u32, _height: u32) {
    core::arch::naked_asm!(
        "pushl %esi",
        "pushl %edi",
        "movl 12(%esp), %esi",    // ESI = object (1st param, shifted by 2 pushes)
        "movl 20(%esp), %edi",    // EDI = height (3rd param, shifted by 2 pushes)
        "pushl 16(%esp)",         // push width (2nd param, shifted by 2 pushes + 1 push)
        "movl $1, %ecx",         // ECX = 1
        "calll *({addr})",        // RET 0x4 cleans the pushed width
        "popl %edi",
        "popl %esi",
        "retl",
        addr = sym TSM_INIT_ADDR,
        options(att_syntax),
    );
}

/// Runtime address of GfxResource__Create_Maybe (set by init_constructor_addrs()).
static mut GFX_RESOURCE_CREATE_ADDR: u32 = 0;

/// Bridge to GfxResource__Create_Maybe (0x4F6300).
/// Convention: usercall(ECX=gfx_handler, EAX=data_ptr) + 1 stack(output), RET 0x4.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_gfx_resource_create(
    _gfx_handler: *mut u8,
    _data_ptr: *const u8,
    _output: *mut u8,
) -> *mut u8 {
    core::arch::naked_asm!(
        "pushl %ebp",
        "pushl %ebx",
        "movl 12(%esp), %ecx",   // ECX = gfx_handler (0=ebx,4=ebp,8=ret,12=handler)
        "movl 16(%esp), %eax",   // EAX = data_ptr (16=data)
        "pushl 20(%esp)",        // push output (20=output, but shifts after push!)
        "calll *({addr})",       // RET 0x4 cleans output
        "popl %ebx",
        "popl %ebp",
        "retl",
        addr = sym GFX_RESOURCE_CREATE_ADDR,
        options(att_syntax),
    );
}

/// Runtime address of GfxDir__FindEntry (set by init_constructor_addrs()).
static mut GFX_FIND_ENTRY_ADDR: u32 = 0;

/// Bridge to GfxDir__FindEntry (0x566520).
/// Convention: usercall(EAX=name) + 1 stack(gfx_handler), RET 0x4.
/// Returns entry pointer or null.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_gfx_find_entry(_name: *const u8, _gfx_handler: *mut u8) -> *mut u8 {
    core::arch::naked_asm!(
        "movl 4(%esp), %eax",     // EAX = name
        "pushl 8(%esp)",          // push gfx_handler (callee cleans via RET 0x4)
        "calll *({addr})",
        "retl",                   // caller cleans our 2 cdecl params
        addr = sym GFX_FIND_ENTRY_ADDR,
        options(att_syntax),
    );
}

/// Bridge to GfxHandler__LoadDir (0x5663E0).
/// Convention: usercall(EAX=handler), plain RET. Returns nonzero on success.
#[cfg(target_arch = "x86")]
unsafe fn call_gfx_load_dir(handler: *mut u8, addr: u32) -> i32 {
    let result: i32;
    core::arch::asm!(
        "call {addr}",
        addr = in(reg) addr,
        inlateout("eax") handler => result,
        out("ecx") _,
        out("edx") _,
        clobber_abi("C"),
    );
    result
}

// GfxDir__LoadImage is usercall(ESI=gfx_handler) + 1 stack(name), RET 0x4.
// Returns raw image pointer or null.
static mut GFX_LOAD_IMAGE_ADDR: u32 = 0;

#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_gfx_load_image(_gfx_handler: *mut u8, _name: *const u8) -> *mut u8 {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",     // ESI = gfx_handler
        "pushl 12(%esp)",         // push name (callee cleans via RET 0x4)
        "calll *({addr})",
        "popl %esi",
        "retl",
        addr = sym GFX_LOAD_IMAGE_ADDR,
        options(att_syntax),
    );
}

/// Helper: load image via GfxDir__LoadImage + wrap as DisplayGfx.
/// Used by arrow sprite loop when GfxDir__FindEntry returns null.
#[cfg(target_arch = "x86")]
unsafe fn call_gfx_load_and_wrap(
    gfx_handler: *mut u8,
    name: *const u8,
    display_ctx: *mut u8,
) -> *mut u8 {
    let image = call_gfx_load_image(gfx_handler, name);
    if image.is_null() {
        return core::ptr::null_mut();
    }
    // FUN_004F5F80(display_ctx, image, 1) — stdcall, RET 0xC (3 params)
    let f: unsafe extern "stdcall" fn(*mut u8, *mut u8, u32) -> *mut u8 =
        core::mem::transmute(rb(0x4F5F80) as usize);
    let result = f(display_ctx, image, 1);
    // Release the raw image
    let image_vt = *(image as *const *const u32);
    let destroy: unsafe extern "thiscall" fn(*mut u8, u32) = core::mem::transmute(*image_vt);
    destroy(image, 1);
    result
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
        Self {
            base: base as *const u8,
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
        &mut (*(self.blocks() as *mut FullTeamBlock).add(team_idx + 2))
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
    #[inline]
    pub unsafe fn ammo_index(&self, team_index: usize, weapon_id: u32) -> usize {
        let alliance_id = self.team_header(team_index).weapon_alliance as usize;
        alliance_id * 142 + weapon_id as usize
    }
}
