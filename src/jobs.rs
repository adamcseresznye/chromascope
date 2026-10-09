//! Cooperative synchronous job control. Readers remain owned by the caller thread.
use crate::domain::{EngineError, Result};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
#[derive(Clone, Debug)]
pub struct JobControl {
    cancelled: Arc<AtomicBool>,
    completed: Arc<AtomicUsize>,
    exhausted: Arc<AtomicBool>,
    pub max_scans: usize,
    pub max_source_bytes: u64,
}
impl Default for JobControl {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            completed: Arc::new(AtomicUsize::new(0)),
            exhausted: Arc::new(AtomicBool::new(false)),
            max_scans: 2_000_000,
            max_source_bytes: 50_000_000_000,
        }
    }
}
impl JobControl {
    pub fn with_limits(max_scans: usize, max_source_bytes: u64) -> Result<Self> {
        if max_scans == 0 || max_source_bytes == 0 {
            return Err(EngineError::new(
                "invalid_parameters",
                "Resource limits must be positive",
            ));
        }
        Ok(Self {
            max_scans,
            max_source_bytes,
            ..Self::default()
        })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    pub fn completed_scans(&self) -> usize {
        self.completed.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(EngineError::new("cancelled", "Operation cancelled"));
        }
        if self.exhausted.load(Ordering::Relaxed) {
            return Err(EngineError::new("resource_limit", "Scan budget exhausted"));
        }
        Ok(())
    }
    pub(crate) fn step(&self) -> bool {
        if self.check().is_err() {
            return false;
        }
        let mut completed = self.completed.load(Ordering::Relaxed);
        loop {
            if completed >= self.max_scans {
                self.exhausted.store(true, Ordering::Relaxed);
                return false;
            }
            match self.completed.compare_exchange_weak(
                completed,
                completed + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => completed = actual,
            }
        }
    }
    pub(crate) fn reset_progress(&self) {
        self.completed.store(0, Ordering::Relaxed);
        self.exhausted.store(false, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}
pub struct JobHandle {
    pub id: crate::domain::JobId,
    pub control: JobControl,
    state: Arc<std::sync::Mutex<JobState>>,
    response: std::sync::mpsc::Receiver<Result<crate::engine::Response>>,
}
impl JobHandle {
    pub fn state(&self) -> JobState {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn cancel(&self) {
        self.control.cancel();
    }
    /// Nonblocking result reception for progress/paging transports.
    pub fn try_result(&self) -> Option<Result<crate::engine::Response>> {
        match self.response.try_recv() {
            Ok(value) => Some(value),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(EngineError::new(
                "adapter_failure",
                "Worker disconnected",
            ))),
        }
    }
    pub fn wait(self) -> Result<crate::engine::Response> {
        self.response
            .recv()
            .map_err(|e| EngineError::new("adapter_failure", e))?
    }
}
struct Work {
    path: std::path::PathBuf,
    request: crate::domain::Request,
    control: JobControl,
    state: Arc<std::sync::Mutex<JobState>>,
    tx: std::sync::mpsc::SyncSender<Result<crate::engine::Response>>,
}
fn recover_panic<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or_else(|_| {
        Err(EngineError::new(
            "adapter_failure",
            "Analytical worker panicked",
        ))
    })
}
/// Fixed reader/worker concurrency and a bounded admission queue. Drop closes admission.
pub struct Scheduler {
    tx: std::sync::mpsc::SyncSender<Work>,
}
impl Scheduler {
    pub fn new(workers: usize, queue_capacity: usize) -> Result<Self> {
        if !(1..=16).contains(&workers) || !(1..=256).contains(&queue_capacity) {
            return Err(EngineError::new(
                "invalid_parameters",
                "Use 1-16 workers and 1-256 queued jobs",
            ));
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<Work>(queue_capacity);
        let rx = Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..workers {
            let rx = rx.clone();
            std::thread::spawn(move || loop {
                let work = match rx.lock().unwrap_or_else(|e| e.into_inner()).recv() {
                    Ok(w) => w,
                    Err(_) => break,
                };
                *work.state.lock().unwrap_or_else(|e| e.into_inner()) = JobState::Running;
                let result = recover_panic(|| {
                    crate::engine::execute(&work.path, work.request, &work.control)
                });
                *work.state.lock().unwrap_or_else(|e| e.into_inner()) = match &result {
                    Ok(_) => JobState::Succeeded,
                    Err(e) if e.code == "cancelled" => JobState::Cancelled,
                    Err(_) => JobState::Failed,
                };
                let _ = work.tx.send(result);
            });
        }
        Ok(Self { tx })
    }
    pub fn submit(
        &self,
        path: std::path::PathBuf,
        request: crate::domain::Request,
        control: JobControl,
    ) -> Result<JobHandle> {
        let state = Arc::new(std::sync::Mutex::new(JobState::Queued));
        let (tx, response) = std::sync::mpsc::sync_channel(1);
        let work = Work {
            path,
            request,
            control: control.clone(),
            state: state.clone(),
            tx,
        };
        self.tx
            .try_send(work)
            .map_err(|e| EngineError::new("resource_limit", e))?;
        Ok(JobHandle {
            id: crate::domain::JobId::default(),
            control,
            state,
            response,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_becomes_structured_failure_and_later_work_can_run() {
        let result: Result<()> = recover_panic(|| panic!("authored worker failure"));
        assert_eq!(result.unwrap_err().code, "adapter_failure");
        assert!(recover_panic(|| Ok(())).is_ok());
        let failed: Result<()> = recover_panic(|| Err(EngineError::new("cancelled", "test")));
        assert_eq!(failed.unwrap_err().code, "cancelled");
    }
    #[test]
    fn shared_scan_budget_is_atomic_under_contention() {
        let control = JobControl::with_limits(1000, 1_000_000).unwrap();
        let admitted = std::thread::scope(|scope| {
            let handles = (0..16)
                .map(|_| {
                    let control = &control;
                    scope.spawn(move || {
                        let mut count = 0;
                        while control.step() {
                            count += 1;
                        }
                        count
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .sum::<usize>()
        });
        assert_eq!(admitted, 1000);
        assert_eq!(control.completed_scans(), 1000);
        assert_eq!(control.check().unwrap_err().code, "resource_limit");
    }
    #[test]
    fn cancellation_during_work_and_exact_scan_limit() {
        let control = JobControl::with_limits(2, 1_000_000).unwrap();
        assert!(control.step());
        assert!(control.step());
        assert!(control.check().is_ok());
        assert!(!control.step());
        assert_eq!(control.check().unwrap_err().code, "resource_limit");
        control.reset_progress();
        assert!(control.step());
        control.cancel();
        assert!(!control.step());
        assert_eq!(control.check().unwrap_err().code, "cancelled");
        assert_eq!(control.completed_scans(), 1);
    }
}
