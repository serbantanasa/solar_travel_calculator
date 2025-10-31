//! SPICE ephemeris helpers and metadata built on top of the CSPICE toolkit.

use std::ffi::{CStr, CString};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use cspice_sys::{
    SpiceBoolean, SpiceDouble, SpiceInt, erract_c, et2utc_c, failed_c, furnsh_c, getmsg_c,
    kclear_c, reset_c, spkezr_c, str2et_c,
};
use thiserror::Error;

pub mod kernels;

use kernels::{KERNEL_CATALOG, KernelDescriptor};

/// Basic metadata describing a local SPICE kernel.
#[derive(Debug)]
pub struct KernelSummary {
    pub descriptor: &'static KernelDescriptor,
    pub path: PathBuf,
    pub file_size_bytes: u64,
}

/// Position, velocity, and light-time returned from SPICE.
#[derive(Debug, Clone, Copy)]
pub struct StateVector {
    pub position_km: [f64; 3],
    pub velocity_km_s: [f64; 3],
    pub light_time_seconds: f64,
}

/// Errors surfaced while validating or querying the SPICE toolkit.
#[derive(Debug, Error)]
pub enum EphemerisError {
    #[error("kernel `{name}` is missing at {path}")]
    MissingKernel { name: &'static str, path: PathBuf },
    #[error("kernel `{name}` path contains invalid UTF-8: {path}")]
    InvalidKernelPath { name: &'static str, path: PathBuf },
    #[error("failed to read metadata for kernel `{name}`: {source}")]
    Io {
        name: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid epoch string `{epoch}`")]
    InvalidEpoch { epoch: String },
    #[error("SPICE kernel call failed: {message}")]
    Spice { message: String },
}

/// Ensure the CSPICE runtime has all required kernels loaded.
static INITIALIZED: OnceLock<()> = OnceLock::new();
static INITIALIZE_LOCK: Mutex<()> = Mutex::new(());

pub fn load_default_kernels() -> Result<(), EphemerisError> {
    if INITIALIZED.get().is_some() {
        return Ok(());
    }
    let _lock = INITIALIZE_LOCK.lock().unwrap();
    if INITIALIZED.get().is_some() {
        return Ok(());
    }
    initialize_spice()?;
    INITIALIZED
        .set(())
        .expect("INITIALIZED OnceLock set exactly once");
    Ok(())
}

/// Normalize a SPICE target name for heliocentric queries.
///
/// For major planets, prefer barycenter targets when querying relative to the Sun
/// to maintain consistency across bodies that are otherwise mixed (e.g., EARTH vs MARS BARYCENTER).
/// Non-planetary targets (moons, asteroids) are passed through unchanged.
pub fn normalize_heliocentric_target_name(name: &str) -> String {
    if name.to_ascii_uppercase().contains("BARYCENTER") {
        return name.to_string();
    }
    match name.to_ascii_uppercase().as_str() {
        "MERCURY" => "MERCURY BARYCENTER".to_string(),
        "VENUS" => "VENUS BARYCENTER".to_string(),
        "EARTH" => "EARTH BARYCENTER".to_string(),
        "MARS" => "MARS BARYCENTER".to_string(),
        "JUPITER" => "JUPITER BARYCENTER".to_string(),
        "SATURN" => "SATURN BARYCENTER".to_string(),
        "URANUS" => "URANUS BARYCENTER".to_string(),
        "NEPTUNE" => "NEPTUNE BARYCENTER".to_string(),
        "PLUTO" => "PLUTO BARYCENTER".to_string(),
        other => other.to_string(),
    }
}

/// Summarize the local kernel set with file sizes and descriptions.
pub fn kernel_summaries() -> Result<Vec<KernelSummary>, EphemerisError> {
    validate_kernel_paths()?;
    KERNEL_CATALOG
        .iter()
        .map(|descriptor| {
            let path = descriptor.local_path();
            let metadata = fs::metadata(&path).map_err(|source| EphemerisError::Io {
                name: descriptor.filename,
                source,
            })?;
            Ok(KernelSummary {
                descriptor,
                path,
                file_size_bytes: metadata.len(),
            })
        })
        .collect()
}

/// Query the state vector of a target relative to an observer.
pub fn state_vector(
    target: &str,
    observer: &str,
    reference_frame: &str,
    aberration_correction: &str,
    epoch: &str,
) -> Result<StateVector, EphemerisError> {
    load_default_kernels()?;

    let target_c = CString::new(target).unwrap();
    let observer_c = CString::new(observer).unwrap();
    let reference_frame_c = CString::new(reference_frame).unwrap();
    let aberration_c = CString::new(aberration_correction).unwrap();
    let epoch_c = CString::new(epoch).unwrap();

    let mut ephemeris_time: SpiceDouble = 0.0;
    unsafe {
        str2et_c(epoch_c.as_ptr() as *mut i8, &mut ephemeris_time);
    }
    check_for_spice_error()?;

    state_vector_et_internal(
        &target_c,
        &observer_c,
        &reference_frame_c,
        &aberration_c,
        ephemeris_time,
    )
}

/// Query the state vector by supplying ephemeris seconds past J2000 directly.
pub fn state_vector_et(
    target: &str,
    observer: &str,
    reference_frame: &str,
    aberration_correction: &str,
    ephemeris_time: f64,
) -> Result<StateVector, EphemerisError> {
    load_default_kernels()?;

    let target_c = CString::new(target).unwrap();
    let observer_c = CString::new(observer).unwrap();
    let reference_frame_c = CString::new(reference_frame).unwrap();
    let aberration_c = CString::new(aberration_correction).unwrap();

    state_vector_et_internal(
        &target_c,
        &observer_c,
        &reference_frame_c,
        &aberration_c,
        ephemeris_time,
    )
}

/// Convert a time string understood by SPICE into ephemeris seconds past J2000.
pub fn epoch_seconds(epoch: &str) -> Result<f64, EphemerisError> {
    load_default_kernels()?;
    let epoch_c = CString::new(epoch).map_err(|_| EphemerisError::InvalidEpoch {
        epoch: epoch.to_string(),
    })?;
    let mut et = 0.0;
    unsafe {
        str2et_c(epoch_c.as_ptr() as *mut i8, &mut et);
    }
    check_for_spice_error()?;
    Ok(et)
}

/// Format an ephemeris time (seconds past J2000) into a UTC calendar string.
pub fn format_epoch(et: f64) -> Result<String, EphemerisError> {
    load_default_kernels()?;
    let mut buffer = vec![0i8; 64];
    let fmt = CString::new("C").unwrap();
    unsafe {
        et2utc_c(
            et,
            fmt.as_ptr() as *mut i8,
            3,
            buffer.len() as SpiceInt,
            buffer.as_mut_ptr(),
        );
    }
    check_for_spice_error()?;
    let c_str = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    Ok(c_str.to_string_lossy().trim().to_string())
}

fn initialize_spice() -> Result<(), EphemerisError> {
    validate_kernel_paths()?;
    unsafe {
        kclear_c();
    }
    configure_error_handling();
    for descriptor in KERNEL_CATALOG {
        let c_path = path_to_cstring(descriptor)?;
        unsafe {
            furnsh_c(c_path.as_ptr() as *mut i8);
        }
        check_for_spice_error()?;
    }
    Ok(())
}

fn validate_kernel_paths() -> Result<(), EphemerisError> {
    for descriptor in KERNEL_CATALOG {
        let path = descriptor.local_path();
        if !path.exists() {
            return Err(EphemerisError::MissingKernel {
                name: descriptor.filename,
                path,
            });
        }
        if path.to_str().is_none() {
            return Err(EphemerisError::InvalidKernelPath {
                name: descriptor.filename,
                path,
            });
        }
    }
    Ok(())
}

fn path_to_cstring(descriptor: &KernelDescriptor) -> Result<CString, EphemerisError> {
    let path = descriptor.local_path();
    let path_str = path
        .to_str()
        .ok_or_else(|| EphemerisError::InvalidKernelPath {
            name: descriptor.filename,
            path: path.clone(),
        })?;
    CString::new(path_str).map_err(|_| EphemerisError::InvalidKernelPath {
        name: descriptor.filename,
        path,
    })
}

fn configure_error_handling() {
    const SET: &[u8] = b"SET\0";
    const RETURN_MODE: &[u8] = b"RETURN\0";
    unsafe {
        erract_c(
            SET.as_ptr() as *mut i8,
            0 as SpiceInt,
            RETURN_MODE.as_ptr() as *mut i8,
        );
    }
}

fn state_vector_et_internal(
    target: &CString,
    observer: &CString,
    reference_frame: &CString,
    aberration_correction: &CString,
    ephemeris_time: SpiceDouble,
) -> Result<StateVector, EphemerisError> {
    let mut state: [SpiceDouble; 6] = [0.0; 6];
    let mut light_time: SpiceDouble = 0.0;
    unsafe {
        spkezr_c(
            target.as_ptr() as *mut i8,
            ephemeris_time,
            reference_frame.as_ptr() as *mut i8,
            aberration_correction.as_ptr() as *mut i8,
            observer.as_ptr() as *mut i8,
            state.as_mut_ptr(),
            &mut light_time,
        );
    }
    check_for_spice_error()?;

    Ok(StateVector {
        position_km: [state[0], state[1], state[2]],
        velocity_km_s: [state[3], state[4], state[5]],
        light_time_seconds: light_time,
    })
}

fn check_for_spice_error() -> Result<(), EphemerisError> {
    unsafe {
        if failed_c() != 0 as SpiceBoolean {
            const LONG: &[u8] = b"LONG\0";
            let mut buffer = vec![0i8; 1024];
            getmsg_c(
                LONG.as_ptr() as *mut i8,
                buffer.len() as SpiceInt,
                buffer.as_mut_ptr(),
            );
            reset_c();
            let message = CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .trim()
                .to_string();
            return Err(EphemerisError::Spice { message });
        }
    }
    Ok(())
}
