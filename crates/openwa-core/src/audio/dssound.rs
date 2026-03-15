use crate::fixed::Fixed;

/// DSSound — DirectSound audio subsystem.
///
/// Created by DSSound__Constructor (0x573D50).
/// Vtable: 0x66AF20
/// Size: 0xBE0 bytes (allocated by GameEngine__InitHardware).
///
/// Manages sound playback through DirectSound.
/// Has 8 channel descriptors, 500 sound channel slots, a 64-entry
/// buffer pool, and a master volume control.

type Ptr32 = u32;

/// Channel descriptor (0x18 bytes). 8 entries at DSSound+0x14.
/// FUN_005742A0 initializes: field_00=-1, field_04=-1, field_10=0.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ChannelDescriptor {
    pub field_00: u32,   // init -1
    pub field_04: u32,   // init -1
    pub _pad_08: [u32; 2],
    pub field_10: u32,   // init 0
    pub _pad_14: u32,
}

const _: () = assert!(core::mem::size_of::<ChannelDescriptor>() == 0x18);

#[repr(C)]
pub struct DSSound {
    /// 0x000: Vtable pointer (0x66AF20)
    pub vtable: Ptr32,
    /// 0x004: HWND (set after construction)
    pub hwnd: Ptr32,
    /// 0x008: IDirectSound* (from DirectSoundCreate)
    pub direct_sound: Ptr32,
    /// 0x00C: Primary buffer caps/format output from init_buffers
    pub primary_buffer_caps: u32,
    /// 0x010: IDirectSoundBuffer* (primary buffer)
    pub primary_buffer: Ptr32,
    /// 0x014-0xD3: 8 channel descriptors (0x18 bytes each = 0xC0 total)
    pub channel_descs: [ChannelDescriptor; 8],
    /// 0x0D4-0x8A3: 500 channel slot indices (zeroed by constructor)
    pub channel_slots: [u32; 500],
    /// 0x8A4: Master volume (Fixed, init 0x10000 = 1.0)
    pub volume: Fixed,
    /// 0x8A8-0x9A7: 64 buffer pool shadow entries (init all -1 by FUN_00574260)
    pub buffer_pool_shadow: [u32; 64],
    /// 0x9A8-0xAA7: 64 buffer pool indices (init 0..63 by FUN_00574260)
    pub buffer_pool: [u32; 64],
    /// 0xAA8: Buffer pool count (init 0x40 = 64)
    pub buffer_pool_count: u32,
    /// 0xAAC-0xBAB: Unknown (0x100 bytes)
    pub _unknown_aac: [u8; 0x100],
    /// 0xBAC: Buffer pool state (init 0 by FUN_00574260)
    pub buffer_pool_state: u32,
    /// 0xBB0: Unknown field (zeroed by constructor)
    pub _field_bb0: u32,
    /// 0xBB4: Status flag 1 (init 1)
    pub status_1: u32,
    /// 0xBB8: Status flag 2 (init 1)
    pub status_2: u32,
    /// 0xBBC: Init success flag — set to 1 when DirectSoundCreate +
    /// init_buffers + IDirectSoundBuffer::Play all succeed.
    pub init_success: u32,
    /// 0xBC0-0xBDF: Unknown trailing fields
    pub _unknown_bc0: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<DSSound>() == 0xBE0);

impl DSSound {
    /// Construct a DSSound with all fields initialized, matching the
    /// original constructor (0x573D50) + helpers FUN_005742A0/FUN_00574260.
    ///
    /// Uses WA's vtable pointer for identity (same pattern as DisplayBase).
    ///
    /// # Safety
    /// Must be called from within the WA.exe process (needs rebased vtable).
    pub unsafe fn new(hwnd: u32) -> Self {
        use crate::address::va;
        use crate::rebase::rb;

        let mut snd: Self = core::mem::zeroed();

        // Vtable (WA's .rdata, identity-checked)
        snd.vtable = rb(va::DS_SOUND_VTABLE);

        // HWND
        snd.hwnd = hwnd;

        // Volume = 1.0 (16.16 fixed-point)
        snd.volume = Fixed(0x10000);

        // 8 channel descriptors: field_00=-1, field_04=-1, field_10=0
        // (rest is zero from mem::zeroed)
        for desc in &mut snd.channel_descs {
            desc.field_00 = 0xFFFF_FFFF;
            desc.field_04 = 0xFFFF_FFFF;
        }

        // 500 channel slots: already zeroed

        // Buffer pool: shadow entries all -1, indices 0..63
        for (i, slot) in snd.buffer_pool.iter_mut().enumerate() {
            *slot = i as u32;
        }
        for entry in &mut snd.buffer_pool_shadow {
            *entry = 0xFFFF_FFFF;
        }
        snd.buffer_pool_count = 64;

        // Status flags
        snd.status_1 = 1;
        snd.status_2 = 1;
        // init_success stays 0 until COM init succeeds

        snd
    }
}
