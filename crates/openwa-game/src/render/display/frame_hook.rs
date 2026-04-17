//! `FramePostProcessHook` — abstract per-frame post-processing hook.
//!
//! 4-byte polymorphic class (just a vtable pointer). Concrete subclasses
//! override slots 1 and 2 of [`FramePostProcessHookVtable`] to implement
//! per-frame work that runs after `DisplayGfx` finishes drawing the frame.
//! Subclasses store their state in globals or are themselves singletons —
//! the textbook "4-byte hook interface" pattern (lesson #34 in the display
//! porting notes).
//!
//! The only concrete subclass shipped in WA is `ScreenshotHook` (vtable
//! 0x66A2C4) which writes the rendered `layer_0` `DisplayBitGrid` as a
//! numbered PNG when `g_DDGame->screenshot_pending` is set.
//!
//! ## Lifetime
//!
//! Lives in a `std::vector<FramePostProcessHook*>` at `DisplayGfx + 0x24DF8`
//! (see `gfx.rs::hook_vec_proxy/first/last/end`). Inserted during
//! `DisplayGfx::Init`, dispatched once per frame from `RenderFrame_Maybe`
//! → `DispatchFramePostProcessHooks` (0x56CDB0), and freed by
//! `DisplayGfx::DestructorImpl` (slot 0) which iterates the vec and calls
//! `vtable[0](1)` on every entry.

use crate::bitgrid::DisplayBitGrid;

// Note: `FRAME_POST_PROCESS_HOOK_VTABLE` is emitted by the
// `#[vtable(va = ...)]` attribute macro on `FramePostProcessHookVtable`
// below; it does not need a separate `define_addresses!` entry.
crate::define_addresses! {
    class "FramePostProcessHook" {
        /// `FramePostProcessHook::Destructor` (0x569BF0) — trivial:
        /// rebinds vtable to base + frees if `flags & 1`.
        fn/Thiscall FRAME_POST_PROCESS_HOOK_DESTRUCTOR = 0x0056_9BF0;
    }

    class "ScreenshotHook" {
        /// Concrete `ScreenshotHook` vtable (0x66A2C4) — destructor +
        /// `GetCaptureRequest` + `CaptureToPng`. Has the same shape as
        /// `FramePostProcessHookVtable` (no Rust struct of its own,
        /// since `ScreenshotHook` adds no instance fields).
        vtable SCREENSHOT_HOOK_VTABLE = 0x0066_A2C4;
        /// `ScreenshotHook::GetCaptureRequest` (0x56D170) — vtable[1].
        /// Returns 2 if `g_DDGame->screenshot_pending` is set, else 0.
        fn/Thiscall SCREENSHOT_HOOK_GET_CAPTURE_REQUEST = 0x0056_D170;
        /// `ScreenshotHook::CaptureToPng` (0x56D180) — vtable[2].
        /// Formats `"%s%06d.png"` and writes the rendered layer_0 surface
        /// to disk via `FUN_0056C6F0`.
        fn/Thiscall SCREENSHOT_HOOK_CAPTURE_TO_PNG = 0x0056_D180;
    }
}

/// `FramePostProcessHook` — abstract polymorphic class (just a vtable ptr).
///
/// All instance state lives in the concrete subclass — for `ScreenshotHook`
/// the "state" is `g_DDGame->screenshot_pending` and the running PNG counter
/// in globals, not fields on the object itself.
#[repr(C)]
pub struct FramePostProcessHook {
    pub vtable: *const FramePostProcessHookVtable,
}

const _: () = assert!(core::mem::size_of::<FramePostProcessHook>() == 4);

/// Vtable for `FramePostProcessHook` (3 slots, 0x66A2B8).
///
/// Verified call conventions from the dispatcher
/// `DisplayGfx::DispatchFramePostProcessHooks` (0x56CDB0):
///
/// - `vtable[1]` (`get_capture_request`) at 0x56CE01-0x56CE08: pure
///   thiscall, no stack args, returns `i32` in EAX.
/// - `vtable[2]` (`capture`) at 0x56CE98-0x56CEA8: thiscall, one stack
///   arg `layer_0: *mut DisplayBitGrid` (loaded from
///   `DisplayGfx + 0x3D9C`).
/// - `vtable[0]` (destructor) is called from `DisplayGfx::DestructorImpl`
///   at 0x56A1BF-0x56A1C5 with `flags = 1`.
///
/// Slots 1 and 2 are `_purecall` (pure virtual) on the abstract base vtable;
/// only the concrete `ScreenshotHook` vtable provides real implementations.
#[openwa_game::vtable(size = 3, va = 0x0066_A2B8, class = "FramePostProcessHook")]
pub struct FramePostProcessHookVtable {
    /// scalar deleting destructor (0x569BF0 on the base vtable).
    /// Calls return value is `*mut FramePostProcessHook` per the MSVC ABI.
    #[slot(0)]
    pub destructor: fn(this: *mut FramePostProcessHook, flags: u32) -> *mut FramePostProcessHook,
    /// `get_capture_request` — returns non-zero if this hook wants to run
    /// this frame. ScreenshotHook returns 2 when a screenshot is pending,
    /// 0 otherwise. The dispatcher tracks the max return value across all
    /// hooks and only invokes `capture` if any hook returned non-zero.
    #[slot(1)]
    pub get_capture_request: fn(this: *mut FramePostProcessHook) -> i32,
    /// `capture` — runs the hook's per-frame work, given the rendered
    /// `layer_0` `DisplayBitGrid` from `DisplayGfx + 0x3D9C`.
    #[slot(2)]
    pub capture: fn(this: *mut FramePostProcessHook, layer_0: *mut DisplayBitGrid),
}

bind_FramePostProcessHookVtable!(FramePostProcessHook, vtable);

/// `ScreenshotHook` — the only concrete subclass shipped in WA.
///
/// Layout matches `FramePostProcessHook` exactly (no instance fields). The
/// vtable at 0x66A2C4 differs only in slots 1 and 2: `GetCaptureRequest`
/// reads `g_DDGame->screenshot_pending`, and `CaptureToPng` writes the
/// rendered surface to a numbered `.png` file.
pub type ScreenshotHook = FramePostProcessHook;
