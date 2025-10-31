//! Utility binary to download commonly used SPICE kernels into `data/spice/`.
//!
//! The download set is intentionally small to keep onboarding fast. Additional
//! kernels can be added by extending the catalog in `ephemeris::kernels`.

use solar_travel_calculator::ephemeris;
use solar_travel_calculator::ephemeris::kernels::KERNEL_CATALOG;
use solar_travel_calculator::importer::{self, KernelStatus};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let statuses = importer::download_kernels(KERNEL_CATALOG)?;
    for status in statuses {
        match status {
            KernelStatus::Downloaded(path) => println!("[downloaded] {}", path.display()),
            KernelStatus::AlreadyPresent(path) => println!("[skip] {}", path.display()),
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
