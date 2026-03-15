use std::path::Path;

use openwa_core::game::scheme::{
    ExtendedOptions, SchemeFile, SchemeVersion, EXTENDED_OPTIONS_DEFAULTS,
    EXTENDED_OPTIONS_SIZE, SCHEME_PAYLOAD_V1, SCHEME_PAYLOAD_V2, WEAPONS_V1_COUNT,
};

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

// === Typed field accessor tests ===

#[test]
fn beginner_options() {
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();
    let opts = scheme.options();

    // Verified against hex dump of beginner.wsc payload bytes
    assert_eq!(opts.hot_seat_delay, 10);   // 0x0A
    assert_eq!(opts.retreat_time, 5);
    assert_eq!(opts.rope_retreat_time, 5);
    assert_eq!(opts.display_total_round_time, true);
    assert_eq!(opts.automatic_replays, true);
    assert_eq!(opts.fall_damage, 0);
    assert_eq!(opts.artillery_mode, false);
    assert_eq!(opts.bounty_mode, 0);
    assert_eq!(opts.stockpiling, 1);    // On
    assert_eq!(opts.worm_select, 1);    // Manual
    assert_eq!(opts.sudden_death_event, 3); // Nothing
    assert_eq!(opts.worm_energy, 150);  // 0x96
}

#[test]
fn classic_options() {
    let path = Path::new(FIXTURES).join("classic.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();
    let opts = scheme.options();

    // Verified against hex dump: 05 05 05 00 01 01 00 00 00 01 02 ...
    assert_eq!(opts.hot_seat_delay, 5);
    assert_eq!(opts.retreat_time, 5);
    assert_eq!(opts.rope_retreat_time, 5);
    assert_eq!(opts.display_total_round_time, false);
    assert_eq!(opts.automatic_replays, true);
    assert_eq!(opts.fall_damage, 1);
    assert_eq!(opts.artillery_mode, false);
    assert_eq!(opts.stockpiling, 0);    // Off
    assert_eq!(opts.worm_select, 1);    // Manual
    assert_eq!(opts.worm_energy, 100);  // hex: 0x64
}

#[test]
fn beginner_weapons() {
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();

    // Weapon 0 = Bazooka: at payload offset 36
    // hex bytes at file offset 0x29: 0A 03 00 00
    let bazooka = scheme.weapon(0).unwrap();
    assert_eq!(bazooka.ammo, 10);
    assert_eq!(bazooka.power, 3);
    assert_eq!(bazooka.delay, 0);
    assert_eq!(bazooka.crate_probability, 0);

    // V2 weapon: index 45 = Freeze, at payload offset 36 + 45*4 = 216
    // Should be available for V2 scheme
    let freeze = scheme.weapon(45);
    assert!(freeze.is_some());

    // Total weapon count for V2
    assert_eq!(scheme.weapon_count(), 64);
}

#[test]
fn classic_weapons_v1_limits() {
    let path = Path::new(FIXTURES).join("classic.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();

    // V1 has 45 weapons
    assert_eq!(scheme.weapon_count(), WEAPONS_V1_COUNT);

    // Weapon 0 = Bazooka — classic has 10 ammo too
    let bazooka = scheme.weapon(0).unwrap();
    assert_eq!(bazooka.ammo, 10);

    // V2 super weapon index should return None for V1 scheme
    assert!(scheme.weapon(45).is_none());
    assert!(scheme.weapon(63).is_none());
}

#[test]
fn weapon_out_of_range() {
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();
    assert!(scheme.weapon(64).is_none());
    assert!(scheme.weapon(100).is_none());
}

#[test]
fn options_roundtrip() {
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();
    let opts = scheme.options();
    let serialized = opts.to_bytes();
    assert_eq!(&serialized[..], &scheme.payload[..36]);
}

#[test]
fn v1_no_extended_options() {
    let path = Path::new(FIXTURES).join("classic.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();
    assert!(scheme.extended_options().is_none());

    // But defaults should still work
    let defaults = scheme.extended_options_or_defaults();
    assert_eq!(defaults.data_version, 0);
}

#[test]
fn v2_super_weapons_zeroed_region() {
    // In the beginner.wsc V2 file, super weapons after the used ones should exist
    let path = Path::new(FIXTURES).join("beginner.wsc");
    let scheme = SchemeFile::from_file(&path).unwrap();

    // Armageddon is weapon index 63 (last V2 super weapon)
    let armageddon = scheme.weapon(63).unwrap();
    // It should be parseable (values may be zero)
    let _ = armageddon.ammo;
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

// === ExtendedOptions validation tests ===

/// Build a valid 110-byte extended options buffer from scratch.
/// ROM defaults contain intentionally invalid V3 values (0x80 in bool fields),
/// so we construct a minimal valid buffer instead.
fn make_valid_extended_options() -> [u8; EXTENDED_OPTIONS_SIZE] {
    let mut b = [0u8; EXTENDED_OPTIONS_SIZE];
    // data_version = 0 (already zero)
    // gravity must be >= 1
    b[0x08..0x0C].copy_from_slice(&1i32.to_le_bytes());
    // terrain_friction < 0x28CCD (leave at 0 — valid)
    // petrol_touch_decay must be nonzero
    b[0x31] = 1;
    // max_flamelet_count must be nonzero
    b[0x32..0x34].copy_from_slice(&1u16.to_le_bytes());
    // speeds must be positive (> 0)
    b[0x34..0x38].copy_from_slice(&1i32.to_le_bytes()); // max_projectile_speed
    b[0x38..0x3C].copy_from_slice(&1i32.to_le_bytes()); // max_rope_speed
    b[0x3C..0x40].copy_from_slice(&1i32.to_le_bytes()); // max_jet_pack_speed
    // game_engine_speed in [0x1000, 0x800000]
    b[0x40..0x44].copy_from_slice(&0x10000i32.to_le_bytes());
    // sheep_heavens_gate in [1, 7]
    b[0x6A] = 1;
    b
}

#[test]
fn validate_valid_extended_options() {
    let b = make_valid_extended_options();
    assert!(ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_bad_data_version() {
    let mut b = make_valid_extended_options();
    b[0x00] = 1; // data_version must be 0
    assert!(!ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_bad_gravity() {
    let mut b = make_valid_extended_options();
    // gravity = 0 (below minimum of 1)
    b[0x08..0x0C].copy_from_slice(&0i32.to_le_bytes());
    assert!(!ExtendedOptions::validate_bytes(&b));

    // gravity = negative
    b[0x08..0x0C].copy_from_slice(&(-1i32).to_le_bytes());
    assert!(!ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_bad_bool() {
    let mut b = make_valid_extended_options();
    // unrestrict_rope at offset 0x12: value 2 is not a valid bool
    b[0x12] = 2;
    assert!(!ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_bad_tristate() {
    let mut b = make_valid_extended_options();
    // explosions_push_all at offset 0x26: must be 0, 1, or 0x80
    b[0x26] = 2;
    assert!(!ExtendedOptions::validate_bytes(&b));

    // 0x80 should be valid
    b[0x26] = 0x80;
    assert!(ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_bad_skipwalking() {
    let mut b = make_valid_extended_options();
    // skipwalking at 0x49: must be -1, 0, or 1
    b[0x49] = 2u8; // 2 as i8 is invalid
    assert!(!ExtendedOptions::validate_bytes(&b));

    // -1 (0xFF) should be valid
    b[0x49] = 0xFF;
    assert!(ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_zero_petrol_touch_decay() {
    let mut b = make_valid_extended_options();
    b[0x31] = 0; // must be nonzero
    assert!(!ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_bad_sheep_heavens_gate() {
    let mut b = make_valid_extended_options();
    b[0x6A] = 0; // must be in [1, 7]
    assert!(!ExtendedOptions::validate_bytes(&b));

    b[0x6A] = 8; // too high
    assert!(!ExtendedOptions::validate_bytes(&b));

    b[0x6A] = 7; // max valid
    assert!(ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rejects_negative_speeds() {
    let mut b = make_valid_extended_options();
    // max_projectile_speed at 0x34: must be positive
    b[0x34..0x38].copy_from_slice(&(-1i32).to_le_bytes());
    assert!(!ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_game_engine_speed_range() {
    let mut b = make_valid_extended_options();

    // Too low (below 0x1000)
    b[0x40..0x44].copy_from_slice(&0xFFFi32.to_le_bytes());
    assert!(!ExtendedOptions::validate_bytes(&b));

    // Minimum valid (0x1000)
    b[0x40..0x44].copy_from_slice(&0x1000i32.to_le_bytes());
    assert!(ExtendedOptions::validate_bytes(&b));

    // Maximum valid (0x800000)
    b[0x40..0x44].copy_from_slice(&0x80_0000i32.to_le_bytes());
    assert!(ExtendedOptions::validate_bytes(&b));

    // Too high (0x800001)
    b[0x40..0x44].copy_from_slice(&0x80_0001i32.to_le_bytes());
    assert!(!ExtendedOptions::validate_bytes(&b));
}

#[test]
fn validate_rom_defaults_fail_sheep_gate() {
    // ROM defaults have sheep_heavens_gate=0, which is intentionally invalid.
    // WA uses defaults as a fallback when V3 validation fails, but never validates them.
    assert!(!ExtendedOptions::validate_bytes(&EXTENDED_OPTIONS_DEFAULTS));
}
