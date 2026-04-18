//! Proc macros for OpenWA reverse engineering.
//!
//! ## `#[derive(FieldRegistry)]`
//!
//! Generates a `HasFieldRegistry` impl for `#[repr(C)]` structs, producing
//! a sorted `StructFields` table from the struct's field names, offsets,
//! sizes, and doc comments. Fields prefixed with `_unknown` or `_pad` are
//! excluded (they represent unmapped regions).
//!
//! ```ignore
//! #[derive(FieldRegistry)]
//! #[repr(C)]
//! pub struct DDGame {
//!     /// DDKeyboard pointer
//!     pub keyboard: *mut DDKeyboard,
//!     pub _unknown_04: [u8; 4],
//!     /// Game PRNG state
//!     pub rng_state: u32,
//! }
//! // Generates: DDGame::field_registry() -> &'static StructFields
//! // with entries for "keyboard" and "rng_state" (skips _unknown_04)
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Lit, Meta, parse_macro_input};

// =========================================================================
// #[derive(Vtable)]
// =========================================================================

/// Derive a typed vtable struct from a sparse slot definition.
///
/// Only declare known methods with `#[slot(N)]`. The macro generates the full
/// `#[repr(C)]` struct with `usize` gap-fillers for unknown slots, plus:
///
/// - Registry metadata (`HasVtableRegistry` impl + `inventory` registration)
/// - A companion `bind_{VtableName}!` macro for generating calling wrappers
/// - Optional `pub const` + `AddrEntry` if `va` is specified
/// - Compile-time size assertion
///
/// # Attributes
///
/// - `#[vtable(size = N)]` — total number of slots (required)
/// - `#[vtable(va = 0x...)]` — Ghidra VA of the vtable in .rdata (optional)
/// - `#[vtable(class = "Name")]` — owning C++ class name (optional)
///
/// # Example
///
/// ```ignore
/// #[derive(Vtable)]
/// #[vtable(size = 5, va = 0x0066_A2E4, class = "Palette")]
/// pub struct PaletteVtable {
///     #[slot(2)]
///     pub set_mode: unsafe extern "thiscall" fn(*mut Palette, u32),
///     #[slot(3)]
///     pub init: unsafe extern "thiscall" fn(*mut Palette),
///     #[slot(4)]
///     pub reset: unsafe extern "thiscall" fn(*mut Palette),
/// }
/// ```
/// Attribute macro that transforms a sparse vtable definition into a full
/// `#[repr(C)]` struct with gap-fillers, registry metadata, and a binding macro.
///
/// See module docs for usage.
#[proc_macro_attribute]
pub fn vtable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let attr_tokens = proc_macro2::TokenStream::from(attr);
    match vtable_impl(attr_tokens, input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn vtable_impl(
    attr: proc_macro2::TokenStream,
    input: DeriveInput,
) -> syn::Result<proc_macro2::TokenStream> {
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();
    let vis = &input.vis;

    // Parse vtable(size = N, va = 0x..., class = "...") from the attribute args
    let (vtable_size, vtable_va, class_name) = parse_vtable_attr_args(attr, struct_name)?;

    // Parse fields — each must have #[slot(N)]
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    struct_name,
                    "Vtable requires named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                struct_name,
                "Vtable can only be used on structs",
            ));
        }
    };

    // Collect slots. #[slot(N)] is optional — if omitted, indices are assigned
    // sequentially. If ANY field has #[slot(N)], all must (mixing is an error).
    let mut slots: Vec<SlotInfo> = Vec::new();
    let mut has_explicit_slot = false;
    let mut has_implicit_slot = false;

    for (field_idx, field) in fields.iter().enumerate() {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new_spanned(field, "Vtable fields must be named"))?;

        let explicit_index = parse_slot_attr(&field.attrs, ident)?;

        let slot_index = match explicit_index {
            Some(idx) => {
                has_explicit_slot = true;
                idx
            }
            None => {
                has_implicit_slot = true;
                field_idx as u32
            }
        };

        if has_explicit_slot && has_implicit_slot {
            return Err(syn::Error::new_spanned(
                ident,
                "cannot mix fields with and without #[slot(N)] — use it on all or none",
            ));
        }

        if slot_index >= vtable_size {
            return Err(syn::Error::new_spanned(
                ident,
                format!("slot index {} >= vtable size {}", slot_index, vtable_size),
            ));
        }

        // Check for duplicates
        if slots.iter().any(|s| s.index == slot_index) {
            return Err(syn::Error::new_spanned(
                ident,
                format!("duplicate slot index {}", slot_index),
            ));
        }

        let doc = extract_doc_comment(&field.attrs);
        let kept_attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("slot"))
            .cloned()
            .collect();

        // Normalize fn type: plain `fn(...)` becomes `unsafe extern "thiscall" fn(...)`
        let normalized_ty = normalize_vtable_fn_type(&field.ty);

        slots.push(SlotInfo {
            index: slot_index,
            ident: ident.clone(),
            ty: normalized_ty,
            doc,
            attrs: kept_attrs,
            vis: field.vis.clone(),
        });
    }

    // Sort by slot index
    slots.sort_by_key(|s| s.index);

    // Generate struct fields: gap-fillers + typed slots
    let mut struct_fields = Vec::new();
    let mut next_index = 0u32;

    for slot in &slots {
        // Fill gaps with usize fields
        while next_index < slot.index {
            let gap_name = quote::format_ident!("_slot_{}", next_index);
            struct_fields.push(quote! {
                pub #gap_name: usize
            });
            next_index += 1;
        }

        // Emit the typed slot
        let ident = &slot.ident;
        let ty = &slot.ty;
        let attrs = &slot.attrs;
        let slot_vis = &slot.vis;
        struct_fields.push(quote! {
            #(#attrs)*
            #slot_vis #ident: #ty
        });
        next_index = slot.index + 1;
    }

    // Fill trailing gaps
    while next_index < vtable_size {
        let gap_name = quote::format_ident!("_slot_{}", next_index);
        struct_fields.push(quote! {
            pub #gap_name: usize
        });
        next_index += 1;
    }

    // Generate registry slot entries
    let slot_entries: Vec<_> = slots
        .iter()
        .map(|s| {
            let idx = s.index;
            let name = s.ident.to_string();
            let doc = &s.doc;
            quote! {
                openwa_game::registry::VtableSlotEntry {
                    index: #idx,
                    name: #name,
                    doc: #doc,
                }
            }
        })
        .collect();
    let slot_entry_count = slot_entries.len();

    let class_name_str = class_name.as_deref().unwrap_or("");
    let va_value = vtable_va.unwrap_or(0);

    // Generate the info static name
    let info_static = quote::format_ident!("_VTABLE_INFO_{}", struct_name);

    // Generate optional pub const + AddrEntry for the VA
    let va_items = if let Some(va) = vtable_va {
        // Derive const name: class "Palette" → PALETTE_VTABLE, or struct PaletteVtable → PALETTE_VTABLE
        let const_name_str = if let Some(ref class) = class_name {
            format!("{}_VTABLE", to_screaming_snake(class))
        } else {
            to_screaming_snake(&struct_name_str)
        };
        let const_ident = quote::format_ident!("{}", const_name_str);
        let class_expr = if let Some(ref class) = class_name {
            quote! { Some(#class) }
        } else {
            quote! { None }
        };
        quote! {
            #vis const #const_ident: u32 = #va;

            openwa_game::inventory::submit! {
                openwa_game::registry::AddrEntry {
                    va: #va,
                    name: #const_name_str,
                    kind: openwa_game::registry::AddrKind::Vtable,
                    calling_conv: None,
                    class_name: #class_expr,
                    doc: "",
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate the companion bind macro
    let bind_macro_name = quote::format_ident!("bind_{}", struct_name);
    let bind_methods = generate_bind_methods(&slots)?;

    let vtable_size_usize = vtable_size as usize;

    let expanded = quote! {
        #[repr(C)]
        #vis struct #struct_name {
            #(#struct_fields),*
        }

        const _: () = assert!(
            core::mem::size_of::<#struct_name>() == #vtable_size_usize * core::mem::size_of::<usize>(),
            "vtable struct size mismatch"
        );

        // Vtable registry metadata
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #info_static: openwa_game::registry::VtableInfo = openwa_game::registry::VtableInfo {
            struct_name: #struct_name_str,
            class_name: #class_name_str,
            ghidra_va: #va_value,
            slot_count: #vtable_size,
            slots: {
                const ENTRIES: [openwa_game::registry::VtableSlotEntry; #slot_entry_count] = [
                    #(#slot_entries),*
                ];
                &ENTRIES
            },
        };

        impl openwa_game::registry::HasVtableRegistry for #struct_name {
            fn vtable_info() -> &'static openwa_game::registry::VtableInfo {
                &#info_static
            }
        }

        openwa_game::inventory::submit! {
            openwa_game::registry::VtableRegistration {
                info: &#info_static,
            }
        }

        #va_items

        // Mark *const ThisVtable as a valid vtable pointer type for CTask<V>.
        unsafe impl openwa_game::task::Vtable for *const #struct_name {}

        /// Bind this vtable's known methods as wrapper methods on a class struct.
        ///
        /// Usage: `bind_VtableName!(ClassName, vtable_field_name);`
        #[macro_export]
        macro_rules! #bind_macro_name {
            ($class:ty, $($vtable_field:ident).+) => {
                impl $class {
                    #bind_methods
                }
            };
        }
    };

    Ok(expanded)
}

/// A parsed vtable slot definition.
struct SlotInfo {
    index: u32,
    ident: syn::Ident,
    ty: syn::Type,
    doc: String,
    attrs: Vec<syn::Attribute>,
    vis: syn::Visibility,
}

/// Generate the method wrappers for the bind macro.
///
/// For each slot, parses the fn pointer type to extract:
/// - `this` mutability (*mut → &mut self, *const → &self)
/// - Remaining parameters (with generated names p0, p1, ...)
/// - Return type
fn generate_bind_methods(slots: &[SlotInfo]) -> syn::Result<proc_macro2::TokenStream> {
    let mut methods = Vec::new();

    for slot in slots {
        let ident = &slot.ident;
        let doc = &slot.doc;

        // Parse the fn pointer type
        let bare_fn = match &slot.ty {
            syn::Type::BareFn(bf) => bf,
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "vtable slot type must be a bare fn pointer (e.g., unsafe extern \"thiscall\" fn(...))",
                ));
            }
        };

        // First param is `this` — determine mutability
        let inputs = &bare_fn.inputs;
        if inputs.is_empty() {
            return Err(syn::Error::new_spanned(
                bare_fn,
                "vtable fn must have at least a `this` parameter",
            ));
        }

        let this_is_const = is_const_ptr(&inputs[0].ty);
        let self_param = if this_is_const {
            quote! { &self }
        } else {
            quote! { &mut self }
        };
        let self_cast = if this_is_const {
            quote! { self as *const Self }
        } else {
            quote! { self as *mut Self }
        };

        // Raw-pointer `this` param type for the _raw variant
        let raw_this_ty = if this_is_const {
            quote! { *const Self }
        } else {
            quote! { *mut Self }
        };

        // Remaining params
        let mut param_decls = Vec::new();
        let mut param_names = Vec::new();
        for (i, arg) in inputs.iter().skip(1).enumerate() {
            let param_name = if let Some(ref name) = arg.name {
                let (ident, _) = name;
                ident.clone()
            } else {
                quote::format_ident!("p{}", i)
            };
            let param_ty = &arg.ty;
            param_decls.push(quote! { #param_name: #param_ty });
            param_names.push(param_name);
        }

        // Return type
        let ret_ty = match &bare_fn.output {
            syn::ReturnType::Default => quote! {},
            syn::ReturnType::Type(_, ty) => quote! { -> #ty },
        };

        // `_raw` variant name
        let raw_ident = quote::format_ident!("{}_raw", ident);
        let raw_doc = if doc.is_empty() {
            "Raw-pointer variant — avoids creating `&mut self`, preventing LLVM noalias \
             miscompilation when WA code modifies the same memory through bridge calls."
                .to_string()
        } else {
            format!(
                "{}\n\n\
                 Raw-pointer variant — avoids creating `&mut self`, preventing LLVM noalias \
                 miscompilation when WA code modifies the same memory through bridge calls.",
                doc
            )
        };

        let doc_attr = if doc.is_empty() {
            quote! {}
        } else {
            quote! { #[doc = #doc] }
        };

        methods.push(quote! {
            #doc_attr
            pub unsafe fn #ident(#self_param #(, #param_decls)*) #ret_ty {
                unsafe { ((*self.$($vtable_field).+).#ident)(#self_cast #(, #param_names)*) }
            }

            #[doc = #raw_doc]
            pub unsafe fn #raw_ident(this: #raw_this_ty #(, #param_decls)*) #ret_ty {
                unsafe { ((*(*this).$($vtable_field).+).#ident)(this #(, #param_names)*) }
            }
        });
    }

    Ok(quote! { #(#methods)* })
}

/// Normalize a vtable fn pointer type: if it's a plain `fn(...)` (no `unsafe`,
/// no explicit ABI), transform it to `unsafe extern "thiscall" fn(...)`.
///
/// Already-qualified types (e.g., `unsafe extern "thiscall" fn(...)`) pass through unchanged.
fn normalize_vtable_fn_type(ty: &syn::Type) -> syn::Type {
    if let syn::Type::BareFn(bare) = ty {
        let needs_unsafe = bare.unsafety.is_none();
        let needs_abi = bare.abi.is_none();

        if needs_unsafe || needs_abi {
            let mut normalized = bare.clone();
            if needs_unsafe {
                normalized.unsafety = Some(syn::token::Unsafe::default());
            }
            if needs_abi {
                normalized.abi = Some(syn::Abi {
                    extern_token: syn::token::Extern::default(),
                    name: Some(syn::LitStr::new("thiscall", proc_macro2::Span::call_site())),
                });
            }
            return syn::Type::BareFn(normalized);
        }
    }
    ty.clone()
}

/// Check if a type is `*const T` (as opposed to `*mut T`).
fn is_const_ptr(ty: &syn::Type) -> bool {
    if let syn::Type::Ptr(ptr) = ty {
        ptr.const_token.is_some()
    } else {
        false
    }
}

/// Parse the attribute arguments: `size = N, va = 0x..., class = "..."`.
fn parse_vtable_attr_args(
    attr: proc_macro2::TokenStream,
    span: &syn::Ident,
) -> syn::Result<(u32, Option<u32>, Option<String>)> {
    // Parse as a comma-separated list of key = value pairs
    struct VtableArgs {
        size: Option<u32>,
        va: Option<u32>,
        class: Option<String>,
    }

    let mut args = VtableArgs {
        size: None,
        va: None,
        class: None,
    };

    // Use syn's parser by wrapping in parens
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("size") {
            let _eq: syn::Token![=] = meta.input.parse()?;
            let lit: syn::LitInt = meta.input.parse()?;
            args.size = Some(lit.base10_parse()?);
        } else if meta.path.is_ident("va") {
            let _eq: syn::Token![=] = meta.input.parse()?;
            let lit: syn::LitInt = meta.input.parse()?;
            args.va = Some(lit.base10_parse()?);
        } else if meta.path.is_ident("class") {
            let _eq: syn::Token![=] = meta.input.parse()?;
            let lit: syn::LitStr = meta.input.parse()?;
            args.class = Some(lit.value());
        } else {
            return Err(meta.error("unknown vtable attribute"));
        }
        Ok(())
    });

    syn::parse::Parser::parse2(parser, attr)?;

    let size = args
        .size
        .ok_or_else(|| syn::Error::new_spanned(span, "vtable attribute requires `size = N`"))?;

    Ok((size, args.va, args.class))
}

/// Parse #[slot(N)] attribute from a field.
fn parse_slot_attr(attrs: &[syn::Attribute], _span: &syn::Ident) -> syn::Result<Option<u32>> {
    for attr in attrs {
        if attr.path().is_ident("slot") {
            let index: syn::LitInt = attr.parse_args()?;
            return Ok(Some(index.base10_parse()?));
        }
    }
    Ok(None)
}

/// Convert "PaletteVtable" or "Palette" to "PALETTE_VTABLE" or "PALETTE".
fn to_screaming_snake(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            // Don't insert underscore between consecutive uppercase (e.g., "DDGame" → "DD_GAME")
            let prev = name.chars().nth(i - 1).unwrap_or('_');
            if prev.is_lowercase() || prev.is_numeric() {
                result.push('_');
            } else if let Some(next) = name.chars().nth(i + 1)
                && next.is_lowercase()
            {
                result.push('_');
            }
        }
        result.push(ch.to_ascii_uppercase());
    }
    result
}

/// Derive `HasFieldRegistry` for a `#[repr(C)]` struct.
///
/// Generates a static `StructFields` table with one `FieldEntry` per named
/// field (excluding `_unknown*` and `_pad*` prefixes). Offsets and sizes
/// are computed at compile time via `core::mem::offset_of!()` and
/// `core::mem::size_of::<Type>()`.
#[proc_macro_derive(FieldRegistry, attributes(field))]
pub fn derive_field_registry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();

    // Build a map of generic type params → their defaults (e.g., V → *const c_void).
    // This allows the derive to work on generic structs like CTask<V = *const c_void>
    // by substituting generic params with concrete defaults in size_of expressions.
    let generic_defaults: std::collections::HashMap<String, syn::Type> = input
        .generics
        .type_params()
        .filter_map(|tp| {
            tp.default
                .as_ref()
                .map(|def| (tp.ident.to_string(), def.clone()))
        })
        .collect();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return syn::Error::new_spanned(struct_name, "FieldRegistry requires named fields")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                struct_name,
                "FieldRegistry can only be used on structs",
            )
            .to_compile_error()
            .into();
        }
    };

    // Collect field entries, skipping _unknown* and _pad* fields
    let mut entries = Vec::new();
    for field in fields {
        let field_ident = match &field.ident {
            Some(id) => id,
            None => continue,
        };
        let field_name = field_ident.to_string();

        // Skip unknown/padding fields
        if field_name.starts_with("_unknown") || field_name.starts_with("_pad") {
            continue;
        }

        // Extract doc comment from attributes
        let doc = extract_doc_comment(&field.attrs);
        let field_ty = &field.ty;

        // Substitute generic type params (e.g., `V` or `CTask<V>`) with their
        // defaults for size_of. offset_of uses the bare struct name which
        // Rust resolves with defaults applied.
        let size_ty = substitute_generics(field_ty, &generic_defaults);

        // Determine ValueKind: check for #[field(kind = "...")] override, else infer
        let kind =
            parse_field_kind_attr(&field.attrs).unwrap_or_else(|| infer_value_kind(field_ty));

        entries.push(quote! {
            openwa_game::registry::FieldEntry {
                offset: core::mem::offset_of!(#struct_name, #field_ident) as u32,
                name: #field_name,
                size: core::mem::size_of::<#size_ty>() as u32,
                kind: #kind,
                doc: #doc,
            }
        });
    }

    let entry_count = entries.len();

    // Generate a unique static name to avoid collisions across structs.
    let fields_static = quote::format_ident!("_FIELD_REGISTRY_{}", struct_name);

    let expanded = quote! {
        // The static StructFields table, accessible both from the trait impl
        // and from the inventory registration (which needs a const context).
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #fields_static: openwa_game::registry::StructFields = openwa_game::registry::StructFields {
            struct_name: #struct_name_str,
            fields: {
                // Fields are already sorted by offset because #[repr(C)]
                // guarantees declaration order = memory order.
                const ENTRIES: [openwa_game::registry::FieldEntry; #entry_count] = [
                    #(#entries),*
                ];
                &ENTRIES
            },
        };

        impl openwa_game::registry::HasFieldRegistry for #struct_name {
            fn field_registry() -> &'static openwa_game::registry::StructFields {
                &#fields_static
            }
        }

        // Register in the global struct registry so struct_fields_for() works.
        openwa_game::inventory::submit! {
            openwa_game::registry::StructRegistration {
                fields: &#fields_static,
            }
        }
    };

    expanded.into()
}

/// Extract the concatenated doc comment from a field's attributes.
fn extract_doc_comment(attrs: &[syn::Attribute]) -> String {
    let mut doc = String::new();
    for attr in attrs {
        if let Meta::NameValue(nv) = &attr.meta
            && nv.path.is_ident("doc")
            && let syn::Expr::Lit(expr_lit) = &nv.value
            && let Lit::Str(s) = &expr_lit.lit
        {
            let text = s.value();
            let trimmed = text.trim();
            if !doc.is_empty() && !trimmed.is_empty() {
                doc.push(' ');
            }
            doc.push_str(trimmed);
        }
    }
    doc
}

/// Parse an explicit `#[field(kind = "Fixed")]` attribute override.
fn parse_field_kind_attr(attrs: &[syn::Attribute]) -> Option<proc_macro2::TokenStream> {
    for attr in attrs {
        if !attr.path().is_ident("field") {
            continue;
        }
        let mut kind_str = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("kind") {
                let value = meta.value()?;
                let lit: Lit = value.parse()?;
                if let Lit::Str(s) = lit {
                    kind_str = Some(s.value());
                }
            }
            Ok(())
        });
        if let Some(k) = kind_str {
            return Some(value_kind_token(&k));
        }
    }
    None
}

/// Infer `ValueKind` from a field's Rust type.
fn infer_value_kind(ty: &syn::Type) -> proc_macro2::TokenStream {
    match ty {
        syn::Type::Ptr(_) => value_kind_token("Pointer"),
        syn::Type::Array(_) => value_kind_token("Raw"),
        syn::Type::Path(tp) => {
            let last = tp.path.segments.last().map(|s| s.ident.to_string());
            match last.as_deref() {
                Some("u8") => value_kind_token("U8"),
                Some("u16") => value_kind_token("U16"),
                Some("u32") => value_kind_token("U32"),
                Some("i8") => value_kind_token("I8"),
                Some("i16") => value_kind_token("I16"),
                Some("i32") => value_kind_token("I32"),
                Some("bool") => value_kind_token("Bool"),
                Some("Fixed") => value_kind_token("Fixed"),
                Some("ClassType") => value_kind_token("Enum"),
                _ => value_kind_token("Raw"),
            }
        }
        _ => value_kind_token("Raw"),
    }
}

/// Produce a `openwa_game::registry::ValueKind::Variant` token stream.
fn value_kind_token(variant: &str) -> proc_macro2::TokenStream {
    let ident = quote::format_ident!("{}", variant);
    quote! { openwa_game::registry::ValueKind::#ident }
}

/// Recursively substitute generic type parameters with their defaults.
///
/// For example, given `CTask<V>` with defaults `{V → *const c_void}`,
/// produces `CTask<*const c_void>`. Handles nested generics and all
/// common type forms (paths, pointers, arrays, etc.).
fn substitute_generics(
    ty: &syn::Type,
    defaults: &std::collections::HashMap<String, syn::Type>,
) -> syn::Type {
    if defaults.is_empty() {
        return ty.clone();
    }

    match ty {
        syn::Type::Path(tp) => {
            // Check if the entire path is a generic param name (e.g., `V`)
            if tp.qself.is_none() && tp.path.segments.len() == 1 {
                let seg = &tp.path.segments[0];
                if seg.arguments.is_empty()
                    && let Some(default) = defaults.get(&seg.ident.to_string())
                {
                    return default.clone();
                }
            }
            // Recursively substitute within generic arguments (e.g., CTask<V>)
            let mut tp = tp.clone();
            for seg in &mut tp.path.segments {
                if let syn::PathArguments::AngleBracketed(ref mut args) = seg.arguments {
                    for arg in &mut args.args {
                        if let syn::GenericArgument::Type(inner_ty) = arg {
                            *inner_ty = substitute_generics(inner_ty, defaults);
                        }
                    }
                }
            }
            syn::Type::Path(tp)
        }
        syn::Type::Ptr(ptr) => {
            let mut ptr = ptr.clone();
            ptr.elem = Box::new(substitute_generics(&ptr.elem, defaults));
            syn::Type::Ptr(ptr)
        }
        syn::Type::Array(arr) => {
            let mut arr = arr.clone();
            arr.elem = Box::new(substitute_generics(&arr.elem, defaults));
            syn::Type::Array(arr)
        }
        syn::Type::Reference(r) => {
            let mut r = r.clone();
            r.elem = Box::new(substitute_generics(&r.elem, defaults));
            syn::Type::Reference(r)
        }
        _ => ty.clone(),
    }
}
