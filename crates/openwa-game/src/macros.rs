/// Define known WA.exe addresses and register them in the global address registry.
///
/// Supports two forms:
///
/// **Class blocks** — group vtable, constructor, and vtable methods under a class name:
/// ```ignore
/// define_addresses! {
///     class "CTask" {
///         /// CTask vtable - 7 virtual method pointers
///         vtable CTASK_VTABLE = 0x00669F8C;
///         /// CTask constructor
///         ctor/Stdcall CTASK_CONSTRUCTOR = 0x005625A0;
///         vmethod CTASK_VT0_INIT = 0x00562710;
///     }
/// }
/// ```
///
/// **Standalone entries** — functions and globals not belonging to a class:
/// ```ignore
/// define_addresses! {
///     /// Game PRNG
///     fn/Fastcall ADVANCE_GAME_RNG = 0x0053F320;
///     global G_GAME_SESSION = 0x007A0884;
/// }
/// ```
///
/// Each entry generates:
/// 1. A `pub const NAME: u32 = value;` in the current scope
/// 2. An `inventory::submit!(AddrEntry { ... })` for global registry collection
///
/// Entry kinds: `vtable`, `ctor`, `vmethod`, `fn`, `global`, `string`, `data`
///
/// Optional calling convention suffix: `/Stdcall`, `/Thiscall`, `/Fastcall`, `/Cdecl`, `/Usercall`
#[macro_export]
macro_rules! define_addresses {
    // Main entry point: parse a sequence of class blocks and standalone entries
    (@items $($rest:tt)*) => {
        $crate::define_addresses!(@parse $($rest)*);
    };

    // --- Class block ---
    (@parse class $class_name:literal { $($body:tt)* } $($rest:tt)*) => {
        $crate::define_addresses!(@class_items $class_name $($body)*);
        $crate::define_addresses!(@parse $($rest)*);
    };

    // --- Standalone entries (with doc + calling conv) ---
    (@parse $(#[doc = $doc:literal])* fn / $conv:ident $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Function, Some($crate::registry::CallingConv::$conv), None, concat!($($doc,)* ""));
        $crate::define_addresses!(@parse $($rest)*);
    };
    // Standalone fn without calling conv
    (@parse $(#[doc = $doc:literal])* fn $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Function, None, None, concat!($($doc,)* ""));
        $crate::define_addresses!(@parse $($rest)*);
    };
    // Standalone global
    (@parse $(#[doc = $doc:literal])* global $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Global, None, None, concat!($($doc,)* ""));
        $crate::define_addresses!(@parse $($rest)*);
    };
    // Standalone string literal
    (@parse $(#[doc = $doc:literal])* string $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, StringLiteral, None, None, concat!($($doc,)* ""));
        $crate::define_addresses!(@parse $($rest)*);
    };
    // Standalone data table
    (@parse $(#[doc = $doc:literal])* data $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, DataTable, None, None, concat!($($doc,)* ""));
        $crate::define_addresses!(@parse $($rest)*);
    };

    // Base case: empty
    (@parse) => {};

    // --- Class body items ---
    // vtable
    (@class_items $class_name:literal $(#[doc = $doc:literal])* vtable $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Vtable, None, Some($class_name), concat!($($doc,)* ""));
        $crate::define_addresses!(@class_items $class_name $($rest)*);
    };
    // ctor with calling conv
    (@class_items $class_name:literal $(#[doc = $doc:literal])* ctor / $conv:ident $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Constructor, Some($crate::registry::CallingConv::$conv), Some($class_name), concat!($($doc,)* ""));
        $crate::define_addresses!(@class_items $class_name $($rest)*);
    };
    // ctor without calling conv
    (@class_items $class_name:literal $(#[doc = $doc:literal])* ctor $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Constructor, None, Some($class_name), concat!($($doc,)* ""));
        $crate::define_addresses!(@class_items $class_name $($rest)*);
    };
    // vmethod
    (@class_items $class_name:literal $(#[doc = $doc:literal])* vmethod $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, VtableMethod, None, Some($class_name), concat!($($doc,)* ""));
        $crate::define_addresses!(@class_items $class_name $($rest)*);
    };
    // fn inside class (with calling conv)
    (@class_items $class_name:literal $(#[doc = $doc:literal])* fn / $conv:ident $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Function, Some($crate::registry::CallingConv::$conv), Some($class_name), concat!($($doc,)* ""));
        $crate::define_addresses!(@class_items $class_name $($rest)*);
    };
    // fn inside class (without calling conv)
    (@class_items $class_name:literal $(#[doc = $doc:literal])* fn $name:ident = $value:expr_2021; $($rest:tt)*) => {
        $(#[doc = $doc])*
        pub const $name: u32 = $value;
        $crate::define_addresses!(@submit $name, $value, Function, None, Some($class_name), concat!($($doc,)* ""));
        $crate::define_addresses!(@class_items $class_name $($rest)*);
    };
    // Base case: empty class body
    (@class_items $class_name:literal) => {};

    // --- Submit helper ---
    (@submit $name:ident, $value:expr_2021, $kind:ident, $conv:expr_2021, $class:expr_2021, $doc:expr_2021) => {
        $crate::inventory::submit! {
            $crate::registry::AddrEntry {
                va: $value,
                name: stringify!($name),
                kind: $crate::registry::AddrKind::$kind,
                calling_conv: $conv,
                class_name: $class,
                doc: $doc,
            }
        }
    };

    // Top-level entry point
    ($($tokens:tt)*) => {
        $crate::define_addresses!(@parse $($tokens)*);
    };
}

/// Replace specific slots in a WA.exe vtable with Rust function pointers.
///
/// Expands to executable code for use inside `install()` functions.
/// Uses `patch_vtable` (VirtualProtect) to make .rdata writable.
///
/// # Syntax
///
/// Slots can be identified by **method name** (resolved via `offset_of!`)
/// or by **slot index** (literal number):
///
/// ```ignore
/// vtable_replace!(DSSoundVtable, va::DS_SOUND_VTABLE, {
///     play_sound => my_play_sound,                    // by name, pure replace
///     load_wav [originals::LOAD_WAV] => my_load_wav,  // by name, save original
///     slot 23 => my_returns_1,                        // by index (legacy)
/// })?;
/// ```
///
/// The `[static]` syntax saves the original fn pointer to the given `AtomicU32`
/// before overwriting. Omit it for pure replacement (no call-through).
///
/// Size is derived from `size_of::<VtableType>() / 4`.
#[macro_export]
macro_rules! vtable_replace {
    ($vtable_ty:ty, $va:expr_2021, { $($slot:tt)* }) => {{
        let vtable_ptr = $crate::rebase::rb($va) as *mut u32;
        let slot_count = core::mem::size_of::<$vtable_ty>() / 4;
        unsafe {
            $crate::vtable::patch_vtable(vtable_ptr, slot_count, |vt| {
                $crate::vtable_replace!(@slots vt, $vtable_ty, $($slot)*);
            })
        }
    }};

    // --- Index-based (slot N) ---

    // Slot with original save: slot N [static_path] => replacement,
    (@slots $vt:ident, $vtable_ty:ty, slot $idx:literal [$orig:expr_2021] => $replacement:expr_2021, $($rest:tt)*) => {
        {
            let slot = $vt.add($idx);
            $orig.store(unsafe { *slot }, core::sync::atomic::Ordering::Relaxed);
            unsafe { *slot = $replacement as *const () as u32; }
        }
        $crate::vtable_replace!(@slots $vt, $vtable_ty, $($rest)*);
    };

    // Slot without original save: slot N => replacement,
    (@slots $vt:ident, $vtable_ty:ty, slot $idx:literal => $replacement:expr_2021, $($rest:tt)*) => {
        {
            let slot = $vt.add($idx);
            unsafe { *slot = $replacement as *const () as u32; }
        }
        $crate::vtable_replace!(@slots $vt, $vtable_ty, $($rest)*);
    };

    // --- Name-based (method_name) ---

    // Method with original save: method_name [static_path] => replacement,
    (@slots $vt:ident, $vtable_ty:ty, $method:ident [$orig:expr_2021] => $replacement:expr_2021, $($rest:tt)*) => {
        {
            const _SLOT_IDX: usize =
                core::mem::offset_of!($vtable_ty, $method) / core::mem::size_of::<usize>();
            let slot = $vt.add(_SLOT_IDX);
            $orig.store(unsafe { *slot }, core::sync::atomic::Ordering::Relaxed);
            unsafe { *slot = $replacement as *const () as u32; }
        }
        $crate::vtable_replace!(@slots $vt, $vtable_ty, $($rest)*);
    };

    // Method without original save: method_name => replacement,
    (@slots $vt:ident, $vtable_ty:ty, $method:ident => $replacement:expr_2021, $($rest:tt)*) => {
        {
            const _SLOT_IDX: usize =
                core::mem::offset_of!($vtable_ty, $method) / core::mem::size_of::<usize>();
            let slot = $vt.add(_SLOT_IDX);
            unsafe { *slot = $replacement as *const () as u32; }
        }
        $crate::vtable_replace!(@slots $vt, $vtable_ty, $($rest)*);
    };

    // Base case
    (@slots $vt:ident, $vtable_ty:ty,) => {};
}

/// Call a virtual method through a typed vtable pointer.
///
/// Assumes `$obj` is a raw pointer to a struct whose first field `vtable`
/// is `*const SomeVtable`, and `$method` is a function pointer field on
/// that vtable struct.
///
/// # Examples
///
/// ```ignore
/// // Single arg (thiscall — `self` is implicit in ECX):
/// vcall!(pal, reset);
///
/// // With extra arguments:
/// vcall!(pal, set_mode, 7);
/// ```
///
/// Expands to: `((*(*$obj).vtable).$method)($obj, $($args),*)`
#[macro_export]
macro_rules! vcall {
    ($obj:expr_2021, $method:ident $(, $args:expr_2021)*) => {
        ((*(*$obj).vtable).$method)($obj $(, $args)*)
    };
}
