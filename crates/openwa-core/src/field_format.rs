//! Extensible field value formatting for debug tools.
//!
//! Provides a [`FieldFormatter`] trait and a top-level [`format_field`] function
//! that writes human-readable field values to any `fmt::Write` target.
//!
//! Default formatters handle scalars, Fixed, pointers, and booleans.
//! Custom formatters (e.g., for game-specific enums) can be registered via
//! `inventory::submit!` from any crate.

use core::fmt;

use crate::registry::{FieldEntry, ValueKind};

/// Context passed to formatters (avoids bloating the trait signature).
pub struct FormatContext {
    /// ASLR delta for pointer resolution (runtime_base - 0x400000).
    pub delta: u32,
}

/// Trait for formatting field values.
///
/// Implementations are registered via `inventory::submit!` and dispatched
/// by [`format_field`] based on `ValueKind`.
///
/// Writes to `&mut dyn fmt::Write` — no allocations required.
pub trait FieldFormatter: Send + Sync + 'static {
    /// Which `ValueKind`(s) this formatter handles.
    fn handles(&self) -> &[ValueKind];

    /// Format `data` (the raw bytes of the field) into `w`.
    fn format_field(
        &self,
        w: &mut dyn fmt::Write,
        data: &[u8],
        field: &FieldEntry,
        ctx: &FormatContext,
    ) -> fmt::Result;
}

// Inventory collection for custom formatters.
inventory::collect!(Box<dyn FieldFormatter>);

/// Write a formatted field value to any `fmt::Write` target.
///
/// Dispatches to registered custom formatters first, then falls back to
/// built-in defaults based on `field.kind`.
pub fn format_field(
    w: &mut dyn fmt::Write,
    data: &[u8],
    field: &FieldEntry,
    ctx: &FormatContext,
) -> fmt::Result {
    // Check custom formatters first
    for formatter in inventory::iter::<Box<dyn FieldFormatter>> {
        if formatter.handles().contains(&field.kind) {
            return formatter.format_field(w, data, field, ctx);
        }
    }

    // Built-in defaults
    match field.kind {
        ValueKind::U8 if data.len() >= 1 => {
            write!(w, "{}", data[0])
        }
        ValueKind::I8 if data.len() >= 1 => {
            write!(w, "{}", data[0] as i8)
        }
        ValueKind::U16 if data.len() >= 2 => {
            let v = u16::from_le_bytes([data[0], data[1]]);
            write!(w, "{}", v)
        }
        ValueKind::I16 if data.len() >= 2 => {
            let v = i16::from_le_bytes([data[0], data[1]]);
            write!(w, "{}", v)
        }
        ValueKind::U32 if data.len() >= 4 => {
            let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if v > 0xFFFF {
                write!(w, "0x{:08X}", v)
            } else {
                write!(w, "{}", v)
            }
        }
        ValueKind::I32 if data.len() >= 4 => {
            let v = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            write!(w, "{}", v)
        }
        ValueKind::Bool if data.len() >= 1 => match data[0] {
            0 => write!(w, "false"),
            1 => write!(w, "true"),
            v => write!(w, "??(0x{:02X})", v),
        },
        ValueKind::Fixed if data.len() >= 4 => {
            let raw = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let f = crate::fixed::Fixed(raw);
            write!(w, "{:.4} (0x{:08X})", f.to_f32(), raw as u32)
        }
        ValueKind::Pointer if data.len() >= 4 => {
            let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if v == 0 {
                write!(w, "null")
            } else {
                format_pointer(w, v, ctx.delta)
            }
        }
        ValueKind::Enum if data.len() >= 4 => {
            // Default enum formatting — just show the numeric value.
            // Custom formatters can override for specific enums.
            let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            write!(w, "enum({})", v)
        }
        ValueKind::CString => {
            let nul = data.iter().position(|&b| b == 0).unwrap_or(data.len());
            let s = core::str::from_utf8(&data[..nul]).unwrap_or("<invalid utf8>");
            write!(w, "\"{}\"", s)
        }
        ValueKind::Struct => {
            write!(w, "<struct {} bytes>", data.len())
        }
        // Raw fallback: hex dump
        _ => format_raw_hex(w, data),
    }
}

/// Format a pointer value using the registry for symbolication.
fn format_pointer(w: &mut dyn fmt::Write, value: u32, delta: u32) -> fmt::Result {
    let ghidra = value.wrapping_sub(delta);

    // Try static registry lookup
    if let Some(resolved) = crate::registry::lookup_va(ghidra) {
        if resolved.offset == 0 {
            return write!(w, "0x{:08X} ({})", value, resolved.entry.name);
        } else {
            return write!(
                w,
                "0x{:08X} ({}+0x{:X})",
                value, resolved.entry.name, resolved.offset
            );
        }
    }

    // Try live object identification
    #[cfg(target_os = "windows")]
    if let Some(live) = crate::registry::identify_live_pointer(value) {
        if let Some(field) = live.field {
            return write!(
                w,
                "0x{:08X} ({}+0x{:X} .{})",
                value, live.object.class_name, live.offset, field.name
            );
        } else {
            return write!(
                w,
                "0x{:08X} ({}+0x{:X})",
                value, live.object.class_name, live.offset
            );
        }
    }

    // Try vtable-based class detection
    #[cfg(target_os = "windows")]
    if unsafe { crate::mem::can_read(value, 4) } {
        let first_dword = unsafe { *(value as *const u32) };
        let vtable_ghidra = first_dword.wrapping_sub(delta);
        if let Some(class) = crate::registry::vtable_class_name(vtable_ghidra) {
            return write!(w, "0x{:08X} ({}*)", value, class);
        }
    }

    // Bare hex
    write!(w, "0x{:08X}", value)
}

/// Format raw bytes as hex.
fn format_raw_hex(w: &mut dyn fmt::Write, data: &[u8]) -> fmt::Result {
    for (i, byte) in data.iter().enumerate() {
        if i > 0 {
            write!(w, " ")?;
        }
        write!(w, "{:02X}", byte)?;
        // Truncate long arrays
        if i >= 15 && data.len() > 16 {
            write!(w, " ... ({} bytes)", data.len())?;
            break;
        }
    }
    Ok(())
}
