use core::ffi::c_char;

use openwa_core::fixed::Fixed;

pub use openwa_core::weapon::{
    FireMethod, FireType, KnownWeaponId, SpecialFireSubtype, WeaponId, is_super_weapon,
};

use crate::engine::world::GameWorld;

// ============================================================
// WeaponSpawnData — launch parameters passed to MissileEntity ctor
// ============================================================

/// Spawn parameters for a weapon projectile (0x2C = 44 bytes, 11 DWORDs).
///
/// Built on the stack by fire sub-functions (ProjectileFire, GrenadeFire, etc.)
/// and passed as param_4 to MissileEntity::Constructor. Copied verbatim into
/// MissileEntity at offset 0x130 (spawn_params field).
///
/// Source: runtime inspection via debug CLI + MissileEntity constructor decompilation.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct WeaponSpawnData {
    /// [0] Team index of the worm that fired this projectile.
    pub owner_id: u32,
    /// [1] Unknown — observed as 1.
    pub _unknown_04: u32,
    /// [2] Fixed16.16 X position at launch.
    pub spawn_x: Fixed,
    /// [3] Fixed16.16 Y position at launch.
    pub spawn_y: Fixed,
    /// [4] Fixed16.16 horizontal velocity. Copied to WorldEntity.speed_x.
    pub initial_speed_x: Fixed,
    /// [5] Fixed16.16 vertical velocity. Copied to WorldEntity.speed_y.
    pub initial_speed_y: Fixed,
    /// [6] Fixed16.16 aim cursor X at time of fire.
    pub cursor_x: Fixed,
    /// [7] Fixed16.16 aim cursor Y at time of fire.
    pub cursor_y: Fixed,
    /// [8] Index within a cluster volley (0 for single shot, N for Nth sub-pellet).
    /// Determines which half of weapon_data is copied to render_data.
    pub pellet_index: u32,
    /// [9] Fallback timer — copied to render_data[0x19] if that field was zero.
    pub fallback_timer: u32,
    /// [10] Fallback param — copied to render_data[0x11] if that field was zero.
    pub fallback_param: u32,
}
const _: () = assert!(core::mem::size_of::<WeaponSpawnData>() == 0x2C);

// ============================================================
// WeaponEntry — per-weapon data in the weapon table (0x1D0 bytes)
// ============================================================

/// 71 standard entries (indices 0..70), matching the `Weapon` enum.
/// Source: wkJellyWorm/src/CustomWeapons.h (WeaponStruct).
///
/// Known fields from wkJellyWorm and InitWeaponTable (0x53CAB0) analysis.
/// wkJellyWorm copies entire entries via memcpy when creating custom weapons.
#[repr(C)]
pub struct WeaponEntry {
    /// +0x00: Pointer to primary weapon name string.
    pub name1: *const c_char,
    /// +0x04: Pointer to secondary weapon name string.
    pub name2: *const c_char,
    /// +0x08: Panel state (init: 0xFFFFFFFF). wkJellyWorm calls this `panelRow`.
    pub panel_state: i32,
    /// +0x0C: Requires aiming (1 for aimed weapons like Bazooka, 0 for non-aimed like Earthquake).
    /// Runtime-observed: 1 for Bazooka/Grenade/Shotgun, 0 for AirStrike/HomingMissile/Teleport.
    pub requires_aiming: i32,
    /// +0x10: Weapon defined flag. Nonzero = weapon exists in table.
    /// Checked by GameWorld__CheckWeaponAvail to determine if weapon is valid.
    pub defined: i32,
    /// +0x14: Shot count per use (1 for most, 2 for Shotgun/Longbow, 5 for GirderPack).
    pub shot_count: i32,
    /// +0x18: Unknown flag (0 or 1). 0 for NinjaRope/Bungee/Parachute/SelectWorm/JetPack/powerups.
    pub unknown_18: i32,
    /// +0x1C: Retreat timer in ms (0xBB8=3000 for most, 0x1388=5000 for Dynamite/Mine/MingVase,
    /// -1 for non-retreating, -1000 for PneumaticDrill/Teleport, 0 for utility/powerups).
    pub retreat_time: i32,
    /// +0x20: Creates projectile flag (1 for weapons that fire a physical object, 0 for utility).
    /// 0 for Prod/Girder/NinjaRope/Parachute/Teleport/utility weapons.
    pub creates_projectile: i32,
    /// +0x24: Availability flag. Init: 0xFFFFFFFF, then set to 0 (unavailable)
    /// or 1 (available) per weapon. Weapon::None, SkipGo, Surrender default to 0.
    pub availability: i32,
    /// +0x28: Enabled flag (init: 1).
    pub enabled: i32,
    /// +0x2C-0x2F: Unknown.
    pub _unknown_2c: [u8; 4],
    /// +0x30: Weapon fire type. Values map to [`FireType`]:
    /// 1=Projectile, 2=Placed, 3=Strike, 4=Special.
    ///
    /// Type 2 was historically labelled "rope" in WA RE notes — that's wrong.
    /// Real rope-style weapons (NinjaRope, Bungee) live in Special. See
    /// [`FireType::Placed`] for the actual roster.
    pub fire_type: i32,
    /// +0x34: Fire subtype for `FireType::Strike` (parameter data) and
    /// `FireType::Special` (selects the [`SpecialFireSubtype`] handler).
    pub special_subtype: i32,
    /// +0x38: Fire method index for `FireType::Projectile` and
    /// `FireType::Placed` (selects the [`FireMethod`] sub-dispatch).
    pub fire_method: i32,
    /// +0x3C: Fire parameters sub-structure. Pointer to this field is passed
    /// to fire sub-functions (PlacedExplosive, Projectile, CreateWeaponProjectile, etc.).
    pub fire_params: WeaponFireParams,
}
const _: () = assert!(core::mem::size_of::<WeaponEntry>() == 0x1D0);

// SAFETY: `WeaponEntry` contains two `*const c_char` fields (`name1`, `name2`)
// which makes it `!Send + !Sync` by default. WA is single-threaded for game
// logic, and the only thread-shared use of `WeaponEntry` is the read-only
// static fixture in [`crate::game::weapon_data`] where both pointers are
// nulled. No actual cross-thread sharing of WA-mutated WeaponEntry instances
// occurs.
unsafe impl Send for WeaponEntry {}
unsafe impl Sync for WeaponEntry {}

/// Weapon fire parameters — embedded at WeaponEntry+0x3C (0x194 = 404 bytes, 101 DWORDs).
///
/// Pointer to this struct is passed to all fire dispatch sub-functions.
/// The first 94 DWORDs (0x178 bytes) are copied verbatim into MissileEntity.weapon_data
/// by MissileEntity__Constructor. The remaining 7 DWORDs are WeaponEntry-only metadata.
///
/// For single-shot projectiles, MissileEntity.render_data[N] = weapon_data[N+3].
/// For cluster sub-pellets, render_data[N] = weapon_data[N+52].
///
/// Field names confirmed by cross-referencing live memory dumps (debug CLI),
/// MissileEntity constructor decompilation, and render_data physics usage.
#[repr(C)]
pub struct WeaponFireParams {
    /// [0] +0x3C: Polymorphic.
    /// Missile: pellet count (Bazooka=2, Mortar=2).
    /// Hitscan: shots per trigger (Shotgun=1, Handgun=6, Uzi=10, Minigun=20).
    /// Airstrike: number of projectiles (AirStrike=5).
    pub shot_count: i32,
    /// [1] +0x40: Polymorphic.
    /// Missile: spread angle for multi-pellet (Mortar=100).
    /// Hitscan: spread cone (Shotgun=500, Handgun=500).
    /// Airstrike: spacing (NapalmStrike=48).
    pub spread: i32,
    /// [2] +0x44: Polymorphic.
    /// Missile: cluster flag (Grenade=1, ClusterBomb=1, BananaBomb=1).
    /// Hitscan: fire delay between shots (Handgun=10, Uzi=15, Minigun=25).
    pub unknown_0x44: i32,
    /// [3] +0x48: Polymorphic.
    /// Missile: collision radius (Bazooka=2.10, HomingPigeon=66.1).
    /// Hitscan: always Fixed(1) — flag.
    pub collision_radius: Fixed,
    /// [4] +0x4C: Missile only: unknown (HomingMissile=5, HomingPigeon=25, Grenade=10).
    pub unknown_0x4c: i32,
    /// [5] +0x50: Polymorphic.
    /// Missile: explosion damage (Bazooka=100, Grenade=100, Mortar=0).
    /// Hitscan: max range (all=66.1 as Fixed16.16).
    pub unknown_0x50: i32,
    /// [6] +0x54: Polymorphic.
    /// Missile: blast radius (Bazooka=50, Mortar=15, Grenade=50).
    /// Hitscan: impact radius (Shotgun=5, Minigun=20).
    pub unknown_0x54: i32,
    /// [7] +0x58: Hitscan only: unknown (Shotgun=100, Handgun=50, Uzi/Minigun=100).
    pub unknown_0x58: i32,
    /// [8] +0x5C: Hitscan only: damage per hit (Shotgun=25, Handgun/Uzi/Minigun=5).
    pub unknown_0x5c: i32,
    /// [9] +0x60: Missile only: sprite/animation ID (Bazooka=48, Grenade=50, HomingPigeon=175).
    pub sprite_id: i32,
    /// [10] +0x64: Shot type/impact type (Missile: Bazooka=2, Grenade=1. Hitscan: Shotgun=2, Uzi=5).
    pub impact_type: i32,
    /// [11] +0x68: Polymorphic.
    /// Missile: unknown (Bazooka=131).
    /// Hitscan: max range in pixels (all=32767).
    pub unknown_0x68: i32,
    /// [12] +0x6C: Trail effect (Bazooka=50, Mortar=50, most weapons=0).
    pub trail_effect: i32,
    /// [13] +0x70: Gravity percentage (100=normal, 0=no gravity).
    /// → render_data[0x0C] → WorldEntity.gravity_factor.
    pub gravity_pct: i32,
    /// [14] +0x74: Wind influence (Bazooka=50, AquaSheep=200).
    pub wind_influence: i32,
    /// [15] +0x78: Bounce percentage (100=normal elastic, 0=no bounce).
    /// → render_data[0x0D] → WorldEntity.bounce_factor.
    pub bounce_pct: i32,
    /// [16] +0x7C: Unknown (Bazooka=100, most=0).
    pub unknown_0x7c: i32,
    /// [17] +0x80: Unknown (AirStrike=10, MoleSquadron=50).
    pub unknown_0x80: i32,
    /// [18] +0x84: Friction percentage (100=normal, 0=no friction).
    /// → render_data[0x0F] → WorldEntity.friction_factor.
    pub friction_pct: i32,
    /// [19] +0x88: Explosion delay/fuse timer (Grenade=5000, Dynamite=5000).
    pub explosion_delay: i32,
    /// [20] +0x8C: Fuse timer (Bazooka=9000, HomingMissile=10000, SheepLauncher=20000).
    pub fuse_timer: i32,
    /// [21-25] +0x90-0xA0: Various parameters. Remaining primary fields.
    pub unknown_0x90_0xa0: [i32; 5],
    /// [26] +0xA4: Missile type discriminator.
    /// 0=None, 1=Homing, 2=Standard, 3=Sheep, 5=SheepLauncher/Cluster.
    /// → render_data[0x17] → MissileEntity behavior dispatch.
    pub missile_type: i32,
    /// [27] +0xA8: Render size. Bazooka=64.0, Grenade≈66.1.
    pub render_size: Fixed,
    /// [28] +0xAC: Render timer/fuse (Bazooka=1, HomingMissile=58, HomingPigeon=175).
    pub render_timer: i32,
    /// [29-33] +0xB0-0xC0: Homing parameters (only nonzero for homing weapons).
    /// [29]=homing_strength, [30]=homing_sprite, [31]=homing_turn_rate,
    /// [32]=homing_accel, [33]=homing_speed.
    pub homing_params: [i32; 5],
    /// [34-36] +0xC4-0xCC: Additional parameters.
    pub unknown_0xc4_0xcc: [i32; 3],
    /// [37-51] +0xD0-0x10C: Reserved / weapon-specific extended params.
    pub unknown_0xd0_0x10c: [i32; 15],
    /// [52-93] +0x10C-0x1B0: Cluster sub-pellet parameters (mirrors primary [0-41]).
    /// When pellet_index > 0, render_data copies from here instead.
    pub cluster_params: [i32; 42],
    /// [94-100] +0x1B0-0x1CC: WeaponEntry-only metadata (NOT copied to MissileEntity).
    /// [94]+0x1C8: Power percentage (Bazooka=100, Shotgun=10, NinjaRope=100).
    /// [95]+0x1CC: Unknown (Bazooka=100, Grenade=70, Shotgun=20).
    pub entry_metadata: [i32; 7],
}
const _: () = assert!(core::mem::size_of::<WeaponFireParams>() == 0x1D0 - 0x3C);

/// Weapon table — flat array of 71 entries, no header.
///
/// Allocated by `InitWeaponTable` (0x53CAB0), stored at GameWorld+0x510.
/// Total size: 71 × 0x1D0 = 0x80B0 bytes.
#[repr(C)]
pub struct WeaponTable {
    /// Weapon entries array (71 standard weapons, indices 0..70).
    pub entries: [WeaponEntry; 71],
}
const _: () = assert!(core::mem::size_of::<WeaponTable>() == 71 * 0x1D0);

// ============================================================
// Weapon availability query (GameWorld state-driven)
// ============================================================

/// Pure Rust implementation of GameWorld__CheckWeaponAvail (0x53FFC0).
///
/// Convention: fastcall(ECX=world) + unaff_ESI=weapon_index, plain RET. Returns i32.
///
/// Checks whether a weapon (1..0x46) is available given current game state.
pub unsafe fn check_weapon_avail(world: *mut GameWorld, weapon: WeaponId) -> i32 {
    unsafe {
        let gi = (*world).game_info;
        let game_version = (*gi).game_version;
        let num_teams = (*gi).num_teams;

        // Step 1: Special per-weapon disabling rules
        match weapon {
            w if (w.is(KnownWeaponId::Earthquake)
                || w.is(KnownWeaponId::NuclearTest)
                || w.is(KnownWeaponId::Armageddon))
                && (*gi).net_config_2 != 0
                && (*gi).net_weapon_exception == 0 =>
            {
                return 0;
            }
            w if w.is(KnownWeaponId::Donkey) && (*gi).donkey_disabled != 0 => {
                return 0;
            }
            w if w.is(KnownWeaponId::Invisibility) => {
                if (*gi).invisibility_mode == 0 {
                    if (*world).net_session.is_null() {
                        return 0;
                    }
                } else if (num_teams as u32) < 2 {
                    return 0;
                }
            }
            w if w.is(KnownWeaponId::DoubleTurnTime)
                && game_version > 0xD1
                && (*gi).double_turn_time_threshold > 0x7FFF =>
            {
                return 0;
            }
            _ => {}
        }

        // Step 2: Branch on weapon defined flag (nonzero = weapon exists in table).
        let weapon_table = (*world).weapon_table;
        let defined = (*weapon_table).entries[weapon.0 as usize].defined;

        if (*world).is_cavern == 0 || defined != 0 {
            // Main path: check super weapon flag
            let super_result = is_super_weapon(weapon, (*world).version_flag_3 != 0);
            if super_result && (*gi).super_weapon_allowed == 0 {
                // (game_version < 0x2A) - 1: if < 0x2A → 0, else → -1
                return (game_version < 0x2A) as i32 - 1;
            }

            if (*world).supersheep_restricted == 0 {
                return 1;
            }

            // SuperSheep (24) when aquasheep_is_supersheep is set, else AquaSheep (25).
            // Mirrors WA's `Weapon::AquaSheep - (aquasheep_is_supersheep != 0)` arithmetic.
            let restricted_id = if (*gi).aquasheep_is_supersheep != 0 {
                KnownWeaponId::SuperSheep
            } else {
                KnownWeaponId::AquaSheep
            };

            if weapon != restricted_id.into() {
                return 1;
            }

            return 0;
        }

        // Else branch: is_cavern != 0 AND weapon_entry == 0
        if game_version > 0x29 && (*gi).weapon_version_gate != 0 {
            return -2;
        }

        0
    }
}

#[cfg(test)]
mod vanilla_data_tests {
    //! Spot-checks against [`crate::game::weapon_data::VANILLA_WEAPON_DATA`].
    //! Confirms the python generator (`tools/generate_weapon_data.py`) lined
    //! the dump up correctly with the `WeaponEntry` / `WeaponFireParams` field
    //! plan. Picks one weapon from each `fire_type` plus a couple of the
    //! "interesting" ones whose values were referenced when documenting the
    //! struct.
    use super::*;
    use crate::game::weapon_data::VANILLA_WEAPON_DATA;

    fn entry(id: KnownWeaponId) -> &'static WeaponEntry {
        &VANILLA_WEAPON_DATA[id as usize]
    }

    #[test]
    fn bazooka_fire_dispatch_matches_doc() {
        // Bazooka is the canonical projectile weapon — its values are quoted in
        // every WeaponFireParams field doc.
        let e = entry(KnownWeaponId::Bazooka);
        assert_eq!(e.fire_type, 1, "Bazooka fire_type=1 (Projectile)");
        assert_eq!(
            e.fire_method, 3,
            "Bazooka fire_method=3 (CreateWeaponProjectile)"
        );
        assert_eq!(e.fire_params.shot_count, 2);
        assert_eq!(e.fire_params.collision_radius.0, 0x2187E); // ≈ 2.10
        assert_eq!(e.fire_params.sprite_id, 48);
        assert_eq!(e.fire_params.unknown_0x68, 131);
        assert_eq!(e.fire_params.fuse_timer, 9000);
        assert_eq!(e.fire_params.missile_type, 2);
    }

    #[test]
    fn mine_uses_placed_method_1() {
        // Mine is the canonical FireType::Placed / FireMethod::PlacedExplosive
        // weapon — and the only stock weapon that hits the MineEntity::Constructor
        // dispatch arm. Confirms the rename from the historical (wrong) "rope"
        // label still resolves to fire_type=2 / fire_method=1 in the data.
        let e = entry(KnownWeaponId::Mine);
        assert_eq!(e.fire_type, FireType::Placed as i32);
        assert_eq!(e.fire_method, FireMethod::PlacedExplosive as i32);
    }

    #[test]
    fn airstrike_is_strike_type() {
        // fire_type=3 is the air-strike family, despite the WeaponEntry doc
        // historically labelling it "grenade".
        let e = entry(KnownWeaponId::AirStrike);
        assert_eq!(e.fire_type, FireType::Strike as i32);
    }

    #[test]
    fn rope_style_weapons_live_in_special() {
        // The actual rope-style weapons (NinjaRope, Bungee) are FireType::Special,
        // not FireType::Placed — i.e. the historical "Rope" name on the type-2
        // bucket was misdirection. NinjaRope is special_subtype=6, Bungee=7.
        let rope = entry(KnownWeaponId::NinjaRope);
        let bungee = entry(KnownWeaponId::Bungee);
        assert_eq!(rope.fire_type, FireType::Special as i32);
        assert_eq!(rope.special_subtype, 6);
        assert_eq!(bungee.fire_type, FireType::Special as i32);
        assert_eq!(bungee.special_subtype, 7);
    }

    #[test]
    fn no_vanilla_weapon_uses_placed_method_3() {
        // `fire_canister` (CanisterEntity ctor at 0x501A80) is unreachable from
        // any vanilla weapon — confirmed empirically here so future changes
        // that consolidate the dispatch can rely on it. Custom schemes / mods
        // may still hit this arm, so the helper stays in place.
        let placed = FireType::Placed as i32;
        let create_proj = FireMethod::CreateWeaponProjectile as i32;
        for (i, e) in VANILLA_WEAPON_DATA.iter().enumerate() {
            assert!(
                !(e.fire_type == placed && e.fire_method == create_proj),
                "weapon id {i} unexpectedly uses fire_type=Placed/fire_method=CreateWeaponProjectile",
            );
        }
    }
}
