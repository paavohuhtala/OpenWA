// RenderContext — CWormsApp rendering dispatch subobject.
// Manages the software framebuffer and dispatches rendering calls to the active
// renderer backend (CompatRenderer, OpenGLCPU, or DDraw).

crate::define_addresses! {
    class "CWormsApp" {
        /// CWormsApp__FlipSurface — dispatches to renderer backend vtable[1]
        fn/Fastcall CWORMSAPP_FLIP_SURFACE = 0x005A_2700;
        /// ConstructFrameBuffer — allocates software framebuffer, inits renderer
        fn/Fastcall CONSTRUCT_FRAME_BUFFER_THISCALL = 0x005A_2430;
        /// CWormsApp__ReleaseFrameBuffer
        fn/Fastcall CWORMSAPP_RELEASE_FRAME_BUFFER = 0x005A_24A0;
        /// CWormsApp__FillRect — lock surface, memset rows, unlock
        fn/Fastcall CWORMSAPP_FILL_RECT = 0x005A_25C0;
        /// CWormsApp__BlitToFrameBuffer — optimized blit from surface to framebuffer
        fn/Fastcall CWORMSAPP_BLIT_TO_FRAME_BUFFER = 0x005A_2A40;
        /// CWormsApp__DrawLandscape — blit with clipping and transparency
        fn/Fastcall CWORMSAPP_DRAW_LANDSCAPE = 0x005A_2790;
        /// ClearFrameBuffer — memset(framebuffer, 0, w*h)
        fn CWORMSAPP_CLEAR_FRAME_BUFFER = 0x005A_23F0;
    }
}

/// Result buffer for RenderContext and CompatRenderer `__fastcall` vtable calls.
///
/// These classes use `__fastcall` convention: ECX = this, EDX = result buffer pointer.
/// The callee writes a result (typically an HRESULT or pointer) into `value` and
/// returns `EDX` in `EAX`. Most callers allocate this on the stack and ignore the result.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FastcallResult {
    pub value: u32,
    _pad: u32,
}

/// RenderContext vtable (0x662EC8, 25 slots).
///
/// Many slots are thin thunks that forward to the renderer backend
/// via `*(this+0x18)->vtable[N]`. Others manage the software framebuffer
/// or perform blitting directly.
///
/// ## Calling convention
///
/// All methods use `__fastcall`: ECX = this, EDX = `*mut FastcallResult` (caller-allocated
/// 8-byte result buffer). Remaining parameters are on the stack, callee-cleaned.
/// Most callers ignore the result value.
///
/// ## Slot categories
///
/// - **Renderer thunks** (0, 2, 4, 5, 8, 10, 11): Forward to backend
/// - **Framebuffer management** (3, 6, 7, 9, 15/17, 16/18, 20): Manage G_FRAME_BUFFER_*
/// - **Blitting** (19, 23, 24): FillRect, DrawLandscape, BlitToFrameBuffer
/// - **Surface management** (12, 13, 22): Renderer queries and surface allocation
/// - **Stub** (14): Returns 0
#[openwa_core::vtable(size = 25, va = 0x0066_2EC8, class = "RenderContext")]
pub struct RenderContextVtable {
    /// renderer thunk -> backend vtable[0] (init/create) (0x4E3420, RET 0x8)
    #[slot(0)]
    pub renderer_init: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: u32,
        p3: u32,
    ) -> *mut FastcallResult,
    /// flip/present — dispatches to renderer backend vtable[1] (0x5A2700)
    #[slot(1)]
    pub flip_surface: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// renderer thunk -> backend vtable[2] (reset state) (0x4E3440)
    #[slot(2)]
    pub renderer_reset: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// get framebuffer dimensions — reads G_FRAME_BUFFER_WIDTH/HEIGHT (0x5A2660, RET 0x4)
    #[slot(3)]
    pub get_framebuffer_dims: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        out: *mut u32,
    ) -> *mut FastcallResult,
    /// renderer thunk -> backend vtable[4] (enum display modes) (0x4E3460, RET 0x8)
    #[slot(4)]
    pub renderer_enum_modes: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: u32,
        p3: u32,
    ) -> *mut FastcallResult,
    /// renderer thunk -> backend vtable[5] (tail jump) (0x4E3480)
    #[slot(5)]
    pub renderer_slot5: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// construct framebuffer — wa_malloc(w*h), init renderer (0x5A2430, RET 0x8)
    #[slot(6)]
    pub construct_frame_buffer: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        width: i32,
        height: i32,
    ) -> *mut FastcallResult,
    /// release framebuffer — calls renderer teardown, frees buffer (0x5A24A0)
    #[slot(7)]
    pub release_frame_buffer: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: i32,
    ) -> *mut FastcallResult,
    /// renderer thunk -> backend vtable[8] (0x5A24F0)
    #[slot(8)]
    pub renderer_slot8: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// lock framebuffer for pixel access (0x5A2530)
    ///
    /// Returns framebuffer pointer and stride for direct pixel manipulation.
    #[slot(9)]
    pub lock_framebuffer: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: u32,
        p3: u32,
        p4: u32,
        p5: u32,
    ) -> *mut FastcallResult,
    /// renderer thunk -> backend vtable[10] (0x5A2510)
    #[slot(10)]
    pub renderer_restore_dims: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// renderer thunk -> backend vtable[11] (restore surface) (0x5A25A0)
    #[slot(11)]
    pub renderer_restore_surface: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// allocate error/result object (0x5A2720, RET 0x4)
    #[slot(12)]
    pub alloc_error_result: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: u32,
    ) -> *mut FastcallResult,
    /// get renderer surface pointer (0x5A26C0)
    ///
    /// Queries `renderer->vtable[12]` with G_FRAME_BUFFER_PTR.
    #[slot(13)]
    pub get_renderer_surface: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// stub — returns 0 (CGameTask__vt18, 0x545780)
    #[slot(14)]
    pub stub_ret_zero: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// lock surface for reading (0x5A2690, RET 0x8)
    ///
    /// Writes framebuffer pointer to `*out_data` and stride to `*out_stride`.
    #[slot(15)]
    pub lock_surface_read: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        out_data: *mut u32,
        out_stride: *mut u32,
    ) -> *mut FastcallResult,
    /// unlock surface (0x5A2C10, RET 0x4)
    ///
    /// No-op for software framebuffer — just returns success code.
    #[slot(16)]
    pub unlock_surface_read: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: u32,
    ) -> *mut FastcallResult,
    /// lock surface for writing (0x5A2690 — same fn as slot 15, RET 0x8)
    ///
    /// Writes framebuffer pointer to `*out_data` and stride to `*out_stride`.
    #[slot(17)]
    pub lock_surface_write: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        out_data: *mut u32,
        out_stride: *mut u32,
    ) -> *mut FastcallResult,
    /// unlock surface after writing (0x5A2C10 — same fn as slot 16, RET 0x4)
    #[slot(18)]
    pub unlock_surface_write: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        p2: u32,
    ) -> *mut FastcallResult,
    /// fill rectangle — lock, memset rows, unlock (0x5A25C0)
    ///
    /// Params: x, y, width, height, fill_value.
    #[slot(19)]
    pub fill_rect: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        fill_value: u32,
    ) -> *mut FastcallResult,
    /// clear framebuffer — memset(ptr, 0, w*h) (0x5A23F0)
    #[slot(20)]
    pub clear_frame_buffer: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// trampoline to slot 20 via vtable dispatch (0x5A2420)
    #[slot(21)]
    pub clear_frame_buffer_indirect: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// allocate surface object — wa_malloc(0x14) (0x5A2760)
    #[slot(22)]
    pub alloc_surface: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// draw landscape — blit with clipping and optional color-key transparency (0x5A2790)
    #[slot(23)]
    pub draw_landscape: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        surface: *mut u8,
        y_offset: i32,
        flags: u32,
    ) -> *mut FastcallResult,
    /// blit surface to framebuffer — optimized copy with stride alignment (0x5A2A40)
    #[slot(24)]
    pub blit_to_frame_buffer: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        surface: *mut u8,
        y_offset: i32,
    ) -> *mut FastcallResult,
}

/// RenderContext — CWormsApp rendering dispatch subobject.
///
/// Stored as a global pointer at 0x79D6D4 (g_RenderContext).
/// The full object layout is part of CWormsApp; only key fields are mapped here.
#[repr(C)]
pub struct RenderContext {
    /// 0x00: Vtable pointer (0x662EC8)
    pub vtable: *const RenderContextVtable,
    /// 0x04-0x17: Unknown (part of CWormsApp layout)
    pub _unknown_04: [u8; 0x14],
    /// 0x18: Pointer to renderer backend (CompatRenderer, OpenGLCPU, or DDraw object)
    pub renderer_backend: *mut u8,
}

const _: () = assert!(core::mem::size_of::<RenderContext>() == 0x1C);

// Generate calling wrappers: RenderContext::get_renderer_surface_raw(), etc.
// Use the `_raw` variants to avoid LLVM noalias miscompilation.
// Callers must pass a `&mut FastcallResult` buffer (allocated on the stack).
bind_RenderContextVtable!(RenderContext, vtable);
