/// Known addresses in WA.exe 3.8.1 (image base 0x00400000).
///
/// These addresses are discovered through Ghidra analysis and
/// cross-referenced with wkJellyWorm/WormKit sources.
///
/// All addresses are virtual addresses (VA) as loaded in memory.
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

    // === Vtables (in .rdata) ===

    /// CTask vtable - 7 virtual method pointers
    pub const CTASK_VTABLE: u32 = 0x0066_9F8C;
    /// CGameTask vtable - extends CTask vtable with 12 more methods
    pub const CGAMETASK_VTABLE: u32 = 0x0066_41F8;
    /// CGameTask sound emitter vtable (embedded sub-object at offset 0xE8).
    /// 12 slots: [0] GetPosition, [1] GetPosition2, [3] Destructor, [4] HandleMessage.
    pub const CGAMETASK_SOUND_EMITTER_VT: u32 = 0x0066_9CF8;
    /// Alias for backward compatibility with validation code.
    pub const CGAMETASK_VTABLE2: u32 = CGAMETASK_SOUND_EMITTER_VT;
    /// DDGameWrapper vtable
    pub const DDGAME_WRAPPER_VTABLE: u32 = 0x0066_A30C;
    /// GfxHandler vtable (0x19C-byte objects)
    pub const GFX_HANDLER_VTABLE: u32 = 0x0066_B280;
    /// DisplayGfx vtable
    pub const DISPLAY_GFX_VTABLE: u32 = 0x0066_4144;
    /// PCLandscape vtable
    pub const PC_LANDSCAPE_VTABLE: u32 = 0x0066_B208;
    /// LandscapeShader vtable
    pub const LANDSCAPE_SHADER_VTABLE: u32 = 0x0066_B1DC;
    /// DSSound vtable
    pub const DS_SOUND_VTABLE: u32 = 0x0066_AF20;
    /// DDKeyboard vtable (0x33C-byte keyboard object)
    pub const DDKEYBOARD_VTABLE: u32 = 0x0066_AEC8;
    /// Palette vtable (0x28-byte palette object)
    pub const PALETTE_VTABLE_MAYBE: u32 = 0x0066_A2E4;
    /// DisplayBase primary vtable (set by constructor, has _purecall slots)
    pub const DISPLAY_BASE_VTABLE: u32 = 0x0066_45F8;
    /// DisplayBase headless vtable — overlaid after constructor in headless mode,
    /// filling in stub slots for headless operation.
    pub const DISPLAY_BASE_HEADLESS_VTABLE: u32 = 0x0066_A0F8;
    /// Input controller vtable (0x1800-byte object, set inline before FUN_0058C0D0)
    pub const INPUT_CTRL_VTABLE: u32 = 0x0066_B3FC;
    /// TaskStateMachine vtable
    pub const TASK_STATE_MACHINE_VTABLE: u32 = 0x0066_4118;
    /// OpenGLCPU vtable (0x48-byte object)
    pub const OPENGL_CPU_VTABLE: u32 = 0x0067_74C0;
    /// WaterEffect vtable (0xBC-byte object)
    pub const WATER_EFFECT_VTABLE: u32 = 0x0066_B268;
    /// CTaskLand vtable - landscape/terrain task (DDGame+0x054C)
    pub const CTASK_LAND_VTABLE: u32 = 0x0066_4388;
    /// CTaskWorm vtable - worm entity task (constructor 0x50BFB0)
    pub const CTASK_WORM_VTABLE: u32 = 0x0066_44C8;
    /// CTaskTurnGame vtable - global turn flow manager (1 per game)
    pub const CTASK_TURN_GAME_VTABLE: u32 = 0x0066_9F70;
    /// CTaskTeam vtable - per-team task (1 per team, constructor 0x555BF0)
    pub const CTASK_TEAM_VTABLE: u32 = 0x0066_9EE4;
    /// CTaskFilter vtable - role unclear; 4 instances in a 2-team 3-worm game
    pub const CTASK_FILTER_VTABLE: u32 = 0x0066_9DAC;
    /// CTaskDirt vtable - dirt/particle system (1 per game, constructor 0x54EDF0)
    pub const CTASK_DIRT_VTABLE: u32 = 0x0066_9D74;
    /// CTaskSpriteAnim vtable - sprite animation manager (1 per game, constructor 0x5466F0)
    pub const CTASK_SPRITE_ANIM_VTABLE: u32 = 0x0066_9D00;
    /// CTaskCPU vtable - AI/CPU bot controller (1 per game, constructor 0x548620)
    pub const CTASK_CPU_VTABLE: u32 = 0x0066_9D54;
    /// CTaskMissile vtable - projectile/missile entity (constructor 0x507D10)
    pub const CTASK_MISSILE_VTABLE: u32 = 0x0066_4438;
    /// CTaskMine vtable - mine entity (constructor 0x506660)
    pub const CTASK_MINE_VTABLE: u32 = 0x0066_43E8;
    /// CTaskOilDrum vtable - oil drum entity (constructor 0x504AF0)
    pub const CTASK_OILDRUM_VTABLE: u32 = 0x0066_4338;
    /// CTaskCrate vtable - weapon/health/utility crate (constructor 0x502490)
    pub const CTASK_CRATE_VTABLE: u32 = 0x0066_4298;
    /// CTaskCloud vtable - cloud/airstrike entity (constructor 0x5482E0)
    pub const CTASK_CLOUD_VTABLE: u32 = 0x0066_9D38;
    /// CTaskSeaBubble vtable - water bubble particle (constructor 0x554FE0)
    pub const CTASK_SEA_BUBBLE_VTABLE: u32 = 0x0066_9E88;
    /// CTaskFire vtable - fire/flame entity (constructor 0x54F4C0, 0xD8 bytes)
    pub const CTASK_FIRE_VTABLE: u32 = 0x0066_9DD8;
    /// Sprite vtable (0x70-byte objects, 8 entries)
    pub const SPRITE_VTABLE: u32 = 0x0066_418C;

    // === CTask vtable methods (at CTask__vtable) ===

    /// CTask::vtable0 - initialization/unknown
    pub const CTASK_VT0_INIT: u32 = 0x0056_2710;
    /// CTask::Free - destructor/deallocation
    pub const CTASK_VT1_FREE: u32 = 0x0056_2620;
    /// CTask::HandleMessage - message dispatch
    pub const CTASK_VT2_HANDLE_MESSAGE: u32 = 0x0056_2F30;
    /// CTask::vtable3 - unknown
    pub const CTASK_VT3: u32 = 0x0056_13D0;
    /// CTask::vtable4 - unknown (same as vt3 in base)
    pub const CTASK_VT4: u32 = 0x0056_13D0;
    /// CTask::vtable5 - unknown
    pub const CTASK_VT5: u32 = 0x0056_2FA0;
    /// CTask::vtable6 - unknown
    pub const CTASK_VT6: u32 = 0x0056_3000;
    /// CTask::vtable7 - ProcessFrame
    pub const CTASK_VT7_PROCESS_FRAME: u32 = 0x0056_3210;

    // === CGameTask vtable methods (first 8 override CTask, then 12 new) ===

    /// CGameTask::vtable0 override
    pub const CGAMETASK_VT0: u32 = 0x004F_F1C0;
    /// CGameTask::Free override
    pub const CGAMETASK_VT1_FREE: u32 = 0x004F_EF10;
    /// CGameTask::HandleMessage override
    pub const CGAMETASK_VT2_HANDLE_MESSAGE: u32 = 0x004F_F280;

    // === Constructors ===

    /// CTask constructor - initializes base task fields and children list
    pub const CTASK_CONSTRUCTOR: u32 = 0x0056_25A0;
    /// CGameTask constructor - calls CTask ctor, sets physics defaults
    pub const CGAMETASK_CONSTRUCTOR: u32 = 0x004F_ED50;
    /// CTaskWorm constructor
    pub const CTASK_WORM_CONSTRUCTOR: u32 = 0x0050_BFB0;

    // === Game entity constructors (from wkJellyWorm) ===

    pub const CTASK_AIRSTRIKE_CTOR: u32 = 0x0055_53C0;
    pub const CTASK_ARROW_CTOR: u32 = 0x004F_E130;
    pub const CTASK_CANISTER_CTOR: u32 = 0x0050_1A80;
    pub const CTASK_CLOUD_CTOR: u32 = 0x0054_82E0;
    pub const CTASK_CPU_CTOR: u32 = 0x0054_85D0;
    pub const CTASK_CRATE_CTOR: u32 = 0x0050_2490;
    pub const CTASK_CROSS_CTOR: u32 = 0x0050_45C0;
    pub const CTASK_DIRT_CTOR: u32 = 0x0054_EDC0;
    pub const CTASK_FILTER_CTOR: u32 = 0x0054_F3D0;
    pub const CTASK_FIRE_CTOR: u32 = 0x0054_F4C0;
    pub const CTASK_FIREBALL_CTOR: u32 = 0x0055_0890;
    pub const CTASK_FLAME_CTOR: u32 = 0x0054_F0F0;
    pub const CTASK_GAS_CTOR: u32 = 0x0055_4750;
    pub const CTASK_LAND_CTOR: u32 = 0x0050_5440;
    pub const CTASK_MINE_CTOR: u32 = 0x0050_6660;
    pub const CTASK_MISSILE_CTOR: u32 = 0x0050_7D10;
    pub const CTASK_OILDRUM_CTOR: u32 = 0x0050_4AF0;
    pub const CTASK_OLDWORM_CTOR: u32 = 0x0051_FEB0;
    pub const CTASK_SCOREBUBBLE_CTOR: u32 = 0x0055_4CA0;
    pub const CTASK_SEABUBBLE_CTOR: u32 = 0x0055_4FE0;
    pub const CTASK_SMOKE_CTOR: u32 = 0x0055_51D0;
    pub const CTASK_SPRITE_ANIM_CTOR: u32 = 0x0054_66C0;
    pub const CTASK_TEAM_CTOR: u32 = 0x0055_5BB0;
    pub const CTASK_TURNGAME_CTOR: u32 = 0x0055_B280;

    // === Replay / turn management ===

    /// Loads .WAgame replay file, validates magic 0x4157, stores payload at DDGame+0xDB1C.
    /// stdcall(this, mode) where mode: 1=play, 2=getmap, 3=getscheme, 4=repair. ~12KB function.
    pub const REPLAY_LOADER: u32 = 0x0046_2DF0;
    /// Parses "MM:SS.FF" time string → frame number. Returns -1 on failure.
    pub const PARSE_REPLAY_POSITION: u32 = 0x004E_3490;
    /// Routes game messages through the task handler tree.
    pub const GAME_MESSAGE_ROUTER: u32 = 0x0055_3BD0;
    /// TurnGame message dispatcher. Case 2=FrameFinish, Case 4=ProcessInput, Case 0x28=SkipGo.
    /// thiscall + 4 stack params.
    pub const TURNGAME_HANDLE_MESSAGE: u32 = 0x0055_DC00;
    /// Checks TurnGame+0x1F4 (hurry requested). Normal game: sends packet 0x17.
    /// Replay mode (DB08 && DB0A): sets DDGame+0x7E41 instead. Uses ESI (__usercall).
    pub const TURNGAME_HURRY_HANDLER: u32 = 0x0055_E5F0;
    /// Per-frame turn timer: decrements turn timer by 0x14 (20ms) each frame.
    pub const TURN_MANAGER_PROCESS_FRAME: u32 = 0x0055_FDA0;
    /// Iterates teams during ProcessInput, sends packet 0x2B for valid teams.
    pub const TURNGAME_AUTO_SELECT_TEAMS: u32 = 0x0056_11E0;
    /// Control task HandleMessage (vtable 0x669C28). Translates keyboard input (msg 0xC)
    /// into game messages. Case 9 toggles pause flag.
    pub const CONTROL_TASK_HANDLE_MESSAGE: u32 = 0x0054_51F0;
    /// End-of-frame processing. Reads DDGame+0x7E41 (deferred hurry flag),
    /// converts it to local Hurry message (0x17). Also handles frame counters.
    pub const GAME_FRAME_END_PROCESSOR: u32 = 0x0053_1960;
    /// Main frame loop. Processes message queue, calls GameFrameEndProcessor.
    pub const GAME_FRAME_DISPATCHER: u32 = 0x0053_1D00;
    /// Sends game packet if network buffer capacity allows. Checks DDGame+0x98A4.
    pub const SEND_GAME_PACKET_CONDITIONAL: u32 = 0x0053_1880;

    // === Gameplay functions ===

    pub const CREATE_EXPLOSION: u32 = 0x0054_8080;
    pub const SPECIAL_IMPACT: u32 = 0x0051_93D0;
    pub const SPAWN_OBJECT: u32 = 0x0056_1CF0;
    pub const WEAPON_RELEASE: u32 = 0x0051_C3D0;
    pub const WORM_START_FIRING: u32 = 0x0051_B7F0;
    pub const FIRE_WEAPON: u32 = 0x0051_EE60;
    pub const CREATE_WEAPON_PROJECTILE: u32 = 0x0051_E0F0;

    // === Weapon system ===

    pub const INIT_WEAPON_TABLE: u32 = 0x0053_CAB0;
    pub const COUNT_ALIVE_WORMS: u32 = 0x0052_25A0;
    pub const GET_AMMO: u32 = 0x0052_25E0;
    pub const ADD_AMMO: u32 = 0x0052_2640;
    /// Not the main ammo decrement path — only 5 xrefs across 3 functions,
    /// never observed firing during normal gameplay. The real decrement is
    /// likely inlined at weapon-firing call sites. Unhooked until verified.
    pub const SUBTRACT_AMMO: u32 = 0x0052_2680;

    // === Team/worm accessor functions (DDGame + 0x4628 area) ===

    /// Counts teams by alliance membership, sets current_alliance + counters.
    /// usercall(EAX=base, EDI=alliance_id) → void, plain RET.
    pub const COUNT_TEAMS_BY_ALLIANCE: u32 = 0x0052_2030;
    /// Sums health of all worms on a team. Returns 0 if team eliminated.
    /// fastcall(ECX=team_index, EDX=base) → EAX=total_health, plain RET.
    pub const GET_TEAM_TOTAL_HEALTH: u32 = 0x0052_24D0;
    /// Checks if a worm is in a "special" state (dying, drowning, etc.).
    /// usercall(EAX=team_index, ECX=worm_index, [ESP+4]=base) → EAX=bool, RET 0x4.
    pub const IS_WORM_IN_SPECIAL_STATE: u32 = 0x0052_26B0;
    /// Reads worm X,Y position into output pointers.
    /// usercall(EAX=team_index, ECX=worm_index, [ESP+4]=base, [ESP+8]=&x, [ESP+C]=&y), RET 0xC.
    pub const GET_WORM_POSITION: u32 = 0x0052_2700;
    /// Checks if any worm has state 0x64 (100). 11 xrefs in gameplay code.
    /// Despite the comparison to 100, reads worms[].state NOT .health.
    /// usercall(EAX=base) → EAX=bool, plain RET.
    pub const CHECK_WORM_STATE_0X64: u32 = 0x0052_28D0;
    /// Per-team version of CheckWormState0x64. Returns 1 if any worm on the
    /// specified team has state==0x64. 1 xref.
    /// usercall(EAX=team_idx, ECX=base) → EAX=bool, plain RET.
    pub const CHECK_TEAM_WORM_STATE_0X64: u32 = 0x0052_2930;
    /// Scans all teams for any worm with state 0x8b. 1 xref.
    /// usercall(EAX=base) → EAX=bool, plain RET.
    pub const CHECK_ANY_WORM_STATE_0X8B: u32 = 0x0052_2970;
    /// Sets the active worm for a team. flag=0 deactivates, flag=N sets worm N active.
    /// Called on turn transitions and worm selection (Tab key).
    /// usercall(EAX=base, EDX=team_idx, ESI=worm_index) → void, plain RET. 3 xrefs.
    pub const SET_ACTIVE_WORM_MAYBE: u32 = 0x0052_2500;

    // === Game session ===

    /// `GameEngine__InitHardware` — initializes all hardware subsystems: display,
    /// sound, keyboard, palette, DDGameWrapper, DDNetGameWrapper, and stores
    /// the resulting pointers into `G_GAME_SESSION`.
    ///
    /// `__thiscall(this=GameInfo, hwnd, param3, param4)` → 1=ok 0=fail, `RET 0xC`.
    ///
    /// In headless/stats mode (`GameInfo+0xF914 != 0`) creates a `GameStats` object
    /// with the DDInput vtable instead of a real display+audio stack.
    pub const GAME_ENGINE_INIT_HARDWARE: u32 = 0x0056_D350;

    /// `GameSession__Run` — allocates the `GameSession` struct (0x120 bytes),
    /// calls `GameEngine__InitHardware`, runs the game main loop, then calls
    /// `GameEngine__Shutdown`. Uses ESI implicitly for the `GameInfo` config.
    pub const GAME_SESSION_RUN: u32 = 0x0057_2F50;

    /// `GameSession__Constructor` — usercall(`EAX=this`), sets vtable and
    /// zero-inits all fields. Called with a freshly `malloc`'d 0x120-byte buffer.
    pub const GAME_SESSION_CONSTRUCTOR: u32 = 0x0058_BFA0;

    /// `GameEngine__Shutdown` — destroys all subsystems in reverse creation order
    /// (streaming audio, DDGameWrapper, DisplayGfx, DDKeyboard, Palette).
    /// `stdcall(param_1)` → void.
    pub const GAME_ENGINE_SHUTDOWN: u32 = 0x0056_DCD0;

    // === Graphics / rendering ===

    pub const CONSTRUCT_DD_GAME: u32 = 0x0056_E220;
    pub const CONSTRUCT_DD_GAME_WRAPPER: u32 = 0x0056_DEF0;
    /// DDGameWrapper::InitReplay — usercall(EAX=game_info, ESI=this), plain RET.
    /// Opens replay/recording files based on GameInfo+0xDB08/0xDB09 flags.
    pub const DDGAMEWRAPPER_INIT_REPLAY: u32 = 0x0056_F860;
    /// DDGame::InitGameState — stdcall(this=DDGameWrapper*), RET 0x4.
    /// Initializes game state fields in DDGame after DDGame__Constructor.
    pub const DDGAME_INIT_GAME_STATE: u32 = 0x0052_6500;
    /// DisplayGfx__Constructor_Maybe — stdcall(this) → DisplayGfx*.
    /// Constructs the 0x24E28-byte DisplayGfx object.
    pub const DISPLAYGFX_CTOR: u32 = 0x0056_9C10;
    /// DDDisplay::Init — usercall(ECX=height) + stdcall(display_gfx, hwnd, width, flags), RET 0x10 → 0 on failure.
    /// ECX must be set to game_info+0xF3B8 (height) before calling.
    /// Resolution retry loop in GameEngine__InitHardware updates GameInfo+0xF3B4/0xF3B8.
    pub const DDISPLAY_INIT: u32 = 0x0056_9D00;
    /// Alias kept for callers that use the old name.
    pub const CONSTRUCT_DD_DISPLAY: u32 = DDISPLAY_INIT;
    /// DisplayBase__Constructor — stdcall(this) → DisplayBase*.
    /// Constructs the 0x3560-byte base display object (used standalone in headless mode).
    pub const DISPLAY_BASE_CTOR: u32 = 0x0052_2DB0;
    /// Streaming audio constructor — stdcall(IDirectSound*, path_config_ptr) → *mut u8.
    /// Constructs 0x354-byte streaming audio object (only if GameInfo+0xDAA4 != 0).
    pub const STREAMING_AUDIO_CTOR: u32 = 0x0058_BC10;
    /// Input controller initializer — usercall(ESI=this) + stdcall(game_info_p4, hwnd, param3, joycount).
    /// Initializes 0x1800-byte input ctrl. Returns 0 on failure. RET 0x10.
    pub const INPUT_CTRL_INIT: u32 = 0x0058_C0D0;
    /// DDNetGameWrapper__Constructor_Maybe — stdcall(this) → *mut u8.
    /// Constructs 0x2C-byte DDNetGameWrapper. Returns param_1.
    pub const DDNETGAME_WRAPPER_CTOR: u32 = 0x0056_D1F0;
    /// Timer object constructor — usercall(ESI=this, EAX=init_val), plain RET.
    /// Constructs 0x30-byte timer shell and allocates 2×0x20E0 internal buffers.
    pub const GAME_ENGINE_TIMER_CTOR: u32 = 0x0053_E950;
    pub const CONSTRUCT_FRAME_BUFFER: u32 = 0x005A_2430;
    pub const BLIT_SCREEN: u32 = 0x005A_2020;
    pub const RQ_RENDER_DRAWING_QUEUE: u32 = 0x0054_2350;
    pub const DRAW_LANDSCAPE: u32 = 0x005A_2790;
    pub const RQ_DRAW_PIXEL: u32 = 0x0054_1D60;
    pub const RQ_DRAW_LINE_STRIP: u32 = 0x0054_1DD0;
    pub const RQ_DRAW_POLYGON: u32 = 0x0054_1E50;
    pub const RQ_DRAW_SCALED: u32 = 0x0054_1ED0;
    pub const RQ_DRAW_RECT: u32 = 0x0054_1F40;
    pub const RQ_DRAW_SPRITE_GLOBAL: u32 = 0x0054_1FE0;
    pub const RQ_DRAW_SPRITE_LOCAL: u32 = 0x0054_2060;
    pub const RQ_DRAW_SPRITE_OFFSET: u32 = 0x0054_20E0;
    pub const RQ_DRAW_BITMAP_GLOBAL: u32 = 0x0054_2170;
    pub const RQ_DRAW_TEXTBOX_LOCAL: u32 = 0x0054_2200;
    pub const RQ_DRAW_CLIPPED_SPRITE_MAYBE: u32 = 0x0054_22A0;

    // RenderQueue helpers
    pub const RQ_CLIP_COORDINATES: u32 = 0x0054_2BA0;
    pub const RQ_GET_CAMERA_OFFSET_MAYBE: u32 = 0x0054_2B10;
    pub const RQ_CLIP_WITH_REF_OFFSET_MAYBE: u32 = 0x0054_2C70;
    pub const RQ_TRANSFORM_WITH_ZOOM_MAYBE: u32 = 0x0054_2D50;
    pub const RQ_SMOOTH_INTERPOLATE_MAYBE: u32 = 0x0054_2E60;
    pub const RQ_UPDATE_CLIP_BOUNDS_MAYBE: u32 = 0x0054_2F10;
    pub const RQ_SATURATE_CLIP_BOUNDS_MAYBE: u32 = 0x0054_2F70;

    // Render pipeline
    pub const RENDER_FRAME_MAYBE: u32 = 0x0056_E040;
    pub const GAME_RENDER_MAYBE: u32 = 0x0053_3DC0;
    pub const RENDER_TERRAIN_MAYBE: u32 = 0x0053_5000;
    pub const RENDER_HUD_MAYBE: u32 = 0x0053_4F20;
    pub const RENDER_TURN_STATUS_MAYBE: u32 = 0x0053_4E00;
    pub const PALETTE_MANAGE_MAYBE: u32 = 0x0053_3C80;
    pub const PALETTE_ANIMATE_MAYBE: u32 = 0x0053_3A80;
    pub const LOAD_SPRITE: u32 = 0x0052_3400;
    pub const CONSTRUCT_OPENGL_CPU: u32 = 0x005A_0850;
    pub const OPENGL_INIT: u32 = 0x0059_F000;
    pub const DDGAME_INIT_FIELDS: u32 = 0x0052_6120;
    pub const GFX_HANDLER_LOAD_DIR: u32 = 0x0056_63E0;
    pub const GFX_DIR_FIND_ENTRY: u32 = 0x0056_6520;
    pub const GFX_DIR_LOAD_IMAGE: u32 = 0x0056_66D0;

    // === Higher-level drawing functions ===

    /// DrawBungeeTrail — stdcall(task_ptr, style, fill), RET 0xC.
    /// Draws bungee drop trajectory using DrawSpriteLocal + DrawPolygon/DrawLineStrip.
    /// Triggered by weapon check (field_0x30==4, field_0x34==7 → Bungee) in FUN_00519F60.
    /// Gated by task+0xBC flag set by InitWormTrail (0x5008D0).
    pub const DRAW_BUNGEE_TRAIL: u32 = 0x0050_0720;
    /// DrawCrosshairLine — usercall(EDI=task_ptr), plain RET.
    /// Draws the crosshair aiming line using DrawPolygon + DrawSpriteLocal.
    pub const DRAW_CROSSHAIR_LINE: u32 = 0x0051_97D0;

    // === Sprite system ===

    /// ConstructSprite — usercall EAX=sprite_ptr, ECX=context_ptr.
    /// Initializes 0x70-byte sprite struct, sets vtable to 0x66418C.
    pub const CONSTRUCT_SPRITE: u32 = 0x004F_AA30;
    /// Sprite destructor — thiscall, vtable slot 0.
    pub const DESTROY_SPRITE: u32 = 0x004F_AA80;
    /// LoadSpriteFromVfs — usercall EAX=filename, ECX=file_archive, 2 stack params
    /// (sprite_ptr, gfx_dir). Reads .spr data from archive via GfxDir.
    pub const LOAD_SPRITE_FROM_VFS: u32 = 0x004F_AAF0;
    /// ProcessSprite — usercall EAX=sprite_ptr, 1 stack param (raw_data_ptr).
    /// Parses .spr binary format: palette, frames, bitmap data.
    pub const PROCESS_SPRITE: u32 = 0x004F_AB80;

    // === Landscape ===

    pub const CONSTRUCT_PC_LANDSCAPE: u32 = 0x0057_ACB0;
    pub const DESTRUCT_PC_LANDSCAPE: u32 = 0x0057_B540;
    pub const CONSTRUCT_SPRITE_REGION: u32 = 0x0057_DB20;
    pub const REDRAW_LAND_REGION: u32 = 0x0057_CC10;
    pub const WRITE_LAND_RAW: u32 = 0x0057_C300;
    /// Applies explosion crater to terrain — destroys pixels + collision (vtable slot 2).
    pub const PC_LANDSCAPE_APPLY_EXPLOSION: u32 = 0x0057_C820;
    /// Draws 8px checkered borders at landscape edges (vtable slot 6).
    pub const PC_LANDSCAPE_DRAW_BORDERS: u32 = 0x0057_D7F0;
    /// Redraws a single terrain row (vtable slot 8).
    pub const PC_LANDSCAPE_REDRAW_ROW: u32 = 0x0057_CF60;
    /// Clips and merges dirty rectangles for terrain redraw.
    pub const PC_LANDSCAPE_CLIP_AND_MERGE: u32 = 0x0057_D2B0;

    // === Sound ===

    /// DSSound__Constructor — usercall(EAX=this), plain RET. Inits vtable + zero fields.
    pub const CONSTRUCT_DS_SOUND: u32 = 0x0057_3D50;
    /// FUN_00573E50 — usercall(EAX=dssound) + cdecl(out_at_0x10, out_at_0x0C), plain RET.
    /// Sets up primary DirectSound buffer after DirectSoundCreate.
    pub const DSSOUND_INIT_BUFFERS: u32 = 0x0057_3E50;
    /// DirectSoundCreate IAT thunk → dsound.dll. stdcall(pGuid, ppDS, pUnkOuter).
    pub const DIRECTSOUND_CREATE: u32 = 0x005B_493E;
    pub const PLAY_SOUND_LOCAL: u32 = 0x004F_DFE0;
    pub const PLAY_SOUND_GLOBAL: u32 = 0x0054_6E20;

    // --- Sound queue dispatch (bridge: queue → DSSound) ---

    /// IsSoundSuppressed — checks mute flag + frame counter.
    /// fastcall(ECX=ddgame). Returns 0 if sound OK, 1 if suppressed.
    pub const IS_SOUND_SUPPRESSED: u32 = 0x0052_61E0;

    /// DispatchGlobalSound — suppression check + DSSound vtable slot 3 (play_sound).
    /// fastcall(ECX=?, EDX=task_turn_game) + 4 stack params (sound_id, flags, volume, pitch).
    pub const DISPATCH_GLOBAL_SOUND: u32 = 0x0052_6270;

    /// RecordActiveSound — inserts into 64-entry ring buffer (ActiveSoundTable).
    /// usercall(EAX=table, ESI=emitter) + 4 stack(pos_x, pos_y, volume, channel_handle).
    pub const RECORD_ACTIVE_SOUND: u32 = 0x0054_6260;

    /// ComputeDistanceParams — computes volume/pan from world position.
    /// fastcall(ECX=out_pan, EDX=out_volume) + 3 stack(ddgame_offset, pos_x, pos_y).
    /// If GameInfo+0xF38C == 0, returns volume=0x10000, pan=0.
    pub const COMPUTE_DISTANCE_PARAMS: u32 = 0x0054_6300;

    /// DispatchLocalSound — core: distance attenuation + DSSound slot 4 + RecordActiveSound.
    /// usercall(EAX=base_volume, EDI=ddgame_offset) + 4 stack(sound_id, flags, pos_x, pos_y).
    pub const DISPATCH_LOCAL_SOUND: u32 = 0x0054_6360;

    /// PlayLocalNoEmitter — thin wrapper → DispatchLocalSound.
    /// thiscall(ECX=?) + 3 stack(sound_id, flags, pitch).
    pub const PLAY_LOCAL_NO_EMITTER: u32 = 0x0054_6430;

    /// PlayLocalWithEmitter — gets pos from emitter vtable, then DispatchLocalSound.
    /// usercall(ESI=emitter) + stack params.
    pub const PLAY_LOCAL_WITH_EMITTER: u32 = 0x0054_63F0;

    /// PlaySoundPooled_Direct — bypasses queue, same suppression + DSSound slot 4.
    /// fastcall(ECX=?, EDX=task) + 3 stack params.
    pub const PLAY_SOUND_POOLED_DIRECT: u32 = 0x0054_6B50;

    /// Distance3D_Attenuation — elliptical distance model for positional audio.
    /// usercall(EAX=camera_pos_ptr) + 6 stack params. Returns volume + pan via out ptrs.
    pub const DISTANCE_3D_ATTENUATION: u32 = 0x0054_30F0;

    // === Speech / Voice Lines ===

    /// Speech line table in .rdata: array of {u32 id, *const u8 name},
    /// 61 entries + null terminator. Maps speech line IDs to WAV filenames.
    pub const SPEECH_LINE_TABLE: u32 = 0x006A_F770;

    /// Iterates teams, calls LoadSpeechBank for each. Clears DDGameWrapper+0x488→DDGame+0x77E4
    /// speech slot table. usercall(ESI=DDGameWrapper), plain RET.
    pub const DSSOUND_LOAD_ALL_SPEECH_BANKS: u32 = 0x0057_1A70;

    /// Iterates speech line table, calls LoadSpeechWAV per entry.
    /// usercall(EAX=DDGameWrapper) + 3 stack(team_index, speech_base_path, speech_dir), RET 0xC.
    pub const DSSOUND_LOAD_SPEECH_BANK: u32 = 0x0057_1660;

    /// DDGameWrapper__LoadSpeechWAV — loads a single speech WAV.
    /// Searches DDGameWrapper's name table for existing buffer, reuses if found.
    /// Otherwise calls DSSound vtable slot 12 (load_wav) to create new buffer.
    /// usercall(ESI=DDGameWrapper) + 4 stack(team_index, line_id, wav_path, full_path), RET 0x10.
    /// Returns 1 on success, 0 on failure.
    pub const DDGAMEWRAPPER_LOAD_SPEECH_WAV: u32 = 0x0057_1530;

    /// Loads all 126 SFX WAVs from data\wav\Effects\.
    /// stdcall(DDGame), RET 0x4.
    pub const DSSOUND_LOAD_EFFECT_WAVS: u32 = 0x0057_14B0;

    // === WAV Player ===

    /// Opens WAV via mmioOpenA, parses RIFF chunks, creates IDirectSoundBuffer.
    /// usercall(ESI=result_out) + stack(player, path, 0), RET 0xC.
    pub const WAV_PLAYER_LOAD_AND_PLAY: u32 = 0x0059_9B40;

    /// Calls IDirectSoundBuffer::Play on loaded buffer.
    /// usercall(EDI=result_out) + stack(player, volume), RET 0x8.
    pub const WAV_PLAYER_PLAY: u32 = 0x0059_96E0;

    /// Stops and releases current DirectSound buffer.
    /// usercall(ESI=player, EDI=result_out), plain RET.
    pub const WAV_PLAYER_STOP: u32 = 0x0059_9670;

    // === Fanfare / FE SFX ===

    /// FeSfx WavPlayer global instance (used by PlayFeSfx).
    pub const FESFX_WAV_PLAYER: u32 = 0x006A_C888;

    /// Fanfare WavPlayer global instance (used by PlayFanfare_Default).
    pub const FANFARE_WAV_PLAYER: u32 = 0x006A_C890;

    /// WA data path string buffer (char[0x81]).
    pub const WA_DATA_PATH: u32 = 0x0088_E282;

    /// Team config fanfare name lookup — jump table with 49 cases (0-48).
    /// usercall(ECX=index_0based, EAX=output_buf), plain RET.
    /// Writes null-terminated country/config name into buffer at EAX.
    pub const GET_TEAM_CONFIG_NAME: u32 = 0x004A_62A0;

    /// Builds `\user\Fanfare\<name>.wav`, plays via WavPlayer.
    /// stdcall(team_config_index), RET 0x4.
    pub const PLAY_FANFARE_DEFAULT: u32 = 0x004D_7500;

    /// Loads fanfare WAV with fallback to PlayFanfare_Default.
    /// thiscall(ECX=name) + 2 stack params, RET 0x8.
    pub const PLAY_FANFARE: u32 = 0x004D_7630;

    /// Gets current team, calls PlayFanfare.
    /// usercall(EAX=index), plain RET.
    pub const PLAY_FANFARE_CURRENT_TEAM: u32 = 0x004D_78E0;

    /// Builds `fesfx\<name>.wav`, plays via WavPlayer.
    /// stdcall(sfx_name), RET 0x4.
    pub const PLAY_FE_SFX: u32 = 0x004D_7960;

    // === Input ===

    /// DDKeyboard::PollKeyboardState — drains WM_KEY messages, calls
    /// GetKeyboardState, normalizes to both key_state (+0x11C) and
    /// prev_state (+0x21C) buffers. stdcall(this), RET 0x4.
    pub const DDKEYBOARD_POLL_KEYBOARD_STATE: u32 = 0x0057_2290;

    // === MFC wrappers ===

    /// AfxCtxMessageBoxA — cdecl(hwnd, text, caption, flags) → int.
    /// MFC activation-context wrapper around user32!MessageBoxA.
    pub const AFXCTX_MESSAGEBOX_A: u32 = 0x005C_2055;
    /// CWormsApp::DoMessageBox — thiscall(this, text, type, help_context) → int.
    /// MFC virtual override that shows either a frontend dialog or a raw MessageBoxA.
    pub const CWORMSAPP_DO_MESSAGEBOX: u32 = 0x004E_B730;

    // === Chat / UI ===

    pub const SHOW_CHAT_MESSAGE: u32 = 0x0052_ACB0;
    pub const ON_CHAT_INPUT: u32 = 0x0052_B730;

    // === Frontend / menu screens ===

    /// Main navigation loop (CWinApp::Run override) — dispatches on g_CurrentScreen
    pub const FRONTEND_MAIN_NAVIGATION_LOOP: u32 = 0x004E_6440;
    pub const FRONTEND_CHANGE_SCREEN: u32 = 0x0044_7A20;
    /// Wraps DoModal: palette transition + custom DoModal
    pub const FRONTEND_DO_MODAL_WRAPPER: u32 = 0x0044_7960;
    pub const FRONTEND_FRAME_CONSTRUCTOR: u32 = 0x004E_CCA0;
    pub const FRONTEND_DIALOG_CONSTRUCTOR: u32 = 0x0044_6BA0;
    pub const FRONTEND_PALETTE_ANIMATION: u32 = 0x0042_2180;
    pub const FRONTEND_LOAD_TRANSITION_PAL: u32 = 0x0044_7AA0;
    pub const FRONTEND_PRE_TRANSITION_CLEANUP: u32 = 0x004E_4AE0;
    /// Post-screen cleanup: destroys previous dialog
    pub const FRONTEND_POST_SCREEN_CLEANUP: u32 = 0x004E_B450;
    pub const FRONTEND_ON_INITIAL_LOAD: u32 = 0x0042_9830;
    pub const FRONTEND_LAUNCH_SINGLE_PLAYER: u32 = 0x0044_1D80;
    pub const FRONTEND_ON_MULTIPLAYER: u32 = 0x0044_E850;
    pub const FRONTEND_ON_NETWORK: u32 = 0x0044_EC10;
    pub const FRONTEND_ON_MINIMIZE: u32 = 0x0048_6A10;
    pub const FRONTEND_ON_OPTIONS_ACCEPT: u32 = 0x0048_DAB0;
    pub const FRONTEND_ON_START_GAME: u32 = 0x004F_14A0;
    pub const CDIALOG_DO_MODAL_CUSTOM: u32 = 0x0040_FD60;
    /// Custom message pump (replaces MFC's RunModalLoop)
    pub const CDIALOG_CUSTOM_MSG_PUMP: u32 = 0x0040_FBE0;
    /// Per-frame idle handler: cursor, mouse, paint dispatch
    pub const FRONTEND_DIALOG_ON_IDLE: u32 = 0x0040_FF90;
    /// Traverses control tree, paints dirty controls
    pub const FRONTEND_DIALOG_PAINT_CONTROL_TREE: u32 = 0x0040_BF60;
    /// Renders background image for frontend dialog
    pub const FRONTEND_DIALOG_RENDER_BACKGROUND: u32 = 0x0040_4250;
    /// Surface blit operation (calls surface->vtable[11])
    pub const SURFACE_BLIT: u32 = 0x0040_3BF0;

    // === Frontend dialog constructors (from main navigation loop) ===

    pub const FRONTEND_DEATHMATCH_CTOR: u32 = 0x0044_0F40;
    pub const FRONTEND_LOCAL_MP_CTOR: u32 = 0x0049_C420;
    pub const FRONTEND_TRAINING_CTOR: u32 = 0x004E_0880;
    pub const FRONTEND_MISSIONS_CTOR: u32 = 0x0049_9190;
    pub const FRONTEND_POST_INIT_CTOR: u32 = 0x004C_91B0;
    pub const FRONTEND_MAIN_MENU_CTOR: u32 = 0x0048_66C0;
    pub const FRONTEND_SINGLE_PLAYER_CTOR: u32 = 0x004D_69F0;
    pub const FRONTEND_CAMPAIGN_A_CTOR: u32 = 0x004A_2B70;
    pub const FRONTEND_CAMPAIGN_B_CTOR: u32 = 0x004A_24D0;
    pub const FRONTEND_ADV_SETTINGS_CTOR: u32 = 0x0042_79E0;
    pub const FRONTEND_INTRO_MOVIE_CTOR: u32 = 0x0047_0870;
    pub const FRONTEND_NETWORK_HOST_CTOR: u32 = 0x004A_DCA0;
    pub const FRONTEND_NETWORK_ONLINE_CTOR: u32 = 0x004A_CBC0;
    pub const FRONTEND_NETWORK_PROVIDER_CTOR: u32 = 0x004A_7990;
    pub const FRONTEND_NETWORK_SETTINGS_CTOR: u32 = 0x004C_23C0;
    pub const FRONTEND_LAN_CTOR: u32 = 0x0048_0A80;
    pub const FRONTEND_WORMNET_CTOR: u32 = 0x0047_2400;
    pub const FRONTEND_LOBBY_HOST_CTOR: u32 = 0x004B_0160;
    pub const FRONTEND_LOBBY_GAME_START_CTOR: u32 = 0x004B_DBE0;

    // === Scheme file operations ===

    /// Reads .wsc file into scheme struct: stdcall(dest, path, flag, out_ptr), RET 0x10
    /// Opens modeRead|typeBinary, reads header + version-dependent payload to dest+0x14
    pub const SCHEME_READ_FILE: u32 = 0x004D_3890;
    /// Checks if scheme file exists: stdcall(name) → 0=not found, 1=found, RET 0x4
    pub const SCHEME_FILE_EXISTS: u32 = 0x004D_4CD0;
    /// Saves scheme struct to .wsc file: thiscall(this, name, flag), RET 0x8
    pub const SCHEME_SAVE_FILE: u32 = 0x004D_44F0;
    /// Variant file-exists check for numbered schemes ({%02d} Name.wsc)
    pub const SCHEME_FILE_EXISTS_NUMBERED: u32 = 0x004D_4E00;
    /// Version detection: compares patterns to determine v1/v2/v3
    pub const SCHEME_DETECT_VERSION: u32 = 0x004D_4480;
    /// Extracts built-in schemes from PE resources to User\Schemes\ directory
    pub const SCHEME_EXTRACT_BUILTINS: u32 = 0x004D_5720;
    /// Copies payload data + V3 defaults into scheme struct: fastcall(?, data, dest, name)
    pub const SCHEME_INIT_FROM_DATA: u32 = 0x004D_5020;
    /// Validates weapon ammo counts against max table (39 weapons): returns 0=ok, 1=over limit
    pub const SCHEME_CHECK_WEAPON_LIMITS: u32 = 0x004D_50E0;
    /// Validates V3 extended options field ranges: returns 0=valid, 1=invalid
    pub const SCHEME_VALIDATE_EXTENDED_OPTIONS: u32 = 0x004D_5110;
    /// Scans User\Schemes\ for {NN} name.wsc files, marks found indices in global array
    pub const SCHEME_SCAN_DIRECTORY: u32 = 0x004D_54E0;
    /// Slot 13 feature/availability check — returns bool (obfuscated hash check)
    pub const SCHEME_SLOT13_CHECK: u32 = 0x004D_A4C0;

    // === Scheme data (in .rdata/.data) ===

    /// V3 extended options defaults (110 bytes), applied to V1/V2 schemes
    pub const SCHEME_V3_DEFAULTS: u32 = 0x0064_9AB8;
    /// Per-weapon max ammo table (39 bytes), used by CheckWeaponLimits
    pub const SCHEME_WEAPON_AMMO_LIMITS: u32 = 0x006A_D130;
    /// Weapon power byte in active scheme struct, stride 4 per weapon (39 weapons)
    /// Used by CheckWeaponLimits: compares each byte against SCHEME_WEAPON_AMMO_LIMITS
    pub const SCHEME_ACTIVE_WEAPON_DATA: u32 = 0x0088_DB05;
    /// Scheme slot presence flags (14 bytes: index 0 unused, 1-13 = slots), set by ScanDirectory
    pub const SCHEME_SLOT_FLAGS: u32 = 0x006B_329C;
    /// Guard global for gameplay modifier application in ReadFile (nonzero = apply modifiers)
    pub const SCHEME_MODIFIER_GUARD: u32 = 0x0088_E460;

    // === Configuration / registry ===

    /// Theme file size check: cdecl() -> u32 (file length or 0)
    pub const THEME_GET_FILE_SIZE: u32 = 0x0044_BA80;
    /// Theme file load: stdcall(dest_buffer)
    pub const THEME_LOAD: u32 = 0x0044_BB20;
    /// Theme file save: stdcall(buffer, size)
    pub const THEME_SAVE: u32 = 0x0044_BBC0;
    /// Recursive registry key deletion: stdcall(hkey, subkey) -> i32
    pub const REGISTRY_DELETE_KEY_RECURSIVE: u32 = 0x004E_4D10;
    /// Registry cleanup — deletes all 4 subsections + clears INI: stdcall(struct_ptr)
    pub const REGISTRY_CLEAN_ALL: u32 = 0x004C_90D0;
    /// Loads game options from registry into GameInfo struct: stdcall(game_info_ptr)
    pub const GAMEINFO_LOAD_OPTIONS: u32 = 0x0046_0AC0;
    /// Reads CrashReportURL from Options registry key: cdecl() -> *const u8
    pub const OPTIONS_GET_CRASH_REPORT_URL: u32 = 0x005A_63F0;

    // === MFC / ATL library functions ===

    /// ATL::CSimpleStringT<char,0>::operator= — thiscall(this, &src)
    /// Assigns one CString to another with refcount management.
    pub const CSTRING_OPERATOR_ASSIGN: u32 = 0x0040_1D20;
    /// String resource lookup + assign — stdcall(dest_cstring_ptr) with EDX=resource_id
    /// Looks up localized string by resource ID, assigns to dest CString.
    pub const CSTRING_ASSIGN_RESOURCE: u32 = 0x004A_39F0;
    /// CSimpleStringT::SetString — thiscall(this, str_ptr, str_len)
    /// Low-level string buffer assignment (called by operator= and assign_resource).
    pub const CSTRING_SET_STRING: u32 = 0x0040_1EA0;

    // === Lobby / network ===

    pub const LOBBY_HOST_COMMANDS: u32 = 0x004B_9B00;
    pub const LOBBY_CLIENT_COMMANDS: u32 = 0x004A_ABB0;
    /// Allocates space in a packet queue and writes the packet type + data.
    /// __usercall: EAX = queue pointer, ECX = data size. Stack: packet_type, data_ptr.
    /// Returns 1 on success, 0 if queue full. RET 0x8.
    pub const SEND_GAME_PACKET_WRAPPED: u32 = 0x0054_1130;
    pub const LOBBY_DISPLAY_MESSAGE: u32 = 0x0049_3CB0;
    pub const LOBBY_SEND_GREENTEXT: u32 = 0x004A_A990;
    pub const LOBBY_PRINT_USED_VERSION: u32 = 0x004B_7E20;
    pub const LOBBY_ON_DISCONNECT: u32 = 0x004B_AE40;
    pub const LOBBY_ON_GAME_END: u32 = 0x004B_AEC0;
    pub const LOBBY_ON_MESSAGE: u32 = 0x004B_D400;
    pub const LOBBY_DIALOG_CONSTRUCTOR: u32 = 0x004C_D9A0;
    pub const NETWORK_IS_AVAILABLE: u32 = 0x004D_4920;

    // === Memory ===

    /// WA internal malloc — cdecl(size: u32) → *mut u8. Statically-linked MSVC 2005 CRT.
    pub const WA_MALLOC: u32 = 0x005C_0AE3;
    pub const WA_MALLOC_MEMSET: u32 = 0x0053_E910;
    pub const WA_FREE: u32 = 0x005D_0D2B;

    // === Bitmap font system ===

    /// Loads all four font sizes from BMP files
    pub const FONT_LOAD_FONTS: u32 = 0x0041_4680;
    /// Core glyph renderer: iterates chars, blits from sprite sheet
    pub const FONT_RENDER_GLYPHS: u32 = 0x0041_43D0;
    /// Higher-level text draw: measures string, creates surface, renders
    pub const FONT_DRAW_TEXT: u32 = 0x0042_7830;
    /// DDDisplay::DrawTextOnBitmap — thiscall(font_id, bitmap, hAlign, vAlign, msg, a7, a8)
    pub const DDDISPLAY_DRAW_TEXT_ON_BITMAP: u32 = 0x0052_36B0;
    /// DDDisplay::ConstructTextbox — thiscall(dst, length, fontid)
    pub const DDDISPLAY_CONSTRUCT_TEXTBOX: u32 = 0x004F_AF00;
    /// SetTextboxText — stdcall(textbox, msg, textcolor, color1, color2, a6, a7, opacity)
    pub const SET_TEXTBOX_TEXT: u32 = 0x004F_B070;

    // === Global variables (in .data) ===

    /// Current screen ID driving the main navigation loop dispatch
    pub const G_CURRENT_SCREEN: u32 = 0x006B_3504;
    /// Character width table (256 bytes)
    pub const G_CHAR_WIDTH_TABLE: u32 = 0x006B_2DD9;
    /// Main frontend frame window (CWnd*)
    pub const G_FRONTEND_FRAME: u32 = 0x006B_3908;
    /// Main frontend HWND
    pub const G_FRONTEND_HWND: u32 = 0x006B_390C;
    /// Skip-to-main-menu flag (set after intro movie)
    pub const G_SKIP_TO_MAIN_MENU: u32 = 0x007A_083D;
    /// Auto-network mode flag
    pub const G_AUTO_NETWORK_FLAG: u32 = 0x007A_083F;
    /// DDDisplayWrapper pointer (valid during entire frontend lifetime)
    pub const G_DD_DISPLAY_WRAPPER: u32 = 0x0079_D6D4;
    /// Font array — 4 fonts, each 0x241C bytes
    pub const G_FONT_ARRAY: u32 = 0x007A_0F58;
    /// Main menu active flag (0xFF during screen 18)
    pub const G_MAIN_MENU_ACTIVE: u32 = 0x007C_0A20;
    /// MFC CWinApp singleton
    pub const G_CWINAPP: u32 = 0x007C_03D0;
    /// Network mode (0=LAN, nonzero=WormNET)
    pub const G_NETWORK_MODE: u32 = 0x007C_0D40;
    /// Network sub-type selector
    pub const G_NETWORK_SUBTYPE: u32 = 0x007C_0D68;
    /// Game session context pointer (contains subsystem pointers at known offsets)
    /// +0xA0 = DDGameWrapper*, +0xAC = DDDisplay*, +0xA8 = DSSound*, etc.
    pub const G_GAME_SESSION: u32 = 0x007A_0884;
    /// Fullscreen mode flag — non-zero when game is running fullscreen.
    /// Read by GameEngine__InitHardware for cursor/screen-center computation.
    pub const G_FULLSCREEN_FLAG: u32 = 0x007A_084C;
    /// Suppress-cursor flag — if non-zero, skip SetCursorPos/ClipCursor in hardware init.
    pub const G_SUPPRESS_CURSOR: u32 = 0x0088_E485;
    /// IAT thunk for MapWindowPoints (USER32.dll import).
    pub const IAT_MAP_WINDOW_POINTS: u32 = 0x0061_A588;
    /// Total sprite data bytes loaded (accumulated by ProcessSprite)
    pub const G_SPRITE_DATA_BYTES: u32 = 0x007A_0864;
    /// Total sprite frame count loaded
    pub const G_SPRITE_FRAME_COUNT: u32 = 0x007A_0868;
    /// Total sprite pixel area loaded (sum of frame w×h)
    pub const G_SPRITE_PIXEL_AREA: u32 = 0x007A_086C;
    /// Total palette entry bytes loaded (entry_count × 3)
    pub const G_SPRITE_PALETTE_BYTES: u32 = 0x007A_0870;

    // === Configuration globals (for GameInfo__LoadOptions) ===

    /// Base directory string (null-terminated)
    pub const G_BASE_DIR: u32 = 0x0088_E282;
    /// 64-byte data block copied into GameInfo+0xF485
    pub const G_GAMEINFO_BLOCK_F485: u32 = 0x0088_DFF3;
    /// "data\land.dat" string constant (14 bytes)
    pub const G_LAND_DAT_STRING: u32 = 0x0064_DA58;
    /// Unknown byte read into GameInfo+0xF3A0
    pub const G_CONFIG_BYTE_F3A0: u32 = 0x007C_0D38;
    /// 5 DWORDs: GameInfo offsets +0xF3B4..+0xF3D0
    pub const G_CONFIG_DWORDS_F3B4: u32 = 0x0088_E39C;
    /// Guard flag for conditional config copies
    pub const G_CONFIG_GUARD: u32 = 0x0088_C374;
    /// 4 DWORDs (conditional): GameInfo offsets +0xF3F4..+0xF400
    pub const G_CONFIG_DWORDS_F3F4: u32 = 0x0088_E3B8;
    /// DWORD → GameInfo+0xDAE8
    pub const G_CONFIG_DWORD_DAE8: u32 = 0x0088_E390;
    /// 2 DWORDs → GameInfo+0xF3D4, +0xF3D8
    pub const G_CONFIG_DWORDS_F3D4: u32 = 0x0088_E3B0;
    /// 3 DWORDs → GameInfo+0xF3C4..+0xF3CC
    pub const G_CONFIG_DWORDS_F3C4: u32 = 0x0088_E400;
    /// DWORD → GameInfo+0xF3E4
    pub const G_CONFIG_DWORD_F3E4: u32 = 0x0088_E44C;
    /// Streams directory path buffer
    pub const G_STREAMS_DIR: u32 = 0x0088_AE18;
    /// Random stream indices (16 entries)
    pub const G_STREAM_INDICES: u32 = 0x0088_AE9C;
    /// Stream index end sentinel
    pub const G_STREAM_INDICES_END: u32 = 0x0088_AEDC;
    /// DAT_0088E394 flag for stream volume
    pub const G_STREAM_FLAG: u32 = 0x0088_E394;
    /// Stream volume byte
    pub const G_STREAM_VOLUME: u32 = 0x0088_AEDD;

    /// CrashReportURL static buffer (0x400 bytes)
    pub const G_CRASH_REPORT_URL: u32 = 0x0079_FFD8;

    // === Frame buffer globals ===

    /// Frame buffer pixel data pointer (malloc'd width*height bytes)
    pub const G_FRAME_BUFFER_PTR: u32 = 0x007A_0EEC;
    /// Frame buffer width
    pub const G_FRAME_BUFFER_WIDTH: u32 = 0x007A_0EF0;
    /// Frame buffer height
    pub const G_FRAME_BUFFER_HEIGHT: u32 = 0x007A_0EF4;

    // === Trig lookup tables ===

    /// Sine lookup table — 1024 entries of i32 (fixed-point 16.16).
    /// Indexed by `(angle >> 6) & 0x3FF`. Adjacent entries used for interpolation.
    pub const G_SIN_TABLE: u32 = 0x006A_1860;
    /// Cosine lookup table — 1024 entries of i32 (fixed-point 16.16).
    pub const G_COS_TABLE: u32 = 0x006A_1C60;

    // === Vertex scratch buffer ===

    /// Global vertex scratch buffer used by DrawBungeeTrail/DrawCrosshairLine.
    /// 12 bytes per vertex (x, y, third field). At least ~200 vertices capacity.
    pub const G_VERTEX_SCRATCH_BUFFER: u32 = 0x008B_1470;

    // === Game context (DDGame struct offsets) ===
    // These are offsets from the DDGame base pointer, not absolute addresses.
    // DDGame pointer is obtained from hookConstructDDGameWrapper param.

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

    // === GameInfo struct offsets ===
    // These are offsets from the GameInfo pointer at DDGame+0x24
    // (DDGame.game_info). Access pattern: *(DDGame+0x24) + offset.

    pub mod game_info_offsets {
        // === Speech configuration ===

        /// Number of teams with speech enabled (byte). Used by LoadAllSpeechBanks.
        pub const SPEECH_TEAM_COUNT: u32 = 0x44C;
        /// Per-team speech config stride (0xC2 = 0x81 base path + 0x41 dir name).
        pub const SPEECH_TEAM_STRIDE: u32 = 0xC2;
        /// Offset to per-team speech base path (char[0x81]).
        /// Access: GameInfo + SPEECH_BASE_PATH + team_index * SPEECH_TEAM_STRIDE.
        pub const SPEECH_BASE_PATH: u32 = 0xF486;
        /// Offset to per-team speech directory name (char[0x41]).
        /// Access: GameInfo + SPEECH_DIR + team_index * SPEECH_TEAM_STRIDE.
        pub const SPEECH_DIR: u32 = 0xF507;
        /// Default speech base path (for fallback when team-specific WAV not found).
        pub const DEFAULT_SPEECH_BASE_PATH: u32 = 0xF3C4;
        /// Default speech directory name (for fallback).
        pub const DEFAULT_SPEECH_DIR: u32 = 0xF445;

        // === Replay configuration ===

        /// Replay state flag A — checked by TurnGame_HurryHandler and FrameFinish.
        /// Both DB08 and DB0A must be non-zero for replay-mode hurry path.
        pub const REPLAY_STATE_FLAG_A: u32 = 0xDB08;
        /// Replay state flag B — checked together with flag A for replay mode.
        pub const REPLAY_STATE_FLAG_B: u32 = 0xDB0A;
        /// Replay active flag — set to 1 by ReplayLoader when loading a .WAgame file.
        pub const REPLAY_ACTIVE: u32 = 0xDB48;
        /// Input replay file path (string buffer, 0x400 bytes).
        pub const REPLAY_INPUT_PATH: u32 = 0xDB60;
        /// Output replay file path (for recording, 0x400 bytes).
        pub const REPLAY_OUTPUT_PATH: u32 = 0xDF60;
    }
}
