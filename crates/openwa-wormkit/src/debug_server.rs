//! TCP debug server for live memory inspection.
//!
//! When `OPENWA_DEBUG_SERVER=1`, spawns a background thread that listens
//! on `127.0.0.1:19840` and serves memory read requests. One client at
//! a time. All addresses are Ghidra VAs — the server rebases automatically.

use std::io::{self, BufReader, BufWriter};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use openwa_core::address::va;
use openwa_core::mem;
use openwa_core::rebase::rb;
use openwa_debug_proto::*;

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
        Request::Read { addr, len, absolute } => handle_read(addr, len, absolute),
        Request::ReadChain { addr, chain, len, absolute } => handle_read_chain(addr, &chain, len, absolute),
        Request::Suspend => {
            crate::debug_sync::suspend();
            Response::Suspended { frame: crate::debug_sync::current_frame() }
        }
        Request::Resume => {
            crate::debug_sync::resume();
            Response::Resumed
        }
        Request::Step { count } => {
            crate::debug_sync::step(count);
            // Wait briefly for the step to complete, then report
            std::thread::sleep(Duration::from_millis(100));
            Response::Suspended { frame: crate::debug_sync::current_frame() }
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
        std::ptr::copy_nonoverlapping(
            runtime_addr as *const u8,
            data.as_mut_ptr(),
            len as usize,
        );

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
