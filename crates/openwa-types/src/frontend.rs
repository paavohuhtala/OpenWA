/// Frontend screen identifiers.
///
/// Used by FrontendChangeScreen (0x447A20) to transition between
/// menu screens. The screen ID is passed to CDialog::EndDialog as
/// the result code, then returned from DoModal to drive navigation.
///
/// The frontend uses MFC CDialog modal dialogs, not the CTask hierarchy.
/// The main frame window is stored at global 0x6B3908.
///
/// Source: Ghidra decompilation of 65 call sites + wkJellyWorm Lobby.cpp
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ScreenId {
    /// Return to previous screen / cancel action
    Cancel = 0,
    /// Main menu screen
    MainMenu = 10,
    /// Local multiplayer setup
    LocalMultiplayer = 11,
    /// Window minimized / inactive state (checked via IsIconic)
    Minimized = 15,
    /// Initial game loading screen
    InitialLoad = 16,
    /// Single player game (Deathmatch)
    SinglePlayer = 21,
    /// Network options sub-screen
    NetworkSubScreen = 26,
    /// Start game path A (game setup, param+0x244 == 0)
    StartGameA = 35,
    /// Start game path B (game setup fallthrough)
    StartGameB = 36,
    /// Network setup path A (param+0x140 == 0)
    NetworkSetupA = 37,
    /// Network setup path B (fallthrough)
    NetworkSetupB = 38,
    /// Options accepted / theme saved (writes current.thm)
    OptionsAccept = 39,
    /// Network provider selection screen
    NetworkSelection = 1700,
    /// LAN game
    Lan = 1702,
    /// WormNET online lobby
    WormNet = 1703,
    /// Lobby as host
    LobbyHost = 1704,
    /// Lobby as client / join game
    LobbyClient = 1706,
    /// Lobby game start
    LobbyGameStart = 1707,
}

impl TryFrom<i32> for ScreenId {
    type Error = i32;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Cancel),
            10 => Ok(Self::MainMenu),
            11 => Ok(Self::LocalMultiplayer),
            15 => Ok(Self::Minimized),
            16 => Ok(Self::InitialLoad),
            21 => Ok(Self::SinglePlayer),
            26 => Ok(Self::NetworkSubScreen),
            35 => Ok(Self::StartGameA),
            36 => Ok(Self::StartGameB),
            37 => Ok(Self::NetworkSetupA),
            38 => Ok(Self::NetworkSetupB),
            39 => Ok(Self::OptionsAccept),
            1700 => Ok(Self::NetworkSelection),
            1702 => Ok(Self::Lan),
            1703 => Ok(Self::WormNet),
            1704 => Ok(Self::LobbyHost),
            1706 => Ok(Self::LobbyClient),
            1707 => Ok(Self::LobbyGameStart),
            _ => Err(value),
        }
    }
}
