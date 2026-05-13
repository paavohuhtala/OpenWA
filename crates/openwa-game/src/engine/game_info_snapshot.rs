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
/// [`crate::wa::frontend::launch_game_session`]'s entry.
///
/// The in-memory snapshot (used by Snapshot-replay launches via [`restore`])
/// is first-wins: only the first capture per process populates the slot, so
/// re-launches don't overwrite a clean baseline the user wants to replay.
///
/// The disk dump always runs and overwrites, tagged `_rust` or `_wa`
/// depending on whether the current run's `InitSession` was the Rust port
/// or the WA original. That lets `gameinfo_dumps/launch_entry_{rust,wa}.bin`
/// stay current across multiple launches in the same process.
pub fn capture() {
    if let Ok(mut guard) = SNAPSHOT.lock() {
        if guard.is_none() {
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

    let tag = if crate::engine::init_session::RUST_INIT_SESSION_RAN
        .load(core::sync::atomic::Ordering::Relaxed)
    {
        "launch_entry_rust"
    } else {
        "launch_entry_wa"
    };
    match dump_to_disk(tag) {
        Ok(path) => {
            let _ = openwa_core::log::log_line(&format!(
                "[gameinfo-snapshot] auto-dumped to {}",
                path.display()
            ));
        }
        Err(e) => {
            let _ =
                openwa_core::log::log_line(&format!("[gameinfo-snapshot] auto-dump failed: {e}"));
        }
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

/// Dump the captured snapshot (if any) to disk under `gameinfo_dumps/`.
/// Returns `Err` if no snapshot has been captured yet.
pub fn dump_snapshot_to_disk(label: &str) -> std::io::Result<std::path::PathBuf> {
    let guard = SNAPSHOT
        .lock()
        .map_err(|_| std::io::Error::other("snapshot mutex poisoned"))?;
    let bytes = guard
        .as_ref()
        .ok_or_else(|| std::io::Error::other("no snapshot captured yet"))?;
    write_bytes_to_dump(label, bytes)
}

/// Dump the live `GameInfo` to disk as a binary + diffable hex sidecar.
///
/// Two files are written under `gameinfo_dumps/` (relative to the
/// process CWD — typically the game directory):
///
/// - `<label>.bin` — raw 0xF91C bytes
/// - `<label>.hex` — one line per 16 bytes formatted
///   `0xOFFSET: XX XX ... XX`, suitable for `diff` between dumps
///
/// `label` should be unique per dump (e.g. include a timestamp + a tag
/// describing what action you just took). Returns the binary path on
/// success.
pub fn dump_to_disk(label: &str) -> std::io::Result<std::path::PathBuf> {
    let mut buf = vec![0u8; GAME_INFO_SIZE];
    unsafe {
        let src = rb(va::G_GAME_INFO) as *const u8;
        core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), GAME_INFO_SIZE);
    }
    write_bytes_to_dump(label, &buf)
}

fn write_bytes_to_dump(label: &str, bytes: &[u8]) -> std::io::Result<std::path::PathBuf> {
    use std::fs;
    use std::io::Write;

    let dir = std::path::Path::new("gameinfo_dumps");
    fs::create_dir_all(dir)?;

    let bin_path = dir.join(format!("{label}.bin"));
    let hex_path = dir.join(format!("{label}.hex"));

    fs::write(&bin_path, bytes)?;

    let mut hex = fs::File::create(&hex_path)?;
    for (i, chunk) in bytes.chunks(16).enumerate() {
        write!(hex, "0x{:05X}:", i * 16)?;
        for b in chunk {
            write!(hex, " {b:02X}")?;
        }
        writeln!(hex)?;
    }

    let _ = openwa_core::log::log_line(&format!(
        "[gameinfo-snapshot] dumped to {} ({} bytes)",
        bin_path.display(),
        bytes.len()
    ));

    Ok(bin_path)
}
