use openwa_core::fixed::Fixed;

/// Rust port of `FixedSlewToward` (0x00534BC0).
///
/// Slews `*state` toward `target` with step size
/// `max(min_step, |target - *state| * rate)`, clamped to the gap so it
/// never overshoots. `rate` is a Fixed fraction: `ONE` closes the full
/// gap in one call, `HALF` closes half, `ZERO` falls back to `min_step`.
///
/// `force_set` overwrites `*state` with `target` before the slew, which
/// then degenerates to a snap. Returns `true` iff `*state == target` on
/// entry.
#[allow(dead_code)]
pub(crate) unsafe fn fixed_slew_toward(
    state: *mut Fixed,
    target: Fixed,
    min_step: Fixed,
    rate: Fixed,
    force_set: bool,
) -> bool {
    unsafe {
        if force_set {
            *state = target;
        }
        let prev = *state;
        let already_settled = prev == target;
        let delta = target - prev;
        let step = delta.mul_raw(rate).abs().max(min_step);

        if delta.abs() > step {
            let signed_step = if target < prev { -step } else { step };
            *state = prev + signed_step;
        } else {
            *state = target;
        }

        already_settled
    }
}

#[cfg(test)]
pub(crate) mod fixed_slew_tests {
    use super::fixed_slew_toward;
    use openwa_core::fixed::Fixed;

    #[test]
    fn already_settled_returns_true_and_leaves_state() {
        let mut s = Fixed::from_raw(100);
        let already = unsafe {
            fixed_slew_toward(
                &mut s,
                Fixed::from_raw(100),
                Fixed::from_raw(1),
                Fixed::ONE,
                false,
            )
        };
        assert!(already);
        assert_eq!(s, Fixed::from_raw(100));
    }

    #[test]
    fn full_rate_snaps_to_target_in_one_call() {
        // rate = ONE (1.0) → step = |delta|, then clamped to |delta| → snap.
        let mut s = Fixed::ZERO;
        let already = unsafe {
            fixed_slew_toward(&mut s, Fixed::from_raw(100), Fixed::ZERO, Fixed::ONE, false)
        };
        assert!(!already);
        assert_eq!(s, Fixed::from_raw(100));
    }

    #[test]
    fn half_rate_takes_multiple_steps() {
        // rate = HALF (0.5). On each call, step = |delta|/2.
        // delta=100 → step=50 → new=50; delta=50 → step=25 → new=75; ...
        let mut s = Fixed::ZERO;
        let target = Fixed::from_raw(100);
        unsafe {
            let settled1 = fixed_slew_toward(&mut s, target, Fixed::ZERO, Fixed::HALF, false);
            assert!(!settled1);
            assert_eq!(s, Fixed::from_raw(50));
            let settled2 = fixed_slew_toward(&mut s, target, Fixed::ZERO, Fixed::HALF, false);
            assert!(!settled2);
            assert_eq!(s, Fixed::from_raw(75));
        }
    }

    #[test]
    fn min_step_floor_wins_when_rate_too_small() {
        // rate = ZERO → scaled = 0, so step falls back to min_step.
        let mut s = Fixed::ZERO;
        unsafe {
            fixed_slew_toward(
                &mut s,
                Fixed::from_raw(100),
                Fixed::from_raw(5),
                Fixed::ZERO,
                false,
            );
        }
        assert_eq!(s, Fixed::from_raw(5));
    }

    #[test]
    fn backward_slew_moves_down() {
        let mut s = Fixed::from_raw(100);
        unsafe {
            fixed_slew_toward(&mut s, Fixed::ZERO, Fixed::from_raw(10), Fixed::ZERO, false);
        }
        assert_eq!(s, Fixed::from_raw(90));
    }

    #[test]
    fn force_set_snaps_and_reports_settled() {
        let mut s = Fixed::from_raw(42);
        let settled = unsafe {
            fixed_slew_toward(
                &mut s,
                Fixed::from_raw(100),
                Fixed::from_raw(5),
                Fixed::HALF,
                true,
            )
        };
        // force_set overwrites first, so prev == target on entry to the
        // "already settled" check → returns true, state == target.
        assert!(settled);
        assert_eq!(s, Fixed::from_raw(100));
    }
}
