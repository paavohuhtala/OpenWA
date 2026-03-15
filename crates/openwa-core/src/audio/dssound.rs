use crate::fixed::Fixed;
/// DSSound — DirectSound audio subsystem.
///
/// Created by DSSound__Constructor (0x573D50).
/// Vtable: 0x66AF20
/// Size: 0xBE0 bytes (allocated by GameEngine__InitHardware).
///
/// Manages sound playback through DirectSound.
/// Has 500 sound channel slots and a master volume control.
///
/// PARTIAL: Only constructor-confirmed and InitHardware-confirmed fields.
#[repr(C)]
pub struct DSSound {
    /// 0x000: Vtable pointer (0x66AF20)
    pub vtable: *mut u8,
    /// 0x004: HWND (set to hwnd after construction)
    pub hwnd: u32,
    /// 0x008: IDirectSound* (from DirectSoundCreate)
    pub direct_sound: *mut u8,
    /// 0x00C: Output param from DSSOUND_INIT_BUFFERS (primary buffer caps/format)
    pub primary_buffer_caps: u32,
    /// 0x010: IDirectSoundBuffer* (primary buffer, from DSSOUND_INIT_BUFFERS)
    pub primary_buffer: *mut u8,
    /// 0x014-0x8A3: Unknown fields (includes 500-entry channel slot table)
    pub _unknown_014: [u8; 0x890],
    /// 0x8A4: Master volume (Fixed, init 0x10000 = 1.0)
    pub volume: Fixed,
    /// 0x8A8-0xBB3: Unknown
    pub _unknown_8a8: [u8; 0x30C],
    /// 0xBB4: Status flag 1 (init 1)
    pub status_1: u32,
    /// 0xBB8: Status flag 2 (init 1)
    pub status_2: u32,
    /// 0xBBC: Init success flag — set to 1 when DirectSoundCreate +
    /// DSSOUND_INIT_BUFFERS + IDirectSoundBuffer::Play all succeed.
    pub init_success: u32,
    /// 0xBC0-0xBDF: Unknown trailing fields
    pub _unknown_bc0: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<DSSound>() == 0xBE0);
