//! `LocalizedTemplate` — per-game localized string resolver + cache.
//!
//! Wraps WA's basic localization system (see [`super::string_resource`]) with
//! two memoization tables and a small templating language. Constructed by
//! `GameEngine::InitHardware` at `0x0056D47A`, owned by
//! [`GameSession`](crate::engine::GameSession) at `+0xBC`, and copied into
//! [`GameWorld`](crate::engine::GameWorld) at `+0x18` by the GameWorld
//! constructor.
//!
//! Resolved by `LocalizedTemplate__Resolve` (0x0053EA30,
//! `__stdcall(this, token) -> *const c_char`) and
//! `LocalizedTemplate__ResolveSplitArray` (0x0053EC70, returns
//! `*const *const c_char` — the resolved string split on `\x1A`).
//!
//! The post-processing pass walks WA's escape codes:
//! - `\x1B<hex>,` — push `(wa_version_threshold < hex)` onto the branch stack
//!   (older-WA gate).
//! - `\x1C<hex>,` — push `(hex != 0 ? false : force_zero_branch)`
//!   (default/else gate).
//! - `\x1D` — XOR-toggle the top of the branch stack.
//! - `\x1E` — close-brace / pop.
//! - `\x1A` — array separator (only consumed by `ResolveSplitArray`).
//!
//! Output bytes are emitted only when the entire branch stack is non-zero.

use core::ffi::c_char;

use crate::FieldRegistry;
use crate::address::va;
use crate::rebase::rb;
use crate::wa::string_resource::StringRes;

static mut RESOLVE_ADDR: u32 = 0;

/// Initialize the `LocalizedTemplate__Resolve` bridge address. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        RESOLVE_ADDR = rb(va::LOCALIZED_TEMPLATE_RESOLVE);
    }
}

/// Bridge for `LocalizedTemplate__Resolve` (0x0053EA30, stdcall RET 8).
/// Returns a pointer to the resolved template string (with WA's escape
/// codes processed and the result cached on the [`LocalizedTemplate`])
/// for the given token id.
pub unsafe fn resolve(template: *mut LocalizedTemplate, token: StringRes) -> *const c_char {
    unsafe { resolve_raw(template, token.as_offset()) }
}

/// Raw-id variant of [`resolve`], used by callers that pass numeric tokens
/// directly (e.g. ports of WA functions that hard-code resource ids).
pub unsafe fn resolve_raw(template: *mut LocalizedTemplate, token_id: u32) -> *const c_char {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut LocalizedTemplate, u32) -> *const c_char =
            core::mem::transmute(RESOLVE_ADDR as usize);
        func(template, token_id)
    }
}

/// Typed entry point for [`resolve_split_array_raw`]. Prefer this in
/// hand-written callers; pass `res::*` constants from [`crate::wa::string_resource`].
pub unsafe fn resolve_split_array(
    template: *mut LocalizedTemplate,
    token: StringRes,
) -> *mut *mut c_char {
    unsafe { resolve_split_array_raw(template, token.as_offset()) }
}

/// Resolve `token_id` into a NULL-terminated array of C-string pointers,
/// memoized in the per-template [`LocalizedTemplate::split_array_cache`].
///
/// Pure Rust port of WA 0x0053EC70 (`LocalizedTemplate__ResolveSplitArray`,
/// `__usercall(EDI=token_id) + stdcall(this)`, RET 0x4).
///
/// On a cache miss this calls [`resolve_raw`] to materialize the post-processed
/// template string, **mutates that cached string in place** by replacing each
/// `\x1A` separator with `\0`, and allocates an `(n+1)`-slot pointer array via
/// [`wa_malloc`](crate::wa_alloc::wa_malloc) where each slot points into the
/// shared template buffer. The trailing slot is `NULL`.
///
/// Subsequent calls return the cached array unchanged.
///
/// Raw-id form parallels [`resolve_raw`] for callers that want to pass numeric
/// tokens (e.g. WA-port shims). Prefer [`resolve_split_array`].
pub unsafe fn resolve_split_array_raw(
    template: *mut LocalizedTemplate,
    token_id: u32,
) -> *mut *mut c_char {
    unsafe {
        use crate::wa_alloc::wa_malloc;

        let slot = (*template).split_array_cache.add(token_id as usize);
        if !(*slot).is_null() {
            return *slot;
        }

        // [`resolve_raw`] hands out `*const c_char` for read-only consumers, but the
        // underlying buffer is `wa_malloc`'d heap memory cached on `*template` and is
        // already shared with WA's own `ResolveSplitArray`, which mutates `\x1A`
        // separators to `\0` here exactly as we do. The const→mut conversion is
        // intentional aliasing of a writable buffer, not constness laundering.
        let s = resolve_raw(template, token_id).cast_mut();

        // Walk to end-of-string.
        let mut end = s;
        while *end != 0 {
            end = end.add(1);
        }
        // Trim trailing \x1A separators.
        while end > s && *end.sub(1) == 0x1A {
            end = end.sub(1);
        }
        *end = 0;

        // Count segments: 1 + number of remaining \x1A separators.
        let mut count: usize = 1;
        let mut p = s;
        while *p != 0 {
            if *p == 0x1A {
                count += 1;
            }
            p = p.add(1);
        }

        // Allocate (count + 1) pointer slots; the last is the NULL terminator.
        let bytes = ((count + 1) * core::mem::size_of::<*mut c_char>()) as u32;
        let arr = wa_malloc(bytes) as *mut *mut c_char;
        if arr.is_null() {
            return core::ptr::null_mut();
        }

        *arr = s;
        let mut idx: usize = 1;
        let mut p = s;
        while *p != 0 {
            if *p == 0x1A {
                *p = 0;
                *arr.add(idx) = p.add(1);
                idx += 1;
            }
            p = p.add(1);
        }
        *arr.add(idx) = core::ptr::null_mut();

        *slot = arr;
        arr
    }
}

/// Owned 0x30-byte cache header. The two cache arrays are each
/// `wa_malloc(0x20E0)` — 2104 slots, indexed by `StringRes::as_offset()`.
///
/// Constructed by `LocalizedTemplate__Constructor` (0x0053E950),
/// `__usercall(EAX = wa_version_threshold, ESI = this) -> EAX = this`.
///
/// Layout has no destructor in the binary; cached `__strdup` strings appear
/// to leak by design.
#[derive(FieldRegistry)]
#[repr(C)]
pub struct LocalizedTemplate {
    /// 0x00: Active WA version, sourced from `GameInfo+0xD778`. Compared
    /// against the hex operand of `\x1B<hex>,` to gate version-conditional
    /// template branches.
    pub wa_version_threshold: i32,
    /// 0x04: 2104-slot cache of resolved (post-processed) template strings.
    /// Indexed by token id; `NULL` = not yet resolved.
    pub string_cache: *mut *mut c_char,
    /// 0x08: 2104-slot cache of `\x1A`-split arrays. Each populated slot is a
    /// NULL-terminated `*const c_char` array allocated by
    /// `LocalizedTemplate__ResolveSplitArray`.
    pub split_array_cache: *mut *mut *mut c_char,
    /// 0x0C: Default-branch override read by `\x1C0,...` template tokens.
    /// Zero-initialized; no writers found in the binary.
    pub force_zero_branch: u8,
    pub _pad_0d: [u8; 0x23],
}

const _: () = assert!(core::mem::size_of::<LocalizedTemplate>() == 0x30);
