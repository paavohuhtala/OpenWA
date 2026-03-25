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
            let expr = resolve_address_expr(addr_str, port);
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
        Some("inspect") => {
            let class_name = positional.get(1).unwrap_or_else(|| {
                eprintln!("Usage: inspect <class_name> <addr>");
                process::exit(1);
            }).clone();
            let addr_str = positional.get(2).unwrap_or_else(|| {
                eprintln!("Usage: inspect <class_name> <addr>");
                process::exit(1);
            });
            let expr = resolve_address_expr(addr_str, port);
            match expr {
                AddressExpr::Simple { addr, absolute } =>
                    Request::Inspect { class_name, addr, chain: vec![], absolute },
                AddressExpr::Chain { addr, chain, absolute } =>
                    Request::Inspect { class_name, addr, chain, absolute },
            }
        }
        Some("objects") => Request::ListObjects,
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
        Response::InspectResult { class_name, ghidra_addr, runtime_addr, fields } => {
            println!(
                "{} at ghidra:0x{:08X} (runtime:0x{:08X})\n",
                class_name, ghidra_addr, runtime_addr
            );
            for f in fields {
                println!(
                    "  +0x{:04X}  {:<20} [{:>2}]  {}",
                    f.offset, f.name, f.size, f.display
                );
            }
        }
        Response::ObjectList { objects } => {
            if objects.is_empty() {
                println!("No tracked objects");
            } else {
                println!("Tracked objects ({}):\n", objects.len());
                for obj in objects {
                    println!(
                        "  {:<20} runtime:0x{:08X}  size:0x{:X}  fields:{}",
                        obj.class_name, obj.runtime_addr, obj.size, obj.field_count
                    );
                }
            }
        }
        Response::AliasResult(alias) => {
            println!("{} at runtime:0x{:08X}", alias.class_name, alias.runtime_addr);
        }
        Response::FieldResult(field) => {
            println!("offset:0x{:X} size:{}", field.offset, field.size);
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

/// Split a string on '+' or '[' into (base, offset_str). Returns (s, None) if no compound.
fn split_compound(s: &str) -> (&str, Option<&str>) {
    if let Some(pos) = s.find('+') {
        let base = &s[..pos];
        let off = &s[pos + 1..];
        (base, Some(off))
    } else if let Some(pos) = s.find('[') {
        let base = &s[..pos];
        let off_str = s[pos + 1..].trim_end_matches(']');
        (base, Some(off_str))
    } else {
        (s, None)
    }
}

/// Resolve an offset string — could be numeric or a field name.
fn resolve_offset(s: &str, class: &Option<String>, port: u16) -> u32 {
    if looks_like_hex(s) {
        return parse_u32(s);
    }

    // Try as field name
    if let Some(cls) = class {
        match send_request(port, &Request::ResolveField {
            class_name: cls.clone(),
            field_name: s.to_string(),
        }) {
            Ok(Response::FieldResult(f)) => return f.offset,
            _ => {}
        }
    }

    // Fall back to numeric parse (will exit on error)
    parse_u32(s)
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

/// A resolved chain segment — either a normal "deref then offset" step,
/// or a field-name step where the offset should be applied BEFORE the deref.
enum ChainSegment {
    /// Normal hex offset: protocol uses it directly (deref current, add offset).
    HexOffset(u32),
    /// Field offset: needs to be folded into the previous address/step
    /// (add offset to current, THEN deref).
    FieldOffset(u32),
}

/// Resolve a symbolic address expression via the debug server.
///
/// The underlying chain protocol semantics: each step is "deref current addr,
/// then add offset." For hex chains like `0x7A0884->0xA0->0x2C`, this works
/// directly — the offset is added after the deref.
///
/// For field-name chains like `ddgame->task_land`, the user intent is "go to
/// the task_land field (offset 0x54C) and follow that pointer." We translate
/// this by folding the field offset into the PREVIOUS address/step:
///
///   `ddgame->task_land`  → base=ddgame+0x54C, chain=[0]
///   `wrapper->ddgame->rng_state` → base=wrapper+ddgame_off, chain=[rng_state_off]
fn resolve_address_expr(s: &str, port: u16) -> AddressExpr {
    let (s, absolute) = if let Some(rest) = s.strip_prefix("abs:") {
        (rest, true)
    } else {
        (s.as_ref(), false)
    };

    // Split on "->" for chain syntax
    let parts: Vec<&str> = if s.contains("->") {
        s.split("->").collect()
    } else {
        vec![s]
    };

    // Resolve the base (first part)
    let base_part = parts[0];
    let (base_addr, base_absolute, mut current_class) = resolve_base(base_part, absolute, port);

    if parts.len() == 1 {
        return AddressExpr::Simple { addr: base_addr, absolute: base_absolute };
    }

    // Resolve each chain segment, tracking whether it's a field name or hex.
    let mut segments: Vec<ChainSegment> = Vec::new();
    for part in &parts[1..] {
        segments.push(resolve_chain_segment_typed(part, &current_class, port));
        current_class = None;
    }

    // Convert to protocol chain format.
    // HexOffset segments pass through directly (deref then add offset).
    // FieldOffset segments get folded into the previous position (add offset,
    // then deref via a [0] chain entry).
    let mut final_base = base_addr;
    let mut chain: Vec<u32> = Vec::new();

    for seg in &segments {
        match seg {
            ChainSegment::HexOffset(offset) => {
                chain.push(*offset);
            }
            ChainSegment::FieldOffset(offset) => {
                // Fold into previous: add to base or previous chain entry
                if chain.is_empty() {
                    final_base = final_base.wrapping_add(*offset);
                } else {
                    let last = chain.last_mut().unwrap();
                    *last = last.wrapping_add(*offset);
                }
                chain.push(0); // deref with no extra offset
            }
        }
    }

    AddressExpr::Chain { addr: final_base, chain, absolute: base_absolute }
}

/// Resolve the base address part — could be a hex literal, a named alias, or alias+offset.
fn resolve_base(s: &str, absolute: bool, port: u16) -> (u32, bool, Option<String>) {
    let (base_str, offset_str) = split_compound(s);

    // If it looks like hex (starts with digit or 0x), parse directly
    if looks_like_hex(base_str) {
        let extra = offset_str.map(|o| parse_u32(o)).unwrap_or(0);
        return (parse_hex(base_str).wrapping_add(extra), absolute, None);
    }

    // Try resolving as a named alias
    match send_request(port, &Request::ResolveAlias { name: base_str.to_string() }) {
        Ok(Response::AliasResult(alias)) => {
            // Resolved! Use runtime address (absolute)
            let class = Some(alias.class_name);
            let extra = offset_str.map(|o| resolve_offset(o, &class, port)).unwrap_or(0);
            (alias.runtime_addr.wrapping_add(extra), true, class)
        }
        _ => {
            // Fall back to hex parse (will error if not valid hex)
            let extra = offset_str.map(|o| parse_u32(o)).unwrap_or(0);
            (parse_hex(base_str).wrapping_add(extra), absolute, None)
        }
    }
}

/// Resolve a chain segment, returning whether it was a field name or hex offset.
fn resolve_chain_segment_typed(s: &str, current_class: &Option<String>, port: u16) -> ChainSegment {
    let (base_str, offset_str) = split_compound(s);
    let extra = offset_str.map(|o| resolve_offset(o, current_class, port)).unwrap_or(0);

    // If it looks like hex, parse directly — standard chain semantics
    if looks_like_hex(base_str) {
        return ChainSegment::HexOffset(parse_u32(base_str).wrapping_add(extra));
    }

    // Try resolving as a field name if we know the current class
    if let Some(class) = current_class {
        match send_request(port, &Request::ResolveField {
            class_name: class.clone(),
            field_name: base_str.to_string(),
        }) {
            Ok(Response::FieldResult(f)) => {
                return ChainSegment::FieldOffset(f.offset.wrapping_add(extra));
            }
            _ => {}
        }
    }

    // Fall back to numeric parse — treat as hex semantics
    ChainSegment::HexOffset(parse_u32(base_str).wrapping_add(extra))
}

/// Check if a string looks like a hex number (starts with 0x or a digit).
fn looks_like_hex(s: &str) -> bool {
    s.starts_with("0x") || s.starts_with("0X") || s.chars().next().map_or(false, |c| c.is_ascii_digit())
}

fn print_usage() {
    eprintln!("Usage: openwa-debug <command> [args...] [--port N] [--format hex|raw]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  ping                      Check if server is running");
    eprintln!("  help                      List server commands");
    eprintln!("  read <addr> [len]         Read memory at address (default len=256)");
    eprintln!("  inspect <class> <addr>    Typed struct inspection (named fields)");
    eprintln!("  objects                   List tracked live objects");
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
    eprintln!("  ddgame                    Named alias (resolved via server)");
    eprintln!("  ddgame->rng_state         Field name chain (deref + field lookup)");
    eprintln!("  ddgame+rng_state          Named offset (no deref, just offset)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --port <N>                Server port (default: 19840)");
    eprintln!("  --format hex|raw          Output format (default: hex)");
}
