//! Configuration and installation discovery for OpenWA tools.

use std::path::{Path, PathBuf};

use windows_sys::Win32::System::Registry::*;

const WA_REGISTRY_KEY: &[u8] = b"Software\\Team17SoftwareLTD\\WormsArmageddon\0";
const WA_PATH_VALUE: &[u8] = b"PATH\0";

/// Locate the Worms Armageddon installation directory.
///
/// Search order:
/// 1. `OPENWA_WA_PATH` environment variable — may be either the WA.exe path or
///    the installation directory. If it ends with `.exe`, the parent directory
///    is returned.
/// 2. Windows registry: `HKEY_CURRENT_USER\Software\Team17SoftwareLTD\WormsArmageddon`,
///    value `PATH`.
///
/// Returns `None` if the installation cannot be located.
pub fn find_wa_dir() -> Option<PathBuf> {
    if let Some(dir) = from_env() {
        return Some(dir);
    }
    from_registry()
}

fn from_env() -> Option<PathBuf> {
    let val = std::env::var_os("OPENWA_WA_PATH")?;
    let path = PathBuf::from(val);
    // Accept either a WA.exe path or a directory.
    if path
        .extension()
        .map(|e| e.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
    {
        path.parent().map(|p| p.to_path_buf())
    } else {
        Some(path)
    }
}

fn from_registry() -> Option<PathBuf> {
    let mut buf = [0u8; 512];
    let len = read_registry_sz(WA_REGISTRY_KEY, WA_PATH_VALUE, &mut buf)?;
    let s = std::str::from_utf8(&buf[..len]).ok()?;
    if s.is_empty() {
        return None;
    }
    Some(PathBuf::from(s))
}

/// `User/Schemes` directory under the installation, if it exists.
pub fn find_schemes_dir() -> Option<PathBuf> {
    let dir = find_wa_dir()?.join("User").join("Schemes");
    dir.is_dir().then_some(dir)
}

/// `User/Teams` directory under the installation, if it exists.
pub fn find_teams_dir() -> Option<PathBuf> {
    let dir = find_wa_dir()?.join("User").join("Teams");
    dir.is_dir().then_some(dir)
}

/// Discovered file entry: `(display_name, absolute_path)`. Display names
/// are the file's stem (no extension), suitable for dropdown UIs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveredFile {
    pub name: String,
    pub path: PathBuf,
}

/// Enumerate files in `dir` whose extension matches `ext`
/// (case-insensitive). Sorted alphabetically by display name.
///
/// Returns an empty `Vec` when the directory is missing or unreadable;
/// individual unreadable entries are skipped silently.
pub fn list_files_by_ext(dir: &Path, ext: &str) -> Vec<DiscoveredFile> {
    let mut out = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let matches_ext = path
            .extension()
            .map(|e| e.eq_ignore_ascii_case(ext))
            .unwrap_or(false);
        if !matches_ext {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_owned(),
            None => continue,
        };
        out.push(DiscoveredFile { name, path });
    }
    out.sort();
    out
}

/// Convenience: list every `.wsc` under `User/Schemes`.
pub fn list_schemes() -> Vec<DiscoveredFile> {
    find_schemes_dir()
        .map(|d| list_files_by_ext(&d, "wsc"))
        .unwrap_or_default()
}

/// Convenience: list every `.wgt` under `User/Teams`.
pub fn list_team_files() -> Vec<DiscoveredFile> {
    find_teams_dir()
        .map(|d| list_files_by_ext(&d, "wgt"))
        .unwrap_or_default()
}

/// Read a REG_SZ value from `HKCU\<key>`. Returns the number of bytes written
/// to `buf` (excluding null terminator), or `None` on any failure.
fn read_registry_sz(key: &[u8], value: &[u8], buf: &mut [u8]) -> Option<usize> {
    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let status = RegOpenKeyExA(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_READ, &mut hkey);
        if status != 0 {
            return None;
        }

        let mut data_size = buf.len() as u32;
        let mut value_type: u32 = 0;
        let status = RegQueryValueExA(
            hkey,
            value.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            buf.as_mut_ptr(),
            &mut data_size,
        );
        RegCloseKey(hkey);

        if status != 0 || value_type != REG_SZ || data_size == 0 {
            return None;
        }

        // data_size includes the null terminator
        Some((data_size - 1) as usize)
    }
}
