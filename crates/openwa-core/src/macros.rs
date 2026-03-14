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
    ($obj:expr, $method:ident $(, $args:expr)*) => {
        ((*(*$obj).vtable).$method)($obj $(, $args)*)
    };
}
