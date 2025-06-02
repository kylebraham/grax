use nvml_wrapper::Device;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::{Nvml, error::NvmlError};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{Clear, ClearType},
};

use std::{
    collections::HashMap,
    io::{Write, stdout},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

fn main() -> Result<(), NvmlError> {
    let nvml = Nvml::init()?;
    let device = nvml.device_by_index(0)?;
    let mut sys = System::new();
    let mut stdout = stdout();

    // Hide cursor and clear screen at start
    execute!(stdout, Hide, Clear(ClearType::All)).unwrap();

    // Setup ctrl-c handler with shared AtomicBool
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    while running.load(Ordering::SeqCst) {
        sys.refresh_processes(ProcessesToUpdate::All, true);

        if let Ok(buffer) = get_metrics(&device, &sys) {
            execute!(stdout, MoveTo(0, 0), Clear(ClearType::All)).unwrap();
            write!(stdout, "{}", buffer).unwrap();
            stdout.flush().unwrap();
        }

        // Clear screen and write buffer

        thread::sleep(Duration::from_secs(1));
    }

    // On exit, restore cursor visibility
    execute!(stdout, Show).unwrap();

    Ok(())
}
fn get_metrics(device: &Device, sys: &System) -> Result<String, NvmlError> {
    // Framebuffer string
    let mut buffer = String::new();

    let mut processes = HashMap::new();

    for proc in device.running_compute_processes()? {
        processes.insert(proc.pid, proc.used_gpu_memory);
    }

    for proc in device.running_graphics_processes()? {
        processes.entry(proc.pid).or_insert(proc.used_gpu_memory);
    }

    let utilization = device.utilization_rates()?;
    let mem_info = device.memory_info()?;

    let total_mib = mem_info.total / 1024 / 1024;
    let used_mib = mem_info.used / 1024 / 1024;
    let free_mib = mem_info.free / 1024 / 1024;

    buffer.push_str(&format!("Overall GPU utilization: {}%\n", utilization.gpu));
    buffer.push_str("---------------------------\n\n");
    buffer.push_str(&format!(
        "GPU Memory Usage: {} MiB used / {} MiB total ({} MiB free)\n",
        used_mib, total_mib, free_mib
    ));
    buffer.push_str("---------------------------\n\n");
    buffer.push_str("Processes using GPU memory:\n");
    buffer.push_str("---------------------------\n\n");
    buffer.push_str(&format!(
        "{:<8} {:<24} {:<16}\n",
        "PID", "NAME", "GPU Memory (MiB)"
    ));

    if processes.is_empty() {
        buffer.push_str("(No active GPU processes)\n");
    } else {
        let mut sorted: Vec<_> = processes
            .into_iter()
            .map(|(pid, mem)| {
                let name = get_process_name(&sys, pid);
                (pid, name, mem)
            })
            .collect();

        sorted.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

        for (pid, name, mem) in sorted {
            let mem_mib = match mem {
                UsedGpuMemory::Used(bytes) => bytes / 1024 / 1024,
                UsedGpuMemory::Unavailable => 0,
            };
            buffer.push_str(&format!("{:<8} {:<24} {:<16}\n", pid, name, mem_mib));
        }
    }

    Ok(buffer)
}

fn get_process_name(sys: &System, pid: u32) -> String {
    let pid = Pid::from_u32(pid);
    sys.process(pid)
        .map(|p| p.name().to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
