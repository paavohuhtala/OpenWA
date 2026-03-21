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
            let addr = parse_addr(addr_str);
            let len = positional.get(2)
                .map(|s| parse_u32(s))
                .unwrap_or(256);
            Request::Read { addr, len }
        }
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

fn parse_addr(s: &str) -> u32 {
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
    eprintln!("  read <addr> [len]         Read memory (addr is Ghidra VA, len default 256)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --port <N>                Server port (default: 19840)");
    eprintln!("  --format hex|raw          Output format (default: hex)");
}
