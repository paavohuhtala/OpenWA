//! Turn order display widget — the in-game team banner list with health bars.
//!
//! Three-level hierarchy of UI components:
//! - [`TurnOrderWidget`] — top level, groups teams by alliance
//! - [`TurnOrderAllianceGroup`] — per-alliance, holds teams sharing an alliance
//! - [`TurnOrderTeamEntry`] — per-team, renders banner + health bar
//!
//! All inherit from an animated item list base class (vtable 0x669C90) that
//! provides sin-table interpolated sliding transitions between items.

use crate::ddgame::DDGame;

/// Animated item list — base class for the turn order hierarchy.
///
/// Generic container with count/capacity/item array and animation state
/// for smooth sliding transitions between items using sin-table interpolation.
///
/// Vtable: 0x669C90. Constructor: 0x5457D0.
/// Size: 0x2C bytes (11 DWORDs).
#[repr(C)]
pub struct AnimatedItemList {
    /// 0x00: Vtable pointer.
    pub vtable: u32,
    /// 0x04: Number of items currently in the list.
    pub count: i32,
    /// 0x08: Maximum capacity (6 for turn order lists).
    pub capacity: i32,
    /// 0x0C: Pointer to allocated array of item pointers.
    pub items: *mut *mut u8,
    /// 0x10: Current active item index (-1 = none).
    pub current_index: i32,
    /// 0x14: Animation progress counter.
    pub animation_progress: i32,
    /// 0x18: Animation mode (0 = none, 1 = slide out, 2 = slide in).
    pub animation_mode: i32,
    /// 0x1C: Target item index for animation (-1 = none).
    pub animation_target: i32,
    /// 0x20: Animation offset.
    pub animation_offset: i32,
    /// 0x24: Unknown.
    pub _unknown_24: i32,
    /// 0x28: Initialized flag (set to 1 after setup).
    pub initialized: i32,
}

const _: () = assert!(core::mem::size_of::<AnimatedItemList>() == 0x2C);

/// Turn order widget — top-level UI component at DDGame+0x530.
///
/// Groups teams by alliance. Each alliance gets a [`TurnOrderAllianceGroup`].
/// Inherits animated sliding from [`AnimatedItemList`].
///
/// Vtable: 0x66A088. Constructor: 0x563D40 (stdcall, params: this, DDGame*).
/// Destructor: 0x563E90. Size: 0x4C bytes.
#[repr(C)]
pub struct TurnOrderWidget {
    /// 0x00-0x2B: AnimatedItemList base class.
    pub base: AnimatedItemList,
    /// 0x2C: Number of unique alliances.
    pub alliance_count: i32,
    /// 0x30-0x44: Alliance group pointers (up to 6).
    pub alliance_groups: [*mut TurnOrderAllianceGroup; 6],
    /// 0x48: DDGame pointer (back-reference).
    pub ddgame: *mut DDGame,
}

const _: () = assert!(core::mem::size_of::<TurnOrderWidget>() == 0x4C);

/// Turn order alliance group — per-alliance container.
///
/// Holds a list of teams that share the same alliance.
/// Calls [`AnimatedItemList`] constructor but repurposes base fields.
///
/// Vtable: 0x66A04C. Constructor: 0x563B50. Size: 0x30 bytes.
#[repr(C)]
pub struct TurnOrderAllianceGroup {
    /// 0x00: Vtable pointer (0x66A04C).
    pub vtable: u32,
    /// 0x04: Alliance ID.
    pub alliance_id: i32,
    /// 0x08: DDGame pointer.
    pub ddgame: *mut DDGame,
    /// 0x0C: Inner team list (AnimatedItemList holding TurnOrderTeamEntry items).
    pub team_list: *mut AnimatedItemList,
    /// 0x10-0x2F: Unknown (zeroed, not set by constructor).
    pub _unknown_10: [u8; 0x30 - 0x10],
}

const _: () = assert!(core::mem::size_of::<TurnOrderAllianceGroup>() == 0x30);

/// Turn order team entry — per-team banner with health bar.
///
/// Renders the team banner, name text, and health bar in the turn order panel.
/// The health bar width comes from DDGame.team_health_ratio[team_index]:
/// `bar_pixels = ratio * 100 >> 16 + 4`.
///
/// Vtable: 0x669FA8. Constructor: 0x5630B0. Render: 0x563620. Size: 0x24 bytes.
#[repr(C)]
pub struct TurnOrderTeamEntry {
    /// 0x00: Vtable pointer (0x669FA8).
    pub vtable: u32,
    /// 0x04: Team index (1-based).
    pub team_index: i32,
    /// 0x08: Color/style index (from game_info per-team data).
    pub color_index: i32,
    /// 0x0C: DDGame pointer (back-reference for accessing health ratios etc).
    pub ddgame: *mut DDGame,
    /// 0x10: Textbox object pointer (DDDisplay text renderer, 0x158 bytes).
    /// NULL when sound is disabled (headless mode).
    pub textbox: *mut u8,
    /// 0x14: DisplayGfx sprite for the health bar (vtable 0x664144).
    pub health_bar_sprite: *mut u8,
    /// 0x18: Cached bar width (pixels). Redrawn when this changes.
    pub cached_bar_width: i32,
    /// 0x1C: Cached flash state (eliminated team flashing).
    pub cached_flash_state: i32,
    /// 0x20: Bar height (pixels).
    pub bar_height: i32,
}

const _: () = assert!(core::mem::size_of::<TurnOrderTeamEntry>() == 0x24);
