//! Recent-files tracking for the asset viewer.
//!
//! Persisted as one absolute path per line (UTF-8) in
//! `<data_dir>/OpenWA/recent_files.txt`. `<data_dir>` is resolved via
//! [`dirs::data_dir`]:
//!
//! - Windows: `%APPDATA%` (e.g. `C:\Users\<user>\AppData\Roaming`)
//! - Linux:   `$XDG_DATA_HOME` or `~/.local/share`
//! - macOS:   `~/Library/Application Support`
//!
//! The list is capped at [`MAX_RECENT`] entries, most-recent first.

use std::path::{Path, PathBuf};

pub const MAX_RECENT: usize = 10;

const APP_DIR: &str = "OpenWA";
const FILE_NAME: &str = "recent_files.txt";

fn recent_file_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(APP_DIR).join(FILE_NAME))
}

/// Load the recent-files list. Returns an empty vec on any I/O error
/// (missing file, unreadable dir, unwritable filesystem).
pub fn load() -> Vec<PathBuf> {
    let Some(path) = recent_file_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .take(MAX_RECENT)
        .collect()
}

/// Push `path` to the front of `list`, deduplicating and capping at
/// [`MAX_RECENT`]. Persists the result, silently ignoring I/O errors.
pub fn push(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT);
    let _ = save(list);
}

/// Clear the list and its on-disk backing file.
pub fn clear(list: &mut Vec<PathBuf>) {
    list.clear();
    let _ = save(list);
}

fn save(list: &[PathBuf]) -> std::io::Result<()> {
    let Some(path) = recent_file_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for p in list {
        text.push_str(&p.to_string_lossy());
        text.push('\n');
    }
    std::fs::write(&path, text)
}
