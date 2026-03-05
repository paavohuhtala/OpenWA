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
    /// CGameTask secondary vtable (at object offset 0xE8)
    pub const CGAMETASK_VTABLE2: u32 = 0x0066_9CF8;
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
    /// TaskStateMachine vtable
    pub const TASK_STATE_MACHINE_VTABLE: u32 = 0x0066_4118;
    /// OpenGLCPU vtable (0x48-byte object)
    pub const OPENGL_CPU_VTABLE: u32 = 0x0067_74C0;
    /// WaterEffect vtable (0xBC-byte object)
    pub const WATER_EFFECT_VTABLE: u32 = 0x0066_B268;

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
    pub const GET_AMMO: u32 = 0x0052_25E0;
    pub const ADD_AMMO: u32 = 0x0052_2640;
    pub const SUBTRACT_AMMO: u32 = 0x0052_2680;

    // === Graphics / rendering ===

    pub const CONSTRUCT_DD_GAME: u32 = 0x0056_E220;
    pub const CONSTRUCT_DD_GAME_WRAPPER: u32 = 0x0056_DEF0;
    pub const CONSTRUCT_DD_DISPLAY: u32 = 0x0056_9D00;
    pub const CONSTRUCT_FRAME_BUFFER: u32 = 0x005A_2430;
    pub const BLIT_SCREEN: u32 = 0x005A_2020;
    pub const RENDER_DRAWING_QUEUE: u32 = 0x0054_2350;
    pub const DRAW_LANDSCAPE: u32 = 0x005A_2790;
    pub const DRAW_SPRITE_GLOBAL: u32 = 0x0054_1FE0;
    pub const DRAW_BITMAP_GLOBAL: u32 = 0x0054_2170;
    pub const LOAD_SPRITE: u32 = 0x0052_3400;
    pub const CONSTRUCT_OPENGL_CPU: u32 = 0x005A_0850;
    pub const OPENGL_INIT: u32 = 0x0059_F000;
    pub const DDGAME_INIT_FIELDS: u32 = 0x0052_6120;
    pub const GFX_HANDLER_LOAD_DIR: u32 = 0x0056_63E0;
    pub const GFX_DIR_FIND_ENTRY: u32 = 0x0056_6520;
    pub const GFX_DIR_LOAD_IMAGE: u32 = 0x0056_66D0;

    // === Landscape ===

    pub const CONSTRUCT_PC_LANDSCAPE: u32 = 0x0057_ACB0;
    pub const CONSTRUCT_SPRITE_REGION: u32 = 0x0057_DB20;
    pub const REDRAW_LAND_REGION: u32 = 0x0057_CC10;
    pub const WRITE_LAND_RAW: u32 = 0x0057_C300;

    // === Sound ===

    pub const CONSTRUCT_DS_SOUND: u32 = 0x0057_3D50;
    pub const PLAY_SOUND_LOCAL: u32 = 0x004F_DFE0;
    pub const PLAY_SOUND_GLOBAL: u32 = 0x0054_6E20;

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

    // === Lobby / network ===

    pub const LOBBY_HOST_COMMANDS: u32 = 0x004B_9B00;
    pub const LOBBY_CLIENT_COMMANDS: u32 = 0x004A_ABB0;
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
    /// DrawTextboxLocal — draws textbox at screen position
    pub const DRAW_TEXTBOX_LOCAL: u32 = 0x0054_2200;

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
        /// Offset to weapon panel pointer
        pub const WEAPON_PANEL: u32 = 0x548;
    }
}
