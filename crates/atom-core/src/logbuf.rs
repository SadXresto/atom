use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;

/// Bounded in-memory log shared by the API, CLI and desktop shell.
#[derive(Debug, Serialize, Clone)]
pub struct LogLine {
    pub ts: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

pub struct LogBuffer {
    cap: usize,
    lines: Mutex<VecDeque<LogLine>>,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        LogBuffer { cap, lines: Mutex::new(VecDeque::with_capacity(cap)) }
    }

    pub fn push(&self, level: &str, target: &str, message: impl Into<String>) {
        let line = LogLine {
            ts: chrono::Local::now().to_rfc3339(),
            level: level.to_string(),
            target: target.to_string(),
            message: message.into(),
        };
        if let Ok(mut guard) = self.lines.lock() {
            if guard.len() == self.cap {
                guard.pop_front();
            }
            guard.push_back(line);
        }
    }

    pub fn tail(&self, limit: usize) -> Vec<LogLine> {
        let guard = match self.lines.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let start = guard.len().saturating_sub(limit);
        guard.iter().skip(start).cloned().collect()
    }
}
