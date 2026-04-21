//! Typed stub for `DDGame::network_ecx` — the per-game network session
//! manager. Only the vtable slots used by the end-game state machine
//! (`OnGameState3`, `OnNetworkEndAwaitPeers`) are pinned down; the rest of
//! the layout is unknown. The vtable address is not yet in the registry
//! because multiple transport-specific subclasses may share the slot shape.

use crate::FieldRegistry;

/// Partial vtable for `NetSession`. Observed slots are the per-peer query
/// API used during end-of-round peer synchronisation.
#[openwa_game::vtable(size = 10, class = "NetSession")]
pub struct NetSessionVTable {
    /// slot 4 (+0x10): per-peer score / remaining-timeout. Caller takes
    /// `max()` over all active peers to decide whether to keep waiting.
    #[slot(4)]
    pub peer_score: fn(this: *mut NetSession, idx: u32) -> i32,
    /// slot 5 (+0x14): is a sync still in progress? Non-zero = keep waiting.
    #[slot(5)]
    pub sync_in_progress: fn(this: *mut NetSession) -> u32,
    /// slot 6 (+0x18): is peer `idx` still active/participating?
    #[slot(6)]
    pub peer_active: fn(this: *mut NetSession, idx: u32) -> u32,
    /// slot 9 (+0x24): per-peer "pending / not-yet-ready" query used by the
    /// online `ShouldInterpolate` branch (`FUN_0052D920`). Caller treats a
    /// non-zero return combined with `team_scoring_a[idx] > 0` as "this peer
    /// still hasn't caught up" and suppresses interp. Exact semantics
    /// (vs. slot 6's `peer_active`) are unconfirmed — name is a guess.
    #[slot(9)]
    pub peer_pending_maybe: fn(this: *mut NetSession, idx: u32) -> u32,
}

/// Partial layout of the object at `DDGame::network_ecx`.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct NetSession {
    /// +0x00: Vtable pointer.
    pub vtable: *const NetSessionVTable,
    /// +0x04: Unknown.
    pub _field_04: u32,
    /// +0x08: Number of peers in the session (loop bound for per-peer sweeps).
    pub peer_count: i32,
    /// +0x0C: Our own peer index — excluded from `max_peer_score_raw`.
    pub self_peer_idx: i32,
    // Trailing fields unknown.
}

impl NetSession {
    /// Rust port of `FUN_0053e720`.
    ///
    /// Iterates peers `0..peer_count`, calling `peer_active(i)` to filter
    /// and `peer_score(i)` to score. Returns the max score across active
    /// peers, excluding `self_peer_idx`. Non-positive scores don't raise
    /// the running max (initial value is 0).
    pub unsafe fn max_peer_score_raw(this: *mut NetSession) -> i32 {
        unsafe {
            let count = (*this).peer_count;
            let skip = (*this).self_peer_idx;
            let mut best: i32 = 0;
            for i in 0..count {
                if ((*(*this).vtable).peer_active)(this, i as u32) == 0 {
                    continue;
                }
                if i == skip {
                    continue;
                }
                let s = ((*(*this).vtable).peer_score)(this, i as u32);
                if s > best {
                    best = s;
                }
            }
            best
        }
    }
}
