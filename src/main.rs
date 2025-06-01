use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::{Nvml, error::NvmlError};

use sysinfo::{Pid, ProcessesToUpdate, System};

use std::{
    collections::HashMap,
    io::{Write, stdout},
    thread,
    time::Duration,
};

fn main() -> Result<(), NvmlError> {
    let nvml = Nvml::init()?;
    let device = nvml.device_by_index(0)?;
    let mut sys = System::new();

    loop {
        // Clear screen and move cursor to top
        print!("\x1B[2J\x1B[H");
        stdout().flush().unwrap();

        sys.refresh_processes(ProcessesToUpdate::All, true);

        let mut processes = HashMap::new();

        // Collect compute processes
        for proc in device.running_compute_processes()? {
            processes.insert(proc.pid, proc.used_gpu_memory);
        }

        // Collect compute + graphics processes

        for proc in device.running_graphics_processes()? {
            processes.entry(proc.pid).or_insert(proc.used_gpu_memory);
        }
        // Get overall GPU utilization
        let utilization = device.utilization_rates()?;
        println!("Overall GPU utilization: {}%", utilization.gpu);
        println!("---------------------------\n");

        let mem_info = device.memory_info()?;
        let total_mib = mem_info.total / 1024 / 1024;
        let used_mib = mem_info.used / 1024 / 1024;
        let free_mib = mem_info.free / 1024 / 1024;

        println!(
            "GPU Memory Usage: {} MiB used / {} MiB total ({} MiB free)",
            used_mib, total_mib, free_mib
        );
        println!("---------------------------\n");

        println!("Processes using GPU memory:");
        println!("---------------------------\n");
        println!("{:<8} {:<24} {:<16}", "PID", "NAME", "GPU Memory (MiB)");

        if processes.is_empty() {
            println!("(No active GPU processes)");
        } else {
            // Initialize sysinfo once per loop

            // sys.refresh_processes(ProcessesToUpdate::All, true);

            // Convert to Vec and sort
            let mut sorted: Vec<_> = processes
                .into_iter()
                .map(|(pid, mem)| {
                    let name = get_process_name(&sys, pid);
                    (pid, name, mem)
                })
                .collect();

            // Sort alphabetically by process name (case-insensitive)
            sorted.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

            for (pid, name, mem) in sorted {
                let mem_mib = match mem {
                    UsedGpuMemory::Used(bytes) => bytes / 1024 / 1024,
                    UsedGpuMemory::Unavailable => 0,
                };
                println!("{:<8} {:<24} {:<16}", pid, name, mem_mib);
            }
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn get_process_name(sys: &System, pid: u32) -> String {
    let pid = Pid::from_u32(pid);
    sys.process(pid)
        .map(|p| p.name().to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
