//! Structured address registry for WA.exe reverse engineering.
//!
//! Provides a queryable database of known addresses (functions, vtables,
//! globals, etc.) collected from distributed `define_addresses!` invocations
//! across the codebase. Enables runtime pointer identification for debug tools.
//!
//! # Usage
//!
//! ```ignore
//! use openwa_core::registry;
//!
//! // Look up a Ghidra VA
//! if let Some(resolved) = registry::lookup_va(0x0056_25A0) {
//!     println!("{}", resolved.entry.name); // "CTASK_CONSTRUCTOR"
//! }
//!
//! // Identify a vtable
//! if let Some(class) = registry::vtable_class_name(0x0066_9F8C) {
//!     println!("{}", class); // "CTask"
//! }
//!
//! // Format for debug output
//! println!("{}", registry::format_va(0x0056_25A4)); // "CTASK_CONSTRUCTOR+0x4 (0x5625A4)"
//! ```

use std::sync::OnceLock;

/// What kind of known address this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrKind {
    /// A regular function.
    Function,
    /// A class constructor.
    Constructor,
    /// A vtable pointer table in .rdata.
    Vtable,
    /// A specific method within a vtable.
    VtableMethod,
    /// A global variable in .data/.bss.
    Global,
    /// A string literal in .rdata.
    StringLiteral,
    /// A data table (lookup table, constant array, etc.).
    DataTable,
}

/// Calling convention of a function or constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConv {
    Stdcall,
    Thiscall,
    Fastcall,
    Cdecl,
    /// Non-standard register usage (e.g., ESI/EDI params).
    Usercall,
}

/// A single entry in the address registry.
///
/// Created by `define_addresses!` and collected globally via `inventory`.
#[derive(Debug, Clone, Copy)]
pub struct AddrEntry {
    /// Ghidra virtual address (image base 0x400000).
    pub va: u32,
    /// Constant name (e.g., "CTASK_CONSTRUCTOR").
    pub name: &'static str,
    /// What kind of address this is.
    pub kind: AddrKind,
    /// Calling convention (for functions/constructors).
    pub calling_conv: Option<CallingConv>,
    /// Owning class name (e.g., "CTask"), if part of a class block.
    pub class_name: Option<&'static str>,
    /// Brief description from doc comment.
    pub doc: &'static str,
}

inventory::collect!(AddrEntry);

/// Result of looking up a pointer in the registry.
#[derive(Debug)]
pub struct ResolvedAddr {
    /// The matched registry entry.
    pub entry: &'static AddrEntry,
    /// Offset from the entry's VA (0 = exact match).
    pub offset: u32,
}

// --- Sorted table (built once at first query) ---

static SORTED: OnceLock<Vec<&'static AddrEntry>> = OnceLock::new();

fn sorted_entries() -> &'static [&'static AddrEntry] {
    SORTED.get_or_init(|| {
        let mut v: Vec<&'static AddrEntry> = inventory::iter::<AddrEntry>.into_iter().collect();
        v.sort_by_key(|e| e.va);
        v
    })
}

// --- Query API ---

/// Look up a Ghidra VA. Returns the exact match or the nearest entry below.
///
/// For near-misses, `offset` indicates how far past the entry the VA is.
/// Returns `None` if below all known entries or the offset exceeds 0x10000
/// (likely a different symbol).
pub fn lookup_va(ghidra_va: u32) -> Option<ResolvedAddr> {
    let table = sorted_entries();
    match table.binary_search_by_key(&ghidra_va, |e| e.va) {
        Ok(i) => Some(ResolvedAddr {
            entry: table[i],
            offset: 0,
        }),
        Err(i) if i > 0 => {
            let entry = table[i - 1];
            let offset = ghidra_va - entry.va;
            if offset < 0x10000 {
                Some(ResolvedAddr { entry, offset })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Look up a Ghidra VA, exact match only.
pub fn lookup_va_exact(ghidra_va: u32) -> Option<&'static AddrEntry> {
    let table = sorted_entries();
    table
        .binary_search_by_key(&ghidra_va, |e| e.va)
        .ok()
        .map(|i| table[i])
}

/// Given a Ghidra vtable address, return the class name.
///
/// Replaces all duplicated `vtable_name()` functions throughout the codebase.
pub fn vtable_class_name(ghidra_vtable: u32) -> Option<&'static str> {
    lookup_va_exact(ghidra_vtable)
        .filter(|e| e.kind == AddrKind::Vtable)
        .and_then(|e| e.class_name)
}

/// Format a Ghidra VA as a human-readable string.
///
/// - Exact match: `"CTASK_CONSTRUCTOR (0x5625A0)"`
/// - Near match: `"CTASK_CONSTRUCTOR+0x4 (0x5625A4)"`
/// - Unknown: `"0x005625A4"`
pub fn format_va(ghidra_va: u32) -> String {
    match lookup_va(ghidra_va) {
        Some(r) if r.offset == 0 => {
            format!("{} (0x{:X})", r.entry.name, ghidra_va)
        }
        Some(r) => {
            format!(
                "{}+0x{:X} (0x{:X})",
                r.entry.name, r.offset, ghidra_va
            )
        }
        None => format!("0x{:08X}", ghidra_va),
    }
}

/// Iterate all entries of a given kind.
pub fn entries_by_kind(kind: AddrKind) -> impl Iterator<Item = &'static AddrEntry> {
    sorted_entries().iter().copied().filter(move |e| e.kind == kind)
}

/// Iterate all registered entries (sorted by VA).
pub fn all_entries() -> impl Iterator<Item = &'static AddrEntry> {
    sorted_entries().iter().copied()
}

/// Return the total number of registered entries.
pub fn entry_count() -> usize {
    sorted_entries().len()
}

// --- Struct field registry types (for Phase 4 #[derive(FieldRegistry)]) ---

/// A known field within a struct.
#[derive(Debug, Clone, Copy)]
pub struct FieldEntry {
    /// Offset from struct base.
    pub offset: u32,
    /// Field name (e.g., "rng_state").
    pub name: &'static str,
    /// Field size in bytes.
    pub size: u32,
    /// Brief description.
    pub doc: &'static str,
}

/// Field map for a struct, enabling offset → name lookups.
#[derive(Debug)]
pub struct StructFields {
    /// Struct/class name (e.g., "DDGame").
    pub struct_name: &'static str,
    /// Fields sorted by offset.
    pub fields: &'static [FieldEntry],
}

impl StructFields {
    /// Look up the field at an exact offset.
    pub fn field_at(&self, offset: u32) -> Option<&FieldEntry> {
        self.fields
            .binary_search_by_key(&offset, |f| f.offset)
            .ok()
            .map(|i| &self.fields[i])
    }

    /// Find which field contains the given offset.
    ///
    /// Returns the field and the byte offset within it.
    pub fn field_containing(&self, offset: u32) -> Option<(&FieldEntry, u32)> {
        match self.fields.binary_search_by_key(&offset, |f| f.offset) {
            Ok(i) => Some((&self.fields[i], 0)),
            Err(i) if i > 0 => {
                let f = &self.fields[i - 1];
                if offset < f.offset + f.size {
                    Some((f, offset - f.offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Trait for structs that have an auto-generated field registry.
pub trait HasFieldRegistry {
    fn field_registry() -> &'static StructFields;
}

/// A struct's field registry submitted to the global collection via `inventory`.
///
/// The `#[derive(FieldRegistry)]` macro emits an `inventory::submit!` for each
/// annotated struct, making it discoverable at runtime via
/// [`struct_fields_for`].
pub struct StructRegistration {
    pub fields: &'static StructFields,
}

inventory::collect!(StructRegistration);

static STRUCT_MAP: OnceLock<std::collections::HashMap<&'static str, &'static StructFields>> =
    OnceLock::new();

fn struct_map() -> &'static std::collections::HashMap<&'static str, &'static StructFields> {
    STRUCT_MAP.get_or_init(|| {
        inventory::iter::<StructRegistration>
            .into_iter()
            .map(|r| (r.fields.struct_name, r.fields))
            .collect()
    })
}

/// Look up the field registry for a struct by name.
///
/// Returns `None` if the struct doesn't have `#[derive(FieldRegistry)]`.
pub fn struct_fields_for(struct_name: &str) -> Option<&'static StructFields> {
    struct_map().get(struct_name).copied()
}

/// Look up the field registry for a class identified by its vtable address.
///
/// Combines vtable → class name lookup with struct fields lookup.
pub fn struct_fields_for_vtable(ghidra_vtable: u32) -> Option<&'static StructFields> {
    let class = vtable_class_name(ghidra_vtable)?;
    struct_fields_for(class)
}

/// Return all registered struct field registries.
pub fn all_struct_fields() -> impl Iterator<Item = &'static StructFields> {
    struct_map().values().copied()
}

// =========================================================================
// Live object tracker
// =========================================================================

/// A tracked live heap object.
#[derive(Debug, Clone)]
pub struct LiveObject {
    /// Runtime base address.
    pub ptr: u32,
    /// Object size in bytes (0 if unknown).
    pub size: u32,
    /// Class/struct name (e.g., "DDGame").
    pub class_name: &'static str,
    /// Field registry for this struct (if available).
    pub fields: Option<&'static StructFields>,
}

use std::sync::Mutex;

static LIVE_OBJECTS: Mutex<Vec<LiveObject>> = Mutex::new(Vec::new());

/// Register a live heap object for pointer identification.
///
/// Call this from constructor hooks when a game object is allocated.
pub fn register_live_object(obj: LiveObject) {
    if let Ok(mut v) = LIVE_OBJECTS.lock() {
        // Replace if same pointer already tracked (re-allocation)
        if let Some(existing) = v.iter_mut().find(|o| o.ptr == obj.ptr) {
            *existing = obj;
        } else {
            v.push(obj);
        }
    }
}

/// Unregister a live heap object (e.g., on destruction).
pub fn unregister_live_object(ptr: u32) {
    if let Ok(mut v) = LIVE_OBJECTS.lock() {
        v.retain(|o| o.ptr != ptr);
    }
}

/// Result of identifying a pointer as being inside a tracked live object.
#[derive(Debug)]
pub struct LiveObjectMatch {
    pub object: LiveObject,
    /// Byte offset of the pointer within the object.
    pub offset: u32,
    /// Field at this offset (if known).
    pub field: Option<&'static FieldEntry>,
}

/// Check if a runtime pointer falls inside any tracked live object.
///
/// Returns the matching object and the field at the pointer's offset.
pub fn identify_live_pointer(runtime_ptr: u32) -> Option<LiveObjectMatch> {
    let v = LIVE_OBJECTS.lock().ok()?;
    for obj in v.iter() {
        let end = if obj.size > 0 {
            obj.ptr + obj.size
        } else {
            // Unknown size — use a generous 256KB bound
            obj.ptr + 0x40000
        };
        if runtime_ptr >= obj.ptr && runtime_ptr < end {
            let offset = runtime_ptr - obj.ptr;
            let field = obj.fields.and_then(|f| {
                f.field_at(offset)
                    .or_else(|| f.field_containing(offset).map(|(fe, _)| fe))
            });
            return Some(LiveObjectMatch {
                object: obj.clone(),
                offset,
                field,
            });
        }
    }
    None
}

/// Return a snapshot of all currently tracked live objects.
pub fn live_objects() -> Vec<LiveObject> {
    LIVE_OBJECTS.lock().map(|v| v.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test entries defined via the macro
    define_addresses! {
        class "TestClass" {
            /// A test vtable
            vtable TEST_VTABLE = 0x0066_0000;
            ctor/Stdcall TEST_CTOR = 0x0050_0000;
            vmethod TEST_VMETHOD = 0x0050_1000;
        }

        /// A standalone function
        fn/Fastcall TEST_FUNC = 0x0053_0000;
        global TEST_GLOBAL = 0x007A_0000;
    }

    #[test]
    fn macro_generates_constants() {
        assert_eq!(TEST_VTABLE, 0x0066_0000);
        assert_eq!(TEST_CTOR, 0x0050_0000);
        assert_eq!(TEST_FUNC, 0x0053_0000);
        assert_eq!(TEST_GLOBAL, 0x007A_0000);
    }

    #[test]
    fn registry_collects_entries() {
        // The inventory should have collected our test entries
        let count = entry_count();
        assert!(count >= 5, "expected at least 5 entries, got {count}");
    }

    #[test]
    fn exact_lookup() {
        let entry = lookup_va_exact(0x0066_0000);
        assert!(entry.is_some(), "TEST_VTABLE not found");
        let entry = entry.unwrap();
        assert_eq!(entry.name, "TEST_VTABLE");
        assert_eq!(entry.kind, AddrKind::Vtable);
        assert_eq!(entry.class_name, Some("TestClass"));
    }

    #[test]
    fn near_lookup() {
        let resolved = lookup_va(0x0066_0004);
        assert!(resolved.is_some());
        let resolved = resolved.unwrap();
        assert_eq!(resolved.entry.name, "TEST_VTABLE");
        assert_eq!(resolved.offset, 4);
    }

    #[test]
    fn vtable_class_name_lookup() {
        assert_eq!(vtable_class_name(0x0066_0000), Some("TestClass"));
        // Constructor should not match vtable lookup
        assert_eq!(vtable_class_name(0x0050_0000), None);
    }

    #[test]
    fn format_va_display() {
        let s = format_va(0x0066_0000);
        assert!(s.contains("TEST_VTABLE"), "got: {s}");

        let s = format_va(0x0066_0004);
        assert!(s.contains("TEST_VTABLE+0x4"), "got: {s}");

        let s = format_va(0x0000_1234);
        assert_eq!(s, "0x00001234");
    }

    #[test]
    fn entries_by_kind_filter() {
        let vtables: Vec<_> = entries_by_kind(AddrKind::Vtable).collect();
        assert!(
            vtables.iter().any(|e| e.name == "TEST_VTABLE"),
            "TEST_VTABLE not in vtable list"
        );
        assert!(
            !vtables.iter().any(|e| e.name == "TEST_FUNC"),
            "TEST_FUNC should not be in vtable list"
        );
    }

    #[test]
    fn struct_fields_lookup() {
        static FIELDS: StructFields = StructFields {
            struct_name: "TestStruct",
            fields: &[
                FieldEntry { offset: 0x00, name: "vtable", size: 4, doc: "" },
                FieldEntry { offset: 0x10, name: "health", size: 4, doc: "" },
                FieldEntry { offset: 0x20, name: "name", size: 16, doc: "" },
            ],
        };

        assert_eq!(FIELDS.field_at(0x10).unwrap().name, "health");
        assert!(FIELDS.field_at(0x08).is_none());

        let (f, off) = FIELDS.field_containing(0x22).unwrap();
        assert_eq!(f.name, "name");
        assert_eq!(off, 2);

        assert!(FIELDS.field_containing(0x30).is_none());
    }

    #[test]
    fn derive_field_registry_ctask() {
        use crate::task::CTask;

        let reg = CTask::field_registry();
        assert_eq!(reg.struct_name, "CTask");

        // CTask has known fields at known offsets
        let vtable = reg.field_at(0x00).expect("vtable field at 0x00");
        assert_eq!(vtable.name, "vtable");
        assert_eq!(vtable.size, 4);

        let ddgame = reg.field_at(0x2C).expect("ddgame field at 0x2C");
        assert_eq!(ddgame.name, "ddgame");

        // _unknown_1c should be skipped
        assert!(
            reg.fields.iter().all(|f| !f.name.starts_with("_unknown")),
            "unknown fields should be excluded"
        );
    }

    #[test]
    fn derive_field_registry_ddgame() {
        use crate::engine::DDGame;

        let reg = DDGame::field_registry();
        assert_eq!(reg.struct_name, "DDGame");

        // DDGame has keyboard at 0x00
        let keyboard = reg.field_at(0x00).expect("keyboard at 0x00");
        assert_eq!(keyboard.name, "keyboard");

        // game_info at 0x24
        let gi = reg.field_at(0x24).expect("game_info at 0x24");
        assert_eq!(gi.name, "game_info");

        // Should have many fields (DDGame is huge)
        assert!(
            reg.fields.len() > 20,
            "DDGame should have >20 named fields, got {}",
            reg.fields.len()
        );

        // No unknown fields
        assert!(
            reg.fields.iter().all(|f| !f.name.starts_with("_unknown")),
            "unknown fields should be excluded"
        );
    }

    #[test]
    fn derive_field_registry_game_session() {
        use crate::engine::GameSession;

        let reg = GameSession::field_registry();
        assert_eq!(reg.struct_name, "GameSession");

        // ddgame_wrapper at 0xA0
        let wrapper = reg.field_at(0xA0).expect("ddgame_wrapper at 0xA0");
        assert_eq!(wrapper.name, "ddgame_wrapper");
    }

    #[test]
    fn derive_preserves_doc_comments() {
        use crate::task::CTask;

        let reg = CTask::field_registry();
        let vtable = reg.field_at(0x00).unwrap();
        // Doc comment should be non-empty (we have "0x00: Pointer to virtual method table")
        assert!(!vtable.doc.is_empty(), "doc should be extracted: {:?}", vtable.doc);
    }

    #[test]
    fn struct_fields_for_lookup() {
        // Global registry should find structs by name
        let ddgame = struct_fields_for("DDGame");
        assert!(ddgame.is_some(), "DDGame not found in struct registry");
        assert_eq!(ddgame.unwrap().struct_name, "DDGame");

        let ctask = struct_fields_for("CTask");
        assert!(ctask.is_some(), "CTask not found in struct registry");

        let session = struct_fields_for("GameSession");
        assert!(session.is_some(), "GameSession not found in struct registry");

        // Unknown struct returns None
        assert!(struct_fields_for("FooBarBaz").is_none());
    }

    #[test]
    fn struct_fields_for_vtable_lookup() {
        use crate::address::va;

        // DDGameWrapper vtable → "DDGameWrapper" → DDGameWrapper fields
        let fields = struct_fields_for_vtable(va::DDGAME_WRAPPER_VTABLE);
        assert!(fields.is_some(), "DDGameWrapper fields not found via vtable");
        assert_eq!(fields.unwrap().struct_name, "DDGameWrapper");

        // CTask vtable → "CTask" → CTask fields
        let fields = struct_fields_for_vtable(va::CTASK_VTABLE);
        assert!(fields.is_some(), "CTask fields not found via vtable");
        assert_eq!(fields.unwrap().struct_name, "CTask");
    }
}
