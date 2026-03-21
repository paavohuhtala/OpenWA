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
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
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
        Request::Read { addr, len } => handle_read(addr, len),
    }
}

fn handle_read(ghidra_addr: u32, len: u32) -> Response {
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

    let wa_base = rb(va::IMAGE_BASE);
    let delta = wa_base.wrapping_sub(va::IMAGE_BASE);
    let runtime_addr = ghidra_addr.wrapping_add(delta);

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
