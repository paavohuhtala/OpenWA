//! Selects which "match start" code path the current
//! [`crate::engine::init_session`] run was triggered from.
//!
//! WA's MFC frontend (LobbyDialog → Start handlers like
//! `FrontendLocalMP__OnStartMatch` at `0x004A1260`) commits lobby/scheme
//! pending-state globals into `GameInfo` before calling InitSession. A
//! custom launcher (`openwa-frontend`) doesn't run that flow — it builds
//! match config from its own UI and needs to populate the same downstream
//! state without going through MFC.
//!
//! This enum + the static slot below are the routing point: InitSession
//! reads the slot to know whether to source pending-state from WA's
//! globals or from a Rust-side config struct (TBD).
//!
//! ## Default
//!
//! Slot defaults to [`LaunchSource::Frontend`]. The WA Start path keeps
//! working unchanged because the WA InitSession hook trampoline never
//! sets the slot — it just inherits whatever's there.
//!
//! ## Custom-launcher usage
//!
//! Wrap the InitSession call with a [`LaunchSourceGuard`] so the slot
//! resets after the call:
//!
//! ```ignore
//! let _guard = LaunchSourceGuard::new(LaunchSource::CustomLauncher);
//! init_session(gi, type_label);
//! // guard drops here, slot reverts to Frontend
//! ```
//!
//! ## Thread safety
//!
//! `init_session` and the WA frontend Start handlers all run on WA's main
//! thread (MFC + DirectDraw affinity). The atomic slot is overkill for
//! single-threaded access but cheap; it also lets background threads
//! observe the current value if we ever need diagnostics.

use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LaunchSource {
    /// WA's MFC frontend Start handler. InitSession reads scheme/team
    /// pending state from the WA globals the LobbyDialog populates.
    Frontend = 0,
    /// `openwa-frontend` custom launcher. InitSession will eventually
    /// read pending state from a Rust-side config struct; for now this
    /// just tags the run (helpers are still bridged to WA originals
    /// that read the same globals as Frontend does).
    CustomLauncher = 1,
}

impl LaunchSource {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::CustomLauncher,
            _ => Self::Frontend,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(LaunchSource::Frontend as u8);

pub fn current() -> LaunchSource {
    LaunchSource::from_u8(CURRENT.load(Ordering::Acquire))
}

pub fn set(source: LaunchSource) {
    CURRENT.store(source as u8, Ordering::Release);
}

/// RAII guard that sets the launch source on construction and restores
/// the previous value on drop. Preferred over bare [`set`] because it
/// keeps the slot from leaking across an InitSession boundary if the
/// caller panics or returns early.
pub struct LaunchSourceGuard {
    prev: LaunchSource,
}

impl LaunchSourceGuard {
    pub fn new(source: LaunchSource) -> Self {
        let prev = current();
        set(source);
        Self { prev }
    }
}

impl Drop for LaunchSourceGuard {
    fn drop(&mut self) {
        set(self.prev);
    }
}
