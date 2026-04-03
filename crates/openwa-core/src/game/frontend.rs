/// Frontend screen/result IDs used by the navigation state machine.
///
/// The main navigation loop lives at 0x4E6440 (CWinApp::Run override).
/// It reads `g_CurrentScreen` (0x6B3504) and dispatches to a dialog via
/// DoModal. FrontendChangeScreen (0x447A20) calls EndDialog(screen_id),
/// which becomes the DoModal return value, stored back into g_CurrentScreen.
///
/// IDs fall into two categories:
/// - **Screen IDs** (10-30, 1700-1707): valid dispatch targets in the main loop,
///   each corresponding to a specific dialog constructor.
/// - **Result codes** (0-5, 32, 35-39, 50-58): returned by EndDialog but NOT
///   dispatched by the main loop. They either signal "cancel/back" (0), trigger
///   exit (unknown IDs hit default → exit), or are only meaningful within
///   sub-dialog contexts (e.g., button handlers that launch the game before
///   the main loop sees the code).
///
/// Source: Ghidra decompilation of Frontend__MainNavigationLoop (0x4E6440),
///         65 FrontendChangeScreen call sites, wkJellyWorm Lobby.cpp,
///         + runtime observation via openwa-dll
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ScreenId {
    // --- Dialog result codes (not main-loop dispatch targets) ---
    /// Cancel / return to previous screen (EndDialog result only)
    Cancel = 0,
    /// Splash / intro trigger (result code)
    Splash = 1,
    /// Network option result A
    NetworkOptionA = 2,
    /// Single player prep result
    SinglePlayerPrep = 4,
    /// Single player confirm result
    SinglePlayerConfirm = 5,

    // --- Valid main-loop screen IDs ---
    /// Deathmatch setup dialog (constructor 0x440F40)
    Deathmatch = 10,
    /// Local multiplayer setup dialog (constructor 0x49C420)
    LocalMultiplayer = 11,
    /// Training mode dialog (constructor 0x4E0880)
    Training = 12,
    /// Missions dialog (constructor 0x499190)
    Missions = 14,
    /// Exit frontend — cleans up and returns from CWinApp::Run
    Exit = 15,
    /// Post-init main menu (constructor 0x4C91B0)
    PostInitMenu = 16,
    /// Redirect to MainMenu (sets g_CurrentScreen = 18)
    MainMenuRedirect = 17,
    /// Main menu dialog (constructor 0x4866C0)
    MainMenu = 18,
    /// Single player game dialog (constructor 0x4D69F0)
    SinglePlayer = 21,
    /// Campaign selection A dialog (constructor 0x4A2B70)
    CampaignA = 22,
    /// Campaign selection B dialog (constructor 0x4A24D0)
    CampaignB = 23,
    /// Advanced settings dialog (constructor 0x4279E0)
    AdvancedSettings = 24,
    /// Intro/splash movie screen (constructor 0x470870, hardcodes next=18)
    IntroMovie = 25,
    /// Network host setup dialog (constructor 0x4ADCA0)
    NetworkHostSetup = 27,
    /// Network online setup dialog (constructor 0x4ACBC0)
    NetworkOnlineSetup = 28,
    /// Network dialog A (constructor 0x4BC970)
    NetworkDialogA = 29,
    /// Network dialog B (constructor 0x4BBC00)
    NetworkDialogB = 30,

    // --- Dialog result codes (not main-loop dispatch targets) ---
    //
    // These are returned by EndDialog from sub-dialogs (button handlers,
    // options screens, etc.). If they reach the main loop, they hit the
    // default case → exit. But by then the game has already launched or
    // the action has been taken.
    //
    // 35/36 and 37/38 form offline/online pairs:
    //   Start Game path: this+0x244 flag → 35 (no network) / 36 (network)
    //   Multiplayer path: this+0x140 flag → 37 (no network) / 38 (network)
    /// Scheme/weapon editor done — return from editor
    SchemeEditorDone = 32,
    /// Start game, network unavailable (offline/local path)
    StartGameOffline = 35,
    /// Start game, network available (online path)
    StartGameOnline = 36,
    /// Multiplayer, network unavailable (local play path)
    MultiplayerLocal = 37,
    /// Multiplayer, network available (online play path)
    MultiplayerOnline = 38,
    /// Options accepted — return screen after applying options + saving theme
    OptionsAccepted = 39,
    /// Sub-dialog result: in-game state A (hits default → exit)
    InGameA = 50,
    /// Sub-dialog result: in-game state B (hits default → exit)
    InGameB = 52,
    /// Sub-dialog result: in-game state C (hits default → exit)
    InGameC = 58,

    // --- Network/lobby screen IDs (high range, 0x6A4-0x6AB) ---
    /// Network provider selection dialog (constructor 0x4A7990)
    NetworkProviderSelect = 1700,
    /// Network settings dialog (constructor 0x4C23C0)
    NetworkSettings = 1701,
    /// LAN game dialog (constructor 0x480A80)
    Lan = 1702,
    /// WormNET online lobby dialog (constructor 0x472400)
    WormNet = 1703,
    /// Lobby host dialog (constructor 0x4B0160)
    LobbyHost = 1704,
    /// Lobby host continued (fall-through from 1704 setup)
    LobbyHostContinued = 1705,
    /// Lobby client / join game dialog
    LobbyClient = 1706,
    /// Lobby game start dialog (constructor 0x4BDBE0)
    LobbyGameStart = 1707,
}

impl TryFrom<i32> for ScreenId {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Cancel),
            1 => Ok(Self::Splash),
            2 => Ok(Self::NetworkOptionA),
            4 => Ok(Self::SinglePlayerPrep),
            5 => Ok(Self::SinglePlayerConfirm),
            10 => Ok(Self::Deathmatch),
            11 => Ok(Self::LocalMultiplayer),
            12 => Ok(Self::Training),
            14 => Ok(Self::Missions),
            15 => Ok(Self::Exit),
            16 => Ok(Self::PostInitMenu),
            17 => Ok(Self::MainMenuRedirect),
            18 => Ok(Self::MainMenu),
            21 => Ok(Self::SinglePlayer),
            22 => Ok(Self::CampaignA),
            23 => Ok(Self::CampaignB),
            24 => Ok(Self::AdvancedSettings),
            25 => Ok(Self::IntroMovie),
            27 => Ok(Self::NetworkHostSetup),
            28 => Ok(Self::NetworkOnlineSetup),
            29 => Ok(Self::NetworkDialogA),
            30 => Ok(Self::NetworkDialogB),
            32 => Ok(Self::SchemeEditorDone),
            35 => Ok(Self::StartGameOffline),
            36 => Ok(Self::StartGameOnline),
            37 => Ok(Self::MultiplayerLocal),
            38 => Ok(Self::MultiplayerOnline),
            39 => Ok(Self::OptionsAccepted),
            50 => Ok(Self::InGameA),
            52 => Ok(Self::InGameB),
            58 => Ok(Self::InGameC),
            1700 => Ok(Self::NetworkProviderSelect),
            1701 => Ok(Self::NetworkSettings),
            1702 => Ok(Self::Lan),
            1703 => Ok(Self::WormNet),
            1704 => Ok(Self::LobbyHost),
            1705 => Ok(Self::LobbyHostContinued),
            1706 => Ok(Self::LobbyClient),
            1707 => Ok(Self::LobbyGameStart),
            _ => Err(value),
        }
    }
}
