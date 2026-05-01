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

    pub const IMAGE_BASE: u32 = 0x00400000;
    pub const TEXT_START: u32 = 0x00401000;
    pub const TEXT_END: u32 = 0x00619FFF;
    pub const RDATA_START: u32 = 0x0061A000;
    pub const DATA_START: u32 = 0x00694000;
    pub const DATA_END: u32 = 0x008C5000; // .rsrc starts here; .data/.bss ends just before

    // =========================================================================
    // Class definitions (vtable + constructor + vtable methods)
    // =========================================================================

    // Re-exported from task modules
    pub use crate::game::game_task_message::{
        WORLD_ENTITY_COMPUTE_EXPLOSION_DAMAGE, WORLD_ENTITY_IS_SOUND_HANDLE_EXPIRED,
        WORLD_ENTITY_RELEASE_SOUND_HANDLE,
    };
    pub use crate::task::base::{
        AIRSTRIKE_ENTITY_CTOR, ARROW_ENTITY_CTOR, BASE_ENTITY_CONSTRUCTOR, BASE_ENTITY_VT0_INIT,
        BASE_ENTITY_VT1_FREE, BASE_ENTITY_VT2_HANDLE_MESSAGE, BASE_ENTITY_VT3, BASE_ENTITY_VT5,
        BASE_ENTITY_VT6, BASE_ENTITY_VT7_PROCESS_FRAME, BASE_ENTITY_VTABLE, CANISTER_ENTITY_CTOR,
        CPU_ENTITY_CTOR, CPU_ENTITY_VTABLE, CROSS_ENTITY_CTOR, DIRT_ENTITY_CTOR,
        DIRT_ENTITY_VTABLE, FIREBALL_ENTITY_CTOR, FLAME_ENTITY_CTOR, GAS_ENTITY_CTOR,
        LAND_ENTITY_CTOR, LAND_ENTITY_VTABLE, OLDWORM_ENTITY_CTOR, SCOREBUBBLE_ENTITY_CTOR,
        SEA_BUBBLE_ENTITY_VTABLE, SEABUBBLE_ENTITY_CTOR, SMOKE_ENTITY_CTOR,
        SPRITE_ANIM_ENTITY_CTOR, SPRITE_ANIM_ENTITY_VTABLE,
    };
    pub use crate::task::cloud::{
        CLOUD_ENTITY_CTOR, CLOUD_ENTITY_READ_REPLAY_STATE, CLOUD_ENTITY_VTABLE,
        CLOUD_ENTITY_WRITE_REPLAY_STATE,
    };
    pub use crate::task::filter::{
        FILTER_ENTITY_CTOR, FILTER_ENTITY_SUBSCRIBE, FILTER_ENTITY_VTABLE,
        TEAM_ENTITY_CREATE_WEATHER_FILTER,
    };
    pub use crate::task::fire::{FIRE_ENTITY_CTOR, FIRE_ENTITY_VTABLE};
    pub use crate::task::game_task::{
        CGAMETASK_CONSTRUCTOR, CGAMETASK_SOUND_EMITTER_VT, CGAMETASK_VT0, CGAMETASK_VT1_FREE,
        CGAMETASK_VT2_HANDLE_MESSAGE, CGAMETASK_VTABLE, CHECK_MOVE_COLLISION, TRY_MOVE_POSITION,
    };
    pub use crate::task::mine_oil_drum::{
        MINE_ENTITY_CTOR, MINE_ENTITY_VTABLE, OILDRUM_ENTITY_CTOR, OILDRUM_ENTITY_VTABLE,
    };
    pub use crate::task::missile::{MISSILE_ENTITY_CTOR, MISSILE_ENTITY_VTABLE};
    pub use crate::task::supply_crate::{CRATE_ENTITY_CTOR, CRATE_ENTITY_VTABLE};
    pub use crate::task::team::{TEAM_ENTITY_CTOR, TEAM_ENTITY_VTABLE};
    pub use crate::task::world_root::{
        WORLD_ROOT_AUTO_SELECT_TEAMS, WORLD_ROOT_ENTITY_CTOR, WORLD_ROOT_ENTITY_VTABLE,
        WORLD_ROOT_HANDLE_MESSAGE, WORLD_ROOT_HURRY_HANDLER,
    };
    pub use crate::task::worm::{WORM_ENTITY_CONSTRUCTOR, WORM_ENTITY_VTABLE};

    // Re-exported from audio modules
    pub use crate::audio::dssound::DS_SOUND_VTABLE;
    pub use crate::audio::music::{
        MUSIC_CONSTRUCTOR, MUSIC_DESTRUCTOR, MUSIC_PLAY_TRACK, MUSIC_VOLUME_DB_TABLE, MUSIC_VTABLE,
        STREAMING_AUDIO_FILL_AND_START, STREAMING_AUDIO_INIT, STREAMING_AUDIO_INIT_PLAYBACK,
        STREAMING_AUDIO_OPEN, STREAMING_AUDIO_OPEN_WAV, STREAMING_AUDIO_READ_CHUNK,
        STREAMING_AUDIO_RESET, STREAMING_AUDIO_STOP, STREAMING_AUDIO_TIMER_CALLBACK,
    };
    pub use crate::bitgrid::{
        BIT_GRID_BASE_VTABLE, BIT_GRID_COLLISION_VTABLE, BIT_GRID_DISPLAY_VTABLE, BIT_GRID_INIT,
        BLIT_SPRITE_RECT, BitGridBaseVtable, BitGridCollisionVtable, BitGridDisplayVtable,
        DISPLAY_BIT_GRID_SET_EXTERNAL_BUFFER, DRAW_LINE_CLIPPED, DRAW_LINE_TWO_COLOR,
    };
    pub use crate::engine::game_session::{GAME_SESSION_VTABLE, GameSessionVtable};
    pub use crate::frontend::map_view::MAP_VIEW_VTABLE;
    pub use crate::input::controller::INPUT_CTRL_VTABLE;
    pub use crate::input::mouse::MOUSE_INPUT_VTABLE;
    pub use crate::render::ddraw::compat_renderer::{
        COMPAT_RENDERER_VTABLE, CompatRendererVtable, DDRAW8_RENDERER_VTABLE,
    };
    pub use crate::render::display::base::DISPLAY_BASE_VTABLE;
    pub use crate::render::display::context::{RENDER_CONTEXT_VTABLE, RenderContextVtable};
    pub use crate::render::display::frame_hook::{
        FRAME_POST_PROCESS_HOOK_DESTRUCTOR, FRAME_POST_PROCESS_HOOK_VTABLE,
        FramePostProcessHookVtable, SCREENSHOT_HOOK_CAPTURE_TO_PNG,
        SCREENSHOT_HOOK_GET_CAPTURE_REQUEST, SCREENSHOT_HOOK_VTABLE,
    };
    pub use crate::render::display::gfx::{
        DISPLAY_BASE_DESTRUCTOR_IMPL, DISPLAY_GFX_DESTRUCTOR, DISPLAY_GFX_DESTRUCTOR_IMPL,
        DISPLAY_GFX_DISPATCH_FRAME_POST_PROCESS_HOOKS, DISPLAY_GFX_FREE_LAYER_SPRITE_TABLE,
        TILE_BITMAP_SET_DESTRUCTOR,
    };
    pub use crate::render::display::vtable::DISPLAY_GFX_VTABLE;
    pub use crate::task::game_task::SOUND_EMITTER_VTABLE;

    // Sprite, SpriteBank, PaletteContext — defined alongside their structs
    pub use crate::render::palette::{
        PALETTE_CONTEXT_INIT, PALETTE_CONTEXT_INIT_RANGE, PALETTE_CONTEXT_MAP_COLOR,
    };
    pub use crate::render::sprite::frame_cache::FRAME_CACHE_ALLOCATE;
    pub use crate::render::sprite::{
        CBITMAP_VTABLE_MAYBE, CONSTRUCT_SPRITE, DESTROY_SPRITE, FREE_SPRITE_OBJECT,
        LOAD_SPRITE_BY_NAME, LOAD_SPRITE_FROM_VFS, PROCESS_SPRITE, SPRITE_BANK_GET_FRAME_FOR_BLIT,
        SPRITE_BANK_GET_INFO, SPRITE_BANK_INIT, SPRITE_GET_FRAME_FOR_BLIT, SPRITE_GET_INFO,
        SPRITE_VTABLE,
    };

    crate::define_addresses! {
        class "GameRuntime" {
            /// GameRuntime vtable
            vtable GAME_RUNTIME_VTABLE = 0x0066A30C;
            /// GameRuntime constructor
            ctor/Stdcall CONSTRUCT_GAME_RUNTIME = 0x0056DEF0;
            /// GameRuntime::InitReplay — usercall(EAX=game_info, ESI=this), plain RET
            fn/Usercall GAME_RUNTIME_INIT_REPLAY = 0x0056F860;
            /// GameRuntime__LoadingProgressTick
            fn/Stdcall GAME_RUNTIME_LOADING_PROGRESS_TICK = 0x005717A0;
            /// GameRuntime__LoadSpeechWAV
            fn/Usercall GAME_RUNTIME_LOAD_SPEECH_WAV = 0x00571530;
            /// GameRuntime__DispatchFrame — main frame timing/simulation dispatch (stdcall, 5 params)
            fn/Stdcall GAME_RUNTIME_DISPATCH_FRAME = 0x00529160;
            /// GameRuntime__StepFrame — core single-frame step (usercall EAX=this, 5 stack params, RET 0x14)
            fn/Usercall GAME_RUNTIME_STEP_FRAME = 0x00529F30;
            /// GameRuntime__ShouldContinueFrameLoop — check elapsed time for frame catch-up (usercall EAX=this, 2 stack params, plain RET)
            fn/Usercall GAME_RUNTIME_SHOULD_CONTINUE = 0x0052A840;
            /// GameRuntime__ResetFrameState (usercall EAX=this, no stack params, plain RET)
            fn/Usercall GAME_RUNTIME_RESET_FRAME_STATE = 0x0052A910;
            /// GameRuntime__BroadcastFrameTiming — usercall ESI=this + EDI=fps_scaled,
            /// 4 stdcall stack params (elapsed_lo, elapsed_hi, freq_lo, freq_hi), RET 0x10.
            /// Ported to Rust as `engine::dispatch_frame::broadcast_frame_timing`;
            /// address kept for cross-reference only.
            fn/Usercall GAME_RUNTIME_BROADCAST_FRAME_TIMING = 0x0052A9C0;
            /// GameRuntime__CalcTimingRatio (usercall EAX=this, 1 stack param, RET 0x4)
            fn/Usercall GAME_RUNTIME_CALC_TIMING_RATIO = 0x0052ABF0;
            /// GameRuntime__InitFrameDelay (usercall EAX=this, no stack params, plain RET)
            fn/Usercall GAME_RUNTIME_INIT_FRAME_DELAY = 0x0052CAF0;
            /// `Hud__DrawTeamLabels_Maybe` (0x005332B0). Usercall EAX=this,
            /// no stack params, plain RET. Big HUD-label drawing routine
            /// (DrawText into `display_gfx_c`); called from
            /// `frame_tail_update` once every 150 frames or when latched.
            fn/Usercall HUD_DRAW_TEAM_LABELS_MAYBE = 0x005332B0;
            /// `TeamIndexMap__RemoveHandle_Maybe` (0x00526000). Usercall
            /// EAX=`*mut TeamIndexMap`, EDI=`*mut i32` (handle ptr), plain
            /// RET. Ported to Rust as `TeamIndexMap::remove_handle`; address
            /// kept for cross-reference and `install_trap!` of the WA-side
            /// callers that still reach the original (TeamEntity ctor /
            /// HandleMessage / WorldRootEntity destructor — 17 call sites).
            fn/Usercall TEAM_INDEX_MAP_REMOVE_HANDLE = 0x00526000;
            /// Helper called from the online `ShouldInterpolate` path
            /// (FUN_0052E880). Scans the per-peer input-message queue for any
            /// "gameplay-relevant" message type. Usercall EAX=this +
            /// 1 stdcall stack param (peer_idx), RET 0x4. Still bridged;
            /// its own callee (`FUN_0053e300` input-queue-pop helper) is
            /// also online-only and would require additional bridging.
            fn/Usercall GAME_RUNTIME_PEER_INPUT_QUEUE_SCAN = 0x0052E880;
            /// Tail callee of `ShouldInterpolate_OfflineCheck` (FUN_0052F9C0).
            /// Stdcall(runtime), RET 0x4. Large (~205 instructions, 51 basic
            /// blocks); still bridged as a plain stdcall call from the
            /// offline-branch Rust port.
            fn/Stdcall GAME_RUNTIME_SHOULD_INTERPOLATE_OFFLINE_TAIL = 0x0052F9C0;
            /// GameRuntime__SetupFrameParams (usercall EAX=this, 3 stack
            /// params, RET 0xC). Ported to Rust as
            /// `engine::dispatch_frame::setup_frame_params`; address kept
            /// for cross-reference and the WA-side hook install.
            fn/Usercall GAME_RUNTIME_SETUP_FRAME_PARAMS = 0x00534CA0;
            /// `setup_frame_params` callees — helpers driving the ESC-menu
            /// state machine (`runtime.esc_menu_state`).
            ///
            /// `GameRuntime::IsHudActive` (usercall ESI=this, returns u8):
            /// "should the ESC menu stay open / be allowed to open?"
            /// predicate. Runs the slot-3 `HudDataQuery` (msg 0x7D3) and
            /// inspects end-of-round flags. Ported as
            /// `engine::main_loop::dispatch_frame::is_hud_active`; address
            /// kept for the WA-side hook (still called by `OpenEscMenu`
            /// = FUN_00535200).
            fn/Usercall GAME_RUNTIME_IS_HUD_ACTIVE = 0x00534C30;
            /// `GameRuntime::EscMenu_TickClosed` (usercall EAX=this):
            /// per-frame tick while the ESC menu is closed
            /// (`esc_menu_state == 0`); polls for ESC keypress to open
            /// the menu. Ported as
            /// `engine::main_loop::dispatch_frame::esc_menu_tick_closed`;
            /// address kept for `install_trap!` (no WA-side caller now).
            fn/Usercall GAME_RUNTIME_ESC_MENU_TICK_CLOSED = 0x005351B0;
            /// `GameRuntime::OpenEscMenu` (stdcall(this), RET 0x4):
            /// builds the in-game ESC menu (header scoreboard,
            /// leaderboard rows, action buttons + volume slider) into
            /// `runtime.menu_panel_a`, then sets `esc_menu_state = 1`.
            /// 628 instructions / cyclo 69 / 30 calls. Still bridged.
            fn/Stdcall GAME_RUNTIME_OPEN_ESC_MENU = 0x00535200;
            /// `MenuPanel::AppendItem` (usercall(EAX=x, ESI=panel),
            /// 6 stack params, RET 0x18). Appends one item (button or
            /// slider) to a `MenuPanel`'s items array; called 5× from
            /// `OpenEscMenu` and once from `FUN_00535cf0`. Ported as
            /// `engine::menu_panel::append_item_impl`.
            fn/Usercall MENU_PANEL_APPEND_ITEM = 0x005408F0;
            /// `GameRuntime::EscMenu_TickState1` (usercall EDI=this):
            /// per-frame tick while the menu is open; handles arrow nav
            /// + Enter to activate items.
            fn/Usercall GAME_RUNTIME_ESC_MENU_STATE_1_TICK = 0x00535B10;
            /// `GameRuntime::EscMenu_TickState2` (usercall EDI=this):
            /// per-frame tick for `esc_menu_state == 2` (confirm /
            /// network-end-of-game flow; calls `BeginNetworkGameEnd`).
            fn/Usercall GAME_RUNTIME_ESC_MENU_STATE_2_TICK = 0x00535FC0;
            /// `GameRuntime::OpenEscMenuConfirmDialog` (stdcall(this),
            /// RET 0x4). Builds the Yes/No confirm overlay into
            /// `runtime.menu_panel_b` and sets `esc_menu_state = 2`.
            /// Ported as `esc_menu::open_confirm_dialog`.
            fn/Stdcall GAME_RUNTIME_OPEN_ESC_MENU_CONFIRM_DIALOG = 0x00535CF0;
            /// `MenuPanel::CenterCursorOnFirstKindZero` (usercall(ESI=panel),
            /// plain RET). Walks `panel.items` and parks the cursor at
            /// the center of any item whose `kind == 0` (last one wins).
            /// Ported as `engine::menu_panel::center_cursor_on_first_kind_zero`.
            fn/Usercall MENU_PANEL_CENTER_CURSOR_ON_KIND_ZERO = 0x00540780;
            /// `TeamIndexMap__PopHandle_Maybe` (0x00525F50). Thiscall
            /// `ECX = *mut TeamIndexMap, [ESP+4] = key: i32`, RET 0x4.
            /// Companion to `RemoveHandle`; ported to Rust as
            /// `TeamIndexMap::pop_handle`.
            fn/Thiscall TEAM_INDEX_MAP_POP_HANDLE = 0x00525F50;
            /// GameRuntime__ProcessNetworkFrame (usercall EAX=this, 4 stack params, RET 0x10)
            fn/Usercall GAME_RUNTIME_PROCESS_NETWORK_FRAME = 0x0053DF00;
            /// GameRuntime__IsReplayMode (usercall EAX=this, no stack params, plain RET)
            fn/Usercall GAME_RUNTIME_IS_REPLAY_MODE = 0x00537060;
            /// GameRuntime__PollInput — stdcall(runtime), plain RET. Polls keyboard/input each step.
            fn/Stdcall GAME_RUNTIME_POLL_INPUT = 0x00534910;
            // --- StepFrame sub-calls: end-game state-machine handlers ---
            /// GameRuntime__BeginNetworkGameEnd — network-mode entry (Block A
            /// non-zero `network_ecx` path). Transitions `game_state` to 3.
            /// Usercall(EAX=wrapper), no stack args, plain RET.
            fn/Usercall GAME_RUNTIME_BEGIN_NETWORK_GAME_END = 0x00536270;
            /// GameRuntime__BeginRoundEnd — local round-end entry (Draw / offline
            /// Quit confirm in `EscMenu_TickState2`). Transitions `game_state`
            /// to 4 (ROUND_ENDING), zeroes the game-end fade fields, optionally
            /// clears `frame_delay_counter` to -1, and broadcasts msg 0x75 via
            /// `world_root.handle_message` if `world.game_info[+0xD778] > 0x4C`.
            /// Usercall(EAX=this, [ESP+4]=skip_frame_delay), RET 0x4.
            fn/Usercall GAME_RUNTIME_BEGIN_ROUND_END = 0x00536550;
            /// GameRuntime__OnGameState2. Usercall(EDI=ESI=wrapper), plain RET.
            fn/Usercall GAME_RUNTIME_ON_GAME_STATE_2 = 0x00536470;
            /// GameRuntime__OnGameState3. Usercall(EDI=ESI=wrapper), plain RET.
            fn/Usercall GAME_RUNTIME_ON_GAME_STATE_3 = 0x00536320;
            /// GameRuntime__OnGameState4. Usercall(ESI=wrapper), plain RET.
            /// Increments `game_end_speed` by 0x51E per call; transitions to
            /// `game_state = 5` (EXIT) once the high word reaches 1 (~50 frames).
            fn/Usercall GAME_RUNTIME_ON_GAME_STATE_4 = 0x005365A0;
            /// GameRuntime__ClearWormBuffers — stdcall(world_root, i32), RET 0x8.
            fn/Stdcall GAME_RUNTIME_CLEAR_WORM_BUFFERS = 0x0055C300;
            /// GameRuntime__AdvanceWormFrame — stdcall(world_root), RET 0x4.
            fn/Stdcall GAME_RUNTIME_ADVANCE_WORM_FRAME = 0x0055C590;
            /// BufferObject__ClassifyInputMsg — thiscall(ECX=render_buffer_a).
            /// Returns packed u64 (EDX:EAX): EAX=keep-going flag, EDX=msg subtype.
            fn/Thiscall BUFFER_OBJECT_CLASSIFY_INPUT_MSG = 0x00541100;
            /// GameRuntime__DispatchInputMsg — usercall(EAX=local_buf) +
            /// stdcall(wrapper, msg_type, payload_size), RET 0xC.
            fn/Stdcall GAME_RUNTIME_DISPATCH_INPUT_MSG = 0x00530F80;
        }

        class "GameWorld" {
            /// GameWorld constructor
            ctor/Stdcall CONSTRUCT_GAME_WORLD = 0x0056E220;
            /// GameWorld::InitGameState — stdcall(this=GameRuntime*), RET 0x4
            fn/Stdcall GAME_WORLD_INIT_GAME_STATE = 0x00526500;
            /// GameWorld__InitFields
            fn GAME_WORLD_INIT_FIELDS = 0x00526120;
            /// GameWorld__InitRenderIndices — usercall(ESI=world), plain RET
            fn/Usercall GAME_WORLD_INIT_RENDER_INDICES = 0x00526080;
            /// GameWorld__InitVersionFlags — stdcall(runtime)
            fn/Stdcall GAME_WORLD_INIT_VERSION_FLAGS = 0x00525BE0;
            /// GameWorld__LoadFonts — loads .fnt font resources into the display.
            fn/Usercall GAME_WORLD_LOAD_FONTS = 0x00570F30;
            /// GameRuntime__LoadFontExtension — loads .fex font extension for a font slot.
            fn/Stdcall GAME_RUNTIME_LOAD_FONT_EXTENSION = 0x00570E80;
            /// GameWorld__LoadHudAndWeaponSprites
            fn/Thiscall GAME_WORLD_LOAD_HUD_AND_WEAPON_SPRITES = 0x0053D0E0;
            /// GameWorld__InitPaletteGradientSprites
            fn/Stdcall GAME_WORLD_INIT_PALETTE_GRADIENT_SPRITES = 0x005706D0;
            /// GameWorld__InitFeatureFlags
            fn/Stdcall GAME_WORLD_INIT_FEATURE_FLAGS = 0x00524700;
            /// GameWorld__InitDisplayFinal_Maybe
            fn GAME_WORLD_INIT_DISPLAY_FINAL = 0x0056A830;
            /// GameWorld__IsSuperWeapon
            fn/Usercall IS_SUPER_WEAPON = 0x00565960;
            /// GameWorld__CheckWeaponAvail
            fn/Fastcall CHECK_WEAPON_AVAIL = 0x0053FFC0;
        }

        class "Landscape" {
            /// Landscape vtable
            vtable LANDSCAPE_VTABLE = 0x0066B208;
            /// Landscape constructor (0xB44-byte object)
            ctor/Stdcall LANDSCAPE_CONSTRUCTOR = 0x0057ACB0;
            /// Applies explosion crater to terrain (vtable slot 2)
            fn LANDSCAPE_APPLY_EXPLOSION = 0x0057C820;
            /// Draws 8px checkered borders at landscape edges (vtable slot 6)
            fn LANDSCAPE_DRAW_BORDERS = 0x0057D7F0;
            /// Redraws a single terrain row (vtable slot 8)
            fn LANDSCAPE_REDRAW_ROW = 0x0057CF60;
            /// Clips and merges dirty rectangles for terrain redraw
            fn LANDSCAPE_CLIP_AND_MERGE = 0x0057D2B0;
        }

        class "LandscapeShader" {
            /// LandscapeShader vtable
            vtable LANDSCAPE_SHADER_VTABLE = 0x0066B1DC;
        }

        class "DSSound" {
            // Vtable now defined via #[vtable(...)] in audio/dssound.rs
            /// DSSound constructor — usercall(EAX=this), plain RET
            ctor/Usercall CONSTRUCT_DS_SOUND = 0x00573D50;
            /// DSSound init buffers — usercall(EAX=dssound), plain RET
            fn/Usercall DSSOUND_INIT_BUFFERS = 0x00573E50;
            /// Loads all SFX WAVs
            fn/Stdcall DSSOUND_LOAD_EFFECT_WAVS = 0x005714B0;
            /// Loads all speech banks
            fn/Usercall DSSOUND_LOAD_ALL_SPEECH_BANKS = 0x00571A70;
            /// Loads one speech bank
            fn/Usercall DSSOUND_LOAD_SPEECH_BANK = 0x00571660;
        }

        class "Keyboard" {
            /// Keyboard vtable (0x33C-byte keyboard object)
            vtable KEYBOARD_VTABLE = 0x0066AEC8;
            /// Keyboard::PollState
            fn/Stdcall KEYBOARD_POLL_STATE = 0x00572290;
            /// Keyboard::AcquireInput — usercall(ESI=flag, [ESP+4]=p1)
            fn/Usercall KEYBOARD_ACQUIRE_INPUT = 0x00572500;
            /// Keyboard::CheckAction — usercall(EAX=action, ESI=this, EDI=mode) -> u32
            fn/Usercall KEYBOARD_CHECK_ACTION = 0x00571BA0;
            /// Keyboard::CheckKeyState — usercall(EAX=key, EDX=mode, [ESP+4]=this) -> bool
            fn/Usercall KEYBOARD_CHECK_KEY_STATE = 0x00571B50;
            /// Keyboard::ClearKeyStates — usercall(EAX=this) -> void, plain RET
            fn/Usercall KEYBOARD_CLEAR_KEY_STATES = 0x005722F0;
        }

        // AcquireInput's bridged callees (kept WA-side for this round; no
        // dedicated class blocks yet — these are scattered single-callees).
        // (FRONTEND_DIALOG_UPDATE_CURSOR is already declared in the
        // GameSession bridge block below.)
        /// Cursor::ClipAndRecenter — clip cursor to monitor + SetCursorPos
        /// to GameSession.screen_center. Plain RET, no args.
        /// Now Rust — kept for the trap install in `replacements/mouse.rs`.
        fn/Cdecl CURSOR_CLIP_AND_RECENTER = 0x00573180;
        /// Mouse::PollAndAcquire — full re-grab of mouse + keyboard input
        /// after focus regain. Plain RET, no args.
        fn/Cdecl MOUSE_POLL_AND_ACQUIRE = 0x00572620;
        /// Mouse::ReleaseAndCenter — Alt+G ungrab handler. Plain RET, no args.
        fn/Cdecl MOUSE_RELEASE_AND_CENTER = 0x005725B0;
        /// Display__RestoreSurfaces_Maybe — stdcall(display_ptr) -> u32,
        /// RET 0x4. Recreates lost DDraw surfaces after focus regain.
        fn/Stdcall DISPLAY_RESTORE_SURFACES = 0x0056CA80;

        // MouseInput vtable (formerly misnamed "Palette") is now defined via
        // #[vtable(...)] in input/mouse.rs.

        class "DisplayBase" {
            // Primary vtable now defined via #[vtable(...)] in display/base.rs
            /// DisplayBase headless vtable
            vtable DISPLAY_BASE_HEADLESS_VTABLE = 0x0066A0F8;
            /// DisplayBase constructor (0x3560-byte object)
            ctor/Stdcall DISPLAY_BASE_CTOR = 0x00522DB0;
        }

        class "InputCtrl" {
            // Vtable now defined via #[vtable(...)] in input/controller.rs
            /// Input controller initializer
            fn/Usercall INPUT_CTRL_INIT = 0x0058C0D0;
        }

        // BitGrid vtables and init are now in display::bitgrid via define_addresses! + #[vtable].

        class "OpenGLCPU" {
            /// OpenGLCPU vtable (0x48-byte object)
            vtable OPENGL_CPU_VTABLE = 0x006774C0;
            /// OpenGLCPU constructor
            ctor CONSTRUCT_OPENGL_CPU = 0x005A0850;
        }

        class "WaterEffect" {
            /// WaterEffect vtable (0xBC-byte object)
            vtable WATER_EFFECT_VTABLE = 0x0066B268;
        }

        class "GfxHandler" {
            /// GfxHandler vtable
            vtable GFX_DIR_VTABLE = 0x0066B280;
            /// GfxHandler load sprites
            fn GFX_DIR_LOAD_SPRITES = 0x00570B50;
            /// GfxDir load directory
            fn GFX_DIR_LOAD_DIR = 0x005663E0;
            /// GfxDir find entry
            fn GFX_DIR_FIND_ENTRY = 0x00566520;
            /// GfxDir load image
            fn GFX_DIR_LOAD_IMAGE = 0x005666D0;
        }

        class "GfxDirStream" {
            /// GfxDirStream vtable (6 slots)
            vtable GFX_DIR_STREAM_VTABLE = 0x0066A1C0;
        }

        class "DisplayBase" {
            /// DisplayBase__AllocPaletteSlots — usercall EAX=count, 1 stack(this)
            fn/Usercall DISPLAY_BASE_ALLOC_PALETTE_SLOTS = 0x00523190;
        }

        class "DisplayGfx" {
            /// DisplayGfx constructor
            ctor/Stdcall DISPLAYGFX_CTOR = 0x00569C10;
            /// IMG__DecodeCached: decode cached raw image buffer into DisplayBitGrid
            fn/Stdcall IMG_DECODE_CACHED = 0x004F5E80;
            /// DisplayGfx construct full (5 params)
            fn/Stdcall DISPLAYGFX_CONSTRUCT_FULL = 0x00563FC0;
            /// DisplayGfx init team palette display objects
            fn/Stdcall DISPLAY_GFX_INIT_TEAM_PALETTE_DISPLAY = 0x005703E0;
            /// DisplayGfx__LoadSpriteEx (vtable slot 30) — thiscall
            fn/Thiscall DISPLAY_GFX_LOAD_SPRITE_EX = 0x00523310;
            /// `DisplayGfx__DrawTiledBitmap` (vtable slot 11) — thiscall.
            /// Tile-cached bitmap draw: lazily allocates 0x400-row tile
            /// surfaces from a sprite source descriptor, populates them, and
            /// blits the visible tiles to the display. Reachable today via
            /// `RenderDrawingQueue` case 0xD; needs porting if/when slot is
            /// replaced.
            fn/Thiscall DISPLAY_GFX_DRAW_TILED_BITMAP = 0x0056B8C0;
        }

        class "Font" {
            /// BitmapFont::DrawText — usercall(EAX=BitGrid* dst, EDX=out_width, ESI=FontObject*) +
            /// 5 stack(pen_x, pen_y, msg, out_pen_x, font_id_high), RET 0x14.
            /// Glyph rasterizer for in-game .fnt fonts (NOT the frontend MFC font system).
            /// Ported as `display::vtable::font_draw_text_impl`; address kept for registry.
            fn/Usercall FONT_OBJ_DRAW_TEXT = 0x004FA4E0;
            /// Font object: set param — usercall(ECX=p4, EDX=font_obj) + 2 stack(p3, p5), RET 0x8.
            /// Ported as `display::vtable::font_set_param_impl`; address kept for registry.
            fn/Usercall FONT_OBJ_SET_PARAM = 0x004FA720;
            /// Font object: get metric — usercall(AL=char, EDX=out1, EDI=out2) + 1 stack(font_obj), RET 0x4.
            /// Ported as `display::vtable::font_get_metric_impl`; address kept for registry.
            fn/Usercall FONT_OBJ_GET_METRIC = 0x004FA780;
            /// Font object: get info — usercall(EAX=font_obj, EDX=out2, EDI=out1), plain RET.
            /// Ported as `display::vtable::font_get_info_impl`; address kept for registry.
            fn/Usercall FONT_OBJ_GET_INFO = 0x004FA7D0;
            /// Font object: "set palette" — usercall(ESI=font_obj) + 1 stack(palette_value), RET 0x4.
            /// Misnamed in the original — actually extends `digiwht.fnt` with
            /// derived `'.'` and `';'` glyphs at runtime. Ported as
            /// `display::vtable::font_set_palette_impl`; address kept for registry.
            fn/Usercall FONT_OBJ_SET_PALETTE = 0x004F9F20;
        }

        /// "sprite" type-tag string in .rdata — returned by Sprite/SpriteBank GetInfo
        global STR_SPRITE = 0x00664170;

    }

    // Backward-compat aliases (not registered separately — same address)
    /// BaseEntity::vtable4 (same implementation as vt3 in base)
    pub const BASE_ENTITY_VT4: u32 = BASE_ENTITY_VT3;
    /// Alias for backward compatibility with validation code.
    pub const CGAMETASK_VTABLE2: u32 = CGAMETASK_SOUND_EMITTER_VT;
    /// Alias for callers using the old name.
    pub const CONSTRUCT_DISPLAY_GFX: u32 = DISPLAY_GFX_INIT;
    /// Duplicate: same as LANDSCAPE_CONSTRUCTOR.
    pub const CONSTRUCT_LANDSCAPE: u32 = LANDSCAPE_CONSTRUCTOR;
    /// Duplicate: same as SPRITE_REGION_CONSTRUCTOR.
    pub const CONSTRUCT_SPRITE_REGION: u32 = SPRITE_REGION_CONSTRUCTOR;
    /// Duplicate: same as WORLD_ROOT_ENTITY_CTOR.
    pub const WORLD_ROOT_CONSTRUCTOR: u32 = WORLD_ROOT_ENTITY_CTOR;

    // =========================================================================
    // Replay / turn management
    // =========================================================================

    crate::define_addresses! {
        /// Loads .WAgame replay file, validates magic 0x4157
        fn/Stdcall REPLAY_LOADER = 0x00462DF0;
        /// Parses "MM:SS.FF" time string → frame number
        fn PARSE_REPLAY_POSITION = 0x004E3490;
        /// Read length-prefixed string
        fn/Usercall REPLAY_READ_PREFIXED_STRING = 0x00461340;
        /// Read byte with range validation
        fn/Usercall REPLAY_READ_BYTE_VALIDATED = 0x004614D0;
        /// Read byte with signed range validation
        fn/Usercall REPLAY_READ_BYTE_RANGE = 0x00461540;
        /// Read u16 with range validation
        fn/Usercall REPLAY_READ_U16_VALIDATED = 0x004615B0;
        /// Read worm name
        fn/Usercall REPLAY_READ_WORM_NAME = 0x00461620;
        /// Validate team type byte range
        fn/Fastcall REPLAY_VALIDATE_TEAM_TYPE = 0x00461690;
        /// Post-process team color assignments
        fn/Stdcall REPLAY_PROCESS_TEAM_COLORS = 0x00466460;
        /// Apply scheme default values
        fn REPLAY_PROCESS_SCHEME_DEFAULTS = 0x004670F0;
        /// Process replay feature flags
        fn REPLAY_PROCESS_FLAGS = 0x00467280;
        /// Register observer team entry
        fn/Stdcall REPLAY_REGISTER_OBSERVER = 0x00467BC0;
        /// Process alliance/team setup
        fn REPLAY_PROCESS_ALLIANCE = 0x00468890;
        /// Validate team configuration
        fn/Stdcall REPLAY_VALIDATE_TEAM_SETUP = 0x00465E10;
        /// Routes game messages through the task handler tree
        fn GAME_MESSAGE_ROUTER = 0x00553BD0;
        /// Per-frame turn timer
        fn/Stdcall TURN_MANAGER_PROCESS_FRAME = 0x0055FDA0;
        /// Control task HandleMessage
        fn CONTROL_TASK_HANDLE_MESSAGE = 0x005451F0;
        /// End-of-frame message queue / hurry processing
        fn GAME_FRAME_MESSAGE_PROCESSOR = 0x00531960;
        /// End-of-frame checksum computation (__thiscall, ECX=ctrl, stack=wrapper*)
        fn/Thiscall GAME_FRAME_CHECKSUM_PROCESSOR = 0x005329C0;
        /// Game state serialization for checksum (called by checksum processor)
        fn SERIALIZE_GAME_STATE = 0x00532330;
        /// Game state checksum: ROL-3-ADD hash (__fastcall)
        fn/Fastcall COMPUTE_STATE_CHECKSUM = 0x00546140;
        /// Multi-segment checksum variant
        fn COMPUTE_STATE_CHECKSUM_MULTI = 0x00546170;
        /// Main frame loop
        fn GAME_FRAME_DISPATCHER = 0x00531D00;
        /// Sends game packet if network buffer allows
        fn SEND_GAME_PACKET_CONDITIONAL = 0x00531880;
        /// Process replay state — large function (1032 lines)
        fn/Stdcall REPLAY_PROCESS_STATE = 0x0045D640;
        /// Cleanup observer array
        fn/Usercall REPLAY_CLEANUP_OBSERVERS = 0x0053EE00;
    }

    // =========================================================================
    // Gameplay functions
    // =========================================================================

    crate::define_addresses! {
        /// Game PRNG: rng = (rng + frame_counter) * 0x19660D + 0x3C6EF35F
        fn/Fastcall ADVANCE_GAME_RNG = 0x0053F320;
        /// Terrain hit → debris particles → RNG
        fn GENERATE_DEBRIS_PARTICLES = 0x00546F70;
        fn CREATE_EXPLOSION = 0x00548080;
        fn SPECIAL_IMPACT = 0x005193D0;
        fn SPAWN_OBJECT = 0x00561CF0;
        fn WEAPON_RELEASE = 0x0051C3D0;
        fn WORM_START_FIRING = 0x0051B7F0;
        fn FIRE_WEAPON = 0x0051EE60;
        fn CREATE_WEAPON_PROJECTILE = 0x0051E0F0;
        /// stdcall(worm, fire_params, local_struct), RET 0xC
        fn PROJECTILE_FIRE = 0x0051DFB0;
        /// Strike weapons (AirStrike, NapalmStrike, MineStrike, MoleSquadron, MailStrike).
        /// stdcall(worm, &subtype_34, local_struct), RET 0xC.
        /// Spawns AirStrikeEntity or similar. NOT for grenades — grenades use CWP.
        fn STRIKE_FIRE = 0x0051E2C0;
        /// usercall(ECX=local_struct, EDX=worm, [ESP+4]=fire_params), RET 0x4
        fn PLACED_EXPLOSIVE = 0x0051EC80;
        /// Spawns ArrowEntity (Shotgun, Longbow). Allocates 0x168 bytes.
        /// thiscall(ECX=worm, fire_params, local_struct), RET 0x8.
        fn CREATE_ARROW = 0x0051ED90;
        /// stdcall(worm, fire_params, local_struct), RET 0xC
        fn ROPE_TYPE1_FIRE = 0x0051E1C0;
        /// stdcall(worm, fire_params, local_struct), RET 0xC
        fn ROPE_TYPE3_FIRE = 0x0051E240;
        /// Called by ProjectileFire per shot.
        /// usercall(EDI=spawn_data, stack=[worm, fire_params]), RET 0x8.
        fn PROJECTILE_FIRE_SINGLE = 0x0051DCF0;
        /// Sin lookup table (1024 entries of Fixed16.16). cos = sin + 256 entries.
        global SIN_TABLE = 0x006A1860;
        /// VectorNormalize (simple version, used for game_version < 0x99)
        fn VECTOR_NORMALIZE_SIMPLE = 0x00575590;
        /// VectorNormalize (overflow-safe version, used for game_version >= 0x99)
        fn VECTOR_NORMALIZE_OVERFLOW = 0x005755D0;
    }

    // =========================================================================
    // Weapon system
    // =========================================================================

    crate::define_addresses! {
        /// PlayWormSound: usercall(EDI=worm) + stack(sound_handle_id, volume), RET 0x8.
        /// Stops current streaming sound at worm+0x3B0, then starts a new one.
        fn PLAY_WORM_SOUND = 0x005150D0;
        /// StopWormSound: usercall(ESI=worm), plain RET.
        /// Stops streaming sound at worm+0x3B0 and clears the handle.
        fn STOP_WORM_SOUND = 0x00515180;
        /// SpawnEffect: complex usercall, RET 0x1C.
        /// Builds a 0x408-byte struct from params, SharedData lookup, HandleMessage(0x56).
        fn SPAWN_EFFECT = 0x00547C30;
        fn INIT_WEAPON_TABLE = 0x0053CAB0;
        fn COUNT_ALIVE_WORMS = 0x005225A0;
        fn GET_AMMO = 0x005225E0;
        fn ADD_AMMO = 0x00522640;
        /// Not the main ammo decrement path
        fn SUBTRACT_AMMO = 0x00522680;
    }

    // =========================================================================
    // Team/worm accessor functions
    // =========================================================================

    crate::define_addresses! {
        /// Counts teams by alliance membership
        fn/Usercall COUNT_TEAMS_BY_ALLIANCE = 0x00522030;
        /// Sums health of all worms on a team
        fn/Fastcall GET_TEAM_TOTAL_HEALTH = 0x005224D0;
        /// Checks if a worm is in a "special" state
        fn/Usercall IS_WORM_IN_SPECIAL_STATE = 0x005226B0;
        /// Reads worm X,Y position into output pointers
        fn/Usercall GET_WORM_POSITION = 0x00522700;
        /// Checks if any worm has state 0x64
        fn/Usercall CHECK_WORM_STATE_0X64 = 0x005228D0;
        /// Per-team version of CheckWormState0x64
        fn/Usercall CHECK_TEAM_WORM_STATE_0X64 = 0x00522930;
        /// Scans all teams for any worm with state 0x8b
        fn/Usercall CHECK_ANY_WORM_STATE_0X8B = 0x00522970;
        /// Sets the active worm for a team
        fn/Usercall SET_ACTIVE_WORM_MAYBE = 0x00522500;
    }

    // =========================================================================
    // Game session
    // =========================================================================

    crate::define_addresses! {
        class "GameSession" {
            /// GameSession constructor — replaced by Rust `construct_session`,
            /// trapped (only WA-side caller is `GameSession__Run`, also replaced).
            ctor/Usercall GAME_SESSION_CONSTRUCTOR = 0x0058BFA0;
            /// GameSession__Run
            fn/Usercall GAME_SESSION_RUN = 0x00572F50;
            /// GameSession__ProcessFrame — per-frame processing (desktop check, engine tick, render)
            fn/Cdecl GAME_SESSION_PROCESS_FRAME = 0x00572C80;
            /// GameSession__AdvanceFrame — frame timing + engine vtable dispatch
            fn/Cdecl GAME_SESSION_ADVANCE_FRAME = 0x0056DDC0;
            /// GameSession__PumpMessages — pumps Win32 messages between frames.
            /// Replaced by Rust `pump_messages` (full hook — also called
            /// from `GameRuntime::LoadingProgressTick` on the WA side).
            fn/Cdecl GAME_SESSION_PUMP_MESSAGES = 0x00572E30;
            /// GameSession__OnHeadlessPreLoop_Maybe — clears keyboard/cursor
            /// state, hides frontend, flushes display, primes flag_5c=1.
            /// Called once before the main loop when `g_DisplayModeFlag != 0`.
            /// Replaced by Rust `on_headless_pre_loop` (full hook — two
            /// remaining WA-side callers in the SYSCOMMAND minimize path).
            fn/Stdcall GAME_SESSION_ON_HEADLESS_PRE_LOOP = 0x00572430;
            /// GameSession__WindowProc — engine-mode WNDPROC installed via
            /// `SetWindowLongA` over the original MFC WNDPROC (cached at
            /// `G_MFC_WNDPROC`). Handles in-game keyboard/mouse/palette
            /// messages; falls through to the cached MFC WNDPROC for
            /// anything else.
            fn/Stdcall GAME_SESSION_WINDOW_PROC = 0x00572660;
        }

        /// GameEngine__InitHardware
        fn/Thiscall GAME_ENGINE_INIT_HARDWARE = 0x0056D350;
        /// GameEngine__Shutdown
        fn/Stdcall GAME_ENGINE_SHUTDOWN = 0x0056DCD0;
        /// Helper called by `GameEngine::Shutdown` when `session.input_ctrl != 0`.
        /// `__usercall(EDI=g_GameSession)`, plain RET — reads `[EDI+0xB8]`
        /// (input_ctrl) on entry. Runs once at the top of shutdown.
        fn/Usercall SHUTDOWN_INPUT_CTRL_HELPER_MAYBE = 0x0056DC10;
        /// Helper called by `GameEngine::Shutdown` on the localized-template
        /// just before `wa_free`. `__usercall(EDI=this)`, plain RET — reads
        /// `[EDI+4]` on entry. Destructor body for `LocalizedTemplate`.
        fn/Usercall LOCALIZED_TEMPLATE_DTOR_BODY_MAYBE = 0x0053E9D0;
        /// FrontendDialog__UpdateCursor — reapplies the frontend mouse cursor.
        fn/Stdcall FRONTEND_DIALOG_UPDATE_CURSOR = 0x0040D250;
        /// Frontend__UnhookInputHooks — releases keyboard/mouse hooks if
        /// `g_InputHookMode != 0` (no-op in normal play). Called twice per
        /// `pump_messages` iteration.
        fn/Cdecl FRONTEND_UNHOOK_INPUT_HOOKS = 0x004ED590;
        /// Frontend__InstallInputHooks — installs the WH_GETMESSAGE +
        /// WH_FOREGROUNDIDLE thread-local hooks on the calling thread when
        /// entering a modal-dialog input mode. Takes no parameters; the
        /// caller writes `g_InputHookMode` immediately before the call.
        fn/Cdecl FRONTEND_INSTALL_INPUT_HOOKS = 0x004ED3C0;
        /// Frontend__ForegroundIdleProc — WH_FOREGROUNDIDLE hook proc.
        /// Posts `WM_USER+0x3FFC` to the frontend frame's HWND while
        /// `g_InputHookMode == Animated`. Now ported in Rust; the WA
        /// address is trapped as a safety net (only static xref was
        /// `Frontend::InstallInputHooks` itself).
        fn/Stdcall FRONTEND_FOREGROUND_IDLE_PROC = 0x004ED0D0;
        /// Frontend__GetMessageProc — WH_GETMESSAGE hook proc.
        /// Synthesises `mouse_event` calls to let the engine drain
        /// `WM_MOUSEWHEEL` / `WM_MBUTTON*` bursts while a modal-dialog
        /// input grab is active. Now ported in Rust; trapped as a safety
        /// net since the only static xref was `Frontend::InstallInputHooks`.
        fn/Stdcall FRONTEND_GET_MESSAGE_PROC = 0x004ED160;
        /// Frontend__PumpModalOrSessionFrame — per-callback frame helper
        /// for `Frontend::GetMessageProc`. Calls `GameSession::ProcessFrame`
        /// when a session is initialised; otherwise drains the top modal
        /// dialog's WM_TIMER queue and ticks its transition method.
        /// Now inlined into the Rust port; only static xref was the
        /// just-ported GetMessageProc, so trap as safety net.
        fn/Cdecl FRONTEND_PUMP_MODAL_OR_SESSION_FRAME = 0x004ED050;
        /// Palette__LogChange — appends a "Palette set by 0x%lX %s" line to
        /// `palette.log` with the foreground process name (Toolhelp32). Bridged
        /// from `GameSession::WindowProc` WM_PALETTECHANGED handler.
        fn/Cdecl PALETTE_LOG_CHANGE = 0x00598B50;
        /// Palette__RealizeFromSystem — diffs the system palette against the
        /// game's reference and re-realises with `_memcpy`. Bridged from
        /// `GameSession::WindowProc` WM_PALETTECHANGED + Shift+Pause handlers.
        fn/Cdecl PALETTE_REALIZE_FROM_SYSTEM = 0x005A1110;
        /// Screenshot__SavePng — writes the current backbuffer as a PNG to
        /// `User\Capture\screenNNNN.png`. Bridged from `GameSession::WindowProc`
        /// VK_PAUSE (no-modifier) handler. Returns nonzero on success.
        fn/Cdecl SCREENSHOT_SAVE_PNG = 0x0056C6F0;
        /// Map__SavePngCapture — writes the current level map as a PNG to
        /// `User\SavedLevels\Capture\mapNNNNN.png`. Bridged from
        /// `GameSession::WindowProc` Alt+Pause handler. Argument is the
        /// `GameRuntime*` (GameSession+0xA0).
        fn/Cdecl MAP_SAVE_PNG_CAPTURE = 0x00536810;
    }

    // =========================================================================
    // Graphics / rendering
    // =========================================================================

    crate::define_addresses! {
        /// DisplayGfx::Init
        fn/Usercall DISPLAY_GFX_INIT = 0x00569D00;
        /// DisplayGfx vtable slot 19 — blit sprite
        fn/Thiscall DISPLAY_GFX_BLIT_SPRITE = 0x0056B080;
        /// DisplayGfx flush render lock — releases lock, plain RET
        fn DISPLAY_GFX_FLUSH_RENDER_LOCK = 0x0056A330;
        /// Streaming audio constructor
        fn/Stdcall STREAMING_AUDIO_CTOR = 0x0058BC10;
        /// DDNetGameWrapper constructor
        fn/Stdcall DDNETGAME_WRAPPER_CTOR = 0x0056D1F0;
        /// `LocalizedTemplate__Constructor` — usercall(ESI=this, EAX=wa_version_threshold).
        /// See [`crate::wa::localized_template::LocalizedTemplate`].
        fn/Usercall LOCALIZED_TEMPLATE_CTOR = 0x0053E950;
        /// `LocalizedTemplate__Resolve` — stdcall(template, token) -> *const c_char.
        /// Resolves a localized template string from the gfx-dir's string
        /// table, applies WA's escape-code post-processor, and caches the
        /// result on the [`LocalizedTemplate`]. RET 0x8.
        ///
        /// [`LocalizedTemplate`]: crate::wa::localized_template::LocalizedTemplate
        fn/Stdcall LOCALIZED_TEMPLATE_RESOLVE = 0x0053EA30;
        /// `sprintf` into one of 8 rotating 16-KiB scratch buffers. cdecl,
        /// varargs (caller cleans). Returns a pointer to the formatted string
        /// in the next rotating buffer slot.
        fn SPRINTF_ROTATING_BUFFER = 0x005978A0;
        fn CONSTRUCT_FRAME_BUFFER = 0x005A2430;
        fn BLIT_SCREEN = 0x005A2020;
        fn RQ_RENDER_DRAWING_QUEUE = 0x00542350;
        fn DRAW_LANDSCAPE = 0x005A2790;
        /// `RQ_EnqueueTiledBitmap` — formerly mis-labelled `RQ_DrawPixel`.
        /// Enqueues a tile-cached bitmap draw command (type 0xD), dispatched
        /// by `RenderDrawingQueue` into `DisplayGfx::draw_tiled_bitmap`.
        fn RQ_ENQUEUE_TILED_BITMAP = 0x00541D60;
        fn RQ_DRAW_LINE_STRIP = 0x00541DD0;
        fn RQ_DRAW_POLYGON = 0x00541E50;
        fn RQ_DRAW_CROSSHAIR = 0x00541ED0;
        fn RQ_DRAW_RECT = 0x00541F40;
        fn RQ_DRAW_SPRITE_GLOBAL = 0x00541FE0;
        fn RQ_DRAW_SPRITE_LOCAL = 0x00542060;
        fn RQ_DRAW_SPRITE_OFFSET = 0x005420E0;
        fn RQ_DRAW_BITMAP_GLOBAL = 0x00542170;
        fn RQ_DRAW_TEXTBOX_LOCAL = 0x00542200;
        fn RQ_DRAW_CLIPPED_SPRITE_MAYBE = 0x005422A0;
        fn RQ_CLIP_COORDINATES = 0x00542BA0;
        fn RQ_GET_CAMERA_OFFSET_MAYBE = 0x00542B10;
        fn RQ_CLIP_WITH_REF_OFFSET_MAYBE = 0x00542C70;
        fn RQ_TRANSFORM_WITH_ZOOM_MAYBE = 0x00542D50;
        fn RQ_SMOOTH_INTERPOLATE_MAYBE = 0x00542E60;
        fn RQ_UPDATE_CLIP_BOUNDS_MAYBE = 0x00542F10;
        fn RQ_SATURATE_CLIP_BOUNDS_MAYBE = 0x00542F70;
        fn RENDER_FRAME_MAYBE = 0x0056E040;
        /// `GameRender_Maybe` — per-frame render dispatcher.
        /// usercall(ECX = GameRuntime*), no stack args, plain RET.
        /// Ported in `engine::main_loop::render_frame::game_render`.
        fn/Thiscall GAME_RENDER_MAYBE = 0x00533DC0;
        /// `GameRuntime::DrawAwayOverlay_Maybe` — headful "GAME AWAY"/
        /// "GAME OVER" overlay. usercall(EDI = runtime, [stack]=top_y).
        /// RET 0x4. Bridged from render_frame.
        fn/Usercall GAME_RUNTIME_DRAW_AWAY_OVERLAY = 0x005336E0;
        /// `ClipContext::ClampCameraToBounds` — per-frame "keep camera
        /// within scrollable area" clamp. usercall(EAX=viewport_w,
        /// ECX=max_y, stack=ctx,vh,min_x,min_y,max_x). RET 0x14.
        /// Ported in `render::queue_dispatch::clamp_camera_to_bounds`.
        fn/Usercall CLIP_CONTEXT_CLAMP_CAMERA_TO_BOUNDS = 0x00542F10;
        /// `GameRuntime::RenderEscMenuOverlay` — per-frame ESC-menu blit
        /// (was misnamed `RenderTerrain_Maybe`; the actual terrain renders
        /// via the world entity tree's message-3 handlers, not in any tail
        /// render function). usercall(EAX = GameRuntime*).
        fn/Usercall GAME_RUNTIME_RENDER_ESC_MENU_OVERLAY = 0x00535000;
        /// `MenuPanel::Render` — per-frame incremental redraw of a panel's
        /// canvas; returns `panel.display_a` (DisplayBitGrid*) for caller to
        /// blit. usercall(EDI = MenuPanel*).
        fn/Usercall MENU_PANEL_RENDER = 0x00540B00;
        /// `GameRuntime__RenderWaitingForPeersTextbox` (0x00534F20) —
        /// usercall(EAX=runtime), plain RET. Renders the network "PLEASE
        /// WAIT" textbox during the pre-round window when `game_state == 1`,
        /// `world.net_session != null`, and not all peer teams have joined
        /// yet (per `all_peer_teams_have_joined`). Ported in
        /// `engine::main_loop::render_frame::render_waiting_for_peers_textbox`;
        /// no live xrefs after the port (only caller was `GameRender_Maybe`,
        /// also Rust now). The original Ghidra name `RenderHUD_Maybe` was
        /// grossly misleading — the function only draws this one textbox.
        fn/Usercall RENDER_WAITING_FOR_PEERS_TEXTBOX = 0x00534F20;
        /// `GameRuntime__RenderNetworkEndWaitTextbox` (0x00534E00) —
        /// usercall(EAX=runtime), plain RET. Renders the "PLEASE WAIT %d SEC"
        /// textbox during the network end-of-round handshake (`game_state`
        /// in `{NETWORK_END_AWAITING_PEERS, NETWORK_END_STARTED}`). Ported in
        /// `engine::main_loop::render_frame::render_network_end_wait_textbox`.
        /// The original Ghidra name `RenderTurnStatus_Maybe` was misleading —
        /// the function only draws this one textbox.
        fn/Usercall RENDER_NETWORK_END_WAIT_TEXTBOX = 0x00534E00;
        /// `PaletteManage_Maybe` (0x00533C80) — stdcall(runtime), RET 0x4.
        /// Once every 50 frames, copies layer-2 palette state into
        /// `runtime.palette_ctx_b`, applies a hue rotation, and commits it
        /// via `update_palette`. Ported in
        /// `engine::main_loop::render_frame::palette_manage`.
        fn/Stdcall PALETTE_MANAGE_MAYBE = 0x00533C80;
        /// `PaletteAnimate_Maybe` (0x00533A80) — stdcall(runtime), RET 0x4.
        /// Recomputes all 3 layer palettes (a/b/c) when the cached fade
        /// state changes; blends each toward black with per-layer alphas.
        /// Ported in `engine::main_loop::render_frame::palette_animate`.
        fn/Stdcall PALETTE_ANIMATE_MAYBE = 0x00533A80;
        /// `PaletteContext::RotateHues_Maybe` (0x005415A0) — stdcall(ctx,
        /// frame_group), RET 0x8. Walks `[dirty_range_min..=dirty_range_max]`
        /// of `ctx.rgb_table`; for each entry, converts RGB to HLS, adds
        /// `frame_group` to the hue (mod 240), converts back. Used by
        /// [`PALETTE_MANAGE_MAYBE`] for the per-50-frame palette animation.
        fn/Stdcall PALETTE_CONTEXT_ROTATE_HUES = 0x005415A0;
        /// `PaletteContext::BlendTowardColor_Maybe` (0x005414F0) — usercall
        /// (EAX = alpha (Fixed 0..0x10000), [stack] = ctx, target_rgb),
        /// RET 0x8. For each entry in `[dirty_range_min..=dirty_range_max]`,
        /// linearly blends `ctx.rgb_table[i]` toward `target_rgb` by `alpha`.
        /// Used by [`PALETTE_ANIMATE_MAYBE`] for fade-to-black animations.
        fn/Usercall PALETTE_CONTEXT_BLEND_TOWARD_COLOR = 0x005414F0;
        fn LOAD_SPRITE = 0x00523400;
        fn OPENGL_INIT = 0x0059F000;
        /// IMG__LoadFromDir: look up + decode IMG resource from a .dir archive
        fn IMG_LOAD_FROM_DIR = 0x004F6300;
        /// SpriteGfxTable__Init
        fn/Fastcall SPRITE_GFX_TABLE_INIT = 0x00541620;
        /// RingBuffer__Init
        fn/Usercall RING_BUFFER_INIT = 0x00541060;
        /// WorldEntity__InitTeamScoring
        fn/Fastcall INIT_TEAM_SCORING = 0x00528510;
        /// WorldEntity__InitAllianceData
        fn/Usercall INIT_ALLIANCE_DATA = 0x005262D0;
        /// WorldEntity__InitTurnState
        fn/Usercall INIT_TURN_STATE = 0x00528690;
        /// InitLandscapeBorders — applies the scheme cavern flag to the landscape.
        fn/Usercall INIT_LANDSCAPE_BORDERS = 0x00528480;
        /// HudPanel constructor
        fn/Stdcall HUD_PANEL_CONSTRUCTOR = 0x00524070;
        /// GameWorld__InitTeamsFromSetup
        fn/Stdcall INIT_TEAMS_FROM_SETUP = 0x005220B0;
        /// TeamManager constructor
        fn/Stdcall TEAM_MANAGER_CONSTRUCTOR = 0x00563D40;
        /// GameStateEntity constructor
        fn/Stdcall GAME_STATE_CONSTRUCTOR = 0x00532330;
        /// DisplayGfx::ConstructTextbox
        fn/Stdcall CONSTRUCT_TEXTBOX = 0x004FAF00;
        /// GameWorld__InitWeaponPanel
        fn/Stdcall INIT_WEAPON_PANEL = 0x00567770;
        /// Buffer object constructor
        fn/Stdcall BUFFER_OBJECT_CONSTRUCTOR = 0x00545FD0;
        /// GameStateStream sub-init
        fn/Stdcall GAME_STATE_STREAM_INIT = 0x004FB490;
        /// Display object constructor
        fn/Stdcall DISPLAY_OBJECT_CONSTRUCTOR = 0x00540440;
        /// SpriteRegion constructor (0x9C-byte)
        fn/Stdcall SPRITE_REGION_CONSTRUCTOR = 0x0057DB20;
        fn FUN_570A90 = 0x00570A90;
        fn FUN_570E20 = 0x00570E20;
        /// IMG_Decode
        fn/Stdcall IMG_DECODE = 0x004F5F80;
        /// DrawBungeeTrail
        fn/Stdcall DRAW_BUNGEE_TRAIL = 0x00500720;
        /// DrawCrosshairLine
        fn/Usercall DRAW_CROSSHAIR_LINE = 0x005197D0;
        fn DESTRUCT_LANDSCAPE = 0x0057B540;
        fn REDRAW_LAND_REGION = 0x0057CC10;
        fn WRITE_LAND_RAW = 0x0057C300;
    }

    // =========================================================================
    // Sound
    // =========================================================================

    crate::define_addresses! {
        /// DirectSoundCreate IAT thunk
        fn/Stdcall DIRECTSOUND_CREATE = 0x005B493E;
        fn PLAY_SOUND_LOCAL = 0x004FDFE0;
        fn PLAY_SOUND_GLOBAL = 0x00546E20;
        /// IsSoundSuppressed
        fn/Fastcall IS_SOUND_SUPPRESSED = 0x005261E0;
        /// DispatchGlobalSound
        fn/Fastcall DISPATCH_GLOBAL_SOUND = 0x00526270;
        /// RecordActiveSound
        fn/Usercall RECORD_ACTIVE_SOUND = 0x00546260;
        /// WormEntity::PlaySound2 (FUN_00515020): usercall(EDI=worm) + stdcall(sound_id, volume, flags).
        /// Stop+play on secondary sound handle (+0x3B4). 23 callers in WA.
        fn/Usercall WORM_PLAY_SOUND_2 = 0x00515020;
        /// LoadAndPlayStreamingPositional (0x546BB0): usercall(EAX=task) + stack(volume, sound_id, flags, x, y).
        /// Like LoadAndPlayStreaming but with explicit position. Only caller is PlayWormSound2.
        fn/Usercall LOAD_AND_PLAY_STREAMING_POSITIONAL = 0x00546BB0;
        /// LoadAndPlayStreaming: usercall(EAX=task, ESI=&sound_emitter) + stack(sound_id, flags, volume).
        /// Checks game conditions, then starts a streaming sound. Returns handle | 0x40000000.
        fn/Usercall LOAD_AND_PLAY_STREAMING = 0x00546C20;
        /// ComputeDistanceParams
        fn/Fastcall COMPUTE_DISTANCE_PARAMS = 0x00546300;
        /// DispatchLocalSound
        fn/Usercall DISPATCH_LOCAL_SOUND = 0x00546360;
        /// PlayLocalNoEmitter
        fn/Thiscall PLAY_LOCAL_NO_EMITTER = 0x00546430;
        /// PlayLocalWithEmitter
        fn/Usercall PLAY_LOCAL_WITH_EMITTER = 0x005463F0;
        /// PlaySoundPooled_Direct
        fn/Fastcall PLAY_SOUND_POOLED_DIRECT = 0x00546B50;
        /// Distance3D_Attenuation
        fn/Usercall DISTANCE_3D_ATTENUATION = 0x005430F0;
        /// ActiveSoundTable::stop_sound — stops an active streaming sound by handle.
        fn ACTIVE_SOUND_TABLE_STOP_SOUND = 0x00546490;
    }

    // =========================================================================
    // Speech / Voice Lines / WAV Player / Fanfare
    // =========================================================================

    crate::define_addresses! {
        /// Speech line table in .rdata
        data SPEECH_LINE_TABLE = 0x006AF770;
        /// WAV Player: load and play
        fn/Usercall WAV_PLAYER_LOAD_AND_PLAY = 0x00599B40;
        /// WAV Player: play
        fn/Usercall WAV_PLAYER_PLAY = 0x005996E0;
        /// WAV Player: stop
        fn/Usercall WAV_PLAYER_STOP = 0x00599670;
        /// FeSfx WavPlayer global instance
        global FESFX_WAV_PLAYER = 0x006AC888;
        /// Fanfare WavPlayer global instance
        global FANFARE_WAV_PLAYER = 0x006AC890;
        /// WA data path string buffer
        global WA_DATA_PATH = 0x0088E282;
        /// Team config fanfare name lookup
        fn/Usercall GET_TEAM_CONFIG_NAME = 0x004A62A0;
        /// Builds fanfare path, plays via WavPlayer
        fn/Stdcall PLAY_FANFARE_DEFAULT = 0x004D7500;
        /// Loads fanfare WAV with fallback
        fn/Thiscall PLAY_FANFARE = 0x004D7630;
        /// Gets current team, calls PlayFanfare
        fn/Usercall PLAY_FANFARE_CURRENT_TEAM = 0x004D78E0;
        /// Builds fesfx path, plays via WavPlayer
        fn/Stdcall PLAY_FE_SFX = 0x004D7960;
    }

    // =========================================================================
    // MFC wrappers
    // =========================================================================

    crate::define_addresses! {
        /// AfxCtxMessageBoxA
        fn/Cdecl AFXCTX_MESSAGEBOX_A = 0x005C2055;
        /// CWormsApp::DoMessageBox
        fn/Thiscall CWORMSAPP_DO_MESSAGEBOX = 0x004EB730;
        /// ATL::CSimpleStringT::operator=
        fn/Thiscall CSTRING_OPERATOR_ASSIGN = 0x00401D20;
        /// String resource lookup + assign
        fn/Stdcall CSTRING_ASSIGN_RESOURCE = 0x004A39F0;
        /// CSimpleStringT::SetString
        fn/Thiscall CSTRING_SET_STRING = 0x00401EA0;
        /// CWnd::ShowWindow(this, nShow)
        fn/Thiscall CWND_SHOW_WINDOW = 0x005C643E;
        /// CWnd::MoveWindow(this, x, y, w, h, repaint)
        fn/Thiscall CWND_MOVE_WINDOW = 0x005C6400;
        /// CWnd::SetFocus(this)
        fn/Thiscall CWND_SET_FOCUS = 0x005C649B;
        /// CWnd::FromHandle(hwnd) — static, stdcall(hwnd) returns CWnd*
        fn/Stdcall CWND_FROM_HANDLE = 0x005C353F;
        /// AfxGetModuleState() — cdecl, returns AFX_MODULE_STATE*
        fn/Cdecl AFX_GET_MODULE_STATE = 0x005C83F1;
    }

    // =========================================================================
    // Chat / UI
    // =========================================================================

    crate::define_addresses! {
        fn SHOW_CHAT_MESSAGE = 0x0052ACB0;
        fn ON_CHAT_INPUT = 0x0052B730;
    }

    // =========================================================================
    // Frontend / menu screens
    // =========================================================================

    crate::define_addresses! {
        /// Main navigation loop (CWinApp::Run override)
        fn FRONTEND_MAIN_NAVIGATION_LOOP = 0x004E6440;
        fn/Usercall FRONTEND_CHANGE_SCREEN = 0x00447A20;
        /// Wraps DoModal: palette transition + custom DoModal
        fn FRONTEND_DO_MODAL_WRAPPER = 0x00447960;
        fn FRONTEND_FRAME_CONSTRUCTOR = 0x004ECCA0;
        fn FRONTEND_DIALOG_CONSTRUCTOR = 0x00446BA0;
        fn FRONTEND_PALETTE_ANIMATION = 0x00422180;
        fn FRONTEND_LOAD_TRANSITION_PAL = 0x00447AA0;
        fn FRONTEND_PRE_TRANSITION_CLEANUP = 0x004E4AE0;
        fn FRONTEND_POST_SCREEN_CLEANUP = 0x004EB450;
        fn FRONTEND_ON_INITIAL_LOAD = 0x00429830;
        fn FRONTEND_LAUNCH_SINGLE_PLAYER = 0x00441D80;
        /// Frontend funnel into `GameSession::Run`. Pre/post-game audio +
        /// display + mouse housekeeping wraps the actual game-session call.
        /// Stdcall 4 args (game_world, dialog, p3, p4), RET 0x10.
        fn/Stdcall FRONTEND_LAUNCH_GAME_SESSION = 0x004EC540;
        // ── LaunchGameSession callees (audio cluster, pre-game stop) ──
        /// Usercall(EAX=wav_handle, ESI=&out_local). Audio pre-game state
        /// snapshot; writes to *out_local.
        fn WAV_PLAYER_CHECK_OR_BIND_MAYBE = 0x005997C0;
        /// Stdcall(0). Audio pre-game wav-bank release.
        fn WAV_BANK_RELEASE_ALL_MAYBE = 0x0059A1D0;
        /// Usercall(EDI=&out_local). Audio pre-game stop active channels.
        fn WAV_ACTIVE_CHANNELS_STOP_MAYBE = 0x00599610;
        /// Usercall(ESI=&out_local). Audio post-game DSound channel acquire.
        fn DSOUND_CHANNEL_ACQUIRE_MAYBE = 0x00599360;
        /// Stdcall(0). Audio post-game wav-bank reload.
        fn WAV_BANK_LOAD_ALL_MAYBE = 0x0059A140;
        // ── LaunchGameSession callees (mouse / frontend siblings) ──
        /// Stdcall(1 arg=0). Mouse-cursor recenter on window before game.
        fn MOUSE_CURSOR_RECENTER_ON_WINDOW_MAYBE = 0x004EC0D0;
        /// Stdcall(0). DInput mouse acquire on post-game restore (gated by
        /// `G_POST_GAME_RESTORE_FLAG_MAYBE`).
        fn DINPUT_MOUSE_ACQUIRE_MAYBE = 0x004EC050;
        /// Stdcall(0). Mouse cursor snap to screen center on post-game.
        fn MOUSE_CURSOR_SNAP_TO_SCREEN_CENTER_MAYBE = 0x004EC470;
        /// Stdcall(1 arg=game_world). Mouse mode enter windowed on post-game.
        fn MOUSE_MODE_ENTER_WINDOWED_MAYBE = 0x004EBE00;
        /// Thiscall(this=game_world+0xa4) + 1 stack arg (same value), RET 0x4.
        /// Walks the render tree's children on post-game restore.
        fn/Thiscall GAME_WORLD_RENDER_CHILDREN_MAYBE = 0x0040CAA0;
        /// Usercall(ESI=game_world) + 1 stack arg, RET 0x4. Frontend handler
        /// invoked on the alt-display post-game branch when
        /// `game_world != g_GameWorldInstance`. Composes a localized
        /// "graphics init failed" MessageBox via `Localization__FormatGLibError`
        /// (FUN_0059B3C0), then offers a graphics-mode reset (tokens
        /// 0x786..0x78A). Reads `*game_world` as a `GLibError*`.
        fn FRONTEND_ON_GRAPHICS_INIT_ERROR_MAYBE = 0x004E47D0;
        /// Usercall(EAX=wav_handle, ESI=&out_local). Audio pre-game
        /// channel-prepare; only when game_world == G_MAIN_FRONTEND.
        fn WAV_PLAYER_PREPARE_PLAY = 0x00599930;
        /// Stdcall(1 arg=ring_buffer_addr=0x88DF98), RET 0x4.
        fn STOP_ALL_WAV_PLAYERS_2 = 0x004E31E0;
        // (`WAV_PLAYER_PLAY` is already declared in the audio block above.)
        fn FRONTEND_ON_MULTIPLAYER = 0x0044E850;
        fn FRONTEND_ON_NETWORK = 0x0044EC10;
        fn FRONTEND_ON_MINIMIZE = 0x00486A10;
        fn FRONTEND_ON_OPTIONS_ACCEPT = 0x0048DAB0;
        fn FRONTEND_ON_START_GAME = 0x004F14A0;
        fn CDIALOG_DO_MODAL_CUSTOM = 0x0040FD60;
        fn CDIALOG_CUSTOM_MSG_PUMP = 0x0040FBE0;
        fn FRONTEND_DIALOG_ON_IDLE = 0x0040FF90;
        fn FRONTEND_DIALOG_PAINT_CONTROL_TREE = 0x0040BF60;
        fn FRONTEND_DIALOG_RENDER_BACKGROUND = 0x00404250;
        fn SURFACE_BLIT = 0x00403BF0;
        fn FRONTEND_DEATHMATCH_CTOR = 0x00440F40;
        fn FRONTEND_LOCAL_MP_CTOR = 0x0049C420;
        fn FRONTEND_TRAINING_CTOR = 0x004E0880;
        fn FRONTEND_MISSIONS_CTOR = 0x00499190;
        /// File-existence check via _findfirst (fastcall, ECX=filename)
        fn/Fastcall FILE_EXISTS_CHECK = 0x004DFA30;
        fn FRONTEND_POST_INIT_CTOR = 0x004C91B0;
        fn FRONTEND_MAIN_MENU_CTOR = 0x004866C0;
        fn FRONTEND_SINGLE_PLAYER_CTOR = 0x004D69F0;
        fn FRONTEND_CAMPAIGN_A_CTOR = 0x004A2B70;
        fn FRONTEND_CAMPAIGN_B_CTOR = 0x004A24D0;
        fn FRONTEND_ADV_SETTINGS_CTOR = 0x004279E0;
        fn FRONTEND_INTRO_MOVIE_CTOR = 0x00470870;
        fn FRONTEND_NETWORK_HOST_CTOR = 0x004ADCA0;
        fn FRONTEND_NETWORK_ONLINE_CTOR = 0x004ACBC0;
        fn FRONTEND_NETWORK_PROVIDER_CTOR = 0x004A7990;
        fn FRONTEND_NETWORK_SETTINGS_CTOR = 0x004C23C0;
        fn FRONTEND_LAN_CTOR = 0x00480A80;
        fn FRONTEND_WORMNET_CTOR = 0x00472400;
        fn FRONTEND_LOBBY_HOST_CTOR = 0x004B0160;
        fn FRONTEND_LOBBY_GAME_START_CTOR = 0x004BDBE0;
    }

    // =========================================================================
    // Scheme file operations
    // =========================================================================

    crate::define_addresses! {
        /// Reads .wsc file into scheme struct
        fn/Stdcall SCHEME_READ_FILE = 0x004D3890;
        /// Checks if scheme file exists
        fn/Stdcall SCHEME_FILE_EXISTS = 0x004D4CD0;
        /// Saves scheme struct to .wsc file
        fn/Thiscall SCHEME_SAVE_FILE = 0x004D44F0;
        /// Variant file-exists check for numbered schemes
        fn SCHEME_FILE_EXISTS_NUMBERED = 0x004D4E00;
        /// Version detection
        fn SCHEME_DETECT_VERSION = 0x004D4480;
        /// Extracts built-in schemes from PE resources
        fn SCHEME_EXTRACT_BUILTINS = 0x004D5720;
        /// Copies payload data + V3 defaults into scheme struct
        fn/Fastcall SCHEME_INIT_FROM_DATA = 0x004D5020;
        /// Validates weapon ammo counts
        fn SCHEME_CHECK_WEAPON_LIMITS = 0x004D50E0;
        /// Validates V3 extended options
        fn SCHEME_VALIDATE_EXTENDED_OPTIONS = 0x004D5110;
        /// Scans User\Schemes\ directory
        fn SCHEME_SCAN_DIRECTORY = 0x004D54E0;
        /// Slot 13 feature check
        fn SCHEME_SLOT13_CHECK = 0x004DA4C0;
        /// Load built-in scheme by ID
        fn/Stdcall SCHEME_LOAD_BUILTIN = 0x004D4840;
        /// Validate extended scheme options
        fn/Cdecl SCHEME_VALIDATE_EXTENDED = 0x004D5110;
    }

    // =========================================================================
    // Configuration / registry
    // =========================================================================

    crate::define_addresses! {
        /// Theme file size check
        fn/Cdecl THEME_GET_FILE_SIZE = 0x0044BA80;
        /// Theme file load
        fn/Stdcall THEME_LOAD = 0x0044BB20;
        /// Theme file save
        fn/Stdcall THEME_SAVE = 0x0044BBC0;
        /// Recursive registry key deletion
        fn/Stdcall REGISTRY_DELETE_KEY_RECURSIVE = 0x004E4D10;
        /// Registry cleanup
        fn/Stdcall REGISTRY_CLEAN_ALL = 0x004C90D0;
        /// Loads game options from registry
        fn/Stdcall GAMEINFO_LOAD_OPTIONS = 0x00460AC0;
        /// Reads CrashReportURL from Options
        fn/Cdecl OPTIONS_GET_CRASH_REPORT_URL = 0x005A63F0;
    }

    // =========================================================================
    // Lobby / network
    // =========================================================================

    crate::define_addresses! {
        fn LOBBY_HOST_COMMANDS = 0x004B9B00;
        fn LOBBY_CLIENT_COMMANDS = 0x004AABB0;
        /// Allocates space in packet queue
        fn/Usercall SEND_GAME_PACKET_WRAPPED = 0x00541130;
        fn LOBBY_DISPLAY_MESSAGE = 0x00493CB0;
        fn LOBBY_SEND_GREENTEXT = 0x004AA990;
        fn LOBBY_PRINT_USED_VERSION = 0x004B7E20;
        fn LOBBY_ON_DISCONNECT = 0x004BAE40;
        fn LOBBY_ON_GAME_END = 0x004BAEC0;
        fn LOBBY_ON_MESSAGE = 0x004BD400;
        fn LOBBY_DIALOG_CONSTRUCTOR = 0x004CD9A0;
        fn NETWORK_IS_AVAILABLE = 0x004D4920;
    }

    // =========================================================================
    // Memory / CRT
    // =========================================================================

    crate::define_addresses! {
        /// WA internal malloc — cdecl(size) → *mut u8
        fn/Cdecl WA_MALLOC = 0x005C0AE3;
        fn WA_MALLOC_MEMSET = 0x0053E910;
        fn/Cdecl WA_FREE = 0x005D0D2B;
        /// WA's CRT _fopen
        fn/Cdecl WA_FOPEN = 0x005D3271;
        /// WA's CRT fread
        fn/Cdecl WA_FREAD = 0x005D4531;
        /// WA's CRT fseek
        fn/Cdecl WA_FSEEK = 0x005D38A4;
        /// WA's CRT fclose
        fn/Cdecl WA_FCLOSE = 0x005D399B;
        /// WA's CRT _fileno
        fn/Cdecl WA_FILENO = 0x005D5155;
        /// WA's CRT _get_osfhandle
        fn/Cdecl WA_GET_OSFHANDLE = 0x005D7273;
        /// WA's CRT srand
        fn/Cdecl WA_SRAND = 0x005D293E;
        /// WA's CRT rand
        fn/Cdecl WA_RAND = 0x005D294B;
        /// WA's CRT _gmtime64
        fn/Cdecl WA_GMTIME64 = 0x005D34C0;
        /// WA's CRT malloc (raw)
        fn/Cdecl WA_CRT_MALLOC = 0x005C0AB8;
    }

    // =========================================================================
    // Bitmap font system
    // =========================================================================

    crate::define_addresses! {
        fn FONT_LOAD_FONTS = 0x00414680;
        fn FONT_RENDER_GLYPHS = 0x004143D0;
        fn FONT_DRAW_TEXT = 0x00427830;
        fn/Thiscall DISPLAY_GFX_DRAW_TEXT_ON_BITMAP = 0x005236B0;
        fn/Thiscall DISPLAY_GFX_CONSTRUCT_TEXTBOX = 0x004FAF00;
        fn/Stdcall SET_TEXTBOX_TEXT = 0x004FB070;
    }

    // =========================================================================
    // MapView
    // =========================================================================

    crate::define_addresses! {
        /// MapView constructor
        fn/Stdcall MAP_VIEW_CONSTRUCTOR = 0x00447E80;
        /// MapView load terrain file
        fn/Stdcall MAP_VIEW_LOAD = 0x0044A9A0;
        /// MapView copy info to game state
        fn/Usercall MAP_VIEW_COPY_INFO = 0x00449B60;
        /// Load string resource by ID
        fn/Stdcall WA_LOAD_STRING = 0x00593180;
    }

    // =========================================================================
    // String constants in .rdata
    // =========================================================================

    crate::define_addresses! {
        string STR_CDROM_SPR = 0x0066A3A8;
        string STR_COLOURS_IMG = 0x0066A3B4;
        string STR_MASKS_IMG = 0x0066A3C0;
        /// Empty base path for sprite resource loading
        string SPRITE_RESOURCE_BASE_PATH = 0x00643F2B;
        /// "3.8.1" literal string
        string STR_VERSION_381 = 0x00641C60;
    }

    // =========================================================================
    // Data tables in .rdata/.data
    // =========================================================================

    crate::define_addresses! {
        data SPRITE_RESOURCE_TABLE_1 = 0x006AD2C0;
        data SPRITE_RESOURCE_TABLE_2 = 0x006AF048;
        data WATER_RESOURCE_TABLE = 0x006AF060;
        /// V3 extended options defaults (110 bytes)
        data SCHEME_V3_DEFAULTS = 0x00649AB8;
        /// Per-weapon max ammo table (39 bytes)
        data SCHEME_WEAPON_AMMO_LIMITS = 0x006AD130;
        /// Version string table
        data VERSION_STRING_TABLE = 0x006AB480;
        /// Version suffix table
        data VERSION_SUFFIX_TABLE = 0x00699814;
        /// "data\land.dat" string constant
        string G_LAND_DAT_STRING = 0x0064DA58;
    }

    // =========================================================================
    // Global variables (in .data)
    // =========================================================================

    crate::define_addresses! {
        global G_SPRITE_VERSION_FLAG = 0x006AF050;
        /// `g_DisplayModeFlag`: nonzero = headless / display-disabled mode.
        /// MSVC reaches this in `Frontend::LaunchGameSession` as
        /// `(CWinApp* + 0xCE0B5)` — base-relative addressing on `g_CWinApp`,
        /// not a struct field. Other call sites use the absolute address.
        global G_DISPLAY_MODE_FLAG = 0x0088E485;
        /// One byte before `g_DisplayModeFlag`. Set by `FUN_004EBB70` (alt-display
        /// surface allocation). Read by `Frontend::LaunchGameSession` to gate
        /// the post-game framebuffer reconstruct + ExitProcess fallback.
        /// Same MSVC `(CWinApp* + 0xCE0B4)` base-relative addressing pattern.
        global G_ALT_DISPLAY_SURFACE_ALLOCATED = 0x0088E484;
        /// One byte after `g_DisplayModeFlag`. Snapshot of `g_DisplayModeFlag`
        /// captured by `Frontend::LaunchGameSession` right before `GameSession::Run`.
        /// Currently has no readers we've identified — stored but unused, possibly
        /// for a feature WA never wired up.
        global G_DISPLAY_MODE_FLAG_AT_GAME_START = 0x0088E486;
        /// Two bytes after `g_DisplayModeFlag`. Re-entry latch: set to 1 around
        /// the `MouseModeEnterWindowed` call in the post-game restore path, and
        /// also on the headful-fullscreen ExitProcess fallback. Same base-relative
        /// addressing pattern (`+0xCE0B7`).
        global G_MOUSE_MODE_REENTRY_LATCH = 0x0088E487;
        global G_CURRENT_SCREEN = 0x006B3504;
        global G_CHAR_WIDTH_TABLE = 0x006B2DD9;
        global G_FRONTEND_FRAME = 0x006B3908;
        global G_FRONTEND_HWND = 0x006B390C;
        /// `FlashWindowEx` function pointer — populated at startup via
        /// `GetProcAddress(user32, "FlashWindowEx")`. Null on legacy systems
        /// where the API isn't available; callers fall back to `FlashWindow`.
        global G_FLASH_WINDOW_EX_FN = 0x007A0840;
        /// Set to 1 when the `FlashWindow`-fallback path actually flashed the
        /// window (so OnSYSCOMMAND-restore can call `FlashWindow(false)` to
        /// stop the flash). Unused on the `FlashWindowEx` path because that
        /// API auto-stops on focus.
        global G_WINDOW_FLASHING = 0x007A0844;
        global G_SKIP_TO_MAIN_MENU = 0x007A083D;
        global G_AUTO_NETWORK_FLAG = 0x007A083F;
        /// Input-hook mode flag (u32). Nonzero = an input hook is active; StepFrame
        /// gates PollInput on `world.team_arena.active_worm_count <= active_team_count`
        /// only in that mode (otherwise always polls).
        ///
        /// Mode states (set by `Frontend::InstallInputHooks` family):
        ///  - 0 = no hooks
        ///  - 1 = blocking input grab (keyboard/mouse hooks installed)
        ///  - 2 = timer-driven mode (mode 1 + a frontend-frame timer)
        global G_INPUT_HOOK_MODE = 0x007A0860;
        /// `HHOOK` returned by `SetWindowsHookExA(WH_GETMESSAGE, ...)` in
        /// `Frontend::InstallInputHooks`; consumed by `unhook_input_hooks`.
        /// Despite the historical "keyboard" naming this hook actually
        /// filters mouse messages — the proc at 0x004ED160 inspects
        /// `WM_MOUSEWHEEL`/`WM_MBUTTON*`/`WM_NCLBUTTONDOWN`/`WM_NCRBUTTONDOWN`.
        global G_FRONTEND_MSG_HOOK = 0x006B39B8;
        /// `HHOOK` returned by `SetWindowsHookExA(WH_FOREGROUNDIDLE, ...)` in
        /// `Frontend::InstallInputHooks`; consumed by `unhook_input_hooks`.
        /// Despite the historical "mouse" naming this hook fires on
        /// foreground idle — the proc at 0x004ED0D0 just posts WM_USER+0x3FFC
        /// to the frontend frame to keep its animation alive.
        global G_FRONTEND_IDLE_HOOK = 0x006B32AC;
        /// OS-version sub-field paired with `G_DESKTOP_CHECK_LEVEL`. When
        /// the level is 2 (NT 6+), this byte must be `>= 10` for
        /// `Frontend::InstallInputHooks` to enable the modern (`WM_MBUTTON*`)
        /// mouse-message filter range; otherwise the legacy (`WM_MOUSEWHEEL`)
        /// range is used. Likely the OSVERSIONINFO minor/build field captured
        /// during early startup.
        global G_OS_VERSION_SUB = 0x006B3914;
        /// 1-byte flag (semantics unclear). Cleared on entering input modes
        /// 1 and 2; written by the WH_GETMESSAGE hook callback. Read by
        /// `unhook_input_hooks` to gate the residual mouse-event flush.
        global G_INPUT_HOOK_FLAG_2DD7_MAYBE = 0x006B2DD7;
        /// 1-byte flag set by `Frontend::InstallInputHooks` based on
        /// `g_DesktopCheckLevel`. Selects the `PeekMessageA` filter range
        /// used to drain stale mouse events when leaving input-hook mode:
        ///  - flag != 0: `WM_MBUTTONDOWN ..= WM_MBUTTONDBLCLK`
        ///  - flag == 0: `WM_MOUSEWHEEL` only
        global G_INPUT_HOOK_FILTER_SELECT_MAYBE = 0x006B39C0;
        /// Cached pointer to the original MFC `WindowProcA` for the engine's
        /// game window. Stored by `FUN_004ECD40` immediately before
        /// `SetWindowLongA(..., GWL_WNDPROC, GameSession::WindowProc)` swaps
        /// the WNDPROC. Read by `GameSession::WindowProc`'s outer-guard
        /// fall-through to chain via `CallWindowProcA` for any message it
        /// doesn't handle (i.e. anything outside the keyboard / mouse /
        /// WM_PALETTECHANGED filter, or while `g_InputHookMode != 0` /
        /// `g_InGameLoop == 0`).
        global G_MFC_WNDPROC = 0x006B39C4;
        global G_RENDER_CONTEXT = 0x0079D6D4;
        /// Stipple checkerboard parity — toggled (XOR 1) each render frame in GameRender.
        /// Used by DisplayGfx__BlitStippled to alternate the checkerboard pattern.
        global G_STIPPLE_PARITY = 0x007A087C;
        global G_FONT_ARRAY = 0x007A0F58;
        global G_MAIN_MENU_ACTIVE = 0x007C0A20;
        /// Static `FrontendDialog` instance used for the in-game cursor state
        /// tracking — passed as `param_1` to `FrontendDialog::UpdateCursor`.
        global G_INGAME_FRONTEND_DIALOG = 0x007C0534;
        global G_CWINAPP = 0x007C03D0;
        global G_NETWORK_MODE = 0x007C0D40;
        global G_NETWORK_SUBTYPE = 0x007C0D68;
        /// Game session context pointer
        global G_GAME_SESSION = 0x007A0884;
        global G_FULLSCREEN_FLAG = 0x007A084C;
        /// Frontend "use full-screen rect for cursor clip" flag. Read by
        /// `Cursor::ClipAndRecenter` to decide whether to clip the cursor to
        /// the full monitor (when nonzero) or the work-area (when zero —
        /// respects the Windows taskbar). Toggled by Frontend::LaunchGameSession
        /// and the SC_RESTORE branch of OnSYSCOMMAND. Likely set during the
        /// in-game session and cleared on return to frontend, but the exact
        /// semantics need a separate RE pass — kept `_Maybe`.
        global G_POST_GAME_RESTORE_FLAG_MAYBE = 0x007A0850;
        /// `g_ConsoleMode`. Nonzero when the engine is running in console /
        /// non-UI mode; gates the entire pre/post-game frontend housekeeping
        /// in `Frontend::LaunchGameSession`.
        global G_CONSOLE_MODE = 0x0088CD4C;
        /// 1-byte flag set to 1 right before the `GameSession::Run` call in
        /// `Frontend::LaunchGameSession`, cleared on return. Distinct from
        /// `G_IN_GAME_LOOP` (which tracks the inner main-loop). Read by the
        /// frontend to know "game session is currently active".
        global G_IN_GAME_SESSION_FLAG = 0x007A083E;
        /// Module-static state buffer (0x1900 bytes) passed as the second
        /// arg to `GameSession::Run`. `GameEngine::Shutdown` is given the
        /// same buffer to write back to. Stable across all frontend launch
        /// helpers — they all share this single global instance.
        global G_GAME_SESSION_STATE_BUFFER = 0x008728D4;
        /// 1-byte flag two bytes before `G_FRONTEND_IDLE_HOOK`. Cleared once
        /// in `Frontend::LaunchGameSession` between teardown and `Run`. Use
        /// unknown — possibly an idle-tick suppression latch.
        global G_FRONTEND_TICK_LATCH_MAYBE = 0x006B32AA;
        /// u32 cleared right after `GameSession::Run` returns. Likely a
        /// pending-input or session-active follow-up flag.
        global G_PENDING_INPUT_FLAG_MAYBE = 0x0088C77C;
        /// u32 cleared on the headful-fullscreen ExitProcess fallback path
        /// in `Frontend::LaunchGameSession`. Use unknown.
        global G_FULLSCREEN_RESTORE_FLAG_MAYBE = 0x006A9644;
        /// Zero-init pointer cell, never written anywhere in the binary. Read
        /// by `Frontend::LaunchGameSession` and `FUN_004EBD50`. In the launch
        /// path the comparison `app == g_MainFrontend` therefore always fails
        /// (param is `&g_CWinApp`, this is null), making the gated
        /// `WavPlayer_PreparePlay` call dead code we still emit for fidelity.
        /// Possibly a stub from an early code path that was never finished.
        global G_MAIN_FRONTEND = 0x007C028C;
        /// Zero-init pointer cell, never written anywhere in the binary —
        /// "DDGame" is WA's old codename for it. Read by `Frontend::LaunchGameSession`,
        /// `MouseModeEnterWindowed_Maybe`, `MouseCursorRecenterOnWindow_Maybe`,
        /// and a few others. The launch-path comparison `app != g_GameWorldInstance`
        /// is always true (param is `&g_CWinApp`, this is null), making the
        /// `OnGraphicsInitError + ExitProcess(1)` branch unconditionally fatal
        /// when reached — see the inline comment in `launch_game_session` for
        /// why we skip that branch outright.
        global G_GAME_WORLD_INSTANCE = 0x007C03CC;
        /// Pointer-to-pointer table whose `+0x128` slot holds the wav
        /// player handle used by `Frontend::LaunchGameSession` for
        /// pre/post-game audio save/restore.
        global G_AUDIO_HANDLE_TABLE_PTR = 0x006AC748;
        /// `StopAllWavPlayers_2` argument in `Frontend::LaunchGameSession` —
        /// 16-slot wav-player ring-buffer base.
        global G_WAV_PLAYER_RING_BASE_MAYBE = 0x0088DF98;
        // ── IAT slots used by Frontend::LaunchGameSession ──
        /// `user32!SetActiveWindow` IAT slot.
        global IAT_SET_ACTIVE_WINDOW = 0x0061A5C8;
        /// `user32!SetFocus` IAT slot.
        global IAT_SET_FOCUS = 0x0061A5C0;
        /// `kernel32!ExitProcess` IAT slot.
        global IAT_EXIT_PROCESS = 0x0061A350;
        /// `user32!ClipCursor` IAT slot.
        global IAT_CLIP_CURSOR = 0x0061A668;
        global G_SUPPRESS_CURSOR = 0x0088E485;
        global IAT_MAP_WINDOW_POINTS = 0x0061A588;
        global G_SPRITE_DATA_BYTES = 0x007A0864;
        global G_SPRITE_FRAME_COUNT = 0x007A0868;
        global G_SPRITE_PIXEL_AREA = 0x007A086C;
        global G_SPRITE_PALETTE_BYTES = 0x007A0870;
        global G_GAME_INFO = 0x007749A0;
        global G_FRAME_BUFFER_PTR = 0x007A0EEC;
        global G_FRAME_BUFFER_WIDTH = 0x007A0EF0;
        global G_FRAME_BUFFER_HEIGHT = 0x007A0EF4;
        global G_CRASH_REPORT_URL = 0x0079FFD8;
        global G_VERSION_BYTE = 0x00697702;
        /// In-game-loop flag — set to 1 during message pump in GameSession__PumpMessages
        global G_IN_GAME_LOOP = 0x006B39BC;
        /// Desktop check threshold — ProcessFrame skips desktop check when <= 1
        global G_DESKTOP_CHECK_LEVEL = 0x006B3920;
    }

    // =========================================================================
    // Trig lookup tables / scratch buffers
    // =========================================================================

    crate::define_addresses! {
        /// Sine lookup table — 1024 entries of i32 (fixed-point 16.16)
        data G_SIN_TABLE = 0x006A1860;
        /// Cosine lookup table — 1024 entries of i32 (fixed-point 16.16)
        data G_COS_TABLE = 0x006A1C60;
        /// Global vertex scratch buffer
        global G_VERTEX_SCRATCH_BUFFER = 0x008B1470;
    }

    // =========================================================================
    // Replay globals
    // =========================================================================

    crate::define_addresses! {
        global G_REPLAY_STATE = 0x0087D3F8;
        global G_TEAM_HEADER_DATA = 0x008779E4;
        global G_GAME_INFO = 0x0087D438;
        global G_REPLAY_GAME_ID = 0x0088AF50;
        global G_REPLAY_SUB_FORMAT = 0x0088AF54;
        global G_REPLAY_VERSION_ID = 0x0088ABB0;
        global G_REPLAY_SCHEME_PRESENT = 0x0088AE0C;
        global G_ARTCLASS_COUNTER = 0x0088C790;
        global G_RANDOM_SEED = 0x0088D0B4;
        global G_SAVED_RANDOM_SEED = 0x0088ABAC;
        global G_REPLAY_FILENAME = 0x0088AF58;
        global G_DATA_DIR = 0x0088E078;
        global G_LOG_FILE_PTR = 0x0088C370;
        global G_OBSERVER_ARRAY = 0x0088C35C;
        global G_OBSERVER_COUNT = 0x0088AF4C;
        global G_RECORDING_TIMESTAMP_FLAG = 0x0088C36C;
        global G_REPLAY_VER_FLAG_A = 0x0088AF42;
        global G_REPLAY_VER_FLAG_B = 0x0088AF43;
        global G_REPLAY_GAME_MODE = 0x0088AF44;
        global G_SCHEME_HEADER = 0x0088DAD4;
        global G_SCHEME_DEST = 0x0088DACC;
        global G_SCHEME_DATA = 0x0088DAE0;
        global G_SCHEME_OPTIONS = 0x0088DBB8;
        global G_SCHEME_V3_DATA = 0x0088DC04;
        global G_HOST_PLAYER = 0x008779E0;
        global G_PLAYER_ARRAY = 0x008779E4;
        global G_PLAYER_COUNT = 0x0087D0DE;
        global G_TEAM_DATA = 0x00877FFC;
        global G_TEAM_COUNT = 0x0087D0E0;
        global G_REPLAY_NAME = 0x0087D0E1;
        global G_MAP_BYTE_1 = 0x0087250C;
        global G_MAP_BYTE_2 = 0x00872508;
        global G_MAP_SEED = 0x0087D430;
        global G_WORM_NAMES = 0x00878097;
    }

    // =========================================================================
    // Scheme data globals
    // =========================================================================

    crate::define_addresses! {
        global SCHEME_ACTIVE_WEAPON_DATA = 0x0088DB05;
        global SCHEME_SLOT_FLAGS = 0x006B329C;
        global SCHEME_MODIFIER_GUARD = 0x0088E460;
    }

    // =========================================================================
    // Configuration globals
    // =========================================================================

    crate::define_addresses! {
        global G_BASE_DIR = 0x0088E282;
        global G_GAMEINFO_BLOCK_F485 = 0x0088DFF3;
        global G_CONFIG_BYTE_F3A0 = 0x007C0D38;
        global G_CONFIG_DWORDS_F3B4 = 0x0088E39C;
        global G_CONFIG_GUARD = 0x0088C374;
        global G_CONFIG_DWORDS_F3F4 = 0x0088E3B8;
        global G_CONFIG_DWORD_DAE8 = 0x0088E390;
        global G_CONFIG_DWORDS_F3D4 = 0x0088E3B0;
        global G_CONFIG_DWORDS_F3C4 = 0x0088E400;
        global G_CONFIG_DWORD_F3E4 = 0x0088E44C;
        global G_STREAMS_DIR = 0x0088AE18;
        global G_STREAM_INDICES = 0x0088AE9C;
        global G_STREAM_INDICES_END = 0x0088AEDC;
        global G_STREAM_FLAG = 0x0088E394;
        global G_STREAM_VOLUME = 0x0088AEDD;

        // DispatchFrame unported callees.
        /// ActiveSoundTable__Update — stdcall(self), iterates streaming entries
        /// and drops finished ones. Called each DispatchFrame tick.
        fn/Stdcall ACTIVE_SOUND_TABLE_UPDATE = 0x005464E0;
        /// MSVC CRT `__iob_func` — returns the three-entry `FILE` array.
        /// `iob_func()+0x20` is `stdout`.
        fn/Cdecl CRT_IOB_FUNC = 0x005D4E40;
        /// IAT slot for `fputs` — dereference to get the live import pointer.
        global CRT_FPUTS_IAT = 0x00649468;
        /// MSVC CRT `_ferror(FILE*)`.
        fn/Cdecl CRT_FERROR = 0x005D5126;
        /// IAT slot for `putc` — dereference to get the live import pointer.
        global CRT_PUTC_IAT = 0x006492D4;
        /// Codepage__BuildLut — usercall(EAX=codepage) → returns a
        /// 256-byte translation-table pointer in EAX. Different codepages
        /// are cached at different globals (0x7A0ED0/D4/…). Called from
        /// the end-of-round log recoder after `GetACP()`.
        fn/Usercall CODEPAGE_BUILD_LUT = 0x00592280;
        /// Cached codepage LUT pointer. Lazily initialised on first use
        /// (zero → call `Codepage__BuildLut`, store result here).
        global G_CODEPAGE_LUT = 0x007A0ED8;
        /// Byte flag: when nonzero, log output passes through the
        /// codepage LUT (`LUT[byte + 0x100]`) before being written to the
        /// stream. When zero, bytes are emitted verbatim. Read by
        /// `LogOutput` on construction.
        global G_CODEPAGE_RECODE_FLAG = 0x006B39C2;
        /// Phase-label resource ID table indexed by `wrapper.game_end_phase`
        /// (0..9 → resource IDs 0x704..0x70D). Read by end-of-round banner.
        global G_PHASE_LABEL_RES_TABLE = 0x006A70E0;
        /// Primary localization data record (`*mut LocalizationData`). When
        /// non-null, its per-entry offset is tried first by `LoadStringResource`.
        global G_LOCALIZATION_DATA_PRIMARY = 0x007A0EDC;
        /// Secondary (fallback) localization data record (`*mut LocalizationData`).
        /// Consulted before the primary record — matches the original code's
        /// check order in `WA__LoadStringResource` (0x593180).
        global G_LOCALIZATION_DATA_SECONDARY = 0x007A0EE0;
        /// Default string table: array of `*const c_char`, length
        /// `StringRes::COUNT`. Used when neither localization record overrides
        /// a given entry.
        global G_LOCALIZATION_KEY_TABLE = 0x00697708;
        /// BSS byte latched to 1 on first DispatchFrame pass. Gates a
        /// clamp that inflates `remaining` up to `frame_duration` while
        /// the game hasn't started yet. Purpose not fully confirmed; read
        /// once, written once per frame.
        global G_DISPATCH_FRAME_LATCH = 0x008ACE34;
    }

    // =========================================================================
    // Steamworks SDK bootstrap
    // =========================================================================

    crate::define_addresses! {
        /// Steamworks SDK bootstrap wrapper. Calls `SteamAPI_Init`,
        /// `SteamAPI_RestartAppIfNecessary(217200)`, `BIsSubscribedApp`, and
        /// `SetOverlayNotificationPosition(1)`. Returns 1 on success, 0 on
        /// failure (Steam not running, app not owned, or restart triggered) —
        /// in which case `Frontend__MainNavigationLoop` exits silently.
        fn/Cdecl STEAM_BOOTSTRAP = 0x00598D40;
    }

    // =========================================================================
    // GameWorld struct offsets (not VAs — kept as manual constants)
    // =========================================================================

    pub mod world_offsets {
        /// Offset to WorldRoot object pointer
        pub const WORLD_ROOT: u32 = 0x08;
        /// Offset to game global state pointer
        pub const GAME_GLOBAL: u32 = 0x488;
        /// Offset to PC_Landscape pointer
        pub const LANDSCAPE: u32 = 0x4CC;
        /// Offset to weapon table pointer
        pub const WEAPON_TABLE: u32 = 0x510;
        /// Offset to RenderQueue pointer
        pub const RENDER_QUEUE: u32 = 0x524;
        /// Offset to weapon panel pointer
        pub const WEAPON_PANEL: u32 = 0x548;

        /// Deferred hurry flag. Set to 1 during replay instead of sending network
        /// packet. GameFrameEndProcessor (0x531960) reads this and converts it to
        /// a local Hurry message (EntityMessage 0x17 = 23).
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
