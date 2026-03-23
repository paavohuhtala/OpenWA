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
use syn::{parse_macro_input, Data, DeriveInput, Fields, Lit, Meta};

/// Derive `HasFieldRegistry` for a `#[repr(C)]` struct.
///
/// Generates a static `StructFields` table with one `FieldEntry` per named
/// field (excluding `_unknown*` and `_pad*` prefixes). Offsets and sizes
/// are computed at compile time via `core::mem::offset_of!()` and
/// `core::mem::size_of::<Type>()`.
#[proc_macro_derive(FieldRegistry)]
pub fn derive_field_registry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let struct_name_str = struct_name.to_string();

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
            return syn::Error::new_spanned(struct_name, "FieldRegistry can only be used on structs")
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

        entries.push(quote! {
            openwa_core::registry::FieldEntry {
                offset: core::mem::offset_of!(#struct_name, #field_ident) as u32,
                name: #field_name,
                size: core::mem::size_of::<#field_ty>() as u32,
                doc: #doc,
            }
        });
    }

    let entry_count = entries.len();

    let expanded = quote! {
        impl openwa_core::registry::HasFieldRegistry for #struct_name {
            fn field_registry() -> &'static openwa_core::registry::StructFields {
                // Fields are already sorted by offset because #[repr(C)]
                // guarantees declaration order = memory order.
                static FIELDS: openwa_core::registry::StructFields = openwa_core::registry::StructFields {
                    struct_name: #struct_name_str,
                    fields: {
                        const ENTRIES: [openwa_core::registry::FieldEntry; #entry_count] = [
                            #(#entries),*
                        ];
                        &ENTRIES
                    },
                };
                &FIELDS
            }
        }
    };

    expanded.into()
}

/// Extract the concatenated doc comment from a field's attributes.
fn extract_doc_comment(attrs: &[syn::Attribute]) -> String {
    let mut doc = String::new();
    for attr in attrs {
        if let Meta::NameValue(nv) = &attr.meta {
            if nv.path.is_ident("doc") {
                if let syn::Expr::Lit(expr_lit) = &nv.value {
                    if let Lit::Str(s) = &expr_lit.lit {
                        let text = s.value();
                        let trimmed = text.trim();
                        if !doc.is_empty() && !trimmed.is_empty() {
                            doc.push(' ');
                        }
                        doc.push_str(trimmed);
                    }
                }
            }
        }
    }
    doc
}
