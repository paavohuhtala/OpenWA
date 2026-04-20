// RenderContext — CWormsApp rendering dispatch subobject.
// Manages the software framebuffer and dispatches rendering calls to the active
// renderer backend (CompatRenderer, OpenGLCPU, or DDraw).

crate::define_addresses! {
    class "CWormsApp" {
        /// CWormsApp__FlipSurface — dispatches to renderer backend vtable[1]
        fn/Fastcall CWORMSAPP_FLIP_SURFACE = 0x005A2700;
        /// ConstructFrameBuffer — allocates software framebuffer, inits renderer
        fn/Fastcall CONSTRUCT_FRAME_BUFFER_THISCALL = 0x005A2430;
        /// CWormsApp__ReleaseFrameBuffer
        fn/Fastcall CWORMSAPP_RELEASE_FRAME_BUFFER = 0x005A24A0;
        /// CWormsApp__FillRect — lock surface, memset rows, unlock
        fn/Fastcall CWORMSAPP_FILL_RECT = 0x005A25C0;
        /// CWormsApp__BlitToFrameBuffer — optimized blit from surface to framebuffer
        fn/Fastcall CWORMSAPP_BLIT_TO_FRAME_BUFFER = 0x005A2A40;
        /// CWormsApp__DrawLandscape — blit with clipping and transparency
        fn/Fastcall CWORMSAPP_DRAW_LANDSCAPE = 0x005A2790;
        /// ClearFrameBuffer — memset(framebuffer, 0, w*h)
        fn CWORMSAPP_CLEAR_FRAME_BUFFER = 0x005A23F0;
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
#[openwa_game::vtable(size = 25, va = 0x00662EC8, class = "RenderContext")]
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
        out_data: *mut *mut u8,
        out_stride: *mut u32,
    ) -> *mut FastcallResult,
    /// unlock surface after writing (0x5A2C10 — same fn as slot 16, RET 0x4)
    ///
    /// `data` is the framebuffer pointer previously returned from
    /// `lock_surface_write`. The original ignores it; we still pass it
    /// through for fidelity.
    #[slot(18)]
    pub unlock_surface_write: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        data: *mut u8,
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
    /// draw landscape — blit a `Surface` to the framebuffer with optional
    /// color-key transparency (0x5A2790).
    ///
    /// 8 stack args (verified by RET 0x20 at 0x5a2cba):
    ///   - `surface`: source `Surface*` (its vtable's slot 17 = lock-write,
    ///     slot 18 = unlock are dispatched internally to read pixel data)
    ///   - `dst_x`/`dst_y`: framebuffer destination top-left
    ///   - `src_x`/`src_y`: rect origin within the surface to copy from
    ///   - `width`/`height`: rect size
    ///   - `flags`: bit 1 (`0x2`) = use color-key transparency (skip pixels
    ///     equal to the surface's color-key); bit 0 (`0x1`) is set by the
    ///     caller in `BlitBitmapClipped` and is reserved/ignored by the
    ///     blit loop here.
    ///
    /// Used by `DisplayGfx::DrawTiledBitmap` (slot 11) and
    /// `DisplayGfx::DrawTiledTerrain` (slot 22) via the `CBitmap` blit
    /// helper at `FUN_00403c60`.
    #[slot(23)]
    pub draw_landscape: unsafe extern "fastcall" fn(
        this: *mut RenderContext,
        result: *mut FastcallResult,
        surface: *mut u8,
        dst_x: i32,
        dst_y: i32,
        src_x: i32,
        src_y: i32,
        width: i32,
        height: i32,
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

// ---------------------------------------------------------------------------
// Surface — DDraw/D3D/OpenGL surface object created by alloc_surface (slot 22)
// ---------------------------------------------------------------------------

/// Vtable for `Surface` objects allocated by `RenderContext::alloc_surface`
/// (slot 22). This is the rendering-backend surface interface — different
/// backends (CompatRenderer / OpenGLCPU / DDraw) implement different vtables
/// at the same slot positions.
///
/// Slots typed here are the ones used by `DisplayGfx::DrawTiledBitmap`
/// (slot 11) and `LoadSpriteByName` (which already uses raw transmutes).
/// The remaining slots are unmapped — extend as needed.
///
/// All slots are `__fastcall(this, *FastcallResult, ...args)`. The
/// `FastcallResult.value` field receives a non-zero failure code on
/// `init_surface` failure (slot 11 retries with bpp=4 if bpp=8 fails).
#[openwa_game::vtable(size = 8, class = "Surface")]
pub struct SurfaceVtable {
    /// lock surface for direct pixel access (slot 3).
    ///
    /// Writes the locked surface base pointer to `*out_data` and the row
    /// stride (bytes between rows) to `*out_stride`.
    #[slot(3)]
    pub lock_surface: unsafe extern "fastcall" fn(
        this: *mut Surface,
        result: *mut FastcallResult,
        out_data: *mut *mut u8,
        out_stride: *mut i32,
    ) -> *mut FastcallResult,
    /// unlock surface after editing (slot 4).
    ///
    /// `addr` is the surface base pointer previously obtained from
    /// `lock_surface`'s `out_data`.
    #[slot(4)]
    pub unlock_surface: unsafe extern "fastcall" fn(
        this: *mut Surface,
        result: *mut FastcallResult,
        addr: *mut u8,
    ) -> *mut FastcallResult,
    /// init/recreate surface storage (slot 5).
    ///
    /// `(width, height, bpp)`. Returns 0 in `*result.value` on success;
    /// non-zero on failure.
    #[slot(5)]
    pub init_surface: unsafe extern "fastcall" fn(
        this: *mut Surface,
        result: *mut FastcallResult,
        width: i32,
        height: i32,
        bpp: i32,
    ) -> *mut FastcallResult,
    /// release surface backing storage (slot 6).
    ///
    /// Pure fastcall — no stack args. Writes a result code into
    /// `*result.value` (which most callers ignore). Called from the
    /// `DisplayGfx` destructor's CBitmap-vec teardown loop on every
    /// non-null `CBitmap.surface` before the bitmap itself is freed.
    #[slot(6)]
    pub release: unsafe extern "fastcall" fn(
        this: *mut Surface,
        result: *mut FastcallResult,
    ) -> *mut FastcallResult,
    /// set color-key (transparency) (slot 7).
    ///
    /// Used by `load_sprite_by_name` after `init_surface` to enable
    /// transparency on the freshly-allocated frame surface (color key
    /// `0`, flag `0x10`). The result code is unused by all known callers.
    #[slot(7)]
    pub set_color_key: unsafe extern "fastcall" fn(
        this: *mut Surface,
        result: *mut FastcallResult,
        color_key: u32,
        flags: u32,
    ) -> *mut FastcallResult,
}

/// Backend-specific surface object created by
/// `RenderContext::alloc_surface` (slot 22). Layout beyond the vtable
/// pointer is opaque (varies by backend); we only access it through the
/// `SurfaceVtable` slots.
#[repr(C)]
pub struct Surface {
    /// 0x00: vtable pointer (one of several backend vtables)
    pub vtable: *const SurfaceVtable,
}

bind_SurfaceVtable!(Surface, vtable);
