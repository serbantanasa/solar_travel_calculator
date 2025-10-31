//! SPICE kernel import utilities.

use reqwest::blocking::Client;
use solar_ephem_spice::kernels::{KernelDescriptor, LOCAL_SPICE_DIR};
use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

/// Download all kernels listed in the provided descriptor set.
pub fn download_kernels(
    descriptors: &[KernelDescriptor],
) -> Result<Vec<KernelStatus>, ImportError> {
    fs::create_dir_all(LOCAL_SPICE_DIR)?;
    let client = Client::builder().build()?;
    let mut statuses = Vec::new();

    for descriptor in descriptors {
        let dest = descriptor.local_path();
        if dest.exists() {
            statuses.push(KernelStatus::AlreadyPresent(dest));
            continue;
        }
        download_kernel(&client, descriptor, &dest)?;
        statuses.push(KernelStatus::Downloaded(dest));
    }

    Ok(statuses)
}

fn download_kernel(
    client: &Client,
    descriptor: &KernelDescriptor,
    dest: &Path,
) -> Result<(), ImportError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut response = client.get(descriptor.url).send()?.error_for_status()?;
    let mut file = File::create(dest)?;
    copy(&mut response, &mut file)?;
    Ok(())
}

/// Outcome of attempting to download a kernel.
#[derive(Debug)]
pub enum KernelStatus {
    Downloaded(PathBuf),
    AlreadyPresent(PathBuf),
}
