/// Known addresses in WA.exe 3.8.1 (image base 0x00400000).
///
/// These addresses are discovered through Ghidra analysis and
/// cross-referenced with wkJellyWorm/WormKit sources.
///
/// All addresses are virtual addresses (VA) as loaded in memory.
///
/// Each entry is registered in the global address registry via
/// `define_addresses!`, enabling runtime queries like
/// `registry::vtable_class_name()` and `registry::format_va()`.
pub mod va {
    // Segment layout:
    //   .text:  0x00401000 - 0x00619FFF (code)
    //   .rdata: 0x0061A000 - 0x00693FFF (read-only data)
    //   .data:  0x00694000 - 0x008C4157 (read-write data)
    //   .rsrc:  0x008C5000 - 0x00954FFF (resources)
    //   .reloc: 0x00955000 - 0x00983FFF (relocations)

    pub const IMAGE_BASE: u32 = 0x0040_0000;
    pub const TEXT_START: u32 = 0x0040_1000;
    pub const TEXT_END: u32 = 0x0061_9FFF;
    pub const RDATA_START: u32 = 0x0061_A000;
    pub const DATA_START: u32 = 0x0069_4000;
    pub const DATA_END: u32 = 0x008C_5000; // .rsrc starts here; .data/.bss ends just before

    // =========================================================================
    // Class definitions (vtable + constructor + vtable methods)
    // =========================================================================

    // Re-exported from task modules
    pub use crate::task::base::{
        CTASK_AIRSTRIKE_CTOR, CTASK_ARROW_CTOR, CTASK_CANISTER_CTOR, CTASK_CONSTRUCTOR,
        CTASK_CPU_CTOR, CTASK_CPU_VTABLE, CTASK_CROSS_CTOR, CTASK_DIRT_CTOR, CTASK_DIRT_VTABLE,
        CTASK_FIREBALL_CTOR, CTASK_FLAME_CTOR, CTASK_GAS_CTOR, CTASK_LAND_CTOR, CTASK_LAND_VTABLE,
        CTASK_OLDWORM_CTOR, CTASK_SCOREBUBBLE_CTOR, CTASK_SEABUBBLE_CTOR, CTASK_SEA_BUBBLE_VTABLE,
        CTASK_SMOKE_CTOR, CTASK_SPRITE_ANIM_CTOR, CTASK_SPRITE_ANIM_VTABLE, CTASK_VT0_INIT,
        CTASK_VT1_FREE, CTASK_VT2_HANDLE_MESSAGE, CTASK_VT3, CTASK_VT5, CTASK_VT6,
        CTASK_VT7_PROCESS_FRAME, CTASK_VTABLE,
    };
    pub use crate::task::cloud::{
        CTASK_CLOUD_CTOR, CTASK_CLOUD_READ_REPLAY_STATE, CTASK_CLOUD_VTABLE,
        CTASK_CLOUD_WRITE_REPLAY_STATE,
    };
    pub use crate::task::filter::{
        CTASK_FILTER_CTOR, CTASK_FILTER_SUBSCRIBE, CTASK_FILTER_VTABLE,
        CTASK_TEAM_CREATE_WEATHER_FILTER,
    };
    pub use crate::task::fire::{CTASK_FIRE_CTOR, CTASK_FIRE_VTABLE};
    pub use crate::task::game_task::{
        CGAMETASK_CONSTRUCTOR, CGAMETASK_SOUND_EMITTER_VT, CGAMETASK_VT0, CGAMETASK_VT1_FREE,
        CGAMETASK_VT2_HANDLE_MESSAGE, CGAMETASK_VTABLE,
    };
    pub use crate::task::mine_oil_drum::{
        CTASK_MINE_CTOR, CTASK_MINE_VTABLE, CTASK_OILDRUM_CTOR, CTASK_OILDRUM_VTABLE,
    };
    pub use crate::task::missile::{CTASK_MISSILE_CTOR, CTASK_MISSILE_VTABLE};
    pub use crate::task::supply_crate::{CTASK_CRATE_CTOR, CTASK_CRATE_VTABLE};
    pub use crate::task::team::{CTASK_TEAM_CTOR, CTASK_TEAM_VTABLE};
    pub use crate::task::turn_game::{
        CTASK_TURNGAME_CTOR, CTASK_TURN_GAME_VTABLE, TURNGAME_AUTO_SELECT_TEAMS,
        TURNGAME_HANDLE_MESSAGE, TURNGAME_HURRY_HANDLER,
    };
    pub use crate::task::worm::{CTASK_WORM_CONSTRUCTOR, CTASK_WORM_VTABLE};

    // Re-exported from audio modules
    pub use crate::audio::dssound::DS_SOUND_VTABLE;
    pub use crate::audio::music::{
        MUSIC_CONSTRUCTOR, MUSIC_DESTRUCTOR, MUSIC_PLAY_TRACK, MUSIC_VOLUME_DB_TABLE, MUSIC_VTABLE,
        STREAMING_AUDIO_FILL_AND_START, STREAMING_AUDIO_INIT, STREAMING_AUDIO_INIT_PLAYBACK,
        STREAMING_AUDIO_OPEN, STREAMING_AUDIO_OPEN_WAV, STREAMING_AUDIO_READ_CHUNK,
        STREAMING_AUDIO_RESET, STREAMING_AUDIO_STOP, STREAMING_AUDIO_TIMER_CALLBACK,
    };
    pub use crate::bitgrid::{
        BitGridBaseVtable, BitGridCollisionVtable, BitGridDisplayVtable, BIT_GRID_BASE_VTABLE,
        BIT_GRID_COLLISION_VTABLE, BIT_GRID_DISPLAY_VTABLE, BIT_GRID_INIT, BLIT_SPRITE_RECT,
        DRAW_LINE_CLIPPED, DRAW_LINE_TWO_COLOR,
    };
    pub use crate::display::base::DISPLAY_BASE_VTABLE;
    pub use crate::display::compat_renderer::{
        CompatRendererVtable, COMPAT_RENDERER_VTABLE, DDRAW8_RENDERER_VTABLE,
    };
    pub use crate::display::display_vtable::DISPLAY_GFX_VTABLE;
    pub use crate::display::palette::PALETTE_VTABLE;
    pub use crate::display::render_context::{RenderContextVtable, RENDER_CONTEXT_VTABLE};
    pub use crate::frontend::map_view::MAP_VIEW_VTABLE;
    pub use crate::input::controller::INPUT_CTRL_VTABLE;
    pub use crate::task::game_task::SOUND_EMITTER_VTABLE;

    crate::define_addresses! {
        class "DDGameWrapper" {
            /// DDGameWrapper vtable
            vtable DDGAME_WRAPPER_VTABLE = 0x0066_A30C;
            /// DDGameWrapper constructor
            ctor/Stdcall CONSTRUCT_DD_GAME_WRAPPER = 0x0056_DEF0;
            /// DDGameWrapper::InitReplay — usercall(EAX=game_info, ESI=this), plain RET
            fn/Usercall DDGAMEWRAPPER_INIT_REPLAY = 0x0056_F860;
            /// DDGameWrapper__LoadingProgressTick
            fn/Stdcall DDGAME_WRAPPER_LOADING_PROGRESS_TICK = 0x0057_17A0;
            /// DDGameWrapper__LoadSpeechWAV
            fn/Usercall DDGAMEWRAPPER_LOAD_SPEECH_WAV = 0x0057_1530;
        }

        class "DDGame" {
            /// DDGame constructor
            ctor/Stdcall CONSTRUCT_DD_GAME = 0x0056_E220;
            /// DDGame::InitGameState — stdcall(this=DDGameWrapper*), RET 0x4
            fn/Stdcall DDGAME_INIT_GAME_STATE = 0x0052_6500;
            /// DDGame__InitFields
            fn DDGAME_INIT_FIELDS = 0x0052_6120;
            /// DDGame__InitRenderIndices — usercall(ESI=ddgame), plain RET
            fn/Usercall DDGAME_INIT_RENDER_INDICES = 0x0052_6080;
            /// DDGame__InitVersionFlags — stdcall(ddgame_wrapper)
            fn/Stdcall DDGAME_INIT_VERSION_FLAGS = 0x0052_5BE0;
            /// DDGame__LoadFonts — loads .fnt font resources into the display.
            fn/Usercall DDGAME_LOAD_FONTS = 0x0057_0F30;
            /// DDGameWrapper__LoadFontExtension — loads .fex font extension for a font slot.
            fn/Stdcall DDGAME_WRAPPER_LOAD_FONT_EXTENSION = 0x0057_0E80;
            /// DDGame__LoadHudAndWeaponSprites
            fn/Thiscall DDGAME_LOAD_HUD_AND_WEAPON_SPRITES = 0x0053_D0E0;
            /// DDGame__InitPaletteGradientSprites
            fn/Stdcall DDGAME_INIT_PALETTE_GRADIENT_SPRITES = 0x0057_06D0;
            /// DDGame__InitFeatureFlags
            fn/Stdcall DDGAME_INIT_FEATURE_FLAGS = 0x0052_4700;
            /// DDGame__InitDisplayFinal_Maybe
            fn DDGAME_INIT_DISPLAY_FINAL = 0x0056_A830;
            /// DDGame__IsSuperWeapon
            fn/Usercall IS_SUPER_WEAPON = 0x0056_5960;
            /// DDGame__CheckWeaponAvail
            fn/Fastcall CHECK_WEAPON_AVAIL = 0x0053_FFC0;
        }

        class "PCLandscape" {
            /// PCLandscape vtable
            vtable PC_LANDSCAPE_VTABLE = 0x0066_B208;
            /// PCLandscape constructor (0xB44-byte object)
            ctor/Stdcall PC_LANDSCAPE_CONSTRUCTOR = 0x0057_ACB0;
            /// Applies explosion crater to terrain (vtable slot 2)
            fn PC_LANDSCAPE_APPLY_EXPLOSION = 0x0057_C820;
            /// Draws 8px checkered borders at landscape edges (vtable slot 6)
            fn PC_LANDSCAPE_DRAW_BORDERS = 0x0057_D7F0;
            /// Redraws a single terrain row (vtable slot 8)
            fn PC_LANDSCAPE_REDRAW_ROW = 0x0057_CF60;
            /// Clips and merges dirty rectangles for terrain redraw
            fn PC_LANDSCAPE_CLIP_AND_MERGE = 0x0057_D2B0;
        }

        class "LandscapeShader" {
            /// LandscapeShader vtable
            vtable LANDSCAPE_SHADER_VTABLE = 0x0066_B1DC;
        }

        class "DSSound" {
            // Vtable now defined via #[vtable(...)] in audio/dssound.rs
            /// DSSound constructor — usercall(EAX=this), plain RET
            ctor/Usercall CONSTRUCT_DS_SOUND = 0x0057_3D50;
            /// DSSound init buffers — usercall(EAX=dssound), plain RET
            fn/Usercall DSSOUND_INIT_BUFFERS = 0x0057_3E50;
            /// Loads all SFX WAVs
            fn/Stdcall DSSOUND_LOAD_EFFECT_WAVS = 0x0057_14B0;
            /// Loads all speech banks
            fn/Usercall DSSOUND_LOAD_ALL_SPEECH_BANKS = 0x0057_1A70;
            /// Loads one speech bank
            fn/Usercall DSSOUND_LOAD_SPEECH_BANK = 0x0057_1660;
        }

        class "DDKeyboard" {
            /// DDKeyboard vtable (0x33C-byte keyboard object)
            vtable DDKEYBOARD_VTABLE = 0x0066_AEC8;
            /// DDKeyboard::PollKeyboardState
            fn/Stdcall DDKEYBOARD_POLL_KEYBOARD_STATE = 0x0057_2290;
        }

        // Palette vtable is now defined via #[derive(Vtable)] in display/palette.rs

        class "DisplayBase" {
            // Primary vtable now defined via #[vtable(...)] in display/base.rs
            /// DisplayBase headless vtable
            vtable DISPLAY_BASE_HEADLESS_VTABLE = 0x0066_A0F8;
            /// DisplayBase constructor (0x3560-byte object)
            ctor/Stdcall DISPLAY_BASE_CTOR = 0x0052_2DB0;
        }

        class "InputCtrl" {
            // Vtable now defined via #[vtable(...)] in input/controller.rs
            /// Input controller initializer
            fn/Usercall INPUT_CTRL_INIT = 0x0058_C0D0;
        }

        // BitGrid vtables and init are now in display::bitgrid via define_addresses! + #[vtable].

        class "OpenGLCPU" {
            /// OpenGLCPU vtable (0x48-byte object)
            vtable OPENGL_CPU_VTABLE = 0x0067_74C0;
            /// OpenGLCPU constructor
            ctor CONSTRUCT_OPENGL_CPU = 0x005A_0850;
        }

        class "WaterEffect" {
            /// WaterEffect vtable (0xBC-byte object)
            vtable WATER_EFFECT_VTABLE = 0x0066_B268;
        }

        class "Sprite" {
            /// Sprite vtable (0x70-byte objects, 8 entries)
            vtable SPRITE_VTABLE = 0x0066_418C;
            /// ConstructSprite — usercall EAX=sprite_ptr, ECX=context_ptr
            ctor/Usercall CONSTRUCT_SPRITE = 0x004F_AA30;
            /// Sprite destructor — thiscall, vtable slot 0
            fn/Thiscall DESTROY_SPRITE = 0x004F_AA80;
            /// LoadSpriteFromVfs
            fn/Usercall LOAD_SPRITE_FROM_VFS = 0x004F_AAF0;
            /// ProcessSprite — parses .spr binary format
            fn/Usercall PROCESS_SPRITE = 0x004F_AB80;
            // Note: vtable 0x664144 is BitGrid display-layer vtable,
            // now defined via #[vtable] macro as BITGRID_DISPLAY_VTABLE.
        }

        class "GfxHandler" {
            /// GfxHandler vtable
            vtable GFX_DIR_VTABLE = 0x0066_B280;
            /// GfxHandler load sprites
            fn GFX_DIR_LOAD_SPRITES = 0x0057_0B50;
            /// GfxDir load directory
            fn GFX_DIR_LOAD_DIR = 0x0056_63E0;
            /// GfxDir find entry
            fn GFX_DIR_FIND_ENTRY = 0x0056_6520;
            /// GfxDir load image
            fn GFX_DIR_LOAD_IMAGE = 0x0056_66D0;
        }

        class "DisplayGfx" {
            /// DisplayGfx constructor
            ctor/Stdcall DISPLAYGFX_CTOR = 0x0056_9C10;
            /// DisplayGfx constructor (from raw image)
            ctor/Stdcall DISPLAYGFX_CONSTRUCTOR = 0x004F_5E80;
            /// DisplayGfx construct full (5 params)
            fn/Stdcall DISPLAYGFX_CONSTRUCT_FULL = 0x0056_3FC0;
            /// DisplayGfx init team palette display objects
            fn/Stdcall DISPLAY_GFX_INIT_TEAM_PALETTE_DISPLAY = 0x0057_03E0;
        }

    }

    // Backward-compat aliases (not registered separately — same address)
    /// CTask::vtable4 (same implementation as vt3 in base)
    pub const CTASK_VT4: u32 = CTASK_VT3;
    /// Alias for backward compatibility with validation code.
    pub const CGAMETASK_VTABLE2: u32 = CGAMETASK_SOUND_EMITTER_VT;
    /// Alias for callers using the old name.
    pub const CONSTRUCT_DISPLAY_GFX: u32 = DISPLAY_GFX_INIT;
    /// Duplicate: same as PC_LANDSCAPE_CONSTRUCTOR.
    pub const CONSTRUCT_PC_LANDSCAPE: u32 = PC_LANDSCAPE_CONSTRUCTOR;
    /// Duplicate: same as SPRITE_REGION_CONSTRUCTOR.
    pub const CONSTRUCT_SPRITE_REGION: u32 = SPRITE_REGION_CONSTRUCTOR;
    /// Duplicate: same as CTASK_TURNGAME_CTOR.
    pub const TURN_GAME_CONSTRUCTOR: u32 = CTASK_TURNGAME_CTOR;

    // =========================================================================
    // Replay / turn management
    // =========================================================================

    crate::define_addresses! {
        /// Loads .WAgame replay file, validates magic 0x4157
        fn/Stdcall REPLAY_LOADER = 0x0046_2DF0;
        /// Parses "MM:SS.FF" time string → frame number
        fn PARSE_REPLAY_POSITION = 0x004E_3490;
        /// Read length-prefixed string
        fn/Usercall REPLAY_READ_PREFIXED_STRING = 0x0046_1340;
        /// Read byte with range validation
        fn/Usercall REPLAY_READ_BYTE_VALIDATED = 0x0046_14D0;
        /// Read byte with signed range validation
        fn/Usercall REPLAY_READ_BYTE_RANGE = 0x0046_1540;
        /// Read u16 with range validation
        fn/Usercall REPLAY_READ_U16_VALIDATED = 0x0046_15B0;
        /// Read worm name
        fn/Usercall REPLAY_READ_WORM_NAME = 0x0046_1620;
        /// Validate team type byte range
        fn/Fastcall REPLAY_VALIDATE_TEAM_TYPE = 0x0046_1690;
        /// Post-process team color assignments
        fn/Stdcall REPLAY_PROCESS_TEAM_COLORS = 0x0046_6460;
        /// Apply scheme default values
        fn REPLAY_PROCESS_SCHEME_DEFAULTS = 0x0046_70F0;
        /// Process replay feature flags
        fn REPLAY_PROCESS_FLAGS = 0x0046_7280;
        /// Register observer team entry
        fn/Stdcall REPLAY_REGISTER_OBSERVER = 0x0046_7BC0;
        /// Process alliance/team setup
        fn REPLAY_PROCESS_ALLIANCE = 0x0046_8890;
        /// Validate team configuration
        fn/Stdcall REPLAY_VALIDATE_TEAM_SETUP = 0x0046_5E10;
        /// Routes game messages through the task handler tree
        fn GAME_MESSAGE_ROUTER = 0x0055_3BD0;
        /// Per-frame turn timer
        fn/Stdcall TURN_MANAGER_PROCESS_FRAME = 0x0055_FDA0;
        /// Control task HandleMessage
        fn CONTROL_TASK_HANDLE_MESSAGE = 0x0054_51F0;
        /// End-of-frame message queue / hurry processing
        fn GAME_FRAME_MESSAGE_PROCESSOR = 0x0053_1960;
        /// End-of-frame checksum computation (__thiscall, ECX=ctrl, stack=wrapper*)
        fn/Thiscall GAME_FRAME_CHECKSUM_PROCESSOR = 0x0053_29C0;
        /// Game state serialization for checksum (called by checksum processor)
        fn SERIALIZE_GAME_STATE = 0x0053_2330;
        /// Game state checksum: ROL-3-ADD hash (__fastcall)
        fn/Fastcall COMPUTE_STATE_CHECKSUM = 0x0054_6140;
        /// Multi-segment checksum variant
        fn COMPUTE_STATE_CHECKSUM_MULTI = 0x0054_6170;
        /// Main frame loop
        fn GAME_FRAME_DISPATCHER = 0x0053_1D00;
        /// Sends game packet if network buffer allows
        fn SEND_GAME_PACKET_CONDITIONAL = 0x0053_1880;
        /// Process replay state — large function (1032 lines)
        fn/Stdcall REPLAY_PROCESS_STATE = 0x0045_D640;
        /// Cleanup observer array
        fn/Usercall REPLAY_CLEANUP_OBSERVERS = 0x0053_EE00;
    }

    // =========================================================================
    // Gameplay functions
    // =========================================================================

    crate::define_addresses! {
        /// Game PRNG: rng = (rng + frame_counter) * 0x19660D + 0x3C6EF35F
        fn/Fastcall ADVANCE_GAME_RNG = 0x0053_F320;
        /// Terrain hit → debris particles → RNG
        fn GENERATE_DEBRIS_PARTICLES = 0x0054_6F70;
        fn CREATE_EXPLOSION = 0x0054_8080;
        fn SPECIAL_IMPACT = 0x0051_93D0;
        fn SPAWN_OBJECT = 0x0056_1CF0;
        fn WEAPON_RELEASE = 0x0051_C3D0;
        fn WORM_START_FIRING = 0x0051_B7F0;
        fn FIRE_WEAPON = 0x0051_EE60;
        fn CREATE_WEAPON_PROJECTILE = 0x0051_E0F0;
        /// stdcall(worm, fire_params, local_struct), RET 0xC
        fn PROJECTILE_FIRE = 0x0051_DFB0;
        /// Strike weapons (AirStrike, NapalmStrike, MineStrike, MoleSquadron, MailStrike).
        /// stdcall(worm, &subtype_34, local_struct), RET 0xC.
        /// Spawns CTaskAirStrike or similar. NOT for grenades — grenades use CWP.
        fn STRIKE_FIRE = 0x0051_E2C0;
        /// usercall(ECX=local_struct, EDX=worm, [ESP+4]=fire_params), RET 0x4
        fn PLACED_EXPLOSIVE = 0x0051_EC80;
        /// Spawns CTaskArrow (Shotgun, Longbow). Allocates 0x168 bytes.
        /// thiscall(ECX=worm, fire_params, local_struct), RET 0x8.
        fn CREATE_ARROW = 0x0051_ED90;
        /// stdcall(worm, fire_params, local_struct), RET 0xC
        fn ROPE_TYPE1_FIRE = 0x0051_E1C0;
        /// stdcall(worm, fire_params, local_struct), RET 0xC
        fn ROPE_TYPE3_FIRE = 0x0051_E240;
        /// Called by ProjectileFire per shot.
        /// usercall(EDI=spawn_data, stack=[worm, fire_params]), RET 0x8.
        fn PROJECTILE_FIRE_SINGLE = 0x0051_DCF0;
        /// Sin lookup table (1024 entries of Fixed16.16). cos = sin + 256 entries.
        global SIN_TABLE = 0x006A_1860;
        /// Load a string resource by ID. cdecl(resource_id) -> *const c_char.
        fn/Cdecl LOAD_STRING_RESOURCE = 0x0059_3180;
    }

    // =========================================================================
    // Weapon system
    // =========================================================================

    crate::define_addresses! {
        /// PlayWormSound: usercall(EDI=worm) + stack(sound_handle_id, volume), RET 0x8.
        /// Stops current streaming sound at worm+0x3B0, then starts a new one.
        fn PLAY_WORM_SOUND = 0x0051_50D0;
        /// StopWormSound: usercall(ESI=worm), plain RET.
        /// Stops streaming sound at worm+0x3B0 and clears the handle.
        fn STOP_WORM_SOUND = 0x0051_5180;
        /// SpawnEffect: complex usercall, RET 0x1C.
        /// Builds a 0x408-byte struct from params, SharedData lookup, HandleMessage(0x56).
        fn SPAWN_EFFECT = 0x0054_7C30;
        fn INIT_WEAPON_TABLE = 0x0053_CAB0;
        fn COUNT_ALIVE_WORMS = 0x0052_25A0;
        fn GET_AMMO = 0x0052_25E0;
        fn ADD_AMMO = 0x0052_2640;
        /// Not the main ammo decrement path
        fn SUBTRACT_AMMO = 0x0052_2680;
    }

    // =========================================================================
    // Team/worm accessor functions
    // =========================================================================

    crate::define_addresses! {
        /// Counts teams by alliance membership
        fn/Usercall COUNT_TEAMS_BY_ALLIANCE = 0x0052_2030;
        /// Sums health of all worms on a team
        fn/Fastcall GET_TEAM_TOTAL_HEALTH = 0x0052_24D0;
        /// Checks if a worm is in a "special" state
        fn/Usercall IS_WORM_IN_SPECIAL_STATE = 0x0052_26B0;
        /// Reads worm X,Y position into output pointers
        fn/Usercall GET_WORM_POSITION = 0x0052_2700;
        /// Checks if any worm has state 0x64
        fn/Usercall CHECK_WORM_STATE_0X64 = 0x0052_28D0;
        /// Per-team version of CheckWormState0x64
        fn/Usercall CHECK_TEAM_WORM_STATE_0X64 = 0x0052_2930;
        /// Scans all teams for any worm with state 0x8b
        fn/Usercall CHECK_ANY_WORM_STATE_0X8B = 0x0052_2970;
        /// Sets the active worm for a team
        fn/Usercall SET_ACTIVE_WORM_MAYBE = 0x0052_2500;
    }

    // =========================================================================
    // Game session
    // =========================================================================

    crate::define_addresses! {
        class "GameSession" {
            /// GameSession constructor
            ctor/Usercall GAME_SESSION_CONSTRUCTOR = 0x0058_BFA0;
            /// GameSession__Run
            fn/Usercall GAME_SESSION_RUN = 0x0057_2F50;
        }

        /// GameEngine__InitHardware
        fn/Thiscall GAME_ENGINE_INIT_HARDWARE = 0x0056_D350;
        /// GameEngine__Shutdown
        fn/Stdcall GAME_ENGINE_SHUTDOWN = 0x0056_DCD0;
    }

    // =========================================================================
    // Graphics / rendering
    // =========================================================================

    crate::define_addresses! {
        /// DisplayGfx::Init
        fn/Usercall DISPLAY_GFX_INIT = 0x0056_9D00;
        /// DisplayGfx vtable slot 19 — blit sprite
        fn/Thiscall DISPLAY_GFX_BLIT_SPRITE = 0x0056_B080;
        /// DisplayGfx bitmap sprite info lookup — usercall(EAX=bitmap_obj, EDX=palette), RET 0x18
        fn/Usercall DISPLAY_GFX_GET_BITMAP_SPRITE_INFO = 0x0057_3C50;
        /// DisplayGfx bitmap blit (clipped) — usercall(EAX=this, EDX=width), RET 0x14
        fn/Usercall DISPLAY_GFX_BLIT_BITMAP_CLIPPED = 0x0056_A700;
        /// DisplayGfx bitmap blit (tiled) — usercall(EAX=initial_x, EDI=tile_width), RET 0x10
        fn/Usercall DISPLAY_GFX_BLIT_BITMAP_TILED = 0x0056_A7D0;
        /// DisplayGfx flush render lock — releases lock, plain RET
        fn DISPLAY_GFX_FLUSH_RENDER_LOCK = 0x0056_A330;
        /// Streaming audio constructor
        fn/Stdcall STREAMING_AUDIO_CTOR = 0x0058_BC10;
        /// DDNetGameWrapper constructor
        fn/Stdcall DDNETGAME_WRAPPER_CTOR = 0x0056_D1F0;
        /// Timer object constructor
        fn/Usercall GAME_ENGINE_TIMER_CTOR = 0x0053_E950;
        fn CONSTRUCT_FRAME_BUFFER = 0x005A_2430;
        fn BLIT_SCREEN = 0x005A_2020;
        fn RQ_RENDER_DRAWING_QUEUE = 0x0054_2350;
        fn DRAW_LANDSCAPE = 0x005A_2790;
        fn RQ_DRAW_PIXEL = 0x0054_1D60;
        fn RQ_DRAW_LINE_STRIP = 0x0054_1DD0;
        fn RQ_DRAW_POLYGON = 0x0054_1E50;
        fn RQ_DRAW_CROSSHAIR = 0x0054_1ED0;
        fn RQ_DRAW_RECT = 0x0054_1F40;
        fn RQ_DRAW_SPRITE_GLOBAL = 0x0054_1FE0;
        fn RQ_DRAW_SPRITE_LOCAL = 0x0054_2060;
        fn RQ_DRAW_SPRITE_OFFSET = 0x0054_20E0;
        fn RQ_DRAW_BITMAP_GLOBAL = 0x0054_2170;
        fn RQ_DRAW_TEXTBOX_LOCAL = 0x0054_2200;
        fn RQ_DRAW_CLIPPED_SPRITE_MAYBE = 0x0054_22A0;
        fn RQ_CLIP_COORDINATES = 0x0054_2BA0;
        fn RQ_GET_CAMERA_OFFSET_MAYBE = 0x0054_2B10;
        fn RQ_CLIP_WITH_REF_OFFSET_MAYBE = 0x0054_2C70;
        fn RQ_TRANSFORM_WITH_ZOOM_MAYBE = 0x0054_2D50;
        fn RQ_SMOOTH_INTERPOLATE_MAYBE = 0x0054_2E60;
        fn RQ_UPDATE_CLIP_BOUNDS_MAYBE = 0x0054_2F10;
        fn RQ_SATURATE_CLIP_BOUNDS_MAYBE = 0x0054_2F70;
        fn RENDER_FRAME_MAYBE = 0x0056_E040;
        fn GAME_RENDER_MAYBE = 0x0053_3DC0;
        fn RENDER_TERRAIN_MAYBE = 0x0053_5000;
        fn RENDER_HUD_MAYBE = 0x0053_4F20;
        fn RENDER_TURN_STATUS_MAYBE = 0x0053_4E00;
        fn PALETTE_MANAGE_MAYBE = 0x0053_3C80;
        fn PALETTE_ANIMATE_MAYBE = 0x0053_3A80;
        fn LOAD_SPRITE = 0x0052_3400;
        fn OPENGL_INIT = 0x0059_F000;
        /// GfxResource__Create_Maybe
        fn GFX_RESOURCE_CREATE = 0x004F_6300;
        /// PaletteContext__Init
        fn/Usercall PALETTE_CONTEXT_INIT = 0x0054_11A0;
        /// PaletteContext__MapColor — thiscall(palette_ctx, rgb_u32), returns nearest palette index
        fn/Thiscall PALETTE_CONTEXT_MAP_COLOR = 0x0054_12B0;
        /// SpriteGfxTable__Init
        fn/Fastcall SPRITE_GFX_TABLE_INIT = 0x0054_1620;
        /// RingBuffer__Init
        fn/Usercall RING_BUFFER_INIT = 0x0054_1060;
        /// CGameTask__InitTeamScoring
        fn/Fastcall INIT_TEAM_SCORING = 0x0052_8510;
        /// CGameTask__InitAllianceData
        fn/Usercall INIT_ALLIANCE_DATA = 0x0052_62D0;
        /// CGameTask__InitTurnState
        fn/Usercall INIT_TURN_STATE = 0x0052_8690;
        /// CGameTask__InitLandscapeFlags
        fn/Usercall INIT_LANDSCAPE_FLAGS = 0x0052_8480;
        /// HudPanel constructor
        fn/Stdcall HUD_PANEL_CONSTRUCTOR = 0x0052_4070;
        /// DDGame__InitTeamsFromSetup
        fn/Stdcall INIT_TEAMS_FROM_SETUP = 0x0052_20B0;
        /// TeamManager constructor
        fn/Stdcall TEAM_MANAGER_CONSTRUCTOR = 0x0056_3D40;
        /// CTaskGameState constructor
        fn/Stdcall GAME_STATE_CONSTRUCTOR = 0x0053_2330;
        /// DisplayGfx::ConstructTextbox
        fn/Stdcall CONSTRUCT_TEXTBOX = 0x004F_AF00;
        fn/Stdcall FUN_567770 = 0x0056_7770;
        /// Buffer object constructor
        fn/Stdcall BUFFER_OBJECT_CONSTRUCTOR = 0x0054_5FD0;
        /// GameStateStream sub-init
        fn/Stdcall GAME_STATE_STREAM_INIT = 0x004F_B490;
        /// Display object constructor
        fn/Stdcall DISPLAY_OBJECT_CONSTRUCTOR = 0x0054_0440;
        /// SpriteRegion constructor (0x9C-byte)
        fn/Stdcall SPRITE_REGION_CONSTRUCTOR = 0x0057_DB20;
        fn FUN_570A90 = 0x0057_0A90;
        fn FUN_570E20 = 0x0057_0E20;
        /// IMG_Decode
        fn/Stdcall IMG_DECODE = 0x004F_5F80;
        /// DrawBungeeTrail
        fn/Stdcall DRAW_BUNGEE_TRAIL = 0x0050_0720;
        /// DrawCrosshairLine
        fn/Usercall DRAW_CROSSHAIR_LINE = 0x0051_97D0;
        fn DESTRUCT_PC_LANDSCAPE = 0x0057_B540;
        fn REDRAW_LAND_REGION = 0x0057_CC10;
        fn WRITE_LAND_RAW = 0x0057_C300;
    }

    // =========================================================================
    // Sound
    // =========================================================================

    crate::define_addresses! {
        /// DirectSoundCreate IAT thunk
        fn/Stdcall DIRECTSOUND_CREATE = 0x005B_493E;
        fn PLAY_SOUND_LOCAL = 0x004F_DFE0;
        fn PLAY_SOUND_GLOBAL = 0x0054_6E20;
        /// IsSoundSuppressed
        fn/Fastcall IS_SOUND_SUPPRESSED = 0x0052_61E0;
        /// DispatchGlobalSound
        fn/Fastcall DISPATCH_GLOBAL_SOUND = 0x0052_6270;
        /// RecordActiveSound
        fn/Usercall RECORD_ACTIVE_SOUND = 0x0054_6260;
        /// CTaskWorm::PlaySound2 (FUN_00515020): usercall(EDI=worm) + stdcall(sound_id, volume, flags).
        /// Stop+play on secondary sound handle (+0x3B4). 23 callers in WA.
        fn/Usercall WORM_PLAY_SOUND_2 = 0x0051_5020;
        /// LoadAndPlayStreamingPositional (0x546BB0): usercall(EAX=task) + stack(volume, sound_id, flags, x, y).
        /// Like LoadAndPlayStreaming but with explicit position. Only caller is PlayWormSound2.
        fn/Usercall LOAD_AND_PLAY_STREAMING_POSITIONAL = 0x0054_6BB0;
        /// LoadAndPlayStreaming: usercall(EAX=task, ESI=&sound_emitter) + stack(sound_id, flags, volume).
        /// Checks game conditions, then starts a streaming sound. Returns handle | 0x40000000.
        fn/Usercall LOAD_AND_PLAY_STREAMING = 0x0054_6C20;
        /// ComputeDistanceParams
        fn/Fastcall COMPUTE_DISTANCE_PARAMS = 0x0054_6300;
        /// DispatchLocalSound
        fn/Usercall DISPATCH_LOCAL_SOUND = 0x0054_6360;
        /// PlayLocalNoEmitter
        fn/Thiscall PLAY_LOCAL_NO_EMITTER = 0x0054_6430;
        /// PlayLocalWithEmitter
        fn/Usercall PLAY_LOCAL_WITH_EMITTER = 0x0054_63F0;
        /// PlaySoundPooled_Direct
        fn/Fastcall PLAY_SOUND_POOLED_DIRECT = 0x0054_6B50;
        /// Distance3D_Attenuation
        fn/Usercall DISTANCE_3D_ATTENUATION = 0x0054_30F0;
        /// ActiveSoundTable::stop_sound — stops an active streaming sound by handle.
        fn ACTIVE_SOUND_TABLE_STOP_SOUND = 0x0054_6490;
    }

    // =========================================================================
    // Speech / Voice Lines / WAV Player / Fanfare
    // =========================================================================

    crate::define_addresses! {
        /// Speech line table in .rdata
        data SPEECH_LINE_TABLE = 0x006A_F770;
        /// WAV Player: load and play
        fn/Usercall WAV_PLAYER_LOAD_AND_PLAY = 0x0059_9B40;
        /// WAV Player: play
        fn/Usercall WAV_PLAYER_PLAY = 0x0059_96E0;
        /// WAV Player: stop
        fn/Usercall WAV_PLAYER_STOP = 0x0059_9670;
        /// FeSfx WavPlayer global instance
        global FESFX_WAV_PLAYER = 0x006A_C888;
        /// Fanfare WavPlayer global instance
        global FANFARE_WAV_PLAYER = 0x006A_C890;
        /// WA data path string buffer
        global WA_DATA_PATH = 0x0088_E282;
        /// Team config fanfare name lookup
        fn/Usercall GET_TEAM_CONFIG_NAME = 0x004A_62A0;
        /// Builds fanfare path, plays via WavPlayer
        fn/Stdcall PLAY_FANFARE_DEFAULT = 0x004D_7500;
        /// Loads fanfare WAV with fallback
        fn/Thiscall PLAY_FANFARE = 0x004D_7630;
        /// Gets current team, calls PlayFanfare
        fn/Usercall PLAY_FANFARE_CURRENT_TEAM = 0x004D_78E0;
        /// Builds fesfx path, plays via WavPlayer
        fn/Stdcall PLAY_FE_SFX = 0x004D_7960;
    }

    // =========================================================================
    // MFC wrappers
    // =========================================================================

    crate::define_addresses! {
        /// AfxCtxMessageBoxA
        fn/Cdecl AFXCTX_MESSAGEBOX_A = 0x005C_2055;
        /// CWormsApp::DoMessageBox
        fn/Thiscall CWORMSAPP_DO_MESSAGEBOX = 0x004E_B730;
        /// ATL::CSimpleStringT::operator=
        fn/Thiscall CSTRING_OPERATOR_ASSIGN = 0x0040_1D20;
        /// String resource lookup + assign
        fn/Stdcall CSTRING_ASSIGN_RESOURCE = 0x004A_39F0;
        /// CSimpleStringT::SetString
        fn/Thiscall CSTRING_SET_STRING = 0x0040_1EA0;
    }

    // =========================================================================
    // Chat / UI
    // =========================================================================

    crate::define_addresses! {
        fn SHOW_CHAT_MESSAGE = 0x0052_ACB0;
        fn ON_CHAT_INPUT = 0x0052_B730;
    }

    // =========================================================================
    // Frontend / menu screens
    // =========================================================================

    crate::define_addresses! {
        /// Main navigation loop (CWinApp::Run override)
        fn FRONTEND_MAIN_NAVIGATION_LOOP = 0x004E_6440;
        fn/Usercall FRONTEND_CHANGE_SCREEN = 0x0044_7A20;
        /// Wraps DoModal: palette transition + custom DoModal
        fn FRONTEND_DO_MODAL_WRAPPER = 0x0044_7960;
        fn FRONTEND_FRAME_CONSTRUCTOR = 0x004E_CCA0;
        fn FRONTEND_DIALOG_CONSTRUCTOR = 0x0044_6BA0;
        fn FRONTEND_PALETTE_ANIMATION = 0x0042_2180;
        fn FRONTEND_LOAD_TRANSITION_PAL = 0x0044_7AA0;
        fn FRONTEND_PRE_TRANSITION_CLEANUP = 0x004E_4AE0;
        fn FRONTEND_POST_SCREEN_CLEANUP = 0x004E_B450;
        fn FRONTEND_ON_INITIAL_LOAD = 0x0042_9830;
        fn FRONTEND_LAUNCH_SINGLE_PLAYER = 0x0044_1D80;
        fn FRONTEND_ON_MULTIPLAYER = 0x0044_E850;
        fn FRONTEND_ON_NETWORK = 0x0044_EC10;
        fn FRONTEND_ON_MINIMIZE = 0x0048_6A10;
        fn FRONTEND_ON_OPTIONS_ACCEPT = 0x0048_DAB0;
        fn FRONTEND_ON_START_GAME = 0x004F_14A0;
        fn CDIALOG_DO_MODAL_CUSTOM = 0x0040_FD60;
        fn CDIALOG_CUSTOM_MSG_PUMP = 0x0040_FBE0;
        fn FRONTEND_DIALOG_ON_IDLE = 0x0040_FF90;
        fn FRONTEND_DIALOG_PAINT_CONTROL_TREE = 0x0040_BF60;
        fn FRONTEND_DIALOG_RENDER_BACKGROUND = 0x0040_4250;
        fn SURFACE_BLIT = 0x0040_3BF0;
        fn FRONTEND_DEATHMATCH_CTOR = 0x0044_0F40;
        fn FRONTEND_LOCAL_MP_CTOR = 0x0049_C420;
        fn FRONTEND_TRAINING_CTOR = 0x004E_0880;
        fn FRONTEND_MISSIONS_CTOR = 0x0049_9190;
        fn FRONTEND_POST_INIT_CTOR = 0x004C_91B0;
        fn FRONTEND_MAIN_MENU_CTOR = 0x0048_66C0;
        fn FRONTEND_SINGLE_PLAYER_CTOR = 0x004D_69F0;
        fn FRONTEND_CAMPAIGN_A_CTOR = 0x004A_2B70;
        fn FRONTEND_CAMPAIGN_B_CTOR = 0x004A_24D0;
        fn FRONTEND_ADV_SETTINGS_CTOR = 0x0042_79E0;
        fn FRONTEND_INTRO_MOVIE_CTOR = 0x0047_0870;
        fn FRONTEND_NETWORK_HOST_CTOR = 0x004A_DCA0;
        fn FRONTEND_NETWORK_ONLINE_CTOR = 0x004A_CBC0;
        fn FRONTEND_NETWORK_PROVIDER_CTOR = 0x004A_7990;
        fn FRONTEND_NETWORK_SETTINGS_CTOR = 0x004C_23C0;
        fn FRONTEND_LAN_CTOR = 0x0048_0A80;
        fn FRONTEND_WORMNET_CTOR = 0x0047_2400;
        fn FRONTEND_LOBBY_HOST_CTOR = 0x004B_0160;
        fn FRONTEND_LOBBY_GAME_START_CTOR = 0x004B_DBE0;
    }

    // =========================================================================
    // Scheme file operations
    // =========================================================================

    crate::define_addresses! {
        /// Reads .wsc file into scheme struct
        fn/Stdcall SCHEME_READ_FILE = 0x004D_3890;
        /// Checks if scheme file exists
        fn/Stdcall SCHEME_FILE_EXISTS = 0x004D_4CD0;
        /// Saves scheme struct to .wsc file
        fn/Thiscall SCHEME_SAVE_FILE = 0x004D_44F0;
        /// Variant file-exists check for numbered schemes
        fn SCHEME_FILE_EXISTS_NUMBERED = 0x004D_4E00;
        /// Version detection
        fn SCHEME_DETECT_VERSION = 0x004D_4480;
        /// Extracts built-in schemes from PE resources
        fn SCHEME_EXTRACT_BUILTINS = 0x004D_5720;
        /// Copies payload data + V3 defaults into scheme struct
        fn/Fastcall SCHEME_INIT_FROM_DATA = 0x004D_5020;
        /// Validates weapon ammo counts
        fn SCHEME_CHECK_WEAPON_LIMITS = 0x004D_50E0;
        /// Validates V3 extended options
        fn SCHEME_VALIDATE_EXTENDED_OPTIONS = 0x004D_5110;
        /// Scans User\Schemes\ directory
        fn SCHEME_SCAN_DIRECTORY = 0x004D_54E0;
        /// Slot 13 feature check
        fn SCHEME_SLOT13_CHECK = 0x004D_A4C0;
        /// Load built-in scheme by ID
        fn/Stdcall SCHEME_LOAD_BUILTIN = 0x004D_4840;
        /// Validate extended scheme options
        fn/Cdecl SCHEME_VALIDATE_EXTENDED = 0x004D_5110;
    }

    // =========================================================================
    // Configuration / registry
    // =========================================================================

    crate::define_addresses! {
        /// Theme file size check
        fn/Cdecl THEME_GET_FILE_SIZE = 0x0044_BA80;
        /// Theme file load
        fn/Stdcall THEME_LOAD = 0x0044_BB20;
        /// Theme file save
        fn/Stdcall THEME_SAVE = 0x0044_BBC0;
        /// Recursive registry key deletion
        fn/Stdcall REGISTRY_DELETE_KEY_RECURSIVE = 0x004E_4D10;
        /// Registry cleanup
        fn/Stdcall REGISTRY_CLEAN_ALL = 0x004C_90D0;
        /// Loads game options from registry
        fn/Stdcall GAMEINFO_LOAD_OPTIONS = 0x0046_0AC0;
        /// Reads CrashReportURL from Options
        fn/Cdecl OPTIONS_GET_CRASH_REPORT_URL = 0x005A_63F0;
    }

    // =========================================================================
    // Lobby / network
    // =========================================================================

    crate::define_addresses! {
        fn LOBBY_HOST_COMMANDS = 0x004B_9B00;
        fn LOBBY_CLIENT_COMMANDS = 0x004A_ABB0;
        /// Allocates space in packet queue
        fn/Usercall SEND_GAME_PACKET_WRAPPED = 0x0054_1130;
        fn LOBBY_DISPLAY_MESSAGE = 0x0049_3CB0;
        fn LOBBY_SEND_GREENTEXT = 0x004A_A990;
        fn LOBBY_PRINT_USED_VERSION = 0x004B_7E20;
        fn LOBBY_ON_DISCONNECT = 0x004B_AE40;
        fn LOBBY_ON_GAME_END = 0x004B_AEC0;
        fn LOBBY_ON_MESSAGE = 0x004B_D400;
        fn LOBBY_DIALOG_CONSTRUCTOR = 0x004C_D9A0;
        fn NETWORK_IS_AVAILABLE = 0x004D_4920;
    }

    // =========================================================================
    // Memory / CRT
    // =========================================================================

    crate::define_addresses! {
        /// WA internal malloc — cdecl(size) → *mut u8
        fn/Cdecl WA_MALLOC = 0x005C_0AE3;
        fn WA_MALLOC_MEMSET = 0x0053_E910;
        fn/Cdecl WA_FREE = 0x005D_0D2B;
        /// WA's CRT _fopen
        fn/Cdecl WA_FOPEN = 0x005D_3271;
        /// WA's CRT _fileno
        fn/Cdecl WA_FILENO = 0x005D_5155;
        /// WA's CRT _get_osfhandle
        fn/Cdecl WA_GET_OSFHANDLE = 0x005D_7273;
        /// WA's CRT srand
        fn/Cdecl WA_SRAND = 0x005D_293E;
        /// WA's CRT rand
        fn/Cdecl WA_RAND = 0x005D_294B;
        /// WA's CRT _gmtime64
        fn/Cdecl WA_GMTIME64 = 0x005D_34C0;
        /// WA's CRT malloc (raw)
        fn/Cdecl WA_CRT_MALLOC = 0x005C_0AB8;
    }

    // =========================================================================
    // Bitmap font system
    // =========================================================================

    crate::define_addresses! {
        fn FONT_LOAD_FONTS = 0x0041_4680;
        fn FONT_RENDER_GLYPHS = 0x0041_43D0;
        fn FONT_DRAW_TEXT = 0x0042_7830;
        fn/Thiscall DISPLAY_GFX_DRAW_TEXT_ON_BITMAP = 0x0052_36B0;
        fn/Thiscall DISPLAY_GFX_CONSTRUCT_TEXTBOX = 0x004F_AF00;
        fn/Stdcall SET_TEXTBOX_TEXT = 0x004F_B070;
    }

    // =========================================================================
    // MapView
    // =========================================================================

    crate::define_addresses! {
        /// MapView constructor
        fn/Stdcall MAP_VIEW_CONSTRUCTOR = 0x0044_7E80;
        /// MapView load terrain file
        fn/Stdcall MAP_VIEW_LOAD = 0x0044_A9A0;
        /// MapView copy info to game state
        fn/Usercall MAP_VIEW_COPY_INFO = 0x0044_9B60;
        /// Load string resource by ID
        fn/Stdcall WA_LOAD_STRING = 0x0059_3180;
    }

    // =========================================================================
    // String constants in .rdata
    // =========================================================================

    crate::define_addresses! {
        string STR_CDROM_SPR = 0x0066_A3A8;
        string STR_COLOURS_IMG = 0x0066_A3B4;
        string STR_MASKS_IMG = 0x0066_A3C0;
        /// Empty base path for sprite resource loading
        string SPRITE_RESOURCE_BASE_PATH = 0x0064_3F2B;
        /// "3.8.1" literal string
        string STR_VERSION_381 = 0x0064_1C60;
    }

    // =========================================================================
    // Data tables in .rdata/.data
    // =========================================================================

    crate::define_addresses! {
        data SPRITE_RESOURCE_TABLE_1 = 0x006A_D2C0;
        data SPRITE_RESOURCE_TABLE_2 = 0x006A_F048;
        data WATER_RESOURCE_TABLE = 0x006A_F060;
        /// V3 extended options defaults (110 bytes)
        data SCHEME_V3_DEFAULTS = 0x0064_9AB8;
        /// Per-weapon max ammo table (39 bytes)
        data SCHEME_WEAPON_AMMO_LIMITS = 0x006A_D130;
        /// Version string table
        data VERSION_STRING_TABLE = 0x006A_B480;
        /// Version suffix table
        data VERSION_SUFFIX_TABLE = 0x0069_9814;
        /// "data\land.dat" string constant
        string G_LAND_DAT_STRING = 0x0064_DA58;
    }

    // =========================================================================
    // Global variables (in .data)
    // =========================================================================

    crate::define_addresses! {
        global G_SPRITE_VERSION_FLAG = 0x006A_F050;
        global G_DISPLAY_MODE_FLAG = 0x0088_E485;
        global G_CURRENT_SCREEN = 0x006B_3504;
        global G_CHAR_WIDTH_TABLE = 0x006B_2DD9;
        global G_FRONTEND_FRAME = 0x006B_3908;
        global G_FRONTEND_HWND = 0x006B_390C;
        global G_SKIP_TO_MAIN_MENU = 0x007A_083D;
        global G_AUTO_NETWORK_FLAG = 0x007A_083F;
        global G_RENDER_CONTEXT = 0x0079_D6D4;
        /// Stipple checkerboard parity — toggled (XOR 1) each render frame in GameRender.
        /// Used by DisplayGfx__BlitStippled to alternate the checkerboard pattern.
        global G_STIPPLE_PARITY = 0x007A_087C;
        global G_FONT_ARRAY = 0x007A_0F58;
        global G_MAIN_MENU_ACTIVE = 0x007C_0A20;
        global G_CWINAPP = 0x007C_03D0;
        global G_NETWORK_MODE = 0x007C_0D40;
        global G_NETWORK_SUBTYPE = 0x007C_0D68;
        /// Game session context pointer
        global G_GAME_SESSION = 0x007A_0884;
        global G_FULLSCREEN_FLAG = 0x007A_084C;
        global G_SUPPRESS_CURSOR = 0x0088_E485;
        global IAT_MAP_WINDOW_POINTS = 0x0061_A588;
        global G_SPRITE_DATA_BYTES = 0x007A_0864;
        global G_SPRITE_FRAME_COUNT = 0x007A_0868;
        global G_SPRITE_PIXEL_AREA = 0x007A_086C;
        global G_SPRITE_PALETTE_BYTES = 0x007A_0870;
        global G_GAME_INFO = 0x0077_49A0;
        global G_FRAME_BUFFER_PTR = 0x007A_0EEC;
        global G_FRAME_BUFFER_WIDTH = 0x007A_0EF0;
        global G_FRAME_BUFFER_HEIGHT = 0x007A_0EF4;
        global G_CRASH_REPORT_URL = 0x0079_FFD8;
        global G_VERSION_BYTE = 0x0069_7702;
    }

    // =========================================================================
    // Trig lookup tables / scratch buffers
    // =========================================================================

    crate::define_addresses! {
        /// Sine lookup table — 1024 entries of i32 (fixed-point 16.16)
        data G_SIN_TABLE = 0x006A_1860;
        /// Cosine lookup table — 1024 entries of i32 (fixed-point 16.16)
        data G_COS_TABLE = 0x006A_1C60;
        /// Global vertex scratch buffer
        global G_VERTEX_SCRATCH_BUFFER = 0x008B_1470;
    }

    // =========================================================================
    // Replay globals
    // =========================================================================

    crate::define_addresses! {
        global G_REPLAY_STATE = 0x0087_D3F8;
        global G_TEAM_HEADER_DATA = 0x0087_79E4;
        global G_TEAM_SECONDARY_DATA = 0x0087_D438;
        global G_REPLAY_GAME_ID = 0x0088_AF50;
        global G_REPLAY_SUB_FORMAT = 0x0088_AF54;
        global G_REPLAY_VERSION_ID = 0x0088_ABB0;
        global G_REPLAY_SCHEME_PRESENT = 0x0088_AE0C;
        global G_ARTCLASS_COUNTER = 0x0088_C790;
        global G_RANDOM_SEED = 0x0088_D0B4;
        global G_SAVED_RANDOM_SEED = 0x0088_ABAC;
        global G_REPLAY_FILENAME = 0x0088_AF58;
        global G_DATA_DIR = 0x0088_E078;
        global G_LOG_FILE_PTR = 0x0088_C370;
        global G_OBSERVER_ARRAY = 0x0088_C35C;
        global G_OBSERVER_COUNT = 0x0088_AF4C;
        global G_RECORDING_TIMESTAMP_FLAG = 0x0088_C36C;
        global G_REPLAY_VER_FLAG_A = 0x0088_AF42;
        global G_REPLAY_VER_FLAG_B = 0x0088_AF43;
        global G_REPLAY_GAME_MODE = 0x0088_AF44;
        global G_SCHEME_HEADER = 0x0088_DAD4;
        global G_SCHEME_DEST = 0x0088_DACC;
        global G_SCHEME_DATA = 0x0088_DAE0;
        global G_SCHEME_OPTIONS = 0x0088_DBB8;
        global G_SCHEME_V3_DATA = 0x0088_DC04;
        global G_HOST_PLAYER = 0x0087_79E0;
        global G_PLAYER_ARRAY = 0x0087_79E4;
        global G_PLAYER_COUNT = 0x0087_D0DE;
        global G_TEAM_DATA = 0x0087_7FFC;
        global G_TEAM_COUNT = 0x0087_D0E0;
        global G_REPLAY_NAME = 0x0087_D0E1;
        global G_MAP_BYTE_1 = 0x0087_250C;
        global G_MAP_BYTE_2 = 0x0087_2508;
        global G_MAP_SEED = 0x0087_D430;
        global G_WORM_NAMES = 0x0087_8097;
    }

    // =========================================================================
    // Scheme data globals
    // =========================================================================

    crate::define_addresses! {
        global SCHEME_ACTIVE_WEAPON_DATA = 0x0088_DB05;
        global SCHEME_SLOT_FLAGS = 0x006B_329C;
        global SCHEME_MODIFIER_GUARD = 0x0088_E460;
    }

    // =========================================================================
    // Configuration globals
    // =========================================================================

    crate::define_addresses! {
        global G_BASE_DIR = 0x0088_E282;
        global G_GAMEINFO_BLOCK_F485 = 0x0088_DFF3;
        global G_CONFIG_BYTE_F3A0 = 0x007C_0D38;
        global G_CONFIG_DWORDS_F3B4 = 0x0088_E39C;
        global G_CONFIG_GUARD = 0x0088_C374;
        global G_CONFIG_DWORDS_F3F4 = 0x0088_E3B8;
        global G_CONFIG_DWORD_DAE8 = 0x0088_E390;
        global G_CONFIG_DWORDS_F3D4 = 0x0088_E3B0;
        global G_CONFIG_DWORDS_F3C4 = 0x0088_E400;
        global G_CONFIG_DWORD_F3E4 = 0x0088_E44C;
        global G_STREAMS_DIR = 0x0088_AE18;
        global G_STREAM_INDICES = 0x0088_AE9C;
        global G_STREAM_INDICES_END = 0x0088_AEDC;
        global G_STREAM_FLAG = 0x0088_E394;
        global G_STREAM_VOLUME = 0x0088_AEDD;
    }

    // =========================================================================
    // DDGame struct offsets (not VAs — kept as manual constants)
    // =========================================================================

    pub mod ddgame_offsets {
        /// Offset to TurnGame object pointer
        pub const TURN_GAME: u32 = 0x08;
        /// Offset to game global state pointer
        pub const GAME_GLOBAL: u32 = 0x488;
        /// Offset to PC_Landscape pointer
        pub const PC_LANDSCAPE: u32 = 0x4CC;
        /// Offset to weapon table pointer
        pub const WEAPON_TABLE: u32 = 0x510;
        /// Offset to RenderQueue pointer
        pub const RENDER_QUEUE: u32 = 0x524;
        /// Offset to weapon panel pointer
        pub const WEAPON_PANEL: u32 = 0x548;

        /// Deferred hurry flag. Set to 1 during replay instead of sending network
        /// packet. GameFrameEndProcessor (0x531960) reads this and converts it to
        /// a local Hurry message (TaskMessage 0x17 = 23).
        pub const DEFERRED_HURRY_FLAG: u32 = 0x7E41;
    }

    // =========================================================================
    // GameInfo struct offsets (not VAs — kept as manual constants)
    // =========================================================================

    pub mod game_info_offsets {
        // === Speech configuration ===

        /// Number of teams with speech enabled (byte). Used by LoadAllSpeechBanks.
        pub const SPEECH_TEAM_COUNT: u32 = 0x44C;
        /// Per-team speech config stride (0xC2 = 0x81 base path + 0x41 dir name).
        pub const SPEECH_TEAM_STRIDE: u32 = 0xC2;
        /// Offset to per-team speech base path (char[0x81]).
        pub const SPEECH_BASE_PATH: u32 = 0xF486;
        /// Offset to per-team speech directory name (char[0x41]).
        pub const SPEECH_DIR: u32 = 0xF507;
        /// Default speech base path (for fallback).
        pub const DEFAULT_SPEECH_BASE_PATH: u32 = 0xF3C4;
        /// Default speech directory name (for fallback).
        pub const DEFAULT_SPEECH_DIR: u32 = 0xF445;

        // === Replay configuration ===

        /// Replay state flag A.
        pub const REPLAY_STATE_FLAG_A: u32 = 0xDB08;
        /// Replay state flag B.
        pub const REPLAY_STATE_FLAG_B: u32 = 0xDB0A;
        /// Replay active flag.
        pub const REPLAY_ACTIVE: u32 = 0xDB48;
        /// Input replay file path (string buffer, 0x400 bytes).
        pub const REPLAY_INPUT_PATH: u32 = 0xDB60;
        /// Output replay file path (for recording, 0x400 bytes).
        pub const REPLAY_OUTPUT_PATH: u32 = 0xDF60;
    }
}
