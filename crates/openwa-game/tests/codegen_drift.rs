//! Drift check between hand-written `define_addresses!` blocks and the
//! TOML-driven generated addresses module.
//!
//! Both feed the same `inventory::collect!(AddrEntry)` bucket, distinguished
//! by `AddrEntry.source`. For every VA where both kinds exist, the calling
//! convention (when annotated on both sides) must agree — otherwise the
//! manual entry has drifted from canonical TOML state.
//!
//! Mismatches mean either:
//!   - The manual `define_addresses!` declaration is out of date; update it
//!     or remove it (and let the generated entry stand alone).
//!   - The `re/*.toml` `calling_convention` is wrong; fix it there, validate,
//!     and re-export to Ghidra.
//!
//! Skipped (not a drift signal):
//!   - Manual has `None`: the macro wasn't given a calling convention. Not a
//!     contradiction, just missing data on the manual side.
//!   - Generated has `None`: TOML doesn't carry a `calling_convention` for
//!     this function. Not a drift signal either.

use std::collections::HashMap;

use openwa_game::registry::{AddrEntry, AddrSource, CallingConv};

#[test]
fn manual_addresses_agree_with_generated_calling_conv() {
    let mut by_va: HashMap<u32, (Vec<&AddrEntry>, Vec<&AddrEntry>)> = HashMap::new();
    for e in inventory::iter::<AddrEntry> {
        let bucket = by_va.entry(e.va).or_default();
        match e.source {
            AddrSource::Manual => bucket.0.push(e),
            AddrSource::Generated => bucket.1.push(e),
        }
    }

    let mut errors: Vec<String> = Vec::new();
    let mut compared = 0usize;
    for (va, (manuals, generateds)) in &by_va {
        if manuals.is_empty() || generateds.is_empty() {
            continue;
        }
        for m in manuals {
            for g in generateds {
                let (Some(mcc), Some(gcc)) = (m.calling_conv, g.calling_conv) else {
                    continue;
                };
                compared += 1;
                if !calling_conv_equiv(mcc, gcc) {
                    errors.push(format!(
                        "VA 0x{va:08X}: manual `{}` says {mcc:?}, generated `{}` says {gcc:?}",
                        m.name, g.name,
                    ));
                }
            }
        }
    }

    if !errors.is_empty() {
        let n = errors.len();
        let preview: Vec<String> = errors.iter().take(20).cloned().collect();
        panic!(
            "{n} calling-convention drift(s) between define_addresses! and re/*.toml \
             (compared {compared} annotated pairs). First {} shown:\n{}",
            preview.len(),
            preview.join("\n"),
        );
    }
}

/// Treat `Usercall` as compatible with the underlying base convention when
/// the other side hasn't been promoted to `Usercall`. Codegen sets
/// `Usercall` whenever `custom_storage = true` (any register-passed param),
/// while manual `define_addresses!` typically records the base convention.
/// This isn't drift — both descriptions are true.
fn calling_conv_equiv(a: CallingConv, b: CallingConv) -> bool {
    if a == b {
        return true;
    }
    matches!(
        (a, b),
        (CallingConv::Usercall, _) | (_, CallingConv::Usercall)
    )
}

#[test]
fn registry_lookup_prefers_manual_over_generated_at_same_va() {
    // Find one VA that's annotated on both sides.
    let mut by_va: HashMap<u32, (Option<&AddrEntry>, Option<&AddrEntry>)> = HashMap::new();
    for e in inventory::iter::<AddrEntry> {
        let bucket = by_va.entry(e.va).or_default();
        match e.source {
            AddrSource::Manual => bucket.0 = Some(e),
            AddrSource::Generated => bucket.1 = Some(e),
        }
    }

    let Some((va, _)) = by_va.iter().find(|(_, (m, g))| m.is_some() && g.is_some()) else {
        // No overlap (unlikely once codegen ramps up, but harmless).
        return;
    };
    let resolved = openwa_game::registry::lookup_va(*va).expect("VA should resolve");
    assert_eq!(
        resolved.entry.source,
        AddrSource::Manual,
        "lookup_va should return the Manual entry when both kinds exist at the same VA \
         (got `{}` source={:?})",
        resolved.entry.name,
        resolved.entry.source,
    );
}
