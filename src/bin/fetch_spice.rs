//! Utility binary to download commonly used SPICE kernels into `data/spice/`.
//!
//! The download set is intentionally small to keep onboarding fast. Additional
//! kernels can be added by extending the catalog in `ephemeris::kernels`.

use std::fs::{self, File};
use std::io::copy;
use std::path::Path;

use reqwest::blocking::Client;
use solar_travel_calculator::ephemeris;
use solar_travel_calculator::ephemeris::kernels::{
    KERNEL_CATALOG, KernelDescriptor, LOCAL_SPICE_DIR,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_dir = Path::new(LOCAL_SPICE_DIR);
    fs::create_dir_all(target_dir)?;

    let client = Client::builder().build()?;

    for descriptor in KERNEL_CATALOG {
        let dest = descriptor.local_path();
        if dest.exists() {
            println!(
                "[skip] {} already present at {}",
                descriptor.filename,
                display_path(&dest)
            );
            continue;
        }

        println!("[download] {} -> {}", descriptor.url, display_path(&dest));

        match download_kernel(&client, descriptor, &dest) {
            Ok(()) => println!("[ok] {}", descriptor.filename),
            Err(err) => {
                let _ = fs::remove_file(&dest);
                eprintln!("[error] {}: {}", descriptor.filename, err);
            }
        }
    }

    match ephemeris::kernel_summaries() {
        Ok(summaries) => {
            println!("\nLocal kernel summaries:");
            for summary in summaries {
                println!(
                    "  - {:<13} [{} | {}] {}\n      └ {}",
                    summary.descriptor.filename,
                    summary.descriptor.kind.label(),
                    format_size(summary.file_size_bytes),
                    summary.descriptor.description,
                    display_path(&summary.path)
                );
            }
        }
        Err(err) => eprintln!("[warn] unable to summarize kernels: {err}"),
    }

    Ok(())
}

fn download_kernel(
    client: &Client,
    kernel: &KernelDescriptor,
    dest: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = client.get(kernel.url).send()?.error_for_status()?;
    let mut file = File::create(dest)?;
    copy(&mut response, &mut file)?;
    Ok(())
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0;
    while value >= 1024.0 && unit_idx < UNITS.len() - 1 {
        value /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{bytes} {}", UNITS[unit_idx])
    } else {
        format!("{value:.1} {}", UNITS[unit_idx])
    }
}
