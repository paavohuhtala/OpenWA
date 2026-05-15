use std::path::{Path, PathBuf};

use openwa_core::wgt::WgtFile;

/// Locate `testdata/assets/WG.WGT` relative to the workspace root.
fn fixture_path() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/openwa-core`; the fixture lives two
    // levels up under `testdata/assets`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
        .join("assets")
        .join("WG.WGT")
}

/// Parse the committed `testdata/assets/WG.WGT` fixture (a copy of the
/// retail user roster) and assert structural invariants.
#[test]
fn parse_fixture_wgt() {
    let path = fixture_path();
    let data =
        std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let wgt = WgtFile::from_bytes(&data)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
    assert_eq!(wgt.teams.len() as u8, data[5], "team count matches header");
    assert!(
        !wgt.teams.is_empty(),
        "fixture should contain at least one team"
    );
    for (i, t) in wgt.teams.iter().enumerate() {
        // Control byte is 0 (player) or 1..=5 (CPU). Anything else means
        // the parser has drifted from the per-team layout.
        assert!(
            t.control <= 5,
            "team {i} ({:?}) has invalid control byte {}",
            t.name_str(),
            t.control,
        );
        // Every team has a flag bitmap of the documented size.
        assert_eq!(t.flag.bitmap.len(), openwa_core::wgt::FLAG_BMP_LEN);
        // Custom-grave block presence must match the threshold rule.
        assert_eq!(
            t.custom_grave.is_some(),
            t.grave_id >= openwa_core::wgt::CUSTOM_GRAVE_THRESHOLD,
            "team {i} ({:?}): custom_grave/grave_id mismatch (grave_id={})",
            t.name_str(),
            t.grave_id,
        );
    }
}
