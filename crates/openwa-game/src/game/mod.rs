pub mod class_type;
pub mod create_explosion;
pub mod frontend;
pub mod game_entity_message;
pub mod init_weapon_defaults;
pub mod init_weapon_names;
pub mod init_weapon_table;
pub mod message;
pub mod missile_contact;
pub mod weapon;
pub mod weapon_aim_flags;
pub mod weapon_fire;
pub mod weapon_release;

pub use class_type::ClassType;
pub use frontend::ScreenId;
pub use message::EntityMessage;
pub use openwa_core::scheme;
pub use openwa_core::scheme::{
    Ammunition, BlockRoofing, EXTENDED_OPTIONS_OFFSET, EXTENDED_OPTIONS_SIZE, ExtendedOptions,
    HealthCratesCurePoison, KeepControlAfterSkimming, OPTIONS_OFFSET, OPTIONS_SIZE, PhasedWorms,
    RopeRollDrops, RubberWormGravityType, RubberwormOptions, SCHEME_HEADER_SIZE, SCHEME_MAGIC,
    SCHEME_PAYLOAD_V1, SCHEME_PAYLOAD_V2, SCHEME_PAYLOAD_V3, STANDARD_WEAPONS_OFFSET,
    STANDARD_WEAPONS_SIZE, SUPER_WEAPONS_OFFSET, SUPER_WEAPONS_SIZE, Scheme, SchemeFileError,
    SchemeOptions, SchemeVersion, SheepHeavensGate, Skipwalking, StandardWeapons, StockpilingMode,
    SuddenDeathEvent, SuperWeapons, TriState, WEAPON_ENTRY_SIZE, WEAPONS_TOTAL_COUNT,
    WEAPONS_V1_COUNT, WEAPONS_V1_OFFSET, WEAPONS_V2_COUNT, WEAPONS_V2_OFFSET, WeaponSettings,
    WormSelect,
};
pub use weapon::{
    KnownWeaponId, WeaponEntry, WeaponTable, check_weapon_avail, is_animal, is_fire, is_modifier,
    is_sheep, is_super_weapon, is_utility,
};
