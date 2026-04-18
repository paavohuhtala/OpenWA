//! Typed messages for CTaskTeam.
//!
//! Replaces the raw `(msg_type: u32, size: u32, data: *const u8)` interface
//! for messages where both sender and receiver are implemented in Rust.
//! Messages not yet ported remain as raw vtable dispatches.

use crate::game::TaskMessage;

/// Typed messages handled by CTaskTeam in Rust.
///
/// Each variant corresponds to a specific `TaskMessage` value and carries
/// its payload as typed fields instead of a raw byte buffer.
#[derive(Debug, Clone, Copy)]
pub enum TeamMessage {
    /// 0x2B (TaskMessage::Surrender): Sent by the Surrender weapon (subtype 13).
    /// Sets a per-team flag and optionally broadcasts DetonateWeapon.
    /// Handled by CTaskTurnGame::HandleMessage which also triggers end-turn logic.
    Surrender {
        /// Team index (1-based) identifying which team fired.
        team_index: u32,
    },
    // Future variants:
    // WeaponReleased { team_index: u32, worm_index: u32, ... },
}

impl TeamMessage {
    /// Parse a raw message into a typed TeamMessage, if recognized.
    ///
    /// Returns `None` for unrecognized message types (handled by original WA code).
    ///
    /// # Safety
    /// `data` must be valid for `size` bytes when `size > 0`.
    pub unsafe fn from_raw(msg_type: u32, _size: u32, data: *const u8) -> Option<Self> {
        unsafe {
            match TaskMessage::try_from(msg_type) {
                Ok(TaskMessage::Surrender) => {
                    if data.is_null() {
                        return None;
                    }
                    let team_index = *(data as *const u32);
                    Some(TeamMessage::Surrender { team_index })
                }
                _ => None,
            }
        }
    }

    /// Serialize this message to a raw buffer for broadcast_message interop.
    ///
    /// Writes the payload into `buf` and returns `(msg_type, size)`.
    pub fn to_raw(&self, buf: &mut [u8]) -> (u32, u32) {
        match self {
            TeamMessage::Surrender { team_index } => {
                buf[0..4].copy_from_slice(&team_index.to_ne_bytes());
                (TaskMessage::Surrender as u32, 4)
            }
        }
    }
}
