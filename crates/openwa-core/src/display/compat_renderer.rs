use core::ffi::c_void;

// CompatRenderer — DirectDraw/D3D rendering backend.
// See CompatRenderer struct and CompatRendererVtable below for full docs.

crate::define_addresses! {
    class "DDraw8Renderer" {
        vtable DDRAW8_RENDERER_VTABLE = 0x0067_67F8;
    }
}

#[openwa_core::vtable(size = 37, va = 0x0067_6A90, class = "CompatRenderer")]
pub struct CompatRendererVtable {
    // =====================================================================
    // Primary display management (slots 0-24)
    // =====================================================================
    /// init / DirectDrawCreate + QueryInterface(IDirectDraw2) (0x59D430)
    #[slot(0)]
    pub init: fn(this: *mut CompatRenderer, p2: u32, p3: u32),
    /// set cooperative level — fullscreen exclusive or windowed (0x59D5F0)
    #[slot(1)]
    pub set_cooperative_level: fn(this: *mut CompatRenderer),
    /// reset state — calls teardown, clears initialized flag (0x59D480)
    #[slot(2)]
    pub reset_state: fn(this: *mut CompatRenderer),
    /// get display size — returns stored width/height from +0x3C/+0x40 (0x59D510)
    #[slot(3)]
    pub get_display_size: fn(this: *mut CompatRenderer, out_w: *mut u32, out_h: *mut u32),
    /// enum display modes — IDirectDraw2::EnumDisplayModes (0x59D560)
    #[slot(4)]
    pub enum_display_modes: fn(this: *mut CompatRenderer, p2: u32, p3: u32),
    // Slot 5: stub (CGameTask__vt19)
    /// set display mode + create surfaces — the main initialization (0x59D6D0)
    ///
    /// Calls IDirectDraw2::SetDisplayMode, creates primary surface with back buffer
    /// (fullscreen) or with clipper (windowed). ~0x3F2 bytes.
    #[slot(6)]
    pub set_display_mode: fn(this: *mut CompatRenderer, width: i32, height: i32),
    /// teardown — release surfaces, restore display mode (0x59DAD0)
    ///
    /// Calls timeEndPeriod, releases primary/back/clipper surfaces,
    /// calls IDirectDraw2::RestoreDisplayMode, LockWindowUpdate(NULL).
    #[slot(7)]
    pub teardown: fn(this: *mut CompatRenderer, p2: i32),
    /// vsync stub — calls teardown with 0, effectively a no-op (0x59DCB0)
    #[slot(8)]
    pub vsync_stub: fn(this: *mut CompatRenderer),
    /// set display dimensions — thin wrapper to slot 6 (0x59DCE0)
    #[slot(9)]
    pub set_display_dims: fn(this: *mut CompatRenderer, width: i32, height: i32),
    /// restore display dimensions — calls slot 6 with stored w/h (0x59DCC0)
    #[slot(10)]
    pub restore_display_dims: fn(this: *mut CompatRenderer),
    /// restore lost surface — IDirectDrawSurface::Restore on primary (0x59D4B0)
    #[slot(11)]
    pub restore_surface: fn(this: *mut CompatRenderer) -> u32,
    // Slot 12: stub
    /// flip / present — end-of-frame IDirectDrawSurface::Flip (0x59DB70)
    ///
    /// Handles DDERR_SURFACELOST by calling RestoreSurface and retrying.
    #[slot(13)]
    pub flip: fn(this: *mut CompatRenderer),
    /// get hardware caps — IDirectDraw2::GetCaps for HAL and HEL (0x59DC80)
    #[slot(14)]
    pub get_caps: fn(this: *mut CompatRenderer, out: *mut u8),
    /// lock primary surface (read, 16bpp flags) (0x59D370)
    ///
    /// Returns pitch and pixel pointer through output params.
    #[slot(15)]
    pub lock_primary_read:
        fn(this: *mut CompatRenderer, out_pitch: *mut u32, out_ptr: *mut *mut u8),
    /// unlock primary surface (0x59D3B0)
    #[slot(16)]
    pub unlock_primary_read: fn(this: *mut CompatRenderer),
    /// lock primary surface (write, 32bpp flags) (0x59D390)
    #[slot(17)]
    pub lock_primary_write:
        fn(this: *mut CompatRenderer, out_pitch: *mut u32, out_ptr: *mut *mut u8),
    /// unlock primary surface — same fn as slot 16 (0x59D3B0)
    #[slot(18)]
    pub unlock_primary_write: fn(this: *mut CompatRenderer),
    /// blt filled rect to screen — IDirectDrawSurface::Blt with DDBLT_COLORFILL (0x59DDD0)
    #[slot(19)]
    pub blt_fill_rect:
        fn(this: *mut CompatRenderer, x: i32, y: i32, width: i32, height: i32, color: u32),
    /// blt clear both surfaces — clears front and back buffer (0x59DD00)
    #[slot(20)]
    pub blt_clear_both: fn(this: *mut CompatRenderer),
    /// blt clear current surface — clears active drawing surface (0x59DD70)
    #[slot(21)]
    pub blt_clear_current: fn(this: *mut CompatRenderer),
    // Slots 22-23: stubs
    /// striped blt helper — breaks area into horizontal bands (0x5A2080)
    #[slot(24)]
    pub blt_striped: fn(this: *mut CompatRenderer, surface: *mut u8, y_offset: i32),

    // =====================================================================
    // Offscreen surface sub-object (slots 25-36, via second vtable at +0x04)
    // =====================================================================
    /// sub-object destructor — releases offscreen surface (0x59E7B0)
    #[slot(25)]
    pub offscreen_destructor: fn(this: *mut CompatRenderer, flags: u8) -> *mut CompatRenderer,
    /// lock offscreen surface (0x59DF10)
    ///
    /// Handles DDERR_SURFACELOST with restore+retry.
    /// Returns pitch and pixel pointer (via ESI — usercall).
    #[slot(26)]
    pub lock_offscreen: fn(this: *mut CompatRenderer, out_pitch: *mut u32, out_ptr: *mut *mut u8),
    /// unlock offscreen surface — IDirectDrawSurface::Unlock on +0x10 (0x59E020)
    #[slot(27)]
    pub unlock_offscreen: fn(this: *mut CompatRenderer),
    /// lock offscreen surface variant (0x59E090)
    #[slot(28)]
    pub lock_offscreen_alt:
        fn(this: *mut CompatRenderer, out_pitch: *mut u32, out_ptr: *mut *mut u8),
    /// unlock offscreen surface — same fn as slot 27 (0x59E020)
    #[slot(29)]
    pub unlock_offscreen_alt: fn(this: *mut CompatRenderer),
    /// create offscreen plain surface — IDirectDraw2::CreateSurface (0x59E1D0)
    ///
    /// Creates DDSCAPS_OFFSCREENPLAIN surface, width 4-byte aligned,
    /// 8bpp or 32bpp. Falls back to system memory if VRAM alloc fails.
    #[slot(30)]
    pub create_offscreen_surface:
        fn(this: *mut CompatRenderer, width: i32, height: i32, bpp: i32) -> u32,
    /// release offscreen surface — IDirectDrawSurface::Release on +0x10 (0x59E3B0)
    #[slot(31)]
    pub release_offscreen_surface: fn(this: *mut CompatRenderer),
    /// set color key on offscreen surface (0x59E3F0)
    #[slot(32)]
    pub set_color_key: fn(this: *mut CompatRenderer, color: u32),
    /// get color key from offscreen surface (0x59E480)
    #[slot(33)]
    pub get_color_key: fn(this: *mut CompatRenderer, out_color: *mut u32),
    /// is offscreen surface lost — restore if DDERR_SURFACELOST (0x59E780)
    #[slot(34)]
    pub is_lost: fn(this: *mut CompatRenderer) -> u32,
    /// blt offscreen — IDirectDrawSurface::Blt with source/dest rects (0x59E4D0)
    #[slot(35)]
    pub blt_offscreen: fn(
        this: *mut CompatRenderer,
        dst_x: i32,
        dst_y: i32,
        dst_w: i32,
        dst_h: i32,
        src: *mut c_void,
    ),
    /// blt fast offscreen — IDirectDrawSurface::BltFast with mirror/colorkey (0x59E5F0)
    #[slot(36)]
    pub blt_fast_offscreen:
        fn(this: *mut CompatRenderer, dst_x: i32, dst_y: i32, src: *mut c_void, flags: u32),
}

/// CompatRenderer struct — DirectDraw/D3D rendering backend.
///
/// Wraps IDirectDraw2 and IDirectDrawSurface interfaces for screen presentation.
/// Selected for DIRECTDRAW8, DIRECTDRAW32, DIRECT3D_* display modes.
#[repr(C)]
pub struct CompatRenderer {
    /// 0x00: Primary vtable pointer (0x676A90)
    pub vtable: *const CompatRendererVtable,
    /// 0x04: Sub-object vtable pointer (offscreen surface ops, slots 25-36)
    pub offscreen_vtable: *const c_void,
    /// 0x08: Unknown
    pub _unknown_08: u32,
    /// 0x0C: IDirectDraw2 interface (used for offscreen surface creation)
    pub ddraw2_offscreen: *mut c_void,
    /// 0x10: IDirectDrawSurface (current offscreen surface)
    pub offscreen_surface: *mut c_void,
    /// 0x14: Timer period flag
    pub timer_period: u32,
    /// 0x18: Initialized flag (1 = DirectDraw created)
    pub initialized: u32,
    /// 0x1C: Unknown
    pub _unknown_1c: u32,
    /// 0x20: Windowed mode flag (0 = fullscreen exclusive, nonzero = windowed)
    pub windowed: u32,
    /// 0x24: Display mode field
    pub display_mode: u32,
    /// 0x28: IDirectDraw2 main interface
    pub ddraw2_main: *mut c_void,
    /// 0x2C: IDirectDrawSurface — primary/front surface
    pub primary_surface: *mut c_void,
    /// 0x30: IDirectDrawSurface — back buffer (windowed mode)
    pub back_buffer: *mut c_void,
    /// 0x34: Unknown
    pub _unknown_34: u32,
    /// 0x38: IDirectDrawClipper (windowed mode only)
    pub clipper: *mut c_void,
    /// 0x3C: Stored width
    pub width: u32,
    /// 0x40: Stored height
    pub height: u32,
}

const _: () = assert!(core::mem::size_of::<CompatRenderer>() == 0x44);

// Generate calling wrappers: CompatRenderer::flip(), etc.
bind_CompatRendererVtable!(CompatRenderer, vtable);
