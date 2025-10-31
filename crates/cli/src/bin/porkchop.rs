use anyhow::anyhow;
use clap::Parser;
use solar_core::constants::G0;
use solar_travel_calculator::config::{PlanetConfig, load_planets, load_vehicle_configs};
use solar_travel_calculator::ephemeris::{self, StateVector};
use solar_travel_calculator::export::porkchop as export_porkchop;
use solar_travel_calculator::impulsive::lambert;
use solar_travel_calculator::propulsion::{PropulsionMode, Vehicle};
use solar_travel_calculator::transfer::vehicle as transfer_vehicle;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

#[path = "porkchop/continuous.rs"]
mod continuous;

/// Generate porkchop data (CSV) for impulsive transfers by sweeping departure and arrival epochs.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Porkchop CSV generator (impulsive patched-conic)"
)]
struct Cli {
    /// Departure planet name (case-insensitive)
    #[arg(long)]
    from: String,

    /// Destination planet/moon name (case-insensitive)
    #[arg(long)]
    to: String,

    /// Departure window start epoch (UTC/TDB string)
    #[arg(long)]
    depart_start: String,

    /// Departure window end epoch (UTC/TDB string)
    #[arg(long)]
    depart_end: String,

    /// Arrival window start epoch (UTC/TDB string)
    #[arg(long)]
    arrive_start: Option<String>,

    /// Arrival window end epoch (UTC/TDB string)
    #[arg(long)]
    arrive_end: Option<String>,

    /// Grid step in days
    #[arg(long, default_value_t = 5.0)]
    step_days: f64,

    /// Parking altitude at origin in km (defaults to catalog)
    #[arg(long)]
    origin_altitude: Option<f64>,

    /// Parking altitude at destination in km (defaults to catalog)
    #[arg(long)]
    dest_altitude: Option<f64>,

    /// Use only the long-path Lambert solution (default: try both and pick min)
    #[arg(long, default_value_t = false)]
    long_path: bool,

    /// Output CSV file (use '-' for stdout)
    #[arg(long, default_value = "artifacts/pork.csv")]
    output: PathBuf,

    /// Vehicle name from the vehicle catalog to size burns/propellant.
    #[arg(long, default_value = "Ion Tug Mk1")]
    vehicle: String,
}

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2

struct EphemerisSample {
    et: f64,
    utc: String,
    state: Option<StateVector>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let planets = load_planets("configs/bodies")?;
    let vehicle_catalog = load_vehicle_configs("configs/vehicles")?;
    let spice_lookup: HashMap<String, PlanetConfig> = planets
        .iter()
        .map(|p| (p.spice_name.to_uppercase(), p.clone()))
        .collect();
    let vehicle = transfer_vehicle::select(&vehicle_catalog, Some(&cli.vehicle))?;

    let origin = find_body(&planets, &cli.from)?;
    let destination = find_body(&planets, &cli.to)?;

    let origin_parent = origin
        .parent_spice
        .as_ref()
        .and_then(|ps| spice_lookup.get(&ps.to_uppercase()).cloned());
    let destination_parent = destination
        .parent_spice
        .as_ref()
        .and_then(|ps| spice_lookup.get(&ps.to_uppercase()).cloned());

    let transfer_origin = origin_parent.clone().unwrap_or(origin.clone());
    let transfer_destination = destination_parent.clone().unwrap_or(destination.clone());

    let rpark_dep = origin.radius_km
        + cli
            .origin_altitude
            .unwrap_or(origin.default_parking_altitude_km);
    let rpark_arr = destination.radius_km
        + cli
            .dest_altitude
            .unwrap_or(destination.default_parking_altitude_km);

    let dep_start = ephemeris::epoch_seconds(&cli.depart_start)?;
    let dep_end = ephemeris::epoch_seconds(&cli.depart_end)?;
    if dep_end <= dep_start {
        return Err(anyhow!("departure window end must be after start"));
    }
    let step_s = (cli.step_days.max(0.1)) * 86_400.0;

    if matches!(vehicle.propulsion, PropulsionMode::Continuous { .. }) {
        return continuous::run_continuous_mode(
            &cli,
            &vehicle,
            &origin,
            origin_parent.as_ref(),
            &destination,
            destination_parent.as_ref(),
            dep_start,
            dep_end,
            step_s,
        );
    }

    let arrive_start_str = cli
        .arrive_start
        .as_ref()
        .ok_or_else(|| anyhow!("arrival window start required for impulsive transfers"))?;
    let arrive_end_str = cli
        .arrive_end
        .as_ref()
        .ok_or_else(|| anyhow!("arrival window end required for impulsive transfers"))?;
    let arr_start = ephemeris::epoch_seconds(arrive_start_str)?;
    let arr_end = ephemeris::epoch_seconds(arrive_end_str)?;
    if arr_end <= arr_start {
        return Err(anyhow!("arrival window end must be after start"));
    }

    let dep_transfer_spice =
        ephemeris::normalize_heliocentric_target_name(&transfer_origin.spice_name);
    let arr_transfer_spice =
        ephemeris::normalize_heliocentric_target_name(&transfer_destination.spice_name);

    let dep_samples = build_samples(&dep_transfer_spice, "SUN", dep_start, dep_end, step_s);
    let arr_samples = build_samples(&arr_transfer_spice, "SUN", arr_start, arr_end, step_s);

    let origin_rel_samples = origin_parent.as_ref().map(|parent| {
        build_samples(
            &origin.spice_name,
            &parent.spice_name,
            dep_start,
            dep_end,
            step_s,
        )
    });
    let destination_rel_samples = destination_parent.as_ref().map(|parent| {
        build_samples(
            &destination.spice_name,
            &parent.spice_name,
            arr_start,
            arr_end,
            step_s,
        )
    });

    let mut writer = export_porkchop::writer_for_path(&cli.output)?;
    export_porkchop::write_header(writer.as_mut())?;

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

            if cli.long_path {
                if let Some(branch) = evaluate_branch(dep_state, arr_state, tof, false) {
                    if let Some(res) = assemble_result(
                        &branch,
                        &origin,
                        origin_parent.as_ref(),
                        origin_rel_state,
                        &destination,
                        destination_parent.as_ref(),
                        destination_rel_state,
                        rpark_dep,
                        rpark_arr,
                        &vehicle,
                    ) {
                        branch_results.push(res);
                    }
                }
            } else {
                if let Some(branch) = evaluate_branch(dep_state, arr_state, tof, true) {
                    if let Some(res) = assemble_result(
                        &branch,
                        &origin,
                        origin_parent.as_ref(),
                        origin_rel_state,
                        &destination,
                        destination_parent.as_ref(),
                        destination_rel_state,
                        rpark_dep,
                        rpark_arr,
                        &vehicle,
                    ) {
                        branch_results.push(res);
                    }
                }
                if let Some(branch) = evaluate_branch(dep_state, arr_state, tof, false) {
                    if let Some(res) = assemble_result(
                        &branch,
                        &origin,
                        origin_parent.as_ref(),
                        origin_rel_state,
                        &destination,
                        destination_parent.as_ref(),
                        destination_rel_state,
                        rpark_dep,
                        rpark_arr,
                        &vehicle,
                    ) {
                        branch_results.push(res);
                    }
                }
            }

            branch_results.sort_by(|a, b| a.dv_total.partial_cmp(&b.dv_total).unwrap());
            let (best, path, feasible) = if let Some(best) = branch_results.first() {
                (best.clone(), best.path, true)
            } else {
                (
                    BranchResult::empty(),
                    if cli.long_path { "long" } else { "none" },
                    false,
                )
            };

            let depart_utc = &dep_sample.utc;
            let arrive_utc = &arr_sample.utc;
            let tof_days = (arr_sample.et - dep_sample.et) / 86_400.0;

            let record = export_porkchop::Record {
                depart_et: dep_sample.et,
                arrive_et: arr_sample.et,
                depart_utc,
                arrive_utc,
                tof_days,
                c3: best.c3,
                vinf_dep: best.vinf_dep,
                vinf_arr: best.vinf_arr,
                dv_dep: best.dv_dep,
                dv_arr: best.dv_arr,
                dv_total: best.dv_total,
                propellant_used_kg: best.propellant_used_kg,
                burn_time_s: best.burn_time_s,
                final_mass_kg: best.final_mass_kg,
                path,
                feasible,
                origin_body: origin.spice_name.as_str(),
                dest_body: destination.spice_name.as_str(),
                rpark_dep_km: rpark_dep,
                rpark_arr_km: rpark_arr,
            };
            record.write_to(writer.as_mut())?;
        }
    }

    writer.flush()?;

    Ok(())
}

fn build_samples(
    target: &str,
    observer: &str,
    start: f64,
    end: f64,
    step_s: f64,
) -> Vec<EphemerisSample> {
    let mut samples = Vec::new();
    let mut t = start;
    while t <= end + 1.0 {
        let state = fetch_state(target, observer, t).ok();
        let utc = ephemeris::format_epoch(t).unwrap_or_else(|_| "".to_string());
        samples.push(EphemerisSample { et: t, utc, state });
        t += step_s;
    }
    samples
}

fn fetch_state(
    target: &str,
    observer: &str,
    et: f64,
) -> Result<StateVector, ephemeris::EphemerisError> {
    ephemeris::state_vector_et(target, observer, "ECLIPJ2000", "NONE", et)
}

fn find_body<'a>(planets: &'a [PlanetConfig], name: &str) -> anyhow::Result<PlanetConfig> {
    let upper = name.to_uppercase();
    planets
        .iter()
        .find(|p| p.name.to_uppercase() == upper)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Body '{}' not found in catalog", name))
}

fn norm3(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
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
    path: &'static str,
}

impl BranchResult {
    fn empty() -> Self {
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
            path: "none",
        }
    }
}

struct LambertBranch {
    vinf_dep_vec: [f64; 3],
    vinf_arr_vec: [f64; 3],
    path: &'static str,
}

struct PropulsiveSummary {
    propellant_total: f64,
    burn_time_total: f64,
    final_mass: f64,
}

fn evaluate_branch(
    dep_state: &StateVector,
    arr_state: &StateVector,
    tof: f64,
    short: bool,
) -> Option<LambertBranch> {
    let lam = lambert::solve(
        dep_state.position_km,
        arr_state.position_km,
        tof,
        MU_SUN,
        short,
    )
    .ok()?;

    let (v1_lam, v2_lam) = lam;
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
        path: if short { "short" } else { "long" },
    })
}

fn assemble_result(
    branch: &LambertBranch,
    origin: &PlanetConfig,
    origin_parent: Option<&PlanetConfig>,
    origin_rel_state: Option<&StateVector>,
    destination: &PlanetConfig,
    destination_parent: Option<&PlanetConfig>,
    destination_rel_state: Option<&StateVector>,
    rpark_dep: f64,
    rpark_arr: f64,
    vehicle: &Vehicle,
) -> Option<BranchResult> {
    let vinf_dep_vec = vinf_vector_for_body(origin_parent, &branch.vinf_dep_vec, origin_rel_state)?;
    let vinf_arr_vec = vinf_vector_for_body(
        destination_parent,
        &branch.vinf_arr_vec,
        destination_rel_state,
    )?;

    let vinf_dep = norm3(&vinf_dep_vec);
    let vinf_arr = norm3(&vinf_arr_vec);
    let c3 = vinf_dep * vinf_dep;

    let (dv_dep, dv_arr, dv_total, propulsive) = match vehicle.propulsion {
        PropulsionMode::Continuous { .. } => return None,
        PropulsionMode::Impulsive {
            max_delta_v_km_s, ..
        } => {
            let dv_dep = burn_from_vinf(origin.mu_km3_s2, rpark_dep, vinf_dep);
            let dv_arr = burn_from_vinf(destination.mu_km3_s2, rpark_arr, vinf_arr);
            let dv_total = dv_dep + dv_arr;
            if dv_total > max_delta_v_km_s {
                return None;
            }
            let prop_summary = compute_propellant_and_burn(vehicle, dv_dep, dv_arr)?;
            (dv_dep, dv_arr, dv_total, prop_summary)
        }
        PropulsionMode::Hybrid => return None,
    };

    Some(BranchResult {
        c3,
        vinf_dep,
        vinf_arr,
        dv_dep,
        dv_arr,
        dv_total,
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
        Some([
            vinf_transfer_vec[0],
            vinf_transfer_vec[1],
            vinf_transfer_vec[2],
        ])
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
    match vehicle.propulsion {
        PropulsionMode::Hybrid => return None,
        _ => {}
    }

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
