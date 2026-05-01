#[openwa_game::vtable(size = 1, va = 0x0066B3FC, class = "InputCtrl")]
pub struct InputCtrlVtable {
    /// Destructor(this, flags) — scalar deleting destructor
    #[slot(0)]
    pub destructor: fn(this: *mut InputCtrl, flags: u32),
}

/// Per-team 0x68-byte record inside [`InputCtrl`] starting at offset 0x28.
/// `InputCtrl::Init` populates `num_teams` of these by copying 0x50 bytes
/// verbatim from `GameInfo.team_input_configs[i]` into `data`, then setting
/// the trailing 5 dwords (+0x50..+0x60) to -1, and storing a freshly
/// allocated [`InputBuffer`] in the preceding pre-record slot (so each
/// record's `buffer_ptr` actually lives at offset `+0x28 + i*0x68 - 4`).
///
/// The `data` payload is mostly opaque to InputCtrl::Init — it just memcpys
/// it. The downstream consumer (input dispatch / replay capture) is what
/// actually interprets the fields.
#[repr(C)]
pub struct InputCtrlTeamRecord {
    /// 0x00: Pre-record buffer pointer slot. WA writes the buffer pointer
    /// here at `&record - 4` rather than into the record itself, but laying
    /// the field at the start of the next record is equivalent and lets us
    /// avoid negative-offset arithmetic. Carries an [`InputBuffer`] (0x4000
    /// payload) per team.
    pub prev_record_buffer_ptr: *mut InputBuffer,
    /// 0x04..0x54: Verbatim copy of the team's input config from GameInfo.
    pub config_data: [u8; 0x50],
    /// 0x54..0x68: Trailing 5 dwords initialized to -1 by `InputCtrl::Init`.
    pub trailing_markers: [i32; 5],
}

const _: () = assert!(core::mem::size_of::<InputCtrlTeamRecord>() == 0x68);

/// Allocation wrapper for a heap-allocated input buffer (input ring or per-team).
/// Created by `InputCtrl__InitTeamInputs_Maybe` (0x0053DD50). 0x3c bytes total.
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

/// InputCtrl — input controller subsystem.
///
/// Initializer: `InputCtrl__Init` at 0x0058C0D0, usercall(ESI=this, EAX=starting_team_index,
/// ECX=num_teams) + stdcall(4 stack params), RET 0x10. Vtable: 0x66B3FC. Size: 0x1800 bytes.
///
/// Created by `GameEngine::InitHardware` when its `controlled_display_count` arg is non-zero.
/// Stored at `GameSession+0xB8`.
///
/// PARTIAL: The fields written by `InputCtrl::Init` are typed below. Almost everything else
/// (input polling, dispatch, replay capture) is still opaque.
#[repr(C)]
pub struct InputCtrl {
    /// 0x000: Vtable pointer (0x66B3FC)
    pub vtable: *const InputCtrlVtable,
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
    /// 0x01C: Constant 100 set by `InputCtrl::Init` — purpose unknown.
    pub field_1c: u32,
    /// 0x020: Allocation wrapper for the global input ring buffer (0x1000-byte payload).
    pub input_ring: *mut InputBuffer,
    pub _unknown_024: u32,
    /// 0x028: Per-team records (max 6). Only `num_teams` entries are populated;
    /// the rest are zero from the parent's `_memset`.
    pub team_records: [InputCtrlTeamRecord; MAX_INPUT_CTRL_TEAMS],
    pub _unknown_298: [u8; 0xD74 - 0x298],
    /// 0xD74: Set to 0x3F9 during inline construction (in `GameEngine::InitHardware`,
    /// before `InputCtrl::Init` runs). Purpose unknown.
    pub field_d74: u32,
    pub _unknown_d78: [u8; 0xE80 - 0xD78],
    /// 0xE80..0xE90: Four `i32`-sized markers initialized to -1 by `InputCtrl::Init`.
    pub markers_e80: [i32; 4],
    /// 0xE90: u32 zeroed by `InputCtrl::Init`.
    pub field_e90: u32,
    /// 0xE94..(0xE94 + num_teams*4): Per-team `i32` flags initialized to 1
    /// (only the first `num_teams` entries are written).
    pub flags_e94: [i32; MAX_INPUT_CTRL_TEAMS],
    pub _unknown_eac: [u8; 0xEC8 - 0xEAC],
    /// 0xEC8: u32 zeroed by `InputCtrl::Init`.
    pub field_ec8: u32,
    /// 0xECC..0xF00: 13 `i32`-sized slots initialized to -1, then the first
    /// `num_teams` entries overwritten with 1.
    pub slots_ecc: [i32; 13],
    /// 0xF00..(0xF00 + display_count*4): The verbatim copy of the
    /// `controlled_displays` array passed to `InputCtrl::Init`. Each entry
    /// is a pointer to a "controlled display" object whose `+0xBC` byte is
    /// set to 2 by the same init.
    pub controlled_displays: [*mut u8; 12],
    /// 0xF30: Number of valid entries in `controlled_displays`.
    pub controlled_display_count: i32,
    pub _unknown_f34: [u8; 0x1734 - 0xF34],
    /// 0x1734..0x176C: 14 `i32`-sized slots initialized to 1.
    pub slots_1734: [i32; 14],
    /// 0x176C..0x17D0: 25 `i32`-sized slots zeroed.
    pub slots_176c: [i32; 25],
    /// 0x17D0: u32 zeroed by `InputCtrl::Init`.
    pub field_17d0: u32,
    /// 0x17D4: u32 set to 1 by `InputCtrl::Init`.
    pub field_17d4: u32,
    /// 0x17D8: i32 set to -1 by `InputCtrl::Init` (before the team-record init).
    pub field_17d8: i32,
    pub _unknown_17dc: [u8; 0x1800 - 0x17DC],
}

/// Maximum number of teams the input controller tracks. Matches
/// [`MAX_TEAM_RECORDS`](crate::engine::game_info::MAX_TEAM_RECORDS).
pub const MAX_INPUT_CTRL_TEAMS: usize = 6;

const _: () = assert!(core::mem::size_of::<InputCtrl>() == 0x1800);
const _: () = assert!(core::mem::offset_of!(InputCtrl, num_teams) == 0x008);
const _: () = assert!(core::mem::offset_of!(InputCtrl, starting_team_index) == 0x00C);
const _: () = assert!(core::mem::offset_of!(InputCtrl, game_version) == 0x014);
const _: () = assert!(core::mem::offset_of!(InputCtrl, field_1c) == 0x01C);
const _: () = assert!(core::mem::offset_of!(InputCtrl, input_ring) == 0x020);
const _: () = assert!(core::mem::offset_of!(InputCtrl, team_records) == 0x028);
const _: () = assert!(core::mem::offset_of!(InputCtrl, markers_e80) == 0xE80);
const _: () = assert!(core::mem::offset_of!(InputCtrl, field_e90) == 0xE90);
const _: () = assert!(core::mem::offset_of!(InputCtrl, flags_e94) == 0xE94);
const _: () = assert!(core::mem::offset_of!(InputCtrl, field_ec8) == 0xEC8);
const _: () = assert!(core::mem::offset_of!(InputCtrl, slots_ecc) == 0xECC);
const _: () = assert!(core::mem::offset_of!(InputCtrl, controlled_displays) == 0xF00);
const _: () = assert!(core::mem::offset_of!(InputCtrl, controlled_display_count) == 0xF30);
const _: () = assert!(core::mem::offset_of!(InputCtrl, slots_1734) == 0x1734);
const _: () = assert!(core::mem::offset_of!(InputCtrl, slots_176c) == 0x176C);
const _: () = assert!(core::mem::offset_of!(InputCtrl, field_17d0) == 0x17D0);
const _: () = assert!(core::mem::offset_of!(InputCtrl, field_17d4) == 0x17D4);
const _: () = assert!(core::mem::offset_of!(InputCtrl, field_17d8) == 0x17D8);

// Generate calling wrappers: InputCtrl::destructor()
bind_InputCtrlVtable!(InputCtrl, vtable);

impl InputCtrl {
    /// Vtable[0]: Destroy and optionally free (flags & 1 = free).
    pub unsafe fn destroy(&mut self, flags: u32) {
        unsafe {
            self.destructor(flags);
        }
    }
}

// ─── InputCtrl::Init port ────────────────────────────────────────────────────

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::rebase::rb;

/// Address of `InputCtrl__InitTeamInputs_Maybe` (0x0053DD50), captured at
/// startup. Set by [`init_addrs`].
static mut INIT_TEAM_INPUTS_ADDR: u32 = 0;

/// Initialize address-table entries used by [`init_input_ctrl`] and the
/// inner bridge. Must be called once at DLL load.
pub fn init_addrs() {
    unsafe {
        INIT_TEAM_INPUTS_ADDR = rb(va::INPUT_CTRL_INIT_TEAM_INPUTS);
    }
}

/// Bridge to `InputCtrl__InitTeamInputs_Maybe` (0x0053DD50).
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
    _input_ctrl: *mut InputCtrl,
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

/// Rust port of `InputCtrl::Init` (0x0058C0D0).
///
/// Inputs:
/// - `ctrl`: freshly allocated, `_memset`-zeroed `InputCtrl` (vtable already
///   set by the caller). `field_d74 = 0x3F9` is also written by the caller
///   pre-call (matches WA's inline construction).
/// - `game_info`: source of `num_teams`, `starting_team_index`, `game_version`,
///   and `team_input_configs[..]`.
/// - `controlled_displays` / `_count`: array of "controlled display" object
///   pointers + length, threaded through from `Frontend::LaunchGameSession`'s
///   `p3` / `p4` args. Each display has its `+0xBC` byte set to 2 by this init.
///
/// Returns 1 on success, 0 if the inner team-buffer allocator fails (the
/// WA function technically always returns 1 today, but the check is kept
/// for fidelity with the pre-`if (iVar2 != 0)` guard).
pub unsafe fn init_input_ctrl(
    ctrl: *mut InputCtrl,
    game_info: *mut GameInfo,
    controlled_displays: *const *mut u8,
    controlled_display_count: u32,
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

        // Copy controlled_displays array into ctrl[0xF00..] and store count.
        for i in 0..controlled_display_count as usize {
            (*ctrl).controlled_displays[i] = *controlled_displays.add(i);
        }
        (*ctrl).controlled_display_count = controlled_display_count as i32;

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

        // For each controlled display, set byte at +0xBC to 2.
        for i in 0..controlled_display_count as usize {
            let display = *controlled_displays.add(i);
            *display.add(0xBC) = 2;
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
