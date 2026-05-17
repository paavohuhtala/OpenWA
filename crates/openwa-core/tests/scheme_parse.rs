use std::path::Path;

use deku::DekuContainerWrite;
use openwa_core::scheme::{
    Ammunition, ExtendedOptions, Scheme, SchemeVersion, StockpilingMode, SuddenDeathEvent,
    WormSelect,
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(FIXTURES).join(name)).unwrap()
}

fn canonical_payload_len() -> usize {
    Scheme::default().payload_bytes().len()
}

fn canonical_file_len() -> usize {
    Scheme::default().to_bytes().len()
}

fn assert_canonical_v3_roundtrip(scheme: &Scheme) {
    let written = scheme.to_bytes();
    assert!(written.starts_with(b"SCHM"));
    assert_eq!(written[4], SchemeVersion::V3 as u8);
    assert_eq!(written.len(), canonical_file_len());
    let reparsed = Scheme::try_from(written.as_slice()).unwrap();
    assert_eq!(reparsed, *scheme);
    assert_eq!(reparsed.to_bytes(), written);
}

#[test]
fn parse_beginner_v2() {
    let bytes = fixture_bytes("beginner.wsc");
    assert_eq!(bytes[4], SchemeVersion::V2 as u8);
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    assert_eq!(scheme.payload_bytes().len(), canonical_payload_len());
    assert_eq!(scheme.file_size(), canonical_file_len());
    assert_eq!(scheme.extended_options, ExtendedOptions::default());
    assert_canonical_v3_roundtrip(&scheme);
}

#[test]
fn parse_classic_v1() {
    let bytes = fixture_bytes("classic.wsc");
    assert_eq!(bytes[4], SchemeVersion::V1 as u8);
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    assert_eq!(scheme.payload_bytes().len(), canonical_payload_len());
    assert_eq!(scheme.file_size(), canonical_file_len());
    assert_eq!(scheme.super_weapons, Default::default());
    assert_eq!(scheme.extended_options, ExtendedOptions::default());
    assert_canonical_v3_roundtrip(&scheme);
}

#[test]
fn parse_shopping_v1() {
    let bytes = fixture_bytes("shopping.wsc");
    assert_eq!(bytes[4], SchemeVersion::V1 as u8);
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    assert_eq!(scheme.payload_bytes().len(), canonical_payload_len());
    assert_eq!(scheme.super_weapons, Default::default());
    assert_eq!(scheme.extended_options, ExtendedOptions::default());
    assert_canonical_v3_roundtrip(&scheme);
}

#[test]
fn canonical_roundtrip_beginner() {
    let original = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(original.as_slice()).unwrap();
    assert_canonical_v3_roundtrip(&scheme);
}

#[test]
fn flat_scheme_fields_beginner() {
    let original = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(original.as_slice()).unwrap();

    assert_eq!(scheme.weapons.bazooka.ammo, Ammunition::Infinite);
    assert_canonical_v3_roundtrip(&scheme);
}

#[test]
fn canonical_roundtrip_classic() {
    let original = fixture_bytes("classic.wsc");
    let scheme = Scheme::try_from(original.as_slice()).unwrap();
    assert_canonical_v3_roundtrip(&scheme);
}

#[test]
fn canonical_roundtrip_shopping() {
    let original = fixture_bytes("shopping.wsc");
    let scheme = Scheme::try_from(original.as_slice()).unwrap();
    assert_canonical_v3_roundtrip(&scheme);
}

// === Typed field accessor tests ===

#[test]
fn beginner_options() {
    let bytes = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    let opts = scheme.options;

    // Verified against hex dump of beginner.wsc payload bytes
    assert_eq!(opts.hot_seat_delay, 10); // 0x0A
    assert_eq!(opts.retreat_time, 5);
    assert_eq!(opts.rope_retreat_time, 5);
    assert!(opts.display_total_round_time);
    assert!(opts.automatic_replays);
    assert_eq!(opts.fall_damage, 0);
    assert!(!opts.artillery_mode);
    assert_eq!(opts.bounty_mode, 0);
    assert_eq!(opts.stockpiling, StockpilingMode::On);
    assert_eq!(opts.worm_select, WormSelect::On);
    assert_eq!(opts.sudden_death_event, SuddenDeathEvent::Nothing);
    assert_eq!(opts.worm_energy, 150); // 0x96
}

#[test]
fn classic_options() {
    let bytes = fixture_bytes("classic.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    let opts = scheme.options;

    // Verified against hex dump: 05 05 05 00 01 01 00 00 00 01 02 ...
    assert_eq!(opts.hot_seat_delay, 5);
    assert_eq!(opts.retreat_time, 5);
    assert_eq!(opts.rope_retreat_time, 5);
    assert!(!opts.display_total_round_time);
    assert!(opts.automatic_replays);
    assert_eq!(opts.fall_damage, 1);
    assert!(!opts.artillery_mode);
    assert_eq!(opts.stockpiling, StockpilingMode::Off);
    assert_eq!(opts.worm_select, WormSelect::On);
    assert_eq!(opts.worm_energy, 100); // hex: 0x64
}

#[test]
fn beginner_weapons() {
    let bytes = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();

    // Weapon 0 = Bazooka: at payload offset 36
    // hex bytes at file offset 0x29: 0A 03 00 00
    let bazooka = scheme.weapons.bazooka;
    assert_eq!(bazooka.ammo, Ammunition::Infinite);
    assert_eq!(bazooka.power, 3);
    assert_eq!(bazooka.delay, 0);
    assert_eq!(bazooka.crate_probability, 0);

    // V2 weapon: index 45 = Freeze, at payload offset 36 + 45*4 = 216
    // Should be available for V2 scheme
    let freeze = scheme.super_weapons.freeze;
    let _ = freeze.ammo;
}

#[test]
fn classic_weapons_v1_limits() {
    let bytes = fixture_bytes("classic.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();

    // Weapon 0 = Bazooka — classic has 10 ammo too
    let bazooka = scheme.weapons.bazooka;
    assert_eq!(bazooka.ammo, Ammunition::Infinite);

    // V1 files are canonicalized with a default V2 super weapon block.
    assert_eq!(scheme.super_weapons, Default::default());
}

#[test]
fn named_weapon_fields_cover_last_slots() {
    let bytes = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    let damage_x2 = scheme.weapons.damage_x2;
    let armageddon = scheme.super_weapons.armageddon;
    let _ = (damage_x2.ammo, armageddon.ammo);
}

#[test]
fn options_roundtrip() {
    let bytes = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    let opts = scheme.options;
    let serialized = opts.to_bytes().unwrap();
    let payload_bytes = scheme.payload_bytes();
    assert_eq!(&serialized[..], &payload_bytes[..serialized.len()]);
}

#[test]
fn extended_options_default_bytes_roundtrip() {
    let defaults = ExtendedOptions::default();
    assert_eq!(
        defaults.to_bytes().unwrap(),
        ExtendedOptions::default_bytes()
    );
}

#[test]
fn v1_no_extended_options() {
    let bytes = fixture_bytes("classic.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    assert_eq!(scheme.extended_options, ExtendedOptions::default());

    // But defaults should still work
    let defaults = ExtendedOptions::default();
    assert_eq!(defaults.data_version, 0);
}

#[test]
fn v2_super_weapons_zeroed_region() {
    // In the beginner.wsc V2 file, super weapons after the used ones should exist
    let bytes = fixture_bytes("beginner.wsc");
    let scheme = Scheme::try_from(bytes.as_slice()).unwrap();
    assert_eq!(scheme.extended_options, ExtendedOptions::default());

    // Armageddon is weapon index 63 (last V2 super weapon)
    let armageddon = scheme.super_weapons.armageddon;
    // It should be parseable (values may be zero)
    let _ = armageddon.ammo;
}

#[test]
fn serialization_always_writes_v3() {
    let mut scheme = Scheme::default();
    scheme.super_weapons.freeze.ammo = Ammunition::finite(7).unwrap();
    scheme.extended_options.double_time_stack_limit = 2;

    assert_eq!(scheme.payload_bytes().len(), canonical_payload_len());
    assert_eq!(scheme.to_bytes().len(), canonical_file_len());
    assert_eq!(scheme.to_bytes()[4], SchemeVersion::V3 as u8);
}

/// Parse all .wsc files from the game directory if available.
#[test]
fn parse_all_game_schemes() {
    let Some(wa_dir) = openwa_config::find_wa_dir() else {
        eprintln!("Skipping: WA installation not found");
        return;
    };
    let schemes_dir = wa_dir.join("User/Schemes");
    if !schemes_dir.exists() {
        eprintln!("Skipping: game schemes directory not found");
        return;
    }

    let mut count = 0;
    for entry in std::fs::read_dir(schemes_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "wsc") {
            let original = std::fs::read(&path).unwrap();
            let scheme = Scheme::try_from(original.as_slice())
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));
            assert_canonical_v3_roundtrip(&scheme);
            count += 1;
        }
    }
    eprintln!("Successfully parsed and canonicalized {count} scheme files");
}

// === ExtendedOptions validation tests ===

/// Build a valid extended-options buffer for byte-level validation tests.
fn make_valid_extended_options() -> Vec<u8> {
    ExtendedOptions::default_bytes().to_vec()
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
    b[0x6A] = 0; // must include at least one bit
    assert!(!ExtendedOptions::validate_bytes(&b));

    b[0x6A] = 8; // unknown bit
    assert!(!ExtendedOptions::validate_bytes(&b));

    b[0x6A] = 7; // all known bits
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
fn validate_rom_defaults_pass() {
    // ROM defaults (byte-exact from WA.exe at 0x649AB8) must pass validation.
    assert!(ExtendedOptions::validate_bytes(
        &ExtendedOptions::default_bytes()
    ));
}
