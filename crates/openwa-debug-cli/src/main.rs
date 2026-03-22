//! CLI tool for the OpenWA debug server.
//!
//! Stateless: connects, sends one request, prints the response, disconnects.
//!
//! Usage:
//!   openwa-debug ping
//!   openwa-debug help
//!   openwa-debug read <ghidra_addr> [len] [--format hex|raw] [--port N]

use std::io::{self, BufReader, BufWriter, Write};
use std::net::TcpStream;
use std::process;
use std::time::Duration;

use openwa_debug_proto::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        process::exit(1);
    }

    let mut port = DEFAULT_PORT;
    let mut format = Format::Hex;
    let mut positional = Vec::new();

    // Parse args
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                port = args.get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| { eprintln!("Error: --port requires a number"); process::exit(1); });
            }
            "--format" => {
                i += 1;
                format = match args.get(i).map(|s| s.as_str()) {
                    Some("hex") => Format::Hex,
                    Some("raw") => Format::Raw,
                    _ => { eprintln!("Error: --format must be 'hex' or 'raw'"); process::exit(1); }
                };
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }

    let request = match positional.first().map(|s| s.as_str()) {
        Some("ping") => Request::Ping,
        Some("help") => Request::Help,
        Some("read") => {
            let addr_str = positional.get(1).unwrap_or_else(|| {
                eprintln!("Error: read requires an address");
                process::exit(1);
            });
            // Detect shell eating '->' as redirect: address ends with '-'
            if addr_str.ends_with('-') {
                eprintln!("Error: address '{}' looks truncated — did the shell eat '->'?", addr_str);
                eprintln!("  Hint: quote the argument: read \"0x7A0884->0xA0->0x0\"");
                process::exit(1);
            }
            let expr = parse_address_expr(addr_str);
            let len = positional.get(2)
                .map(|s| parse_u32(s))
                .unwrap_or(256);
            match expr {
                AddressExpr::Simple { addr, absolute } =>
                    Request::Read { addr, len, absolute },
                AddressExpr::Chain { addr, chain, absolute } =>
                    Request::ReadChain { addr, chain, len, absolute },
            }
        }
        Some("suspend") => Request::Suspend,
        Some("resume") => Request::Resume,
        Some("step") => {
            let count = positional.get(1).map(|s| parse_u32(s) as i32).unwrap_or(1);
            Request::Step { count }
        }
        Some("frame") => Request::Frame,
        Some("break") => {
            let frame = positional.get(1)
                .map(|s| if s == "clear" || s == "off" { -1 } else { parse_u32(s) as i32 })
                .unwrap_or_else(|| { eprintln!("Usage: break <frame> | break clear"); process::exit(1); });
            Request::Break { frame }
        }
        Some("snapshot") => Request::Snapshot,
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            print_usage();
            process::exit(1);
        }
        None => {
            print_usage();
            process::exit(1);
        }
    };

    match send_request(port, &request) {
        Ok(response) => print_response(&response, format),
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}

fn send_request(port: u16, request: &Request) -> io::Result<Response> {
    let stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_secs(5),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_nodelay(true)?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    write_frame(&mut writer, request)?;
    read_frame(&mut reader)
}

fn print_response(response: &Response, format: Format) {
    match response {
        Response::Pong => println!("pong"),
        Response::Help { commands } => {
            println!("Available commands:");
            for cmd in commands {
                println!("  {} — {}", cmd.usage, cmd.description);
            }
        }
        Response::ReadResult { ghidra_addr, runtime_addr, data, pointers } => {
            match format {
                Format::Hex => print_hex_read(*ghidra_addr, *runtime_addr, data, pointers),
                Format::Raw => {
                    let stdout = io::stdout();
                    let mut out = stdout.lock();
                    let _ = out.write_all(data);
                }
            }
        }
        Response::ReadChainResult { steps, ghidra_addr, runtime_addr, data, pointers } => {
            match format {
                Format::Hex => {
                    println!("Pointer chain ({} steps):", steps.len());
                    for (i, step) in steps.iter().enumerate() {
                        println!(
                            "  [{}] *(ghidra:0x{:08X}) = 0x{:08X}  + 0x{:X} = 0x{:08X}",
                            i, step.deref_addr, step.value, step.offset, step.result_addr
                        );
                    }
                    println!();
                    print_hex_read(*ghidra_addr, *runtime_addr, data, pointers);
                }
                Format::Raw => {
                    let stdout = io::stdout();
                    let mut out = stdout.lock();
                    let _ = out.write_all(data);
                }
            }
        }
        Response::Suspended { frame } => println!("Suspended at frame {frame}"),
        Response::Resumed => println!("Resumed"),
        Response::FrameInfo { frame, paused, breakpoint } => {
            let state = if *paused { "PAUSED" } else { "running" };
            print!("Frame {frame} [{state}]");
            if *breakpoint >= 0 {
                print!("  breakpoint={breakpoint}");
            }
            println!();
        }
        Response::BreakSet { frame } => {
            if *frame >= 0 {
                println!("Breakpoint set at frame {frame}");
            } else {
                println!("Breakpoint cleared");
            }
        }
        Response::Snapshot { frame, text } => {
            println!("=== Snapshot at frame {frame} ===\n");
            print!("{text}");
        }
        Response::Error { message } => {
            eprintln!("Server error: {message}");
            process::exit(1);
        }
    }
}

fn print_hex_read(ghidra_addr: u32, runtime_addr: u32, data: &[u8], pointers: &[PointerInfo]) {
    println!(
        "Reading 0x{:X} bytes at ghidra:0x{:08X} (runtime:0x{:08X})\n",
        data.len(), ghidra_addr, runtime_addr
    );

    // Hex dump with ASCII sidebar
    for (chunk_idx, chunk) in data.chunks(16).enumerate() {
        let offset = chunk_idx * 16;
        // Offset column
        print!("{:08X}  ", offset);
        // Hex bytes
        for (i, byte) in chunk.iter().enumerate() {
            print!("{:02X} ", byte);
            if i == 7 { print!(" "); }
        }
        // Padding for short last line
        if chunk.len() < 16 {
            for i in chunk.len()..16 {
                print!("   ");
                if i == 7 { print!(" "); }
            }
        }
        // ASCII sidebar
        print!(" |");
        for byte in chunk {
            if *byte >= 0x20 && *byte <= 0x7E {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }

    // Pointer annotations
    if !pointers.is_empty() {
        println!("\nPointers detected:");
        for p in pointers {
            let kind_str = match p.kind {
                PointerKind::Vtable => "VTABLE",
                PointerKind::Code => "CODE",
                PointerKind::Data => "DATA",
                PointerKind::Object => "OBJECT",
                PointerKind::Heap => "HEAP",
            };
            let detail_str = p.detail.as_deref().map(|d| format!("  {d}")).unwrap_or_default();
            println!(
                "  +0x{:03X}  0x{:08X}  {:<7} ghidra:0x{:08X}{}",
                p.offset, p.raw_value, kind_str, p.ghidra_value, detail_str
            );
        }
    }
}

#[derive(Clone, Copy)]
enum Format {
    Hex,
    Raw,
}

/// Parsed address expression — either a simple address or a pointer chain.
enum AddressExpr {
    /// Simple address (with optional offset already applied)
    Simple { addr: u32, absolute: bool },
    /// Pointer chain: start address + list of deref offsets
    Chain { addr: u32, chain: Vec<u32>, absolute: bool },
}

/// Parse an address expression.
///
/// Syntax:
///   0x669F8C              — Ghidra VA
///   abs:0x7FFF0000        — absolute runtime address
///   0x669F8C+0x10         — Ghidra VA + hex offset
///   0x669F8C[0x10]        — same as +0x10 (bracket notation)
///   0x7A0884->0xA0->0x2C  — pointer chain (deref at each ->)
///   abs:0x7FFF0000->4->0  — absolute + chain
///
/// Chain semantics: read DWORD at addr, add first offset, read DWORD, add
/// second offset, ... display memory at final address.
fn parse_address_expr(s: &str) -> AddressExpr {
    let (s, absolute) = if let Some(rest) = s.strip_prefix("abs:") {
        (rest, true)
    } else {
        (s.as_ref(), false)
    };

    // Check for chain syntax: contains "->"
    if s.contains("->") {
        let parts: Vec<&str> = s.split("->").collect();
        let base = parse_addr_with_offset(parts[0]);
        let chain: Vec<u32> = parts[1..].iter().map(|p| parse_compound_offset(p)).collect();
        return AddressExpr::Chain { addr: base, chain, absolute };
    }

    // Simple address with optional offset
    AddressExpr::Simple { addr: parse_addr_with_offset(s), absolute }
}

/// Parse a single address token, possibly with +offset or [offset].
fn parse_addr_with_offset(s: &str) -> u32 {
    let (base_str, offset) = split_compound(s);
    parse_hex(base_str).wrapping_add(offset)
}

/// Parse a chain offset segment, possibly with +offset or [offset].
/// E.g. "0x488+0x10" → 0x498, "0xA0[4]" → 0xA4, "0x510" → 0x510.
fn parse_compound_offset(s: &str) -> u32 {
    let (base_str, offset) = split_compound(s);
    parse_u32(base_str).wrapping_add(offset)
}

/// Split a string on '+' or '[' into (base, offset). Returns (s, 0) if no compound.
fn split_compound(s: &str) -> (&str, u32) {
    if let Some(pos) = s.find('+') {
        let (base, off) = s.split_at(pos);
        (base, parse_u32(&off[1..]))
    } else if let Some(pos) = s.find('[') {
        let base = &s[..pos];
        let off_str = s[pos + 1..].trim_end_matches(']');
        (base, parse_u32(off_str))
    } else {
        (s, 0u32)
    }
}

fn parse_hex(s: &str) -> u32 {
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(s, 16).unwrap_or_else(|_| {
        eprintln!("Error: invalid hex address '{s}'");
        process::exit(1);
    })
}

fn parse_u32(s: &str) -> u32 {
    // Support both hex (0x...) and decimal
    if s.starts_with("0x") || s.starts_with("0X") {
        let hex = &s[2..];
        u32::from_str_radix(hex, 16).unwrap_or_else(|_| {
            eprintln!("Error: invalid hex number '{s}'");
            process::exit(1);
        })
    } else {
        s.parse().unwrap_or_else(|_| {
            eprintln!("Error: invalid number '{s}'");
            process::exit(1);
        })
    }
}

fn print_usage() {
    eprintln!("Usage: openwa-debug <command> [args...] [--port N] [--format hex|raw]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  ping                      Check if server is running");
    eprintln!("  help                      List server commands");
    eprintln!("  read <addr> [len]         Read memory at address (default len=256)");
    eprintln!("  suspend                   Pause game at next frame boundary");
    eprintln!("  resume                    Resume game");
    eprintln!("  step [N]                  Advance N frames (default 1), then pause");
    eprintln!("  frame                     Show current frame and pause state");
    eprintln!("  break <N>                 Set frame breakpoint (break clear to remove)");
    eprintln!("  snapshot                  Dump canonicalized game state (for diffing)");
    eprintln!();
    eprintln!("Address syntax:");
    eprintln!("  0x669F8C                  Ghidra VA (rebased automatically)");
    eprintln!("  abs:0x7FFF0000            Absolute runtime address (no rebase)");
    eprintln!("  0x669F8C+0x10             Address + hex offset");
    eprintln!("  0x669F8C[16]              Address + decimal offset (bracket notation)");
    eprintln!("  0x7A0884->0xA0->0x2C      Pointer chain (deref at each ->)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --port <N>                Server port (default: 19840)");
    eprintln!("  --format hex|raw          Output format (default: hex)");
}
