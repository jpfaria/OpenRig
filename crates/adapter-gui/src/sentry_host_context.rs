//! Responsibility: samples the host machine as a crash-report context (#1070).

use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use sysinfo::{CpuRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();

/// Machine facts plus this process's RSS and CPU since the previous sample.
/// Never blocks: if another thread is sampling, the process stats are skipped.
pub fn host_context() -> Value {
    let system = SYSTEM.get_or_init(|| {
        Mutex::new(System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()),
        ))
    });
    let Ok(mut sys) = system.try_lock() else {
        return json!({ "sampling": "busy" });
    };
    sys.refresh_memory();
    let pid = sysinfo::get_current_pid().ok();
    if let Some(pid) = pid {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory().with_cpu(),
        );
    }
    let process = pid.and_then(|pid| sys.process(pid));
    json!({
        "cpu_brand": sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        "cpu_cores": sys.cpus().len(),
        "cpu_physical_cores": System::physical_core_count(),
        "memory_total_mb": sys.total_memory() / 1_048_576,
        "process_memory_mb": process.map(|p| p.memory() / 1_048_576).unwrap_or(0),
        "process_cpu_percent": process.map(|p| p.cpu_usage()),
    })
}
