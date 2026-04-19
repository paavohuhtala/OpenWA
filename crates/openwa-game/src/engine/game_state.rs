//! `DDGameWrapper::game_state` (offset 0x484) — the main game-loop
//! state variable.
//!
//! Read via vtable slot `get_game_state` (0x528A20) by
//! `advance_frame` to decide whether to keep looping. Written by
//! `init_game_state`, `StepFrame` Block A, the end-of-game state
//! handlers (`OnGameState2/3/4`, `BeginNetworkGameEnd`), and
//! `DispatchFrame`'s replay game-over path.
//!
//! Not an enum because we don't know every variant — transmuting an
//! unknown discriminant would be UB. Raw `u32` constants with a
//! documented transition graph instead.
//!
//! Observed transitions:
//!
//! ```text
//! init_game_state: _ -> RUNNING -> INITIALIZED
//!
//! (offline round ends)
//!   StepFrame Block A (hud_status_code ∈ {6,8}, network_ecx == 0)
//!     -> ROUND_ENDING
//!
//! (network round ends)
//!   StepFrame Block A (network_ecx != 0) -> BeginNetworkGameEnd
//!     -> NETWORK_END_STARTED
//!   OnGameState3 -> NETWORK_END_AWAITING_PEERS
//!   OnGameState2 (all peers confirmed) -> ROUND_ENDING
//!
//!   OnGameState4 (counts ~50 frames) -> EXIT
//! ```
//!
//! `advance_frame` returns the current value each frame.
//! `process_frame` exits the main loop when it sees `EXIT`, and sets
//! `exit_flag` (ending `/getlog` headless runs) when it sees
//! `ROUND_ENDING`.

/// `0` — game is simulating normally.
pub const RUNNING: u32 = 0;

/// `1` — `init_game_state` has finished. Observed as a one-frame
/// transient between init and the main loop starting; the purpose of
/// the distinct value isn't fully pinned down.
pub const INITIALIZED: u32 = 1;

/// `2` — network game-end: waiting for all peers to confirm their
/// end-of-round state. `OnGameState2` (0x00536470) polls each peer via
/// the net-game vtable and transitions to `ROUND_ENDING` once consensus
/// is reached (or the timeout at `wrapper._field_260` expires).
pub const NETWORK_END_AWAITING_PEERS: u32 = 2;

/// `3` — network game-end: local side has committed to ending. Entered
/// via `BeginNetworkGameEnd` (0x00536270) from StepFrame Block A when
/// `network_ecx != 0`. `OnGameState3` (0x00536320) handles the wait
/// before transitioning to `NETWORK_END_AWAITING_PEERS`.
pub const NETWORK_END_STARTED: u32 = 3;

/// `4` — round is ending. Set directly from StepFrame Block A in
/// offline games, or reached via the `3 -> 2 -> 4` path in network
/// games. `OnGameState4` (0x005365A0) then counts ~50 frames before
/// transitioning to `EXIT`.
///
/// Headless `/getlog` runs exit the moment they see this: `process_frame`
/// treats `ROUND_ENDING` as a terminal signal. (Formerly misnamed
/// `EXIT_HEADLESS` — despite the name, this value is reached in both
/// headful and headless modes; only the response differs.)
pub const ROUND_ENDING: u32 = 4;

/// `5` — game has fully exited. Set by `OnGameState4` once its frame
/// counter rolls past the one-second mark. `process_frame` exits the
/// main loop when it sees this.
pub const EXIT: u32 = 5;
