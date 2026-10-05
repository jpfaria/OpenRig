//! Responsibility: samples the host machine as a crash-report context.

use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use sysinfo::{CpuRefreshKind, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();

struct Machine {
    cpu_brand: String,
    cpu_cores: usize,
    cpu_physical_cores: Option<usize>,
    memory_total_mb: u64,
}

static MACHINE: OnceLock<Machine> = OnceLock::new();

fn machine() -> &'static Machine {
    MACHINE.get_or_init(|| {
        let mut sys =
            System::new_with_specifics(RefreshKind::nothing().with_cpu(CpuRefreshKind::nothing()));
        sys.refresh_memory();
        Machine {
            cpu_brand: sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default(),
            cpu_cores: sys.cpus().len(),
            cpu_physical_cores: System::physical_core_count(),
            memory_total_mb: sys.total_memory() / 1_048_576,
        }
    })
}

/// Machine facts plus this process's RSS and CPU since the previous sample.
/// Never blocks: if another thread is sampling, the process stats are skipped.
pub fn host_context() -> Value {
    let machine = machine();
    let mut context = json!({
        "cpu_brand": machine.cpu_brand,
        "cpu_cores": machine.cpu_cores,
        "cpu_physical_cores": machine.cpu_physical_cores,
        "memory_total_mb": machine.memory_total_mb,
    });
    let system = SYSTEM.get_or_init(|| Mutex::new(System::new()));
    let Ok(mut sys) = system.try_lock() else {
        context["process_sampling"] = json!("busy");
        return context;
    };
    let Ok(pid) = sysinfo::get_current_pid() else {
        return context;
    };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory().with_cpu(),
    );
    if let Some(process) = sys.process(pid) {
        context["process_memory_mb"] = json!(process.memory() / 1_048_576);
        context["process_cpu_percent"] = json!(process.cpu_usage());
    }
    context
}
