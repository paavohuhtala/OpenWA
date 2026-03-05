use std::path::Path;

use openwa_types::scheme::{SchemeFile, SchemeVersion, SCHEME_PAYLOAD_V1, SCHEME_PAYLOAD_V2};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

#[test]
fn parse_beginner_v2() {
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let scheme = SchemeFile::from_file(&path).expect("failed to parse beginner.wsc");
    assert_eq!(scheme.version, SchemeVersion::V2);
    assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V2);
    assert_eq!(scheme.file_size(), 297);
}

#[test]
fn parse_classic_v1() {
    let path = Path::new(FIXTURES).join("classic.wsc");
    let scheme = SchemeFile::from_file(&path).expect("failed to parse classic.wsc");
    assert_eq!(scheme.version, SchemeVersion::V1);
    assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V1);
    assert_eq!(scheme.file_size(), 221);
}

#[test]
fn parse_shopping_v1() {
    let path = Path::new(FIXTURES).join("shopping.wsc");
    let scheme = SchemeFile::from_file(&path).expect("failed to parse shopping.wsc");
    assert_eq!(scheme.version, SchemeVersion::V1);
    assert_eq!(scheme.payload.len(), SCHEME_PAYLOAD_V1);
}

#[test]
fn roundtrip_beginner() {
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let original = std::fs::read(&path).unwrap();
    let scheme = SchemeFile::from_bytes(&original).unwrap();
    assert_eq!(scheme.to_bytes(), original);
}

#[test]
fn roundtrip_classic() {
    let path = Path::new(FIXTURES).join("classic.wsc");
    let original = std::fs::read(&path).unwrap();
    let scheme = SchemeFile::from_bytes(&original).unwrap();
    assert_eq!(scheme.to_bytes(), original);
}

#[test]
fn roundtrip_shopping() {
    let path = Path::new(FIXTURES).join("shopping.wsc");
    let original = std::fs::read(&path).unwrap();
    let scheme = SchemeFile::from_bytes(&original).unwrap();
    assert_eq!(scheme.to_bytes(), original);
}

/// Parse all .wsc files from the game directory if available.
#[test]
fn parse_all_game_schemes() {
    let schemes_dir = Path::new("I:/games/SteamLibrary/steamapps/common/Worms Armageddon/User/Schemes");
    if !schemes_dir.exists() {
        eprintln!("Skipping: game schemes directory not found");
        return;
    }

    let mut count = 0;
    for entry in std::fs::read_dir(schemes_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "wsc") {
            let scheme = SchemeFile::from_file(&path)
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));

            // Roundtrip check
            let original = std::fs::read(&path).unwrap();
            assert_eq!(
                scheme.to_bytes(),
                original,
                "roundtrip failed for {}",
                path.display()
            );
            count += 1;
        }
    }
    eprintln!("Successfully parsed and round-tripped {count} scheme files");
}
