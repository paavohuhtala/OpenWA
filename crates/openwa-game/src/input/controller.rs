use core::ffi::c_void;

#[openwa_game::vtable(size = 1, va = 0x0066B3FC, class = "NetInputCtrl")]
pub struct NetInputCtrlVtable {
    /// Destructor(this, flags) — scalar deleting destructor
    #[slot(0)]
    pub destructor: fn(this: *mut NetInputCtrl, flags: u32),
}

/// Network multiplayer peer state. Six instances live in a global array at
/// `0x007C1078` (stride 0x19118 = 0x6446 dwords); one slot per logical team.
/// Populated by lobby setup (`FrontendLobbyHost__Constructor` and friends)
/// before `Frontend::LaunchGameSession` is invoked from a lobby dialog.
///
/// Iterated by [`Network__DispatchPeerMessages`](https://internal.ghidra/0x004B5F30):
/// when `status == 3` (active connected peer), the dispatcher invokes
/// [`Network__ProcessPeerMessage`](https://internal.ghidra/0x004B6AF0) (or
/// `FUN_004AC570` for the host-side variant) to read pending messages off
/// this peer's network stream.
///
/// Pointers to each *active* slot are gathered into the
/// [`NetInputCtrl::peer_connections`] array (with `peer_connection_count`) by
/// `Network__BuildPeerConnectionsArray` (0x00466F70) — that's the array
/// passed to `LaunchGameSession`'s arg3/arg4 from the four lobby-launch
/// callers (FUN_004aec50, FUN_004b77b0, FUN_004bd5d0, FUN_004c1720).
///
/// Currently only the vtable is typed; full layout is opaque.
#[repr(C)]
pub struct PeerState {
    /// 0x000: Vtable pointer. Slot at +0x10 reads incoming messages from the
    /// peer's stream, slot at +0x1C advances state, slot at +0x34 sends
    /// outgoing data.
    pub vtable: *const c_void,
    _opaque: [u8; 0],
}

/// Per-team 0x68-byte record inside [`NetInputCtrl`] starting at offset 0x28.
/// `NetInputCtrl::Init` populates `num_teams` of these by copying 0x50 bytes
/// verbatim from `GameInfo.team_input_configs[i]` into `data`, then setting
/// the trailing 5 dwords (+0x50..+0x60) to -1, and storing a freshly
/// allocated [`InputBuffer`] in the preceding pre-record slot (so each
/// record's `buffer_ptr` actually lives at offset `+0x28 + i*0x68 - 4`).
///
/// The `data` payload is mostly opaque to NetInputCtrl::Init — it just memcpys
/// it. The downstream consumer (input dispatch / replay capture) is what
/// actually interprets the fields.
#[repr(C)]
pub struct NetInputCtrlTeamRecord {
    /// 0x00: Pre-record buffer pointer slot. WA writes the buffer pointer
    /// here at `&record - 4` rather than into the record itself, but laying
    /// the field at the start of the next record is equivalent and lets us
    /// avoid negative-offset arithmetic. Carries an [`InputBuffer`] (0x4000
    /// payload) per team.
    pub prev_record_buffer_ptr: *mut InputBuffer,
    /// 0x04..0x54: Verbatim copy of the team's input config from GameInfo.
    pub config_data: [u8; 0x50],
    /// 0x54..0x68: Trailing 5 dwords initialized to -1 by `NetInputCtrl::Init`.
    pub trailing_markers: [i32; 5],
}

const _: () = assert!(core::mem::size_of::<NetInputCtrlTeamRecord>() == 0x68);

/// Allocation wrapper for a heap-allocated input buffer (input ring or per-team).
/// Created by `NetInputCtrl__InitTeamInputs_Maybe` (0x0053DD50). 0x3c bytes total.
#[repr(C)]
pub struct InputBuffer {
    /// 0x00: Pointer to the heap buffer payload.
    pub data: *mut u8,
    /// 0x04: Capacity in bytes (0x1000 for the global input ring,
    /// 0x4000 for per-team buffers).
    pub capacity: u32,
    /// 0x08..0x1c: Five dwords zeroed at construction.
    pub _zeros: [u32; 5],
    /// 0x1c..0x3c: Remainder of the wrapper struct.
    pub _unknown_1c: [u8; 0x3c - 0x1c],
}

const _: () = assert!(core::mem::size_of::<InputBuffer>() == 0x3c);

/// NetInputCtrl — **network multiplayer input subsystem** (lobby-launched games only).
///
/// Initializer: `NetInputCtrl__Init` at 0x0058C0D0, usercall(ESI=this, EAX=starting_team_index,
/// ECX=num_teams) + stdcall(4 stack params), RET 0x10. Vtable: 0x66B3FC. Size: 0x1800 bytes.
///
/// **Only allocated for lobby-launched multiplayer**. `GameEngine::InitHardware` skips
/// creation entirely when `peer_connection_count == 0`, which is the case for every
/// non-lobby launch path (single-player, replay playback, hotseat, the `MainNavigationLoop`
/// fall-through, etc.). The four `LaunchGameSession` callers that pass non-zero are all
/// reachable from multiplayer lobby dialog code (`FrontendLobbyHost__*` etc); they call
/// `Network__BuildPeerConnectionsArray` (0x00466F70) to gather pointers to active
/// [`PeerState`] slots from the global array at `g_PeerStates` (0x007C1078).
///
/// Stored at `GameSession+0xB8`. Hypothesis (not yet verified by direct producer trace):
/// the per-team 16 KB rings ([`team_records[i].prev_record_buffer_ptr`]) hold per-frame
/// command messages received from each peer, consumed by the simulation in deterministic
/// lockstep order. The control-message dispatcher
/// [`Network__ProcessPeerMessage`](https://internal.ghidra/0x004B6AF0) handles handshake
/// and lobby-options messages, but per-frame command messages presumably land via a
/// different path that we haven't traced yet.
///
/// PARTIAL: The fields written by `NetInputCtrl::Init` are typed below. Internal state
/// updated by other code paths (input polling, command dispatch, network packet handling)
/// is still opaque.
#[repr(C)]
pub struct NetInputCtrl {
    /// 0x000: Vtable pointer (0x66B3FC)
    pub vtable: *const NetInputCtrlVtable,
    pub _unknown_004: [u8; 4],
    /// 0x008: Number of populated `team_records` entries, copied from `GameInfo.num_teams`.
    pub num_teams: i32,
    /// 0x00C: Index of the team whose turn starts the round, copied from
    /// `GameInfo.starting_team_index` (sign-extended from the i8).
    pub starting_team_index: i32,
    pub _unknown_010: u32,
    /// 0x014: Snapshot of `GameInfo.game_version` at init time. Used for
    /// later version-gated behavior in input dispatch.
    pub game_version: i32,
    pub _unknown_018: u32,
    /// 0x01C: Constant 100 set by `NetInputCtrl::Init` — purpose unknown.
    pub field_1c: u32,
    /// 0x020: Allocation wrapper for the global input ring buffer (0x1000-byte payload).
    pub input_ring: *mut InputBuffer,
    pub _unknown_024: u32,
    /// 0x028: Per-team records (max 6). Only `num_teams` entries are populated;
    /// the rest are zero from the parent's `_memset`.
    pub team_records: [NetInputCtrlTeamRecord; MAX_NET_INPUT_CTRL_TEAMS],
    pub _unknown_298: [u8; 0xD74 - 0x298],
    /// 0xD74: Set to 0x3F9 during inline construction (in `GameEngine::InitHardware`,
    /// before `NetInputCtrl::Init` runs). Purpose unknown.
    pub field_d74: u32,
    pub _unknown_d78: [u8; 0xE80 - 0xD78],
    /// 0xE80..0xE90: Four `i32`-sized markers initialized to -1 by `NetInputCtrl::Init`.
    pub markers_e80: [i32; 4],
    /// 0xE90: u32 zeroed by `NetInputCtrl::Init`.
    pub field_e90: u32,
    /// 0xE94..(0xE94 + num_teams*4): Per-team `i32` flags initialized to 1
    /// (only the first `num_teams` entries are written).
    pub flags_e94: [i32; MAX_NET_INPUT_CTRL_TEAMS],
    pub _unknown_eac: [u8; 0xEC8 - 0xEAC],
    /// 0xEC8: u32 zeroed by `NetInputCtrl::Init`.
    pub field_ec8: u32,
    /// 0xECC..0xF00: 13 `i32`-sized slots initialized to -1, then the first
    /// `num_teams` entries overwritten with 1.
    pub slots_ecc: [i32; 13],
    /// 0xF00..(0xF00 + peer_connection_count*4): Pointers to the active
    /// [`PeerState`] slots gathered by `Network__BuildPeerConnectionsArray`
    /// (0x00466F70) before `LaunchGameSession`. Each `PeerState`'s `+0xBC`
    /// byte is set to 2 by `NetInputCtrl::Init` (likely an "active in this
    /// session" flag — semantics TBD).
    pub peer_connections: [*mut PeerState; 12],
    /// 0xF30: Number of active peers — count of valid entries in `peer_connections`.
    pub peer_connection_count: i32,
    pub _unknown_f34: [u8; 0x1734 - 0xF34],
    /// 0x1734..0x176C: 14 `i32`-sized slots initialized to 1.
    pub slots_1734: [i32; 14],
    /// 0x176C..0x17D0: 25 `i32`-sized slots zeroed.
    pub slots_176c: [i32; 25],
    /// 0x17D0: u32 zeroed by `NetInputCtrl::Init`.
    pub field_17d0: u32,
    /// 0x17D4: u32 set to 1 by `NetInputCtrl::Init`.
    pub field_17d4: u32,
    /// 0x17D8: i32 set to -1 by `NetInputCtrl::Init` (before the team-record init).
    pub field_17d8: i32,
    pub _unknown_17dc: [u8; 0x1800 - 0x17DC],
}

/// Maximum number of teams the input controller tracks. Matches
/// [`MAX_TEAM_RECORDS`](crate::engine::game_info::MAX_TEAM_RECORDS).
pub const MAX_NET_INPUT_CTRL_TEAMS: usize = 6;

const _: () = assert!(core::mem::size_of::<NetInputCtrl>() == 0x1800);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, num_teams) == 0x008);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, starting_team_index) == 0x00C);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, game_version) == 0x014);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, field_1c) == 0x01C);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, input_ring) == 0x020);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, team_records) == 0x028);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, markers_e80) == 0xE80);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, field_e90) == 0xE90);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, flags_e94) == 0xE94);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, field_ec8) == 0xEC8);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, slots_ecc) == 0xECC);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, peer_connections) == 0xF00);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, peer_connection_count) == 0xF30);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, slots_1734) == 0x1734);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, slots_176c) == 0x176C);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, field_17d0) == 0x17D0);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, field_17d4) == 0x17D4);
const _: () = assert!(core::mem::offset_of!(NetInputCtrl, field_17d8) == 0x17D8);

// Generate calling wrappers: NetInputCtrl::destructor()
bind_NetInputCtrlVtable!(NetInputCtrl, vtable);

impl NetInputCtrl {
    /// Vtable[0]: Destroy and optionally free (flags & 1 = free).
    pub unsafe fn destroy(&mut self, flags: u32) {
        unsafe {
            self.destructor(flags);
        }
    }
}

// ─── NetInputCtrl::Init port ────────────────────────────────────────────────────

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::rebase::rb;

/// Address of `NetInputCtrl__InitTeamInputs_Maybe` (0x0053DD50), captured at
/// startup. Set by [`init_addrs`].
static mut INIT_TEAM_INPUTS_ADDR: u32 = 0;

/// Initialize address-table entries used by [`init_net_input_ctrl`] and the
/// inner bridge. Must be called once at DLL load.
pub fn init_addrs() {
    unsafe {
        INIT_TEAM_INPUTS_ADDR = rb(va::NET_INPUT_CTRL_INIT_TEAM_INPUTS);
    }
}

/// Bridge to `NetInputCtrl__InitTeamInputs_Maybe` (0x0053DD50).
///
/// WA convention: `__usercall(ECX = game_version)` + `stdcall(input_ctrl,
/// team_configs, num_teams, starting_team_index)`, `RET 0x10`. Allocates the
/// global input ring (`input_ring`) and per-team buffers
/// (`team_records[i].prev_record_buffer_ptr`), memcpys 0x50-byte team configs
/// from `team_configs[i]` into `team_records[i].config_data`, and writes
/// `starting_team_index` / `game_version` / `field_1c=100` / `markers_e80=-1`
/// / `field_10=1`. Always returns 1.
#[unsafe(naked)]
unsafe extern "stdcall" fn call_init_team_inputs(
    _input_ctrl: *mut NetInputCtrl,
    _team_configs: *const crate::engine::game_info::TeamInputConfig,
    _num_teams: u32,
    _starting_team_index: i32,
    _game_version: i32,
) -> u32 {
    // Stack on entry (5 stack args declared):
    //   [ESP+0]  = our_ret
    //   [ESP+4]  = arg1 input_ctrl
    //   [ESP+8]  = arg2 team_configs
    //   [ESP+0xC]= arg3 num_teams
    //   [ESP+0x10]= arg4 starting_team_index
    //   [ESP+0x14]= arg5 game_version  (loaded into ECX)
    //
    // FUN_0053dd50 expects 4 stack args + ECX register, RET 0x10.
    // We pop our_ret into EDX, set ECX from arg5, CALL the function (which
    // sees arg1..arg4 + its own pushed ret), then clean arg5 ourselves and
    // jump back to our_ret. Net effect: stdcall caller sees 5 args cleaned.
    core::arch::naked_asm!(
        "popl %edx",
        "movl 0x10(%esp), %ecx",
        "calll *({fn})",
        "addl $4, %esp",
        "jmpl *%edx",
        fn = sym INIT_TEAM_INPUTS_ADDR,
        options(att_syntax),
    );
}

/// Rust port of `NetInputCtrl::Init` (0x0058C0D0).
///
/// Inputs:
/// - `ctrl`: freshly allocated, `_memset`-zeroed `NetInputCtrl` (vtable already
///   set by the caller). `field_d74 = 0x3F9` is also written by the caller
///   pre-call (matches WA's inline construction).
/// - `game_info`: source of `num_teams`, `starting_team_index`, `game_version`,
///   and `team_input_configs[..]`.
/// - `peer_connections` / `_count`: array of pointers to active [`PeerState`]
///   slots from `g_PeerStates` (0x007C1078), gathered by
///   `Network__BuildPeerConnectionsArray` (0x00466F70). Threaded through from
///   `Frontend::LaunchGameSession`'s arg3 / arg4 — only set by the four
///   lobby-launched callers, otherwise 0/0 and NetInputCtrl is skipped entirely.
///   Each `PeerState`'s `+0xBC` byte is set to 2 by this init (likely an
///   "active in this session" flag).
///
/// Returns 1 on success, 0 if the inner team-buffer allocator fails (the
/// WA function technically always returns 1 today, but the check is kept
/// for fidelity with the pre-`if (iVar2 != 0)` guard).
pub unsafe fn init_net_input_ctrl(
    ctrl: *mut NetInputCtrl,
    game_info: *mut GameInfo,
    peer_connections: *const *mut PeerState,
    peer_connection_count: u32,
) -> u32 {
    unsafe {
        let gi = &*game_info;
        let starting_team_index = gi.starting_team_index as i32; // sign-ext from i8
        let num_teams = gi.num_teams as u32;
        let game_version = gi.game_version;

        (*ctrl).field_17d8 = -1;

        // Inner: malloc the global ring + per-team buffers, memcpy 0x50-byte
        // team configs from GameInfo.team_input_configs, set field_1c/0xc/0x14.
        let ok = call_init_team_inputs(
            ctrl,
            gi.team_input_configs.as_ptr(),
            num_teams,
            starting_team_index,
            game_version,
        );
        if ok == 0 {
            return 0;
        }

        // Copy peer_connections array into ctrl[0xF00..] and store count.
        for i in 0..peer_connection_count as usize {
            (*ctrl).peer_connections[i] = *peer_connections.add(i);
        }
        (*ctrl).peer_connection_count = peer_connection_count as i32;

        // 13 dwords at 0xECC = -1, then first num_teams entries overwritten with 1.
        (*ctrl).slots_ecc = [-1; 13];
        for i in 0..num_teams as usize {
            (*ctrl).slots_ecc[i] = 1;
        }

        // 14 dwords at 0x1734 = 1.
        (*ctrl).slots_1734 = [1; 14];

        // 25 dwords at 0x176C = 0 (already zero from memset; explicit for fidelity).
        (*ctrl).slots_176c = [0; 25];
        (*ctrl).field_17d0 = 0;
        (*ctrl).field_17d4 = 1;

        // For each peer connection, set the byte at PeerState+0xBC to 2.
        for i in 0..peer_connection_count as usize {
            let peer = *peer_connections.add(i);
            *(peer as *mut u8).add(0xBC) = 2;
        }

        // Per-team flags at 0xE94 = 1 for first num_teams entries.
        (*ctrl).field_e90 = 0;
        for i in 0..num_teams as usize {
            (*ctrl).flags_e94[i] = 1;
        }

        (*ctrl).field_ec8 = 0;
        1
    }
}
