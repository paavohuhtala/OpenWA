use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub struct LogEntry {
    pub ts: Instant,
    pub text: String,
}

static LOG_BUF: OnceLock<Mutex<VecDeque<LogEntry>>> = OnceLock::new();

fn buf() -> &'static Mutex<VecDeque<LogEntry>> {
    LOG_BUF.get_or_init(|| Mutex::new(VecDeque::with_capacity(500)))
}

/// Push a message to the debug log ring buffer (max 500 entries).
pub fn push(msg: impl Into<String>) {
    if let Ok(mut b) = buf().lock() {
        if b.len() >= 500 {
            b.pop_front();
        }
        b.push_back(LogEntry { ts: Instant::now(), text: msg.into() });
    }
}

/// Snapshot up to `max` of the most recent entries (cloned).
pub fn snapshot(max: usize) -> Vec<(Instant, String)> {
    let Ok(b) = buf().lock() else { return Vec::new() };
    let start = b.len().saturating_sub(max);
    b.range(start..).map(|e| (e.ts, e.text.clone())).collect()
}

/// Clear all log entries.
pub fn clear() {
    if let Ok(mut b) = buf().lock() {
        b.clear();
    }
}
