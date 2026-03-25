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

// --- Typed inspection types ---

/// A formatted field value from a struct inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValue {
    pub offset: u32,
    pub name: String,
    pub size: u32,
    /// Raw hex representation.
    pub hex: String,
    /// Human-readable formatted value.
    pub display: String,
}

/// A tracked live object in the DLL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveObjectInfo {
    pub runtime_addr: u32,
    pub ghidra_addr: u32,
    pub size: u32,
    pub class_name: String,
    pub field_count: u32,
}

/// Result of resolving a named alias (e.g., "ddgame") to an address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedAlias {
    pub runtime_addr: u32,
    pub class_name: String,
}

/// Result of resolving a field name to its offset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedField {
    pub offset: u32,
    pub size: u32,
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
    /// Typed struct inspection: read all named fields at an address.
    Inspect { class_name: String, addr: u32, chain: Vec<u32>, absolute: bool },
    /// List all tracked live objects.
    ListObjects,
    /// Resolve a named alias (e.g., "ddgame") to a runtime address.
    ResolveAlias { name: String },
    /// Resolve a field name to its offset within a struct.
    ResolveField { class_name: String, field_name: String },
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
    /// Typed struct inspection result.
    InspectResult {
        class_name: String,
        ghidra_addr: u32,
        runtime_addr: u32,
        fields: Vec<FieldValue>,
    },
    /// List of tracked live objects.
    ObjectList { objects: Vec<LiveObjectInfo> },
    /// Resolved named alias.
    AliasResult(ResolvedAlias),
    /// Resolved field offset.
    FieldResult(ResolvedField),
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
