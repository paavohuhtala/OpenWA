pub mod class_type;
pub mod create_explosion;
pub mod frontend;
pub mod game_task_message;
pub mod init_weapon_table;
pub mod message;
pub mod missile_contact;
pub mod weapon;
pub mod weapon_data;
pub mod weapon_fire;
pub mod weapon_release;

pub use class_type::ClassType;
pub use frontend::ScreenId;
pub use message::EntityMessage;
pub use openwa_core::scheme;
pub use openwa_core::scheme::{
    EXTENDED_OPTIONS_DEFAULTS, EXTENDED_OPTIONS_OFFSET, EXTENDED_OPTIONS_SIZE, ExtendedOptions,
    OPTIONS_OFFSET, OPTIONS_SIZE, SCHEME_HEADER_SIZE, SCHEME_MAGIC, SCHEME_PAYLOAD_V1,
    SCHEME_PAYLOAD_V2, SCHEME_PAYLOAD_V3, SCHEME_WEAPON_ORDER, SchemeError, SchemeFile,
    SchemeFileError, SchemeOptions, SchemeVersion, WEAPON_ENTRY_SIZE, WEAPONS_TOTAL_COUNT,
    WEAPONS_V1_COUNT, WEAPONS_V1_OFFSET, WEAPONS_V2_COUNT, WEAPONS_V2_OFFSET, WeaponSettings,
};
pub use weapon::{KnownWeaponId, WeaponEntry, WeaponTable, check_weapon_avail, is_super_weapon};
