//! Background job bookkeeping for long-running compression and benchmark work.
//!
//! Jobs are the only place progress is reported, and progress is only ever set
//! from a real observation (a parsed tool log line, a completed benchmark case,
//! a validated output file). Nothing here fabricates movement.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Import,
    Quantize,
    Benchmark,
    Verification,
    Train,
    Export,
    Deploy,
    Distillation,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobLog {
    pub seq: u64,
    pub ts: String,
    pub stream: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobView {
    pub id: String,
    pub kind: JobKind,
    pub title: String,
    pub status: JobStatus,
    pub stage: String,
    pub progress: f64,
    pub params: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub log_count: u64,
}

#[derive(Debug)]
struct JobInner {
    view: JobView,
    logs: Vec<JobLog>,
    seq: u64,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    Progress { id: String, stage: String, progress: f64 },
    Log { id: String, log: JobLog },
    Status { id: String, status: JobStatus, error: Option<String> },
}

impl JobEvent {
    pub fn job_id(&self) -> &str {
        match self {
            JobEvent::Progress { id, .. } | JobEvent::Log { id, .. } | JobEvent::Status { id, .. } => id,
        }
    }
}

struct Shared {
    jobs: Mutex<HashMap<String, JobInner>>,
    bus: broadcast::Sender<JobEvent>,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, JobInner>> {
        match self.jobs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }
}

/// Cloneable handle to the shared job registry.
#[derive(Clone)]
pub struct JobStore {
    shared: Arc<Shared>,
}

impl Default for JobStore {
    fn default() -> Self {
        Self::new()
    }
}

impl JobStore {
    pub fn new() -> Self {
        let (bus, _) = broadcast::channel(1024);
        JobStore {
            shared: Arc::new(Shared { jobs: Mutex::new(HashMap::new()), bus }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.shared.bus.subscribe()
    }

    /// Create a queued job and return a handle for the worker task.
    pub fn create(&self, kind: JobKind, title: impl Into<String>, params: serde_json::Value) -> (String, JobHandle) {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let view = JobView {
            id: id.clone(),
            kind,
            title: title.into(),
            status: JobStatus::Queued,
            stage: "queued".into(),
            progress: 0.0,
            params,
            result: None,
            error: None,
            created_at: chrono::Local::now().to_rfc3339(),
            started_at: None,
            finished_at: None,
            log_count: 0,
        };
        self.shared.lock().insert(
            id.clone(),
            JobInner { view, logs: Vec::new(), seq: 0, cancel: cancel.clone() },
        );
        (id.clone(), JobHandle { id, shared: self.shared.clone(), cancel })
    }

    pub fn list(&self) -> Vec<JobView> {
        let mut jobs: Vec<JobView> = self.shared.lock().values().map(|j| j.view.clone()).collect();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs
    }

    pub fn get(&self, id: &str) -> Option<JobView> {
        self.shared.lock().get(id).map(|j| j.view.clone())
    }

    pub fn logs(&self, id: &str, after_seq: u64) -> Vec<JobLog> {
        self.shared
            .lock()
            .get(id)
            .map(|j| j.logs.iter().filter(|l| l.seq > after_seq).cloned().collect())
            .unwrap_or_default()
    }

    /// Ask a running job to stop. The worker observes this flag and cleans up
    /// its partial output; it is never reported as success.
    pub fn request_cancel(&self, id: &str) -> bool {
        if let Some(job) = self.shared.lock().get(id) {
            job.cancel.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn active_of_kind(&self, kind: JobKind) -> Option<JobView> {
        self.shared
            .lock()
            .values()
            .find(|j| j.view.kind == kind && matches!(j.view.status, JobStatus::Queued | JobStatus::Running))
            .map(|j| j.view.clone())
    }
}

/// Handle given to the worker. Writes both to the registry and the event bus.
#[derive(Clone)]
pub struct JobHandle {
    id: String,
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
}

impl JobHandle {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn cancelled_error(&self) -> crate::AtomError {
        crate::AtomError::Cancelled("cancelled by user".into())
    }

    fn mutate<F: FnOnce(&mut JobInner)>(&self, f: F) {
        if let Some(job) = self.shared.lock().get_mut(&self.id) {
            f(job);
        }
    }

    pub fn start(&self) {
        let now = chrono::Local::now().to_rfc3339();
        self.mutate(|j| {
            j.view.status = JobStatus::Running;
            j.view.stage = "starting".into();
            j.view.started_at = Some(now);
        });
        self.status(JobStatus::Running, None);
    }

    pub fn log(&self, stream: &str, message: impl Into<String>) {
        let message = message.into();
        let mut out = None;
        self.mutate(|j| {
            j.seq += 1;
            let log = JobLog {
                seq: j.seq,
                ts: chrono::Local::now().to_rfc3339(),
                stream: stream.to_string(),
                message,
            };
            j.logs.push(log.clone());
            j.view.log_count = j.logs.len() as u64;
            out = Some(log);
        });
        if let Some(log) = out {
            let _ = self.shared.bus.send(JobEvent::Log { id: self.id.clone(), log });
        }
    }

    pub fn progress(&self, stage: &str, fraction: f64) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.mutate(|j| {
            // Progress never moves backwards for a given job.
            if fraction >= j.view.progress {
                j.view.progress = fraction;
            }
            j.view.stage = stage.to_string();
        });
        let _ = self.shared.bus.send(JobEvent::Progress {
            id: self.id.clone(),
            stage: stage.to_string(),
            progress: fraction,
        });
    }

    pub fn status(&self, status: JobStatus, error: Option<String>) {
        let _ = self.shared.bus.send(JobEvent::Status { id: self.id.clone(), status, error });
    }

    pub fn finish_ok(&self, result: serde_json::Value) {
        self.mutate(|j| {
            j.view.status = JobStatus::Succeeded;
            j.view.progress = 1.0;
            j.view.stage = "done".into();
            j.view.result = Some(result);
            j.view.finished_at = Some(chrono::Local::now().to_rfc3339());
        });
        self.status(JobStatus::Succeeded, None);
    }

    pub fn finish_err(&self, message: impl Into<String>) {
        let message = message.into();
        self.mutate(|j| {
            j.view.status = JobStatus::Failed;
            j.view.stage = "failed".into();
            j.view.error = Some(message.clone());
            j.view.finished_at = Some(chrono::Local::now().to_rfc3339());
        });
        self.status(JobStatus::Failed, Some(message));
    }

    pub fn finish_cancelled(&self) {
        self.mutate(|j| {
            j.view.status = JobStatus::Cancelled;
            j.view.stage = "cancelled".into();
            j.view.error = Some("cancelled by user".into());
            j.view.finished_at = Some(chrono::Local::now().to_rfc3339());
        });
        self.status(JobStatus::Cancelled, Some("cancelled by user".into()));
    }
}
