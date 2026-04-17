//! TCP debug server for live memory inspection.
//!
//! When `OPENWA_DEBUG_SERVER=1`, spawns a background thread that listens
//! on `127.0.0.1:19840` and serves memory read requests. One client at
//! a time. All addresses are Ghidra VAs — the server rebases automatically.

use std::io::{self, BufReader, BufWriter};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use openwa_debug_proto::*;
use openwa_game::address::va;
use openwa_game::mem;
use openwa_game::rebase::rb;

use crate::log_line;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Start the debug server if `OPENWA_DEBUG_SERVER=1`.
/// Called from `run()` in lib.rs. No-op if env var is not set.
pub fn maybe_start() {
    if std::env::var("OPENWA_DEBUG_SERVER").is_ok() {
        let _ = log_line("[DebugServer] Starting...");
        std::thread::spawn(|| {
            if let Err(e) = server_thread() {
                let _ = log_line(&format!("[DebugServer] Fatal: {e}"));
            }
        });
    }
}

fn server_thread() -> io::Result<()> {
    let listener = match TcpListener::bind(("127.0.0.1", DEFAULT_PORT)) {
        Ok(l) => l,
        Err(e) => {
            let _ = log_line(&format!(
                "[DebugServer] Failed to bind port {}: {e} (skipping)",
                DEFAULT_PORT
            ));
            return Ok(());
        }
    };

    let _ = log_line(&format!(
        "[DebugServer] Listening on 127.0.0.1:{}",
        DEFAULT_PORT
    ));

    while !SHUTDOWN.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let _ = log_line(&format!("[DebugServer] Client connected: {addr}"));
                if let Err(e) = handle_client(stream) {
                    let _ = log_line(&format!("[DebugServer] Client error: {e}"));
                }
                let _ = log_line("[DebugServer] Client disconnected");
            }
            Err(e) => {
                if !SHUTDOWN.load(Ordering::Relaxed) {
                    let _ = log_line(&format!("[DebugServer] Accept error: {e}"));
                }
            }
        }
    }
    Ok(())
}

fn handle_client(stream: TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(300)))?;
    stream.set_nodelay(true)?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    loop {
        let request: Request = match read_frame(&mut reader) {
            Ok(r) => r,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::TimedOut => return Ok(()),
            Err(e) => return Err(e),
        };

        let response = handle_request(request);
        write_frame(&mut writer, &response)?;
    }
}

fn handle_request(request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong,
        Request::Help => Response::Help {
            commands: vec![
                CommandHelp {
                    name: "ping".into(),
                    usage: "ping".into(),
                    description: "Check if the debug server is running".into(),
                },
                CommandHelp {
                    name: "help".into(),
                    usage: "help".into(),
                    description: "List available commands".into(),
                },
                CommandHelp {
                    name: "read".into(),
                    usage: "read <ghidra_addr> [len]".into(),
                    description: "Read memory at a Ghidra VA (default len=256, max 1MB)".into(),
                },
            ],
        },
        Request::Read {
            addr,
            len,
            absolute,
        } => handle_read(addr, len, absolute),
        Request::ReadChain {
            addr,
            chain,
            len,
            absolute,
        } => handle_read_chain(addr, &chain, len, absolute),
        Request::Suspend => {
            crate::debug_sync::suspend();
            Response::Suspended {
                frame: crate::debug_sync::current_frame(),
            }
        }
        Request::Resume => {
            crate::debug_sync::resume();
            Response::Resumed
        }
        Request::Step { count } => {
            crate::debug_sync::step(count);
            // Wait briefly for the step to complete, then report
            std::thread::sleep(Duration::from_millis(100));
            Response::Suspended {
                frame: crate::debug_sync::current_frame(),
            }
        }
        Request::Frame => Response::FrameInfo {
            frame: crate::debug_sync::current_frame(),
            paused: crate::debug_sync::is_paused(),
            breakpoint: crate::debug_sync::breakpoint(),
        },
        Request::Break { frame } => {
            crate::debug_sync::set_breakpoint(frame);
            Response::BreakSet { frame }
        }
        Request::Snapshot => {
            let text = unsafe { crate::snapshot::capture() };
            let frame = crate::debug_sync::current_frame();
            Response::Snapshot { frame, text }
        }
        Request::Inspect {
            class_name,
            addr,
            chain,
            absolute,
        } => handle_inspect(&class_name, addr, &chain, absolute),
        Request::ListObjects => handle_list_objects(),
        Request::ResolveAlias { name } => handle_resolve_alias(&name),
        Request::ResolveField {
            class_name,
            field_name,
        } => handle_resolve_field(&class_name, &field_name),
    }
}

/// Compute ASLR delta and resolve an address to (ghidra, runtime).
fn resolve_addr(addr: u32, absolute: bool) -> (u32, u32, u32) {
    let wa_base = rb(va::IMAGE_BASE);
    let delta = wa_base.wrapping_sub(va::IMAGE_BASE);
    let (ghidra_addr, runtime_addr) = if absolute {
        (addr.wrapping_sub(delta), addr)
    } else {
        (addr, addr.wrapping_add(delta))
    };
    (ghidra_addr, runtime_addr, delta)
}

fn handle_read(addr: u32, len: u32, absolute: bool) -> Response {
    if len == 0 {
        return Response::Error {
            message: "len must be > 0".into(),
        };
    }
    if len > MAX_READ_SIZE {
        return Response::Error {
            message: format!("len {} exceeds max {} bytes", len, MAX_READ_SIZE),
        };
    }

    let (ghidra_addr, runtime_addr, delta) = resolve_addr(addr, absolute);

    unsafe {
        if !mem::can_read(runtime_addr, len) {
            return Response::Error {
                message: format!(
                    "Cannot read {} bytes at ghidra:0x{:08X} (runtime:0x{:08X})",
                    len, ghidra_addr, runtime_addr
                ),
            };
        }

        // Copy the memory region
        let mut data = vec![0u8; len as usize];
        std::ptr::copy_nonoverlapping(runtime_addr as *const u8, data.as_mut_ptr(), len as usize);

        // Classify pointers in the copied data
        let pointers = mem::classify_region(&data, 0, delta);

        Response::ReadResult {
            ghidra_addr,
            runtime_addr,
            data,
            pointers,
        }
    }
}

fn handle_inspect(class_name: &str, addr: u32, chain: &[u32], absolute: bool) -> Response {
    use openwa_game::field_format::{self, FormatContext};
    use openwa_game::registry;

    let fields = match registry::struct_fields_for(class_name) {
        Some(f) => f,
        None => {
            return Response::Error {
                message: format!("No FieldRegistry for '{}'", class_name),
            };
        }
    };

    // Resolve address (possibly through a chain)
    let (_, runtime_addr, delta) = if chain.is_empty() {
        resolve_addr(addr, absolute)
    } else {
        // Walk chain first
        let (_, mut current, delta) = resolve_addr(addr, absolute);
        for &offset in chain {
            unsafe {
                if !mem::can_read(current, 4) {
                    return Response::Error {
                        message: format!("Chain broke: cannot read at runtime:0x{:08X}", current),
                    };
                }
                let value = *(current as *const u32);
                if value == 0 {
                    return Response::Error {
                        message: format!("Chain broke: NULL at runtime:0x{:08X}", current),
                    };
                }
                current = value.wrapping_add(offset);
            }
        }
        (current.wrapping_sub(delta), current, delta)
    };

    let ghidra_addr = runtime_addr.wrapping_sub(delta);
    let ctx = FormatContext { delta };

    let mut result_fields = Vec::new();
    for field in fields.fields {
        unsafe {
            let field_ptr = (runtime_addr + field.offset) as *const u8;
            let size = field.size as usize;

            if !mem::can_read(runtime_addr + field.offset, field.size) {
                result_fields.push(FieldValue {
                    offset: field.offset,
                    name: field.name.to_string(),
                    size: field.size,
                    hex: "<unreadable>".into(),
                    display: "<unreadable>".into(),
                });
                continue;
            }

            let data = core::slice::from_raw_parts(field_ptr, size);

            // Hex representation
            let hex = data
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");

            // Formatted display
            let mut display = String::new();
            let _ = field_format::format_field(&mut display, data, field, &ctx);

            result_fields.push(FieldValue {
                offset: field.offset,
                name: field.name.to_string(),
                size: field.size,
                hex,
                display,
            });
        }
    }

    Response::InspectResult {
        class_name: class_name.to_string(),
        ghidra_addr,
        runtime_addr,
        fields: result_fields,
    }
}

fn handle_list_objects() -> Response {
    use openwa_game::registry;

    let wa_base = rb(va::IMAGE_BASE);
    let wa_end = wa_base + 0x300000; // approximate WA image end
    let delta = wa_base.wrapping_sub(va::IMAGE_BASE);
    let objects = registry::live_objects()
        .into_iter()
        .map(|obj| {
            // ghidra_addr is only meaningful for image-mapped objects, not heap
            let ghidra = if obj.ptr >= wa_base && obj.ptr < wa_end {
                obj.ptr.wrapping_sub(delta)
            } else {
                0
            };
            LiveObjectInfo {
                runtime_addr: obj.ptr,
                ghidra_addr: ghidra,
                size: obj.size,
                class_name: obj.class_name.to_string(),
                field_count: obj.fields.map_or(0, |f| f.fields.len() as u32),
            }
        })
        .collect();

    Response::ObjectList { objects }
}

fn handle_resolve_alias(name: &str) -> Response {
    use openwa_game::registry;

    let name_lower = name.to_lowercase();
    for obj in registry::live_objects() {
        if obj.class_name.to_lowercase() == name_lower {
            return Response::AliasResult(ResolvedAlias {
                runtime_addr: obj.ptr,
                class_name: obj.class_name.to_string(),
            });
        }
    }

    Response::Error {
        message: format!("No tracked object matching '{}'", name),
    }
}

fn handle_resolve_field(class_name: &str, field_name: &str) -> Response {
    use openwa_game::registry;

    // Search the class and its CTask inheritance chain for a field by name.
    // Mirrors the inheritance chain in registry::field_at_inherited.
    const CTASK_CHAIN: &[&str] = &["CGameTask", "CTask"];

    let search_chain: Vec<&str> = std::iter::once(class_name)
        .chain(CTASK_CHAIN.iter().copied())
        .collect();

    for &name in &search_chain {
        if let Some(fields) = registry::struct_fields_for(name) {
            for field in fields.fields {
                if field.name == field_name {
                    return Response::FieldResult(ResolvedField {
                        offset: field.offset,
                        size: field.size,
                    });
                }
            }
        }
    }

    Response::Error {
        message: format!(
            "No field '{}' in '{}' (searched: {})",
            field_name,
            class_name,
            search_chain.join(" -> ")
        ),
    }
}

fn handle_read_chain(addr: u32, chain: &[u32], len: u32, absolute: bool) -> Response {
    if len == 0 || len > MAX_READ_SIZE {
        return Response::Error {
            message: format!("len must be 1..{}", MAX_READ_SIZE),
        };
    }
    if chain.is_empty() {
        return handle_read(addr, len, absolute);
    }

    let (_, mut current_runtime, delta) = resolve_addr(addr, absolute);
    let mut steps = Vec::new();

    // Walk the chain: for each offset, deref current address then add offset
    for &offset in chain.iter() {
        unsafe {
            if !mem::can_read(current_runtime, 4) {
                return Response::Error {
                    message: format!(
                        "Chain broke: cannot read DWORD at runtime:0x{:08X} (ghidra:0x{:08X})",
                        current_runtime,
                        current_runtime.wrapping_sub(delta)
                    ),
                };
            }
            let value = *(current_runtime as *const u32);
            if value == 0 {
                return Response::Error {
                    message: format!(
                        "Chain broke: NULL pointer at runtime:0x{:08X} (ghidra:0x{:08X})",
                        current_runtime,
                        current_runtime.wrapping_sub(delta)
                    ),
                };
            }
            let result_addr = value.wrapping_add(offset);
            steps.push(ChainStep {
                deref_addr: current_runtime.wrapping_sub(delta), // ghidra VA
                value,
                offset,
                result_addr,
            });
            current_runtime = result_addr;
        }
    }

    // Now read memory at the final address
    let final_ghidra = current_runtime.wrapping_sub(delta);

    unsafe {
        if !mem::can_read(current_runtime, len) {
            return Response::Error {
                message: format!(
                    "Cannot read {} bytes at final address runtime:0x{:08X} (ghidra:0x{:08X})",
                    len, current_runtime, final_ghidra
                ),
            };
        }

        let mut data = vec![0u8; len as usize];
        std::ptr::copy_nonoverlapping(
            current_runtime as *const u8,
            data.as_mut_ptr(),
            len as usize,
        );

        let pointers = mem::classify_region(&data, 0, delta);

        Response::ReadChainResult {
            steps,
            ghidra_addr: final_ghidra,
            runtime_addr: current_runtime,
            data,
            pointers,
        }
    }
}
