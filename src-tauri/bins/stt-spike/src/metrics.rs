use std::time::{Duration, Instant};

use anyhow::Result;
use nvml_wrapper::Nvml;

pub struct VramSample {
    pub used_mb: u64,
    pub total_mb: u64,
}

pub fn sample_vram() -> Result<VramSample> {
    let nvml = Nvml::init()?;
    let device = nvml.device_by_index(0)?;
    let mem = device.memory_info()?;
    Ok(VramSample {
        used_mb: mem.used / 1024 / 1024,
        total_mb: mem.total / 1024 / 1024,
    })
}

#[derive(Debug, Clone)]
pub struct Timing {
    pub label: &'static str,
    pub duration: Duration,
}

pub fn time<F, T>(label: &'static str, f: F) -> (T, Timing)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let out = f();
    let duration = start.elapsed();
    (out, Timing { label, duration })
}

pub fn print_timings(ts: &[Timing]) {
    println!("\n=== Timings ===");
    for t in ts {
        println!("  {:28} {:>10} ms", t.label, t.duration.as_millis());
    }
}
