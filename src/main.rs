use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::mem::MaybeUninit;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::Foundation::FILETIME;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};


fn filetime_to_duration(ft: FILETIME) -> Duration {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    Duration::from_nanos(ticks * 100)
}
/// Læs aktuelt RAM-forbrug (Working Set) i bytes
unsafe fn get_memory_counters() -> PROCESS_MEMORY_COUNTERS_EX {
    let mut pmc = MaybeUninit::<PROCESS_MEMORY_COUNTERS_EX>::zeroed();
    let handle = GetCurrentProcess();
    // Henter WorkingSetSize, PeakWorkingSetSize, PrivateUsage, PagefileUsage mm.
    GetProcessMemoryInfo(
        handle,
        pmc.as_mut_ptr() as *mut _,
        std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
    )
        .ok()
        .expect("Kunne ikke hente hukommelsesinfo");
    pmc.assume_init()
}

fn get_cpu_time() -> Duration {
    unsafe {
        let mut creation = MaybeUninit::uninit();
        let mut exit = MaybeUninit::uninit();
        let mut kernel = MaybeUninit::uninit();
        let mut user = MaybeUninit::uninit();

        let handle = GetCurrentProcess();
        let result = GetProcessTimes(
            handle,
            creation.as_mut_ptr(),
            exit.as_mut_ptr(),
            kernel.as_mut_ptr(),
            user.as_mut_ptr(),
        );

        if result.is_ok() {
            let kernel = filetime_to_duration(kernel.assume_init());
            let user   = filetime_to_duration(user.assume_init());
            kernel + user
        } else {
            Duration::ZERO
        }
    }
}
/// Læs billede, konverter til gråtoner, og gem i output-mappe
fn process_image(path: &Path, output_dir: &Path) {
    let img = image::open(path).expect("Kunne ikke åbne billede");
    let mut gray = img.grayscale();

    // Gør billedbehandlingen tungere med fx blur og kontrast mange gange
    for _ in 0..5 {
        gray = gray.blur(1.0);         // Sløret billede
    }

    let filename = path.file_name().unwrap();
    let output_path = output_dir.join(filename);
    gray.save_with_format(output_path, image::ImageFormat::Jpeg)
        .expect("Kunne ikke gemme billede");
}

/// Læser alle .jpg filer i input-mappen
fn get_image_paths() -> Vec<std::path::PathBuf> {
    let input_dir = Path::new("images/input");
    fs::read_dir(input_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|ext| ext == "jpeg").unwrap_or(false))
        .collect()
}
async fn run_async(image_paths: Vec<std::path::PathBuf>) {
    let total = image_paths.len();
    let mid = total / 2;
    let counter = Arc::new(AtomicUsize::new(0));
    let output_dir = Arc::new(PathBuf::from("images/output"));

    let mut handles = vec![];

    for path in image_paths {
        let output_dir = Arc::clone(&output_dir);
        let counter = Arc::clone(&counter);
        let path = path.clone();

        let handle = tokio::spawn(async move {
            // Kald tung CPU-funktion direkte (ideelt set burde denne være offloaded via tokio::task::spawn_blocking)
            process_image(&path, &output_dir);
            let done = counter.fetch_add(1, Ordering::SeqCst) + 1;

            // RAM-målinger ved 10, midtvejs og tæt på slut
            if done == 10 || done == mid || done == total - 10 {
                let mem = unsafe { get_memory_counters() };
                let phase = match done {
                    10 => "Start",
                    x if x == mid => "Midtvejs",
                    x if x == total - 10 => "Slutning",
                    _ => unreachable!(),
                };
                println!("-- Måling ({}) efter {} billeder --", phase, done);
                println!("WorkingSetSize: {} MB", mem.WorkingSetSize / 1_048_576);
                println!("PrivateUsage:   {} MB", mem.PrivateUsage   / 1_048_576);
                println!("PeakPagefile:   {} MB", mem.PagefileUsage  / 1_048_576);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
}


#[tokio::main(flavor = "current_thread")]
async fn main() {
    let logical_cores = 1;

    let wall_start = Instant::now();
    let cpu_start = get_cpu_time();

    run_async(get_image_paths()).await;

    //CPU
    let wall_elapsed = wall_start.elapsed();
    let cpu_end = get_cpu_time();
    let cpu_used = cpu_end - cpu_start;

    let cpu_percent_one_core = (cpu_used.as_secs_f64() / wall_elapsed.as_secs_f64()) * 100.0;
    let cpu_percent_all_cores = cpu_percent_one_core / logical_cores as f64;

    //RAM
    let mem_end      = unsafe { get_memory_counters() };
    let peak_ws    = mem_end.PeakWorkingSetSize    / 1_048_576;

    println!("");
    println!("2) Async færdig på: {:.2?}", wall_elapsed);
    println!("");
    println!("=== CPU Forbrug ===");
    println!("CPU time:  {:.2?}", cpu_used);
    println!("CPU usage (1 kerne): {:.2}%", cpu_percent_one_core);
    println!("CPU usage ift. samlet kapacitet (alle kerner): {:.2}%", cpu_percent_all_cores);
    println!("");
    println!("=== RAM-Forbrug ===");
    println!("Spidsværdi: {} MB", peak_ws);
}

