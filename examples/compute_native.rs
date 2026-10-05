//! The compute probe on Vulkan (`vk::ComputeDevice`): runs the shared jobs
//! (`compute_probe/jobs.rs`) and prints one line each. Its browser half is
//! `compute_web`; `scripts/web-probe/compute` diffs the two.

#[path = "compute_probe/jobs.rs"]
mod jobs;

fn main() {
    let mut dev = match cce_ui::vk::ComputeDevice::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    for line in compute_jobs!(dev,) {
        println!("{line}");
    }
}
