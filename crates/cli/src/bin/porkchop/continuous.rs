use super::Cli;
use anyhow::anyhow;
use solar_travel_calculator::config::PlanetConfig;
use solar_travel_calculator::core::constants::G0;
use solar_travel_calculator::ephemeris::{self, StateVector};
use solar_travel_calculator::export::continuous as export_continuous;
use solar_travel_calculator::lowthrust::{
    ConstantAccelInputs, ContinuousTransferSummary, constant_accel_profile,
};
use solar_travel_calculator::propulsion::{PropulsionMode, Vehicle};

const MIN_TOF_S: f64 = 600.0;
const DISTANCE_TOLERANCE_M: f64 = 10_000.0;
const MAX_SEARCH_TIME_S: f64 = 5.0 * 365.25 * 86_400.0;
const MIN_REFINE_STEP_S: f64 = 1_800.0;

pub(super) fn run_continuous_mode(
    cli: &Cli,
    vehicle: &Vehicle,
    origin: &PlanetConfig,
    origin_parent: Option<&PlanetConfig>,
    destination: &PlanetConfig,
    destination_parent: Option<&PlanetConfig>,
    depart_start: f64,
    depart_end: f64,
    step_s: f64,
) -> anyhow::Result<()> {
    let PropulsionMode::Continuous {
        max_acceleration_m_s2,
        max_thrust_newtons,
        isp_seconds,
    } = vehicle.propulsion
    else {
        return Err(anyhow!(
            "vehicle '{}' is not configured for continuous propulsion",
            vehicle.name
        ));
    };

    let transfer_origin = origin_parent.unwrap_or(origin);
    let transfer_destination = destination_parent.unwrap_or(destination);
    let dep_target = ephemeris::normalize_heliocentric_target_name(&transfer_origin.spice_name);
    let arr_target =
        ephemeris::normalize_heliocentric_target_name(&transfer_destination.spice_name);

    let mut acceleration =
        max_acceleration_m_s2.unwrap_or_else(|| max_thrust_newtons / vehicle.initial_mass_kg());
    if max_thrust_newtons > 0.0 {
        acceleration = acceleration.min(max_thrust_newtons / vehicle.initial_mass_kg());
    }
    if acceleration <= 0.0 {
        return Err(anyhow!(
            "vehicle '{}' must declare a positive acceleration or thrust limit",
            vehicle.name
        ));
    }

    let accel_inputs = ConstantAccelInputs {
        acceleration_m_s2: acceleration,
        isp_seconds,
        initial_mass_kg: vehicle.initial_mass_kg(),
        dry_mass_kg: vehicle.dry_mass_kg,
    };

    let coarse_step = step_s.max(43_200.0); // at least 12h resolution for departure scan

    let best = find_best_departure_profile(
        &accel_inputs,
        &dep_target,
        &arr_target,
        depart_start,
        depart_end,
        coarse_step,
    )?;

    let depart_utc = ephemeris::format_epoch(best.depart_et).unwrap_or_else(|_| "".to_string());
    let arrive_utc = ephemeris::format_epoch(best.arrive_et).unwrap_or_else(|_| "".to_string());

    let metadata = export_continuous::Metadata {
        vehicle: &vehicle.name,
        origin: &origin.name,
        destination: &destination.name,
        depart_et: best.depart_et,
        depart_utc: &depart_utc,
        arrive_et: best.arrive_et,
        arrive_utc: &arrive_utc,
    };

    let samples: Vec<export_continuous::Sample> = best
        .summary
        .samples
        .iter()
        .map(|s| export_continuous::Sample {
            time_s: s.time_s,
            distance_m: s.distance_m,
            velocity_m_s: s.velocity_m_s,
            mass_kg: s.mass_kg,
        })
        .collect();

    let telemetry = export_continuous::TelemetrySummary {
        time_of_flight_s: best.summary.time_of_flight_s,
        burn_time_total_s: best.summary.burn_time_total_s,
        propellant_used_kg: best.summary.propellant_used_kg,
        final_mass_kg: best.summary.final_mass_kg,
        max_velocity_m_s: best.summary.max_velocity_m_s,
        max_velocity_fraction_c: best.summary.max_velocity_fraction_c,
        total_distance_m: best.summary.total_distance_m,
        kinetic_energy_joules: best.summary.kinetic_energy_joules,
        samples,
    };

    export_continuous::write_sidecars(&cli.output, &metadata, &telemetry)?;

    println!(
        "Continuous profile ({})\n  Departure: {}\n  Arrival: {}\n  TOF: {:.2} days\n  Peak velocity: {:.2} km/s ({:.3}% c)\n  Total distance: {:.2} million km\n  Propellant used: {:.2} kg\n  Burn time (total): {:.2} hours",
        vehicle.name,
        depart_utc,
        arrive_utc,
        telemetry.time_of_flight_s / 86_400.0,
        telemetry.max_velocity_m_s / 1_000.0,
        telemetry.max_velocity_fraction_c * 100.0,
        telemetry.total_distance_m / 1.0e9,
        telemetry.propellant_used_kg,
        telemetry.burn_time_total_s / 3_600.0,
    );

    Ok(())
}

struct Candidate {
    depart_et: f64,
    arrive_et: f64,
    summary: ContinuousTransferSummary,
}

struct Evaluation {
    error_m: f64,
    summary: ContinuousTransferSummary,
    actual_distance_m: f64,
}

fn find_best_departure_profile(
    accel_inputs: &ConstantAccelInputs,
    dep_target: &str,
    arr_target: &str,
    depart_start: f64,
    depart_end: f64,
    coarse_step: f64,
) -> anyhow::Result<Candidate> {
    let mut best: Option<Candidate> = None;

    for depart_et in sample_window(depart_start, depart_end, coarse_step) {
        if let Some(candidate) = evaluate_departure(accel_inputs, dep_target, arr_target, depart_et)
        {
            match &best {
                Some(current) if !is_better_candidate(&candidate, current) => {}
                _ => best = Some(candidate),
            }
        }
    }

    let mut best =
        best.ok_or_else(|| anyhow!("continuous transfer infeasible within departure window"))?;

    let refine_start = (best.depart_et - coarse_step).max(depart_start);
    let refine_end = (best.depart_et + coarse_step).min(depart_end);
    let refine_step =
        (coarse_step / 12.0).clamp(MIN_REFINE_STEP_S, coarse_step.max(MIN_REFINE_STEP_S));

    for depart_et in sample_window(refine_start, refine_end, refine_step) {
        if let Some(candidate) = evaluate_departure(accel_inputs, dep_target, arr_target, depart_et)
        {
            if is_better_candidate(&candidate, &best) {
                best = candidate;
            }
        }
    }

    Ok(best)
}

fn evaluate_departure(
    accel_inputs: &ConstantAccelInputs,
    dep_target: &str,
    arr_target: &str,
    depart_et: f64,
) -> Option<Candidate> {
    let dep_state =
        ephemeris::state_vector_et(dep_target, "SUN", "ECLIPJ2000", "NONE", depart_et).ok()?;

    solve_time_of_flight(accel_inputs, &dep_state, depart_et, arr_target)
}

fn solve_time_of_flight(
    accel_inputs: &ConstantAccelInputs,
    dep_state: &StateVector,
    depart_et: f64,
    arr_target: &str,
) -> Option<Candidate> {
    let max_tof = propellant_time_limit(accel_inputs)
        .unwrap_or(MAX_SEARCH_TIME_S)
        .min(MAX_SEARCH_TIME_S);
    if !max_tof.is_finite() || max_tof <= MIN_TOF_S {
        return None;
    }

    let mut lower_tof = MIN_TOF_S;
    let lower_eval =
        evaluate_time_of_flight(accel_inputs, dep_state, depart_et, arr_target, lower_tof)?;
    if lower_eval.error_m >= -DISTANCE_TOLERANCE_M {
        let mut summary = lower_eval.summary;
        summary.total_distance_m = lower_eval.actual_distance_m;
        return Some(Candidate {
            depart_et,
            arrive_et: depart_et + lower_tof,
            summary,
        });
    }

    let mut upper_tof = (lower_tof * 2.0).min(max_tof);
    let mut upper_eval: Option<Evaluation> = None;

    while upper_tof <= max_tof {
        if let Some(eval) =
            evaluate_time_of_flight(accel_inputs, dep_state, depart_et, arr_target, upper_tof)
        {
            if eval.error_m >= 0.0 {
                upper_eval = Some(eval);
                break;
            }
            lower_tof = upper_tof;
            upper_tof = (upper_tof * 2.0).min(max_tof);
            if (upper_tof - lower_tof) < 1.0 {
                upper_tof = max_tof;
            }
        } else {
            return None;
        }
    }

    let mut upper_eval = if let Some(eval) = upper_eval {
        eval
    } else {
        if upper_tof < max_tof {
            upper_tof = max_tof;
        }
        let eval =
            evaluate_time_of_flight(accel_inputs, dep_state, depart_et, arr_target, upper_tof)?;
        if eval.error_m < 0.0 {
            return None;
        }
        eval
    };

    while upper_tof - lower_tof > 5.0 {
        let mid_tof = (lower_tof + upper_tof) * 0.5;
        if let Some(eval) =
            evaluate_time_of_flight(accel_inputs, dep_state, depart_et, arr_target, mid_tof)
        {
            if eval.error_m >= 0.0 {
                upper_tof = mid_tof;
                upper_eval = eval;
            } else {
                lower_tof = mid_tof;
            }
        } else {
            lower_tof = mid_tof;
        }
    }

    let mut summary = upper_eval.summary;
    summary.total_distance_m = upper_eval.actual_distance_m;
    Some(Candidate {
        depart_et,
        arrive_et: depart_et + upper_tof,
        summary,
    })
}

fn evaluate_time_of_flight(
    accel_inputs: &ConstantAccelInputs,
    dep_state: &StateVector,
    depart_et: f64,
    arr_target: &str,
    tof_s: f64,
) -> Option<Evaluation> {
    if !tof_s.is_finite() || tof_s <= 0.0 || tof_s > MAX_SEARCH_TIME_S {
        return None;
    }

    let summary = constant_accel_profile(accel_inputs, tof_s)?;
    let actual_distance = distance_to_target(dep_state, arr_target, depart_et, tof_s).ok()?;

    Some(Evaluation {
        error_m: summary.total_distance_m - actual_distance,
        summary,
        actual_distance_m: actual_distance,
    })
}

fn propellant_time_limit(inputs: &ConstantAccelInputs) -> Option<f64> {
    if inputs.dry_mass_kg <= 0.0 || inputs.initial_mass_kg <= inputs.dry_mass_kg {
        return None;
    }
    let ratio = inputs.dry_mass_kg / inputs.initial_mass_kg;
    if ratio <= 0.0 || ratio >= 1.0 {
        return None;
    }
    let limit = -(inputs.isp_seconds * G0 / inputs.acceleration_m_s2) * ratio.ln();
    if limit.is_finite() && limit > 0.0 {
        Some(limit)
    } else {
        None
    }
}

fn distance_to_target(
    dep_state: &StateVector,
    arr_target: &str,
    depart_et: f64,
    tof_s: f64,
) -> anyhow::Result<f64> {
    let arr_state =
        ephemeris::state_vector_et(arr_target, "SUN", "ECLIPJ2000", "NONE", depart_et + tof_s)?;
    Ok(euclidean_distance_m(
        &dep_state.position_km,
        &arr_state.position_km,
    ))
}

fn euclidean_distance_m(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt() * 1_000.0
}

fn is_better_candidate(candidate: &Candidate, best: &Candidate) -> bool {
    if candidate.summary.time_of_flight_s + 1e-6 < best.summary.time_of_flight_s {
        true
    } else if (candidate.summary.time_of_flight_s - best.summary.time_of_flight_s).abs() <= 1e-6 {
        candidate.summary.total_distance_m + 1.0 < best.summary.total_distance_m
    } else {
        false
    }
}

fn sample_window(start: f64, end: f64, step: f64) -> Vec<f64> {
    let mut samples = Vec::new();
    if end < start {
        return samples;
    }
    let step = step.max(1.0);
    let mut t = start;
    while t <= end + 1.0 {
        samples.push(t);
        t += step;
    }
    if let Some(&last) = samples.last() {
        if end - last > 1.0 {
            samples.push(end);
        }
    } else {
        samples.push(end);
    }
    samples
}
