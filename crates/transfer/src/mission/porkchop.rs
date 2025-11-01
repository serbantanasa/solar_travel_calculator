//! Porkchop grid generation utilities shared between the CLI and mission tooling.

use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json;
use solar_config::PlanetConfig;
use solar_core::constants::G0;
use solar_ephem_spice::{self as ephemeris, StateVector};
use solar_impulsive::lambert;
use solar_propulsion::{PropulsionMode, Vehicle};

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2
pub const WINDOW_DATASET_VERSION: u32 = 1;
const TIME_GROUP_TOLERANCE_S: f64 = 1.0;

#[derive(Debug, Clone)]
pub struct TimeWindow {
    pub start_et: f64,
    pub end_et: f64,
    pub step_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct PorkchopRequest<'a> {
    pub origin_body: &'a PlanetConfig,
    pub origin_parent: Option<&'a PlanetConfig>,
    pub destination_body: &'a PlanetConfig,
    pub destination_parent: Option<&'a PlanetConfig>,
    pub vehicle: &'a Vehicle,
    pub rpark_depart_km: f64,
    pub rpark_arrive_km: f64,
    pub departure_window: TimeWindow,
    pub arrival_window: TimeWindow,
    pub long_path_only: bool,
    pub ignore_vehicle_limits: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PorkchopPath {
    Short,
    Long,
    None,
}

#[derive(Debug, Clone)]
pub struct PorkchopPoint {
    pub depart_et: f64,
    pub arrive_et: f64,
    pub depart_utc: String,
    pub arrive_utc: String,
    pub tof_days: f64,
    pub c3_km2_s2: f64,
    pub vinf_depart_km_s: f64,
    pub vinf_arrive_km_s: f64,
    pub dv_depart_km_s: f64,
    pub dv_arrive_km_s: f64,
    pub dv_total_km_s: f64,
    pub propellant_used_kg: f64,
    pub burn_time_s: f64,
    pub final_mass_kg: f64,
    pub lambert_path: PorkchopPath,
    pub feasible: bool,
}

#[derive(Debug, Clone)]
struct EphemerisSample {
    et: f64,
    utc: String,
    state: Option<StateVector>,
}

#[derive(Clone)]
struct LambertBranch {
    vinf_dep_vec: [f64; 3],
    vinf_arr_vec: [f64; 3],
    path: PorkchopPath,
}

#[derive(Clone)]
struct BranchResult {
    c3: f64,
    vinf_dep: f64,
    vinf_arr: f64,
    dv_dep: f64,
    dv_arr: f64,
    dv_total: f64,
    propellant_used_kg: f64,
    burn_time_s: f64,
    final_mass_kg: f64,
    path: PorkchopPath,
}

impl BranchResult {
    fn empty(path: PorkchopPath) -> Self {
        Self {
            c3: 0.0,
            vinf_dep: 0.0,
            vinf_arr: 0.0,
            dv_dep: 0.0,
            dv_arr: 0.0,
            dv_total: 0.0,
            propellant_used_kg: 0.0,
            burn_time_s: 0.0,
            final_mass_kg: 0.0,
            path,
        }
    }
}

struct PropulsiveSummary {
    propellant_total: f64,
    burn_time_total: f64,
    final_mass: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSample {
    pub depart_et: f64,
    pub depart_utc: String,
    pub arrive_et: f64,
    pub arrive_utc: String,
    pub dv_total_km_s: f64,
    pub dv_depart_km_s: f64,
    pub dv_arrive_km_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDataset {
    pub version: u32,
    pub origin_spice: String,
    pub destination_spice: String,
    pub depart_start_et: f64,
    pub depart_end_et: f64,
    pub step_days: f64,
    pub min_tof_days: f64,
    pub max_tof_days: f64,
    pub min_dv_total_km_s: Option<f64>,
    pub samples: Vec<WindowSample>,
}

impl WindowDataset {
    fn baseline_sample(&self) -> Option<&WindowSample> {
        self.samples.iter().min_by(|a, b| {
            a.dv_total_km_s
                .partial_cmp(&b.dv_total_km_s)
                .unwrap_or(Ordering::Equal)
        })
    }
}

#[derive(Debug, Clone)]
pub struct WindowSuggestion {
    pub baseline: WindowSample,
    pub recommended: WindowSample,
    pub user_total_dv_km_s: f64,
    pub threshold_dv_km_s: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("ephemeris error: {0}")]
    Ephemeris(#[from] ephemeris::EphemerisError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn generate(
    request: &PorkchopRequest<'_>,
) -> Result<Vec<PorkchopPoint>, ephemeris::EphemerisError> {
    let transfer_origin = request.origin_parent.unwrap_or(request.origin_body);
    let transfer_destination = request
        .destination_parent
        .unwrap_or(request.destination_body);

    let dep_transfer_target =
        ephemeris::normalize_heliocentric_target_name(&transfer_origin.spice_name);
    let arr_transfer_target =
        ephemeris::normalize_heliocentric_target_name(&transfer_destination.spice_name);

    let dep_samples = build_samples(&dep_transfer_target, "SUN", &request.departure_window)?;
    let arr_samples = build_samples(&arr_transfer_target, "SUN", &request.arrival_window)?;

    let origin_rel_samples = request.origin_parent.map(|parent| {
        build_samples(
            &request.origin_body.spice_name,
            &parent.spice_name,
            &request.departure_window,
        )
    });
    let destination_rel_samples = request.destination_parent.map(|parent| {
        build_samples(
            &request.destination_body.spice_name,
            &parent.spice_name,
            &request.arrival_window,
        )
    });

    let origin_rel_samples = match origin_rel_samples.transpose() {
        Ok(samples) => samples,
        Err(err) => return Err(err),
    };
    let destination_rel_samples = match destination_rel_samples.transpose() {
        Ok(samples) => samples,
        Err(err) => return Err(err),
    };

    let mut points = Vec::new();

    for (dep_idx, dep_sample) in dep_samples.iter().enumerate() {
        let dep_state = match dep_sample.state.as_ref() {
            Some(state) => state,
            None => continue,
        };
        let origin_rel_state = origin_rel_samples
            .as_ref()
            .and_then(|samples| samples.get(dep_idx))
            .and_then(|sample| sample.state.as_ref());

        for (arr_idx, arr_sample) in arr_samples.iter().enumerate() {
            if arr_sample.et <= dep_sample.et {
                continue;
            }
            let arr_state = match arr_sample.state.as_ref() {
                Some(state) => state,
                None => continue,
            };
            let destination_rel_state = destination_rel_samples
                .as_ref()
                .and_then(|samples| samples.get(arr_idx))
                .and_then(|sample| sample.state.as_ref());

            let tof = arr_sample.et - dep_sample.et;
            let mut branch_results = Vec::new();

            if !request.long_path_only {
                if let Some(branch) = evaluate_branch(dep_state, arr_state, tof, true) {
                    if let Some(result) =
                        assemble_result(&branch, request, origin_rel_state, destination_rel_state)
                    {
                        branch_results.push(result);
                    }
                }
            }

            if let Some(branch) = evaluate_branch(dep_state, arr_state, tof, false) {
                if let Some(result) =
                    assemble_result(&branch, request, origin_rel_state, destination_rel_state)
                {
                    branch_results.push(result);
                }
            }

            branch_results.sort_by(|a, b| match a.dv_total.partial_cmp(&b.dv_total) {
                Some(order) => order,
                None => Ordering::Equal,
            });

            let (best, feasible) = if let Some(best) = branch_results.first() {
                (best.clone(), true)
            } else {
                (
                    BranchResult::empty(if request.long_path_only {
                        PorkchopPath::Long
                    } else {
                        PorkchopPath::None
                    }),
                    false,
                )
            };

            points.push(PorkchopPoint {
                depart_et: dep_sample.et,
                arrive_et: arr_sample.et,
                depart_utc: dep_sample.utc.clone(),
                arrive_utc: arr_sample.utc.clone(),
                tof_days: tof / 86_400.0,
                c3_km2_s2: best.c3,
                vinf_depart_km_s: best.vinf_dep,
                vinf_arrive_km_s: best.vinf_arr,
                dv_depart_km_s: best.dv_dep,
                dv_arrive_km_s: best.dv_arr,
                dv_total_km_s: best.dv_total,
                propellant_used_kg: best.propellant_used_kg,
                burn_time_s: best.burn_time_s,
                final_mass_kg: best.final_mass_kg,
                lambert_path: best.path,
                feasible,
            });
        }
    }

    Ok(points)
}

fn build_samples(
    target: &str,
    observer: &str,
    window: &TimeWindow,
) -> Result<Vec<EphemerisSample>, ephemeris::EphemerisError> {
    let mut samples = Vec::new();
    let mut t = window.start_et;
    while t <= window.end_et + 1.0 {
        let state = ephemeris::state_vector_et(target, observer, "ECLIPJ2000", "NONE", t).ok();
        let utc = ephemeris::format_epoch(t)?;
        samples.push(EphemerisSample { et: t, utc, state });
        t += window.step_seconds;
    }
    Ok(samples)
}

fn evaluate_branch(
    dep_state: &StateVector,
    arr_state: &StateVector,
    tof: f64,
    short: bool,
) -> Option<LambertBranch> {
    let (v1_lam, v2_lam) = lambert::solve(
        dep_state.position_km,
        arr_state.position_km,
        tof,
        MU_SUN,
        short,
    )
    .ok()?;

    let vinf_dep_vec = [
        v1_lam[0] - dep_state.velocity_km_s[0],
        v1_lam[1] - dep_state.velocity_km_s[1],
        v1_lam[2] - dep_state.velocity_km_s[2],
    ];
    let vinf_arr_vec = [
        v2_lam[0] - arr_state.velocity_km_s[0],
        v2_lam[1] - arr_state.velocity_km_s[1],
        v2_lam[2] - arr_state.velocity_km_s[2],
    ];

    Some(LambertBranch {
        vinf_dep_vec,
        vinf_arr_vec,
        path: if short {
            PorkchopPath::Short
        } else {
            PorkchopPath::Long
        },
    })
}

fn assemble_result(
    branch: &LambertBranch,
    request: &PorkchopRequest<'_>,
    origin_rel_state: Option<&StateVector>,
    destination_rel_state: Option<&StateVector>,
) -> Option<BranchResult> {
    let vinf_dep_vec = vinf_vector_for_body(
        request.origin_parent,
        &branch.vinf_dep_vec,
        origin_rel_state,
    )?;
    let vinf_arr_vec = vinf_vector_for_body(
        request.destination_parent,
        &branch.vinf_arr_vec,
        destination_rel_state,
    )?;

    let vinf_dep = norm3(&vinf_dep_vec);
    let vinf_arr = norm3(&vinf_arr_vec);
    let c3 = vinf_dep * vinf_dep;

    let (dv_dep, dv_arr, propulsive) = match request.vehicle.propulsion {
        PropulsionMode::Impulsive {
            max_delta_v_km_s, ..
        } => {
            let dv_dep = burn_from_vinf(
                request.origin_body.mu_km3_s2,
                request.rpark_depart_km,
                vinf_dep,
            );
            let dv_arr = burn_from_vinf(
                request.destination_body.mu_km3_s2,
                request.rpark_arrive_km,
                vinf_arr,
            );
            let total = dv_dep + dv_arr;
            if !request.ignore_vehicle_limits && total > max_delta_v_km_s {
                return None;
            }
            let summary = if request.ignore_vehicle_limits {
                PropulsiveSummary {
                    propellant_total: 0.0,
                    burn_time_total: 0.0,
                    final_mass: request.vehicle.initial_mass_kg(),
                }
            } else {
                compute_propellant_and_burn(request.vehicle, dv_dep, dv_arr)?
            };
            (dv_dep, dv_arr, summary)
        }
        PropulsionMode::Continuous { .. } => return None,
        PropulsionMode::Hybrid => return None,
    };

    Some(BranchResult {
        c3,
        vinf_dep,
        vinf_arr,
        dv_dep,
        dv_arr,
        dv_total: dv_dep + dv_arr,
        propellant_used_kg: propulsive.propellant_total,
        burn_time_s: propulsive.burn_time_total,
        final_mass_kg: propulsive.final_mass,
        path: branch.path,
    })
}

fn vinf_vector_for_body(
    parent: Option<&PlanetConfig>,
    vinf_transfer_vec: &[f64; 3],
    rel_state: Option<&StateVector>,
) -> Option<[f64; 3]> {
    if parent.is_some() {
        let rel = rel_state?;
        Some([
            vinf_transfer_vec[0] - rel.velocity_km_s[0],
            vinf_transfer_vec[1] - rel.velocity_km_s[1],
            vinf_transfer_vec[2] - rel.velocity_km_s[2],
        ])
    } else {
        Some(*vinf_transfer_vec)
    }
}

fn burn_from_vinf(mu: f64, r: f64, vinf: f64) -> f64 {
    let v_circ = (mu / r).sqrt();
    let v_req = (vinf * vinf + 2.0 * mu / r).sqrt();
    (v_req - v_circ).max(0.0)
}

fn compute_propellant_and_burn(
    vehicle: &Vehicle,
    dv_dep: f64,
    dv_arr: f64,
) -> Option<PropulsiveSummary> {
    let mut remaining_prop = vehicle.propellant_mass_kg;
    let mut mass_before = vehicle.initial_mass_kg();

    let (prop_dep, burn_dep, mass_after_dep) =
        compute_single_burn(&vehicle.propulsion, mass_before, dv_dep, remaining_prop)?;
    remaining_prop -= prop_dep;
    mass_before = mass_after_dep;

    let (prop_arr, burn_arr, mass_after_arr) =
        compute_single_burn(&vehicle.propulsion, mass_before, dv_arr, remaining_prop)?;

    if mass_after_arr < vehicle.dry_mass_kg - 1e-6 {
        return None;
    }

    Some(PropulsiveSummary {
        propellant_total: prop_dep + prop_arr,
        burn_time_total: burn_dep + burn_arr,
        final_mass: mass_after_arr,
    })
}

fn compute_single_burn(
    mode: &PropulsionMode,
    mass_before: f64,
    dv_km_s: f64,
    propellant_available: f64,
) -> Option<(f64, f64, f64)> {
    if dv_km_s.abs() < 1e-9 {
        return Some((0.0, 0.0, mass_before));
    }

    let (ve, thrust_opt, accel_limit) = match mode {
        PropulsionMode::Impulsive {
            isp_seconds,
            max_thrust_newtons,
            ..
        } => (isp_seconds * G0, *max_thrust_newtons, None::<f64>),
        PropulsionMode::Continuous {
            max_thrust_newtons,
            isp_seconds,
            max_acceleration_m_s2,
        } => (
            isp_seconds * G0,
            Some(*max_thrust_newtons),
            *max_acceleration_m_s2,
        ),
        PropulsionMode::Hybrid => return None,
    };

    if ve <= 0.0 {
        return None;
    }

    let dv_m_s = dv_km_s * 1000.0;
    let exponent = -dv_m_s / ve;
    let mass_after = mass_before * exponent.exp();
    let prop_used = mass_before - mass_after;

    if prop_used.is_nan() || prop_used < -1e-9 {
        return None;
    }
    if prop_used > propellant_available + 1e-9 {
        return None;
    }

    let burn_time = match thrust_opt {
        Some(thrust) if thrust > 0.0 && prop_used > 0.0 => {
            let avg_mass = 0.5 * (mass_before + mass_after);
            let mut effective_thrust = thrust;
            if let Some(limit) = accel_limit {
                effective_thrust = effective_thrust.min(limit * avg_mass);
            }
            if effective_thrust > 0.0 {
                prop_used * ve / effective_thrust
            } else {
                0.0
            }
        }
        _ => 0.0,
    };

    Some((prop_used, burn_time, mass_after))
}

fn norm3(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn window_sample_from_point(point: &PorkchopPoint) -> WindowSample {
    WindowSample {
        depart_et: point.depart_et,
        depart_utc: point.depart_utc.clone(),
        arrive_et: point.arrive_et,
        arrive_utc: point.arrive_utc.clone(),
        dv_total_km_s: point.dv_total_km_s,
        dv_depart_km_s: point.dv_depart_km_s,
        dv_arrive_km_s: point.dv_arrive_km_s,
    }
}

pub fn compute_window_dataset(
    origin_body: &PlanetConfig,
    origin_parent: Option<&PlanetConfig>,
    destination_body: &PlanetConfig,
    destination_parent: Option<&PlanetConfig>,
    vehicle: &Vehicle,
    rpark_depart_km: f64,
    rpark_arrive_km: f64,
    depart_start_et: f64,
    span_days: f64,
    step_days: f64,
    min_tof_days: f64,
    max_tof_days: f64,
) -> Result<WindowDataset, WindowError> {
    let step_seconds = step_days.max(0.1) * 86_400.0;
    let depart_end_et = depart_start_et + span_days.max(step_days) * 86_400.0;
    let arrival_start_et = depart_start_et + min_tof_days.max(1.0) * 86_400.0;
    let mut arrival_end_et = depart_end_et + max_tof_days.max(min_tof_days) * 86_400.0;
    if arrival_end_et <= arrival_start_et {
        arrival_end_et = arrival_start_et + step_seconds;
    }

    let departure_window = TimeWindow {
        start_et: depart_start_et,
        end_et: depart_end_et,
        step_seconds,
    };
    let arrival_window = TimeWindow {
        start_et: arrival_start_et,
        end_et: arrival_end_et,
        step_seconds,
    };

    let request = PorkchopRequest {
        origin_body,
        origin_parent,
        destination_body,
        destination_parent,
        vehicle,
        rpark_depart_km,
        rpark_arrive_km,
        departure_window,
        arrival_window,
        long_path_only: false,
        ignore_vehicle_limits: true,
    };

    let points = generate(&request)?;

    let mut samples = Vec::new();
    let mut current_depart: Option<f64> = None;
    let mut best_sample: Option<WindowSample> = None;

    for point in points.iter() {
        if !point.feasible {
            continue;
        }

        let is_new_depart = match current_depart {
            Some(et) => (point.depart_et - et).abs() > TIME_GROUP_TOLERANCE_S,
            None => true,
        };

        if is_new_depart {
            if let Some(sample) = best_sample.take() {
                samples.push(sample);
            }
            current_depart = Some(point.depart_et);
            best_sample = Some(window_sample_from_point(point));
        } else if let Some(sample) = best_sample.as_mut() {
            if point.dv_total_km_s < sample.dv_total_km_s {
                *sample = window_sample_from_point(point);
            }
        }
    }

    if let Some(sample) = best_sample {
        samples.push(sample);
    }

    samples.sort_by(|a, b| {
        a.depart_et
            .partial_cmp(&b.depart_et)
            .unwrap_or(Ordering::Equal)
    });

    let min_dv = samples
        .iter()
        .map(|s| s.dv_total_km_s)
        .fold(None, |acc, dv| match acc {
            Some(current) if dv >= current => Some(current),
            _ => Some(dv),
        });

    Ok(WindowDataset {
        version: WINDOW_DATASET_VERSION,
        origin_spice: origin_body.spice_name.clone(),
        destination_spice: destination_body.spice_name.clone(),
        depart_start_et,
        depart_end_et,
        step_days,
        min_tof_days,
        max_tof_days,
        min_dv_total_km_s: min_dv,
        samples,
    })
}

pub fn save_window_dataset(path: &Path, dataset: &WindowDataset) -> Result<(), WindowError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, dataset)?;
    Ok(())
}

pub fn load_window_dataset(path: &Path) -> Result<WindowDataset, WindowError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let dataset = serde_json::from_reader(reader)?;
    Ok(dataset)
}

pub fn analyze_departure(
    dataset: &WindowDataset,
    departure_et: f64,
    total_dv_km_s: f64,
    threshold_factor: f64,
) -> Option<WindowSuggestion> {
    let baseline = dataset.baseline_sample()?.clone();
    let threshold = baseline.dv_total_km_s * threshold_factor;
    if total_dv_km_s <= threshold {
        return None;
    }

    let mut forward_candidate: Option<WindowSample> = None;
    let mut backward_candidate: Option<WindowSample> = None;

    for sample in &dataset.samples {
        if sample.dv_total_km_s > threshold {
            continue;
        }

        if sample.depart_et >= departure_et {
            match &mut forward_candidate {
                Some(best) if sample.dv_total_km_s >= best.dv_total_km_s => {}
                _ => forward_candidate = Some(sample.clone()),
            }
        } else {
            match &mut backward_candidate {
                Some(best) if sample.dv_total_km_s <= best.dv_total_km_s => {
                    *best = sample.clone();
                }
                None => backward_candidate = Some(sample.clone()),
                _ => {}
            }
        }
    }

    let recommended = forward_candidate
        .or(backward_candidate)
        .unwrap_or_else(|| baseline.clone());

    Some(WindowSuggestion {
        baseline,
        recommended,
        user_total_dv_km_s: total_dv_km_s,
        threshold_dv_km_s: threshold,
    })
}
