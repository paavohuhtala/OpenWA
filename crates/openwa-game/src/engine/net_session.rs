//! Typed stub for `GameWorld::network_ecx` — the per-game network session
//! manager. Only the vtable slots used by the end-game state machine
//! (`OnGameState3`, `OnNetworkEndAwaitPeers`) are pinned down; the rest of
//! the layout is unknown. The vtable address is not yet in the registry
//! because multiple transport-specific subclasses may share the slot shape.

use crate::FieldRegistry;
use crate::engine::buffer_object::BufferObject;
use crate::rebase::rb;

crate::define_addresses! {
    /// `GameNet::send_block` (0x0053E380). Usercall(EAX=this), plain RET.
    /// Flushes the outgoing buffer at `NetSession+0x20` to the wire.
    /// Bridged because the implementation is large (~91 instructions of
    /// transport-specific framing) and only one call site needs it from
    /// Rust today.
    fn/Usercall GAME_NET_SEND_BLOCK = 0x0053E380;
}

static mut GAME_NET_SEND_BLOCK_ADDR: u32 = 0;

/// Initialize bridged-function addresses for this module. Called once at
/// DLL load from `dispatch_frame::init_dispatch_addrs`.
pub unsafe fn init_addrs() {
    unsafe {
        GAME_NET_SEND_BLOCK_ADDR = rb(GAME_NET_SEND_BLOCK);
    }
}

/// Bridge for `GameNet::send_block` (0x0053E380). Usercall(EAX=this),
/// plain RET. Tail-call shape: pop ret-addr + the `this` arg, push
/// ret-addr back, jmp to target.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn bridge_send_block(_this: *mut NetSession) {
    core::arch::naked_asm!(
        "pop ecx",
        "pop eax",
        "push ecx",
        "jmp dword ptr [{addr}]",
        addr = sym GAME_NET_SEND_BLOCK_ADDR,
    );
}

/// Partial vtable for `NetSession`. Observed slots are the per-peer query
/// API used during end-of-round peer synchronisation.
#[openwa_game::vtable(size = 11, class = "NetSession")]
pub struct NetSessionVtable {
    /// slot 2 (+0x08): submit an outgoing message buffer for sending. Only
    /// observed call site is `begin_network_game_end`, which builds a 12-byte
    /// end-of-round message in `runtime.ring_buffer_a` and forwards it
    /// through this slot. Exact wire semantics unconfirmed; the buffer is
    /// freshly reset before the call so it contains exactly one message.
    #[slot(2)]
    pub submit_message_buffer: fn(this: *mut NetSession, buffer: *mut BufferObject),
    /// slot 4 (+0x10): per-peer score / remaining-timeout. Caller takes
    /// `max()` over all active peers to decide whether to keep waiting.
    #[slot(4)]
    pub peer_score: fn(this: *mut NetSession, idx: u32) -> i32,
    /// slot 5 (+0x14): is a sync still in progress? Non-zero = keep waiting.
    #[slot(5)]
    pub sync_in_progress: fn(this: *mut NetSession) -> u32,
    /// slot 6 (+0x18): is peer `idx` still active/participating?
    #[slot(6)]
    pub peer_active: fn(this: *mut NetSession, idx: u32) -> u32,
    /// slot 9 (+0x24): per-peer "pending / not-yet-ready" query used by the
    /// online `ShouldInterpolate` branch (`GameRuntime__PeerInputsCaughtUp`). Caller treats a
    /// non-zero return combined with `team_scoring_a[idx] > 0` as "this peer
    /// still hasn't caught up" and suppresses interp. Exact semantics
    /// (vs. slot 6's `peer_active`) are unconfirmed — name is a guess.
    #[slot(9)]
    pub peer_pending_maybe: fn(this: *mut NetSession, idx: u32) -> u32,
    /// slot 10 (+0x28): "still busy with end-of-round handshake" predicate.
    /// Polled by `render_network_end_wait_textbox` (formerly named
    /// `RenderTurnStatus`) in network mode when the `net_end_countdown`
    /// countdown has reached zero — a non-zero return suppresses the
    /// on-screen "PLEASE WAIT" textbox once the timeout has expired and
    /// the predicate signals real work in flight. Exact wire semantics
    /// unconfirmed; name is provisional.
    #[slot(10)]
    pub end_handshake_busy_maybe: fn(this: *mut NetSession) -> u32,
}

/// Partial layout of the object at `GameWorld::network_ecx`.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct NetSession {
    /// +0x00: Vtable pointer.
    pub vtable: *const NetSessionVtable,
    /// +0x04: Unknown.
    pub _field_04: u32,
    /// +0x08: Number of peers in the session (loop bound for per-peer sweeps).
    pub peer_count: i32,
    /// +0x0C: Our own peer index — excluded from `max_peer_score_raw`.
    pub self_peer_idx: i32,
    /// +0x10..+0x1B: Unknown.
    pub _unknown_10: [u8; 0xc],
    /// +0x1C: Reset to 100 by `begin_network_game_end` after submitting the
    /// end-of-round message buffer. Likely a "frames since last outgoing
    /// flush" countdown — semantics unconfirmed.
    pub _field_1c: i32,
    // Trailing fields unknown (GameNet::send_block reads +0x20 as another
    // BufferObject pointer).
}

bind_NetSessionVtable!(NetSession, vtable);

impl NetSession {
    /// Rust port of `NetSession__MaxActivePeerScore`.
    ///
    /// Iterates peers `0..peer_count`, calling `peer_active(i)` to filter
    /// and `peer_score(i)` to score. Returns the max score across active
    /// peers, excluding `self_peer_idx`. Non-positive scores don't raise
    /// the running max (initial value is 0).
    pub unsafe fn max_peer_score_raw(this: *mut NetSession) -> i32 {
        unsafe {
            let count = (*this).peer_count;
            let skip = (*this).self_peer_idx;
            let mut best: i32 = 0;
            for i in 0..count {
                if ((*(*this).vtable).peer_active)(this, i as u32) == 0 {
                    continue;
                }
                if i == skip {
                    continue;
                }
                let s = ((*(*this).vtable).peer_score)(this, i as u32);
                if s > best {
                    best = s;
                }
            }
            best
        }
    }
}
