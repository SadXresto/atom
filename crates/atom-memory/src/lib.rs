//! Real memory and CPU sampling. Every number here comes from the OS.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub ts: String,
    pub total_memory: u64,
    pub used_memory: u64,
    pub available_memory: u64,
    pub system_cpu_percent: f32,
    /// Resident set of the API/desktop process itself.
    pub process_rss: u64,
    pub process_cpu_percent: f32,
    /// Resident set of the spawned llama-server, when a model is loaded.
    pub model_process_pid: Option<u32>,
    pub model_process_rss: Option<u64>,
    pub load_average: [f64; 3],
}

pub struct Monitor {
    sys: System,
    history: VecDeque<Snapshot>,
    capacity: usize,
}

impl Monitor {
    pub fn new(capacity: usize) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Monitor { sys, history: VecDeque::with_capacity(capacity), capacity }
    }

    pub fn sample(&mut self, model_pid: Option<u32>) -> Snapshot {
        self.sys.refresh_memory();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        let pid = Pid::from_u32(std::process::id());
        let (process_rss, process_cpu_percent) = self
            .sys
            .process(pid)
            .map(|p| (p.memory(), p.cpu_usage()))
            .unwrap_or((0, 0.0));

        let (model_process_pid, model_process_rss) = match model_pid {
            Some(pid_u32) => {
                let p = self.sys.process(Pid::from_u32(pid_u32));
                (Some(pid_u32), p.map(|p| p.memory()))
            }
            None => (None, None),
        };

        let load = System::load_average();
        let snap = Snapshot {
            ts: chrono::Local::now().to_rfc3339(),
            total_memory: self.sys.total_memory(),
            used_memory: self.sys.used_memory(),
            available_memory: self.sys.available_memory(),
            system_cpu_percent: self.sys.global_cpu_usage(),
            process_rss,
            process_cpu_percent,
            model_process_pid,
            model_process_rss,
            load_average: [load.one, load.five, load.fifteen],
        };
        if self.history.len() == self.capacity {
            self.history.pop_front();
        }
        self.history.push_back(snap.clone());
        snap
    }

    pub fn latest(&self) -> Option<&Snapshot> {
        self.history.back()
    }

    pub fn history(&self) -> Vec<Snapshot> {
        self.history.iter().cloned().collect()
    }

    /// Drop buffered samples after a model is unloaded so the monitor does not
    /// keep attributing stale readings to a live runtime.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn cpu_count(&self) -> usize {
        self.sys.cpus().len()
    }

    pub fn total_memory(&self) -> u64 {
        self.sys.total_memory()
    }
}
