use clap::{Parser, Subcommand};
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

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Get gpu metric
    Query {
        /// Watch for GPU metric changes.
        #[arg(short, long)]
        watch: bool,
    },
    /// Displau GPU info
    Info,
}

fn main() -> Result<(), NvmlError> {
    let cli = Cli::parse();
    let mut stdout = stdout();

    let nvml = Nvml::init()?;
    let device = nvml.device_by_index(0)?;
    let mut sys = System::new();

    match &cli.command {
        Some(Commands::Query { watch }) => {
            if *watch {
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
                        // Clear screen and write buffer
                        execute!(stdout, MoveTo(0, 0), Clear(ClearType::All)).unwrap();
                        write!(stdout, "{}", buffer).unwrap();
                        stdout.flush().unwrap();
                    }
                    thread::sleep(Duration::from_secs(1));
                }
                // On exit, restore cursor visibility
                execute!(stdout, Show).unwrap();
            } else {
                if let Ok(buffer) = get_metrics(&device, &sys) {
                    write!(stdout, "{}", buffer).unwrap();
                    stdout.flush().unwrap();
                }
            }
        }
        Some(Commands::Info) => {
            // println!("{:<15}: {:?}", "Brand", device.brand()?);

            // println!("{:<15}: {}", "Name", device.name()?);
            // println!(
            //     "{:<15}:  {} (watts) ",
            //     "Power Limit",
            //     (device.enforced_power_limit()? / 1000)
            // );

            // let (total_mem, _, _) = get_gpu_memory_utilization(&device)?;
            // println!("{:<15}:  {} (MiB)", "Total GPU Memory", total_mem);
            println!("{:<16}: {:?}", "Brand", device.brand()?);
            println!("{:<16}: {}", "Name", device.name()?);
            println!(
                "{:<16}: {} (watts)",
                "Power Limit",
                device.enforced_power_limit()? / 1000
            );

            let (total_mem, _, _) = get_gpu_memory_utilization(&device)?;
            println!("{:<16}: {} (MiB)", "Total GPU Memory", total_mem);
        }
        None => {}
    }

    Ok(())
}

fn get_metrics(device: &Device, sys: &System) -> Result<String, NvmlError> {
    // Framebuffer string
    let mut buffer = String::new();
    let utilization = device.utilization_rates()?;
    let (total_mib, used_mib, free_mib) = get_gpu_memory_utilization(&device)?;

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

    get_gpu_processes(device, sys)?
        .iter()
        .for_each(|p| buffer.push_str(p));

    Ok(buffer)
}

fn get_gpu_memory_utilization(device: &Device) -> Result<(u64, u64, u64), NvmlError> {
    let mem_info = device.memory_info()?;

    let total_mib = mem_info.total / 1024 / 1024;
    let used_mib = mem_info.used / 1024 / 1024;
    let free_mib = mem_info.free / 1024 / 1024;

    Ok((total_mib, used_mib, free_mib))
}

fn get_process_name(sys: &System, pid: u32) -> String {
    let pid = Pid::from_u32(pid);
    sys.process(pid)
        .map(|p| p.name().to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn get_gpu_processes(device: &Device, sys: &System) -> Result<Vec<String>, NvmlError> {
    let mut processes = HashMap::new();
    let mut sorted_processes = Vec::new();
    for proc in device.running_compute_processes()? {
        processes.insert(proc.pid, proc.used_gpu_memory);
    }

    for proc in device.running_graphics_processes()? {
        processes.entry(proc.pid).or_insert(proc.used_gpu_memory);
    }

    if !processes.is_empty() {
        sorted_processes = processes
            .into_iter()
            .map(|(pid, mem)| {
                let name = get_process_name(&sys, pid);
                let mem = match mem {
                    UsedGpuMemory::Used(bytes) => bytes / 1024 / 1024,
                    UsedGpuMemory::Unavailable => 0,
                };
                format!("{:<8} {:<24} {:<16}\n", pid, name, mem)
            })
            .collect();
        sorted_processes.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    }

    Ok(sorted_processes)
}
