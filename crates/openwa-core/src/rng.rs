//! WA's LCG random number generator.

/// Single step of WA's LCG: `state * 0x19660D + 0x3C6EF35F`.
///
/// This is the core PRNG used throughout WA.exe. The game RNG
/// ([`DDGame::advance_rng`](crate::engine::ddgame::DDGame::advance_rng))
/// adds `frame_counter` before the multiply; the effect RNG and standalone
/// uses (e.g. cloud spawning) use this directly.
#[inline]
pub fn wa_lcg(state: u32) -> u32 {
    state.wrapping_mul(0x19660D).wrapping_add(0x3C6EF35F)
}
