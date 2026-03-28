//! CTaskFilter vtable hooks.
//!
//! Replaces CTaskFilter::HandleMessage (vtable slot 2).
//! Gate-keeps message propagation via a per-instance subscription table.

use openwa_core::address::va;
use openwa_core::log::log_line;
use openwa_core::task::filter::CTaskFilter;
use openwa_core::task::CTask;

/// CTaskFilter::HandleMessage replacement.
///
/// Messages with `msg_type < 98` are only forwarded if
/// `subscription_table[msg_type] != 0`. Messages >= 98 always pass through.
/// Forwarding calls `broadcast_message` which propagates down to child tasks.
unsafe extern "thiscall" fn filter_handle_message(
    this: *mut CTaskFilter,
    sender: *mut CTask,
    msg_type: u32,
    size: u32,
    data: *const u8,
) {
    // Check subscription table — messages >= 98 always pass through
    let table = &(*this).subscription_table;
    if (msg_type as usize) < table.len() && table[msg_type as usize] == 0 {
        return; // message not subscribed — drop silently
    }

    // Broadcast to children — raw-pointer version avoids noalias UB
    CTask::broadcast_message_raw(this as *mut CTask, sender, msg_type, size, data);
}

pub fn install() -> Result<(), String> {
    use openwa_core::vtable_replace;

    vtable_replace!(openwa_core::task::filter::CTaskFilterVTable, va::CTASK_FILTER_VTABLE, {
        handle_message => filter_handle_message,
    })?;

    let _ = log_line("[Filter] HandleMessage hooked via vtable_replace");
    Ok(())
}
