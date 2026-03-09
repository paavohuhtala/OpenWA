//! Typed handle for DDGameWrapper — the main game engine wrapper object.

use crate::ddgame::DDGame;
use crate::game_info::GameInfo;

/// Zero-cost handle to a DDGameWrapper instance (raw pointer as u32).
///
/// DDGameWrapper is the top-level game engine object (~28KB). It holds
/// the DDGame pointer, graphics handlers, landscape, and display state.
///
/// Pointer chain: DDGameWrapper → DDGame (+0x488) → GameInfo (+0x24).
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DDGameWrapperHandle(pub u32);

impl DDGameWrapperHandle {
    /// Wrap a raw DDGameWrapper pointer.
    pub unsafe fn from_raw(ptr: u32) -> Self {
        Self(ptr)
    }

    /// Read the DDGame pointer from DDGameWrapper+0x488.
    pub unsafe fn ddgame(self) -> *mut DDGame {
        *((self.0 + 0x488) as *const *mut DDGame)
    }

    /// Follow DDGameWrapper → DDGame(+0x488) → GameInfo(+0x24).
    ///
    /// GameInfo holds team configuration, speech paths, replay state, etc.
    /// Some fields are mapped in the struct; others require offset constants
    /// from `game_info_offsets`.
    pub unsafe fn game_info(self) -> *mut GameInfo {
        let ddgame = self.ddgame() as u32;
        *((ddgame + 0x24) as *const *mut GameInfo)
    }

    /// Pointer to the speech slot table (DDGame+0x77E4, 0x5A0 bytes).
    ///
    /// Maps (team_index, speech_line_id) → DSSound buffer index.
    pub unsafe fn speech_slot_table(self) -> *mut u8 {
        (self.ddgame() as u32 + 0x77E4) as *mut u8
    }
}
