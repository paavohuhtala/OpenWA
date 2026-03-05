use crate::fixed::Fixed;
use crate::task::Ptr32;

/// DSSound — DirectSound audio subsystem.
///
/// Created by DSSound__Constructor (0x573D50).
/// Vtable: 0x66AF20
///
/// Manages sound playback through DirectSound.
/// Has 500 sound channel slots and a master volume control.
///
/// PARTIAL: Only constructor-confirmed fields.
/// Ghidra uses DWORD-indexed offsets.
#[repr(C)]
pub struct DSSound {
    /// 0x000: Vtable pointer (0x66AF20)
    pub vtable: Ptr32,
    /// 0x004: Unknown
    pub _unknown_004: [u8; 4],
    /// 0x008: Init field (set to 0)
    pub _field_008: u32,
    /// 0x00C: Init field (set to 0)
    pub _field_00c: u32,
    /// 0x010: Init field (set to 0)
    pub _field_010: u32,
    /// 0x014-0x8A3: Unknown fields
    pub _unknown_014: [u8; 0x890],
    /// 0x8A4: Master volume (Fixed, init 0x10000 = 1.0)
    pub volume: Fixed,
    /// 0x8A8-0xBB3: Unknown (includes 500-entry channel slot table)
    pub _unknown_8a8: [u8; 0x30C],
    /// 0xBB4: Status flag 1 (init 1)
    pub status_1: u32,
    /// 0xBB8: Status flag 2 (init 1)
    pub status_2: u32,
    /// 0xBBC: Status flag 3 (init 0)
    pub status_3: u32,
    /// 0xBC0-end: Unknown trailing fields
    pub _unknown_bc0: [u8; 0x10],
}

// Total size uncertain — this is an estimate based on constructor analysis.
// The 500 dword channel table starts around offset 0xD4+ but exact layout unclear.
const _: () = assert!(core::mem::size_of::<DSSound>() == 0xBD0);
