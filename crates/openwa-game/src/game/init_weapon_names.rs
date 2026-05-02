//! Rust port of WA's `InitWeaponNameStrings` (0x0053C130).
//!
//! Populates the `name1` / `name2` string-pointer pair on every
//! [`WeaponEntry`] in the active [`WeaponTable`]. Three phases:
//!
//! 1. Default every entry to the localized `ERROR_L` string — WA's
//!    "this weapon name is missing" placeholder.
//! 2. Overwrite entry 0's pair with the localized `NONE` string.
//! 3. For each weapon in [`WEAPON_NAMES`], resolve a
//!    `(short_name, long_name)` pair through
//!    [`resolve`](crate::wa::localized_template::resolve) and write
//!    them into `name2` and `name1` respectively.

use crate::engine::world::GameWorld;
use crate::wa::localized_template::resolve;
use crate::wa::string_resource::{StringRes, res, wa_load_string};

/// `(name2_token, name1_token)` per weapon for entries 1..=70. Each
/// pair is `(short, long)` form — WA's `GAME_<WEAPON>` is the short
/// name (panel/HUD), `GAME_LONG_<WEAPON>` is the full label used in
/// turn-status text.
const WEAPON_NAMES: [(StringRes, StringRes); 70] = [
    (res::GAME_BAZOOKA, res::GAME_LONG_BAZOOKA), // 1
    (res::GAME_HOMING_MISSILE, res::GAME_LONG_HOMING_MISSILE), // 2
    (res::GAME_MORTAR, res::GAME_LONG_MORTAR),   // 3
    (res::GAME_HOMING_PIGEON, res::GAME_LONG_HOMING_PIGEON), // 4
    (res::GAME_SHEEP_LAUNCHER, res::GAME_LONG_SHEEP_LAUNCHER), // 5
    (res::GAME_GRENADE, res::GAME_LONG_GRENADE), // 6
    (res::GAME_CLUSTER_BOMB, res::GAME_LONG_CLUSTER_BOMB), // 7
    (res::GAME_BANANA_BOMB, res::GAME_LONG_BANANA_BOMB), // 8
    (res::GAME_AXE, res::GAME_LONG_AXE),         // 9
    (res::GAME_EARTH_QUAKE, res::GAME_LONG_EARTH_QUAKE), // 10
    (res::GAME_SHOTGUN, res::GAME_LONG_SHOTGUN), // 11
    (res::GAME_HANDGUN, res::GAME_LONG_HANDGUN), // 12
    (res::GAME_UZI, res::GAME_LONG_UZI),         // 13
    (res::GAME_MINIGUN, res::GAME_LONG_MINIGUN), // 14
    (res::GAME_LONGBOW, res::GAME_LONG_LONGBOW), // 15
    (res::GAME_FIRE_PUNCH, res::GAME_LONG_FIRE_PUNCH), // 16
    (res::GAME_DRAGON_BALL, res::GAME_LONG_DRAGON_BALL), // 17
    (res::GAME_KAMIKAZE, res::GAME_LONG_KAMIKAZE), // 18
    (res::GAME_SUICIDE_BOMBER, res::GAME_LONG_SUICIDE_BOMBER), // 19
    (res::GAME_PROD, res::GAME_LONG_PROD),       // 20
    (res::GAME_DYNAMITE, res::GAME_LONG_DYNAMITE), // 21
    (res::GAME_MINE, res::GAME_LONG_MINE),       // 22
    (res::GAME_SHEEP, res::GAME_LONG_SHEEP),     // 23
    (res::GAME_SUPER_SHEEP, res::GAME_LONG_SUPER_SHEEP), // 24
    (res::GAME_AQUA_SHEEP, res::GAME_LONG_AQUA_SHEEP), // 25
    (res::GAME_MOLE_BOMB, res::GAME_LONG_MOLE_BOMB), // 26
    (res::GAME_AIR_STRIKE, res::GAME_LONG_AIR_STRIKE), // 27
    (res::GAME_NAPALM_STRIKE, res::GAME_LONG_NAPALM_STRIKE), // 28
    (res::GAME_POSTAL_STRIKE, res::GAME_LONG_POSTAL_STRIKE), // 29
    (res::GAME_MINE_STRIKE, res::GAME_LONG_MINE_STRIKE), // 30
    (res::GAME_MOLE_SQUADRON, res::GAME_LONG_MOLE_SQUADRON), // 31
    (res::GAME_BLOW_TORCH, res::GAME_LONG_BLOW_TORCH), // 32
    (res::GAME_PNEUMATIC_DRILL, res::GAME_LONG_PNEUMATIC_DRILL), // 33
    (res::GAME_GIRDER, res::GAME_LONG_GIRDER),   // 34
    (res::GAME_BASEBALL_BAT, res::GAME_LONG_BASEBALL_BAT), // 35
    (res::GAME_BRIDGEKIT, res::GAME_LONG_BRIDGEKIT), // 36
    (res::GAME_NINJA_ROPE, res::GAME_LONG_NINJA_ROPE), // 37
    (res::GAME_BUNGEE, res::GAME_LONG_BUNGEE),   // 38
    (res::GAME_PARACHUTE, res::GAME_LONG_PARACHUTE), // 39
    (res::GAME_TELEPORT, res::GAME_LONG_TELEPORT), // 40
    (res::GAME_SCALES, res::GAME_LONG_SCALES),   // 41
    (res::GAME_SUPER_BANANA, res::GAME_LONG_SUPER_BANANA), // 42
    (res::GAME_HOLY_GRENADE, res::GAME_LONG_HOLY_GRENADE), // 43
    (res::GAME_FLAME_THROWER, res::GAME_LONG_FLAME_THROWER), // 44
    (res::GAME_SALLY_ARMY, res::GAME_LONG_SALLY_ARMY), // 45
    (res::GAME_MB_BOMB, res::GAME_LONG_MB_BOMB), // 46
    (res::GAME_PETROL_BOMB, res::GAME_LONG_PETROL_BOMB), // 47
    (res::GAME_SKUNK, res::GAME_LONG_SKUNK),     // 48
    (res::GAME_MING_VASE, res::GAME_LONG_MING_VASE), // 49
    (res::GAME_SHEEP_STRIKE, res::GAME_LONG_SHEEP_STRIKE), // 50
    (res::GAME_CARPET_BOMB, res::GAME_LONG_CARPET_BOMB), // 51
    (res::GAME_MAD_COW, res::GAME_LONG_MAD_COW), // 52
    (res::GAME_OLD_WOMAN, res::GAME_LONG_OLD_WOMAN), // 53
    (res::GAME_DONKEY, res::GAME_LONG_DONKEY),   // 54
    (res::GAME_NUCLEAR_BOMB, res::GAME_LONG_NUCLEAR_BOMB), // 55
    (res::GAME_ARMAGEDDON, res::GAME_LONG_ARMAGEDDON), // 56
    (res::GAME_SKIP_GO, res::GAME_LONG_SKIP_GO), // 57
    (res::GAME_SURRENDER, res::GAME_LONG_SURRENDER), // 58
    (res::GAME_SELECT_WORM, res::GAME_LONG_SELECT_WORM), // 59
    (res::GAME_FREEZE, res::GAME_LONG_FREEZE),   // 60
    (res::GAME_MAGIC_BULLET, res::GAME_LONG_MAGIC_BULLET), // 61
    (res::GAME_JET_PACK, res::GAME_LONG_JET_PACK), // 62
    (res::GAME_LOWGRAVITY, res::GAME_LONG_LOWGRAVITY), // 63
    (res::GAME_FASTWALK, res::GAME_LONG_FASTWALK), // 64
    (res::GAME_LASERSIGHT, res::GAME_LONG_LASERSIGHT), // 65
    (res::GAME_INVISIBILITY, res::GAME_LONG_INVISIBILITY), // 66
    (res::GAME_DAMAGEX2, res::GAME_LONG_DAMAGEX2), // 67
    (res::GAME_CRATESPY, res::GAME_LONG_CRATESPY), // 68
    (res::GAME_DOUBLE_TIME, res::GAME_LONG_DOUBLE_TIME), // 69
    (res::GAME_CRATE_SHOWER, res::GAME_LONG_CRATE_SHOWER), // 70
];

/// Pure Rust port of WA's `InitWeaponNameStrings` (0x0053C130,
/// `__usercall(ESI = weapon_table, EDI = localized_template)`).
pub unsafe fn init_weapon_name_strings(world: *mut GameWorld) {
    unsafe {
        let table = (*world).weapon_table;
        let loc_ctx = (*world).localized_template;

        let placeholder = wa_load_string(res::ERROR_L);
        for entry in (*table).entries.iter_mut() {
            entry.name1 = placeholder;
            entry.name2 = placeholder;
        }

        let none = wa_load_string(res::NONE);
        (*table).entries[0].name1 = none;
        (*table).entries[0].name2 = none;

        for (i, &(short_name, long_name)) in WEAPON_NAMES.iter().enumerate() {
            let entry = &mut (*table).entries[i + 1];
            entry.name2 = resolve(loc_ctx, short_name);
            entry.name1 = resolve(loc_ctx, long_name);
        }
    }
}
