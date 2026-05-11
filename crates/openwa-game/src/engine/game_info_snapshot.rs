//! In-process snapshot of the [`GameInfo`] singleton.
//!
//! WA's MFC frontend builds up `GameInfo` field-by-field across the
//! menu-navigation flow (scheme picker → team picker → terrain picker →
//! offline-vs-online → Start). Replicating all of that from a custom UI is
//! a research task — but if the user launches a match through WA's normal
//! frontend once, we can capture the fully-populated `GameInfo` at the
//! entry to [`crate::wa::frontend::launch_game_session`] and reuse it.
//!
//! In-process capture keeps things simple:
//! - ASLR base is constant within the run, so addresses inside the
//!   snapshot remain valid for the duration of the process.
//! - The dialog handler that owns the Start button only writes
//!   scalars / inline buffers (scheme bytes, team records, paths) to
//!   `GameInfo` before calling launch — dynamic heap pointers are created
//!   later inside the launch path, not by the dialog.
//! - One known pointer field (`headless_log_stream` at 0xEF38) is null
//!   for normal headful gameplay, so the snapshot is effectively pure
//!   data.
//!
//! Workflow: open WA, navigate to "Start Offline Game", click Start to
//! kick off any match. The match-entry hook captures the bytes. Exit the
//! match back to the frontend. The custom-frontend Launch button now
//! restores those bytes before re-invoking launch.

use std::sync::Mutex;

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::rebase::rb;

const GAME_INFO_SIZE: usize = core::mem::size_of::<GameInfo>();

static SNAPSHOT: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Capture the current `GameInfo` bytes. Called from
/// [`crate::wa::frontend::launch_game_session`]'s entry. Idempotent — first
/// successful capture wins; later launches are skipped.
pub fn capture() {
    if let Ok(mut guard) = SNAPSHOT.lock() {
        if guard.is_some() {
            return;
        }
        unsafe {
            let src = rb(va::G_GAME_INFO) as *const u8;
            let mut buf = vec![0u8; GAME_INFO_SIZE];
            core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), GAME_INFO_SIZE);
            *guard = Some(buf);
        }
        let _ = openwa_core::log::log_line(&format!(
            "[gameinfo-snapshot] captured {GAME_INFO_SIZE} bytes"
        ));
    }
}

/// True once a snapshot has been captured.
pub fn is_captured() -> bool {
    SNAPSHOT.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Overwrite the live `GameInfo` with the previously captured bytes.
/// Returns `Ok(())` on success, `Err(reason)` if no snapshot exists.
///
/// The known-pointer field `headless_log_stream` is force-zeroed: it
/// pointed at a CRT FILE* that's been closed by the post-match cleanup,
/// and headless logging is off in our prototype anyway.
pub fn restore() -> Result<(), &'static str> {
    let guard = SNAPSHOT.lock().map_err(|_| "snapshot mutex poisoned")?;
    let bytes = guard.as_ref().ok_or("no snapshot captured yet")?;
    if bytes.len() != GAME_INFO_SIZE {
        return Err("snapshot size mismatch");
    }
    unsafe {
        let dst = rb(va::G_GAME_INFO) as *mut u8;
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, GAME_INFO_SIZE);
        let gi = rb(va::G_GAME_INFO) as *mut GameInfo;
        (*gi).headless_log_stream = core::ptr::null_mut();
    }
    Ok(())
}
