//! Rust port of `WeaponSpawn::DecodeDescriptor_Maybe` (0x00565C10) and
//! its two private helpers `FUN_00565bb0` (0x00565BB0) and `FUN_00565b40`
//! (0x00565B40). Reads a [`WeaponEntry`] and returns a [`WeaponAimFlags`]
//! struct with up to 8 per-weapon UI/aim booleans.
//!
//! All 13 WA-side callers (`WormEntity::SelectFuse/Bounce/Herd/Weapon`,
//! `WormEntity::BroadcastWeaponSettings`, `WormEntity::HandleMessage`
//! case 0x5, `TeamEntity::HandleMessage`, plus three other free
//! functions) route through [`decode_weapon_aim_flags_impl`] once the
//! MinHook is active. The two inner sub-helpers are private and have no
//! other WA callers — they're inlined as Rust private functions.
//!
//! Output flag semantics are still being mapped from caller behaviour;
//! the only fully-understood pair is `flag_d` / `flag_e` (consulted by
//! `WormEntity::HandleMessage` case 0x5: when both are `false`, the
//! worm sets `_field_3a0 = 1`, suppressing the aim sprite). Names stay
//! positional until more callers are ported.

use crate::game::weapon::WeaponEntry;
use openwa_core::weapon::FireType;

/// Decoded UI/aim booleans for a [`WeaponEntry`]. Returned by
/// [`decode_weapon_aim_flags`]. Each field corresponds to one of the
/// nine WA-side out-pointers (`out_a` is set inside the inner helper
/// alongside `flag_a`'s `out_a`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WeaponAimFlags {
    /// Set by inner sub-routine when `fire_params[+0x4] == 0`.
    pub flag_a: bool,
    /// Set by inner-inner when `sub_region[+0x5C] ∈ {2, 5}` AND
    /// `sub_region[+0x64] == 0`.
    pub flag_b: bool,
    /// Set by inner-inner when `sub_region[+0x44] == 0`.
    pub flag_c: bool,
    /// Primary aim-sprite flag. Set by:
    /// - inner-inner when `sub_region[+0x5C] == 1`
    /// - fire_type 3 (Strike) when `fire_params.unknown_0x44 == 0`
    /// - fire_type 4 (Special) subtypes Teleport (10) / Girder (17)
    pub flag_d: bool,
    /// Cluster flag. Set by:
    /// - fire_type 3 (Strike) when `fire_params.unknown_0x44 != 0`
    pub flag_e: bool,
    /// Secondary aim flag. Set by:
    /// - fire_type 3 (Strike) — always
    /// - fire_type 4 (Special) subtypes Teleport (10) / Girder (17)
    pub flag_f: bool,
    /// Set by inner-inner when `sub_region[+0x58] != 0`.
    pub flag_g: bool,
    /// Placed-with-subtype flag. Set by:
    /// - fire_type 2 (Placed) when `entry.special_subtype != 0`
    pub flag_h: bool,
}

/// Mirrors WA `FUN_00565b40` (0x00565B40). `block` is a sub-slice of
/// the `&[i32]` view of [`WeaponFireParams`] — byte offsets used by
/// the WA disassembly are converted to `i32` indices (`/4`) for safe
/// slice indexing. Inspects four slots and conditionally sets four
/// flags.
fn decode_inner_inner(block: &[i32], flags: &mut WeaponAimFlags) {
    if block[0x44 / 4] == 0 {
        flags.flag_c = true;
    }
    if block[0x58 / 4] != 0 {
        flags.flag_g = true;
    }
    match block[0x5C / 4] {
        1 => flags.flag_d = true,
        2 | 5 if block[0x64 / 4] == 0 => flags.flag_b = true,
        _ => {}
    }
}

/// Mirrors WA `FUN_00565bb0` (0x00565BB0). Writes [`WeaponAimFlags::flag_a`]
/// based on `block[+0x4]`, runs [`decode_inner_inner`] starting at
/// `block[+0xC]`, then conditionally runs it again starting at
/// `block[+0xD0]` when the discriminant at `block[+0xB4]` is 1 or 3.
fn decode_inner(block: &[i32], flags: &mut WeaponAimFlags) {
    // i32 slot 1 == byte +0x4
    if block[1] == 0 {
        flags.flag_a = true;
    }

    decode_inner_inner(&block[0xC / 4..], flags);

    let sub_idx = block[0xB4 / 4];
    if sub_idx == 1 || sub_idx == 3 {
        decode_inner_inner(&block[0xD0 / 4..], flags);
    }
}

/// Rust-native entry point. Decodes a [`WeaponEntry`] into its UI/aim
/// flag set. Use this for Rust-to-Rust calls; the cdecl wrapper
/// [`decode_weapon_aim_flags_impl`] is only needed for the MinHook
/// trampoline.
pub fn decode_weapon_aim_flags(entry: &WeaponEntry) -> WeaponAimFlags {
    let mut flags = WeaponAimFlags::default();

    let Ok(ft) = FireType::try_from(entry.fire_type) else {
        return flags;
    };

    let fp = entry.fire_params.as_i32_slice();

    match ft {
        FireType::Projectile => {
            if entry.fire_method == 3 {
                decode_inner(fp, &mut flags);
            }
        }
        FireType::Placed => {
            if entry.special_subtype != 0 {
                flags.flag_h = true;
            }
            if entry.fire_method == 2 {
                decode_inner(fp, &mut flags);
            }
        }
        FireType::Strike => {
            // `fire_params.unknown_0x4c` is the discriminant the
            // inner helper consumes — when it's 2, run the inner
            // pass over the `unknown_0x50` region (the pellet-config
            // sub-block at fire_params+0x14, i.e. i32 slot 5).
            if entry.fire_params.unknown_0x4c == 2 {
                decode_inner(&fp[0x14 / 4..], &mut flags);
            }
            flags.flag_f = true;
            if entry.fire_params.unknown_0x44 != 0 {
                flags.flag_e = true;
            } else {
                flags.flag_d = true;
            }
        }
        FireType::Special => {
            // 24-entry LUT at WA `0x00565D48` — only subtypes
            // Teleport (10) and Girder (17) hit the non-default
            // (`flag_f = true; flag_d = true`) path.
            let subtype = entry.special_subtype;
            if subtype == 10 || subtype == 17 {
                flags.flag_f = true;
                flags.flag_d = true;
            }
        }
    }

    flags
}

/// Cdecl wrapper behind the MinHook on `WeaponSpawn::DecodeDescriptor`
/// (0x00565C10). All 8 outputs are unconditionally cleared to zero on
/// entry; the per-`fire_type` switch then sets a subset to `1`.
///
/// The signature mirrors the WA usercall exactly:
/// `__usercall(EAX = out_a, EDX = out_b)` + 7 stack args
/// `(entry, out_c, out_d, out_e, out_f, out_g, out_h)`, RET 0x1C.
///
/// # Safety
/// All output pointers must be valid for writes; `entry` must point to
/// a fully-initialized [`WeaponEntry`]. Concurrent access is forbidden
/// (caller's responsibility — WA is single-threaded for game logic).
pub unsafe extern "cdecl" fn decode_weapon_aim_flags_impl(
    out_a: *mut i32,
    out_b: *mut i32,
    entry: *const WeaponEntry,
    out_c: *mut i32,
    out_d: *mut i32,
    out_e: *mut i32,
    out_f: *mut i32,
    out_g: *mut i32,
    out_h: *mut i32,
) {
    unsafe {
        let flags = decode_weapon_aim_flags(&*entry);
        *out_a = flags.flag_a as i32;
        *out_b = flags.flag_b as i32;
        *out_c = flags.flag_c as i32;
        *out_d = flags.flag_d as i32;
        *out_e = flags.flag_e as i32;
        *out_f = flags.flag_f as i32;
        *out_g = flags.flag_g as i32;
        *out_h = flags.flag_h as i32;
    }
}
