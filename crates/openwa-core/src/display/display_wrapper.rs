// DDDisplayWrapper — CWormsApp rendering interface subobject.
// See DDDisplayWrapper struct and DDDisplayWrapperVtable below for full docs.

crate::define_addresses! {
    class "CWormsApp" {
        /// CWormsApp__FlipSurface — dispatches to renderer backend vtable[1]
        fn/Thiscall CWORMSAPP_FLIP_SURFACE = 0x005A_2700;
        /// ConstructFrameBuffer — allocates software framebuffer, inits renderer
        fn/Thiscall CONSTRUCT_FRAME_BUFFER_THISCALL = 0x005A_2430;
        /// CWormsApp__ReleaseFrameBuffer
        fn/Thiscall CWORMSAPP_RELEASE_FRAME_BUFFER = 0x005A_24A0;
        /// CWormsApp__FillRect — lock surface, memset rows, unlock
        fn/Thiscall CWORMSAPP_FILL_RECT = 0x005A_25C0;
        /// CWormsApp__BlitToFrameBuffer — optimized blit from surface to framebuffer
        fn/Thiscall CWORMSAPP_BLIT_TO_FRAME_BUFFER = 0x005A_2A40;
        /// CWormsApp__DrawLandscape — blit with clipping and transparency
        fn/Thiscall CWORMSAPP_DRAW_LANDSCAPE = 0x005A_2790;
        /// ClearFrameBuffer — memset(framebuffer, 0, w*h)
        fn CWORMSAPP_CLEAR_FRAME_BUFFER = 0x005A_23F0;
    }
}

/// DDDisplayWrapper vtable (0x662EC8, 25 slots).
///
/// Many slots are thin thunks that forward to the renderer backend
/// via `*(this+0x18)->vtable[N]`. Others manage the software framebuffer
/// or perform blitting directly.
///
/// ## Slot categories
///
/// - **Renderer thunks** (0, 2, 4, 5, 8, 10, 11): Forward to backend
/// - **Framebuffer management** (3, 6, 7, 9, 15/17, 16/18, 20): Manage G_FRAME_BUFFER_*
/// - **Blitting** (19, 23, 24): FillRect, DrawLandscape, BlitToFrameBuffer
/// - **Surface management** (12, 13, 22): Renderer queries and surface allocation
/// - **Stub** (14): Returns 0
#[openwa_core::vtable(size = 25, va = 0x0066_2EC8, class = "DDDisplayWrapper")]
pub struct DDDisplayWrapperVtable {
    /// renderer thunk -> backend vtable[0] (init/create) (0x4E3420, RET 0x8)
    #[slot(0)]
    pub renderer_init: fn(this: *mut DDDisplayWrapper, p2: u32, p3: u32),
    /// flip/present — dispatches to renderer backend vtable[1] (0x5A2700)
    #[slot(1)]
    pub flip_surface: fn(this: *mut DDDisplayWrapper),
    /// renderer thunk -> backend vtable[2] (reset state) (0x4E3440)
    #[slot(2)]
    pub renderer_reset: fn(this: *mut DDDisplayWrapper),
    /// get framebuffer dimensions — reads G_FRAME_BUFFER_WIDTH/HEIGHT (0x5A2660, RET 0x4)
    #[slot(3)]
    pub get_framebuffer_dims: fn(this: *mut DDDisplayWrapper, out: *mut u32) -> u32,
    /// renderer thunk -> backend vtable[4] (enum display modes) (0x4E3460, RET 0x8)
    #[slot(4)]
    pub renderer_enum_modes: fn(this: *mut DDDisplayWrapper, p2: u32, p3: u32),
    /// renderer thunk -> backend vtable[5] (tail jump) (0x4E3480)
    #[slot(5)]
    pub renderer_slot5: fn(this: *mut DDDisplayWrapper),
    /// construct framebuffer — wa_malloc(w*h), init renderer (0x5A2430, RET 0x8)
    #[slot(6)]
    pub construct_frame_buffer: fn(this: *mut DDDisplayWrapper, width: i32, height: i32) -> i32,
    /// release framebuffer — calls renderer teardown, frees buffer (0x5A24A0)
    #[slot(7)]
    pub release_frame_buffer: fn(this: *mut DDDisplayWrapper, p2: i32),
    /// renderer thunk -> backend vtable[8] (0x5A24F0)
    #[slot(8)]
    pub renderer_slot8: fn(this: *mut DDDisplayWrapper),
    /// lock framebuffer for pixel access (0x5A2530)
    ///
    /// Returns framebuffer pointer and stride for direct pixel manipulation.
    #[slot(9)]
    pub lock_framebuffer:
        fn(this: *mut DDDisplayWrapper, p2: u32, p3: u32, p4: u32, p5: u32) -> u32,
    /// renderer thunk -> backend vtable[10] (0x5A2510)
    #[slot(10)]
    pub renderer_restore_dims: fn(this: *mut DDDisplayWrapper),
    /// renderer thunk -> backend vtable[11] (restore surface) (0x5A25A0)
    #[slot(11)]
    pub renderer_restore_surface: fn(this: *mut DDDisplayWrapper),
    /// allocate error/result object (0x5A2720, RET 0x4)
    #[slot(12)]
    pub alloc_error_result: fn(this: *mut DDDisplayWrapper, p2: u32) -> u32,
    /// get renderer surface pointer (0x5A26C0)
    ///
    /// Queries `renderer->vtable[12]` with G_FRAME_BUFFER_PTR.
    #[slot(13)]
    pub get_renderer_surface: fn(this: *mut DDDisplayWrapper),
    /// stub — returns 0 (CGameTask__vt18, 0x545780)
    #[slot(14)]
    pub stub_ret_zero: fn(this: *mut DDDisplayWrapper) -> u32,
    /// lock surface for reading — returns framebuffer ptr + width (0x5A2690, RET 0x8)
    #[slot(15)]
    pub lock_surface_read: fn(this: *mut DDDisplayWrapper, p2: u32, p3: u32),
    /// unlock surface (0x5A2C10, RET 0x4)
    ///
    /// No-op for software framebuffer — just returns success code.
    #[slot(16)]
    pub unlock_surface_read: fn(this: *mut DDDisplayWrapper, p2: u32),
    /// lock surface for writing (0x5A2690 — same fn as slot 15, RET 0x8)
    #[slot(17)]
    pub lock_surface_write: fn(this: *mut DDDisplayWrapper, p2: u32, p3: u32),
    /// unlock surface after writing (0x5A2C10 — same fn as slot 16, RET 0x4)
    #[slot(18)]
    pub unlock_surface_write: fn(this: *mut DDDisplayWrapper, p2: u32),
    /// fill rectangle — lock, memset rows, unlock (0x5A25C0)
    ///
    /// Params: x, y, width, height, fill_value.
    #[slot(19)]
    pub fill_rect:
        fn(this: *mut DDDisplayWrapper, x: i32, y: i32, width: i32, height: i32, fill_value: u32),
    /// clear framebuffer — memset(ptr, 0, w*h) (0x5A23F0)
    #[slot(20)]
    pub clear_frame_buffer: fn(this: *mut DDDisplayWrapper),
    /// trampoline to slot 20 via vtable dispatch (0x5A2420)
    #[slot(21)]
    pub clear_frame_buffer_indirect: fn(this: *mut DDDisplayWrapper),
    /// allocate surface object — wa_malloc(0x14) (0x5A2760)
    #[slot(22)]
    pub alloc_surface: fn(this: *mut DDDisplayWrapper) -> *mut u8,
    /// draw landscape — blit with clipping and optional color-key transparency (0x5A2790)
    #[slot(23)]
    pub draw_landscape:
        fn(this: *mut DDDisplayWrapper, surface: *mut u8, y_offset: i32, flags: u32),
    /// blit surface to framebuffer — optimized copy with stride alignment (0x5A2A40)
    #[slot(24)]
    pub blit_to_frame_buffer: fn(this: *mut DDDisplayWrapper, surface: *mut u8, y_offset: i32),
}

/// DDDisplayWrapper struct — CWormsApp rendering dispatch subobject.
///
/// Stored as a global pointer at 0x79D6D4 (g_DDDisplayWrapper).
/// The full object layout is part of CWormsApp; only key fields are mapped here.
#[repr(C)]
pub struct DDDisplayWrapper {
    /// 0x00: Vtable pointer (0x662EC8)
    pub vtable: *const DDDisplayWrapperVtable,
    /// 0x04-0x17: Unknown (part of CWormsApp layout)
    pub _unknown_04: [u8; 0x14],
    /// 0x18: Pointer to renderer backend (CompatRenderer, OpenGLCPU, or DDraw object)
    pub renderer_backend: *mut u8,
}

const _: () = assert!(core::mem::size_of::<DDDisplayWrapper>() == 0x1C);

// Generate calling wrappers: DDDisplayWrapper::flip_surface(), etc.
// NOTE: The generated bind wrappers assume standard thiscall, but
// DDDisplayWrapper methods actually use MFC result-return convention
// (ECX=this, EDX=result_buf_ptr). Use call_wrapper_method() instead.
bind_DDDisplayWrapperVtable!(DDDisplayWrapper, vtable);

impl DDDisplayWrapper {
    /// Call a DDDisplayWrapper vtable method by slot index.
    ///
    /// DDDisplayWrapper methods use an MFC result-return convention:
    /// ECX = this, EDX = pointer to caller-allocated result buffer.
    /// The result is discarded by most callers.
    ///
    /// # Safety
    /// `this` must be a valid DDDisplayWrapper pointer with an initialized vtable.
    #[inline]
    pub unsafe fn call_method(this: *mut DDDisplayWrapper, slot: usize) {
        let vtable = (*this).vtable as *const u32;
        let func: u32 = *vtable.add(slot);
        let mut result_buf: [u32; 2] = [0; 2];
        core::arch::asm!(
            "call {func}",
            func = in(reg) func,
            in("ecx") this,
            inout("edx") result_buf.as_mut_ptr() => _,
            out("eax") _,
            clobber_abi("C"),
        );
    }

    /// Call a DDDisplayWrapper vtable method with one stack parameter.
    ///
    /// Same MFC convention as `call_method`, with an additional stack argument
    /// pushed before the call (callee cleans via RET N).
    ///
    /// # Safety
    /// `this` must be a valid DDDisplayWrapper pointer with an initialized vtable.
    #[inline]
    pub unsafe fn call_method_1(this: *mut DDDisplayWrapper, slot: usize, param: u32) {
        let vtable = (*this).vtable as *const u32;
        let func: u32 = *vtable.add(slot);
        let mut result_buf: [u32; 2] = [0; 2];
        core::arch::asm!(
            "push {param}",
            "call {func}",
            param = in(reg) param,
            func = in(reg) func,
            in("ecx") this,
            inout("edx") result_buf.as_mut_ptr() => _,
            out("eax") _,
            clobber_abi("C"),
        );
    }
}
