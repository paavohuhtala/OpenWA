//! Dual-run side-by-side comparator for render queue producers.
//!
//! Lets a port-in-flight run BOTH the WA original and the Rust port in the
//! same frame and diff the queue commands they emit. The queue's arena
//! design — downward-growing buffer, upward-growing entry list, both reset
//! by integer writes only — makes restoration between the two halves
//! trivial: snapshot two scalars before run A, restore them before run B.
//!
//! Because the two runs happen in the same process at the same instant
//! against the same heap state, pointers that would diverge across whole-
//! frame captures (entity / world / palette / sprite ptrs) are stable
//! across the dual-run. Aux-arena pointers (vertex arrays, textbox
//! bitmaps allocated via `alloc_aux`) are also stable when both impls
//! push the same byte sequence, since both start from the same restored
//! `buffer_offset`.
//!
//! v0 snapshot intentionally only restores the queue's two integer
//! scalars. Add globals (LCG state, stipple parity, …) here as ports
//! surface real divergence from them.
//!
//! Use during a port:
//!
//! ```ignore
//! dual_run("MineEntity::Render", world, || wa_call(this), || rust_port(this));
//! ```

use crate::engine::world::GameWorld;
use crate::render::capture::{CapturedCommand, decode_one_command, format_command};
use crate::render::queue::RenderQueue;

/// Snapshot of the queue scalars needed to undo every `alloc`/`alloc_aux`
/// performed since the snapshot was taken.
#[derive(Debug, Clone, Copy)]
struct QueueSnapshot {
    entry_count: u32,
    buffer_offset: i32,
}

impl QueueSnapshot {
    unsafe fn take(rq: *const RenderQueue) -> Self {
        unsafe {
            Self {
                entry_count: (*rq).entry_count,
                buffer_offset: (*rq).buffer_offset,
            }
        }
    }

    unsafe fn restore(self, rq: *mut RenderQueue) {
        unsafe {
            (*rq).entry_count = self.entry_count;
            (*rq).buffer_offset = self.buffer_offset;
        }
    }
}

/// Decode every command appended since `before` was taken, in append order.
unsafe fn delta_commands(before: QueueSnapshot, rq: *const RenderQueue) -> Vec<CapturedCommand> {
    unsafe {
        let now = (*rq).entry_count;
        if now <= before.entry_count {
            return Vec::new();
        }
        let len = (now - before.entry_count) as usize;
        let mut out = Vec::with_capacity(len);
        for i in before.entry_count..now {
            let ptr = (*rq).entry_ptrs[i as usize];
            out.push(decode_one_command(ptr));
        }
        out
    }
}

/// Run `wa` and `rust` back-to-back against the same `world.render_queue`,
/// restoring the queue between them so each closure observes an identical
/// pre-state. Logs a per-command diff to `OpenWA.log` when the two
/// closures' emitted command lists differ.
///
/// # Safety
///
/// `world` must point to a live [`GameWorld`] with a valid `render_queue`.
/// Both closures must be safe to call sequentially in the current frame.
/// The dual-run does NOT snapshot/restore RNG, stipple parity, or any
/// other side-effect state — pass closures whose only externally
/// observable effect is appending to the render queue. (Side-effecting
/// closures will silently desync from one run to the next.)
pub unsafe fn dual_run<F1, F2>(label: &str, world: *mut GameWorld, wa: F1, rust: F2)
where
    F1: FnOnce(),
    F2: FnOnce(),
{
    unsafe {
        let rq = (*world).render_queue;
        let snap = QueueSnapshot::take(rq);
        wa();
        let wa_cmds = delta_commands(snap, rq);
        snap.restore(rq);
        rust();
        let rust_cmds = delta_commands(snap, rq);
        log_diff(label, &wa_cmds, &rust_cmds);
    }
}

fn log_diff(label: &str, wa: &[CapturedCommand], rust: &[CapturedCommand]) {
    if wa == rust {
        let _ = openwa_core::log::log_line(&format!("[DualRun:{label}] OK ({} cmds)", wa.len()));
        return;
    }

    let _ = openwa_core::log::log_line(&format!(
        "[DualRun:{label}] DIFF (wa={} rust={})",
        wa.len(),
        rust.len()
    ));
    let n = wa.len().max(rust.len());
    for i in 0..n {
        match (wa.get(i), rust.get(i)) {
            (Some(a), Some(b)) if a == b => {
                let _ =
                    openwa_core::log::log_line(&format!("  [{i:>2}] OK   {}", format_command(a)));
            }
            (Some(a), Some(b)) => {
                let _ = openwa_core::log::log_line(&format!(
                    "  [{i:>2}] DIFF wa:   {}",
                    format_command(a)
                ));
                let _ =
                    openwa_core::log::log_line(&format!("       DIFF rust: {}", format_command(b)));
            }
            (Some(a), None) => {
                let _ = openwa_core::log::log_line(&format!(
                    "  [{i:>2}] WA-only   {}",
                    format_command(a)
                ));
            }
            (None, Some(b)) => {
                let _ = openwa_core::log::log_line(&format!(
                    "  [{i:>2}] Rust-only {}",
                    format_command(b)
                ));
            }
            (None, None) => unreachable!(),
        }
    }
}
