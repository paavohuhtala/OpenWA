use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

// --- Pointer classification ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerKind {
    Vtable,
    Code,
    Data,
    Object,
    Heap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerInfo {
    pub offset: u32,
    pub raw_value: u32,
    pub ghidra_value: u32,
    pub kind: PointerKind,
    pub detail: Option<String>,
}

// --- Protocol messages ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Ping,
    Help,
    Read { addr: u32, len: u32, #[serde(default)] absolute: bool },
    /// Walk a pointer chain: start at `addr`, then for each offset in `chain`,
    /// deref the current DWORD and add the offset. Read `len` bytes at the end.
    ReadChain { addr: u32, chain: Vec<u32>, len: u32, absolute: bool },
    /// Pause the game at the next frame boundary.
    Suspend,
    /// Resume the game.
    Resume,
    /// Advance `count` frames, then pause.
    Step { count: i32 },
    /// Query current frame number and pause state.
    Frame,
    /// Set a frame breakpoint (-1 to clear).
    Break { frame: i32 },
    /// Capture a canonicalized game state snapshot.
    Snapshot,
}

/// One step in a resolved pointer chain, for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStep {
    /// Address we read from
    pub deref_addr: u32,
    /// Value (DWORD) we read
    pub value: u32,
    /// Offset added after deref
    pub offset: u32,
    /// Resulting address (value + offset)
    pub result_addr: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandHelp {
    pub name: String,
    pub usage: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Pong,
    Help { commands: Vec<CommandHelp> },
    ReadResult {
        ghidra_addr: u32,
        runtime_addr: u32,
        data: Vec<u8>,
        pointers: Vec<PointerInfo>,
    },
    ReadChainResult {
        /// Each deref step in the chain (for trace output)
        steps: Vec<ChainStep>,
        /// Final address (Ghidra VA)
        ghidra_addr: u32,
        /// Final address (runtime)
        runtime_addr: u32,
        /// Memory at the final address
        data: Vec<u8>,
        /// Pointer annotations in the final data
        pointers: Vec<PointerInfo>,
    },
    /// Game is now suspended at this frame.
    Suspended { frame: i32 },
    /// Game resumed.
    Resumed,
    /// Current frame info.
    FrameInfo { frame: i32, paused: bool, breakpoint: i32 },
    /// Breakpoint set/cleared.
    BreakSet { frame: i32 },
    /// Game state snapshot.
    Snapshot { frame: i32, text: String },
    Error { message: String },
}

// --- Length-prefixed framing ---

pub const DEFAULT_PORT: u16 = 19840;
pub const MAX_READ_SIZE: u32 = 1024 * 1024; // 1 MB
pub const MAX_FRAME_SIZE: usize = 8 * 1024 * 1024; // 8 MB (read data + pointer metadata overhead)

/// Write a length-prefixed MessagePack frame.
pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, msg: &T) -> io::Result<()> {
    let payload =
        rmp_serde::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

/// Read a length-prefixed MessagePack frame.
pub fn read_frame<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    rmp_serde::from_slice(&payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
