use clap::Parser;
use solar_travel_calculator::dynamics::lambert;
use solar_travel_calculator::ephemeris::{self, StateVector};
use solar_travel_calculator::scenario::{PlanetConfig, load_planets};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

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
    arrive_start: String,

    /// Arrival window end epoch (UTC/TDB string)
    #[arg(long)]
    arrive_end: String,

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
}

const MU_SUN: f64 = 1.327_124_400_18e11; // km^3 / s^2

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let planets = load_planets("data/scenarios/planets.yaml")?;

    let origin = find_body(&planets, &cli.from)?;
    let destination = find_body(&planets, &cli.to)?;

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
    let arr_start = ephemeris::epoch_seconds(&cli.arrive_start)?;
    let arr_end = ephemeris::epoch_seconds(&cli.arrive_end)?;
    let step_s = (cli.step_days.max(0.1)) * 86_400.0;

    let dep_body = ephemeris::normalize_heliocentric_target_name(&origin.spice_name);
    let arr_body = ephemeris::normalize_heliocentric_target_name(&destination.spice_name);

    let mut writer: Box<dyn Write> = if cli.output.as_os_str() == "-" {
        Box::new(BufWriter::new(std::io::stdout()))
    } else {
        if let Some(parent) = cli.output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        Box::new(BufWriter::new(File::create(&cli.output)?))
    };

    writeln!(
        writer,
        "depart_et,arrive_et,depart_utc,arrive_utc,tof_days,c3_km2_s2,vinf_dep_km_s,vinf_arr_km_s,dv_dep_km_s,dv_arr_km_s,dv_total_km_s,lambert_path,feasible,origin_body,dest_body,rpark_dep_km,rpark_arr_km"
    )?;

    let mut dt = dep_start;
    while dt <= dep_end + 1.0 {
        let dep_state = match fetch_state(&dep_body, dt) {
            Ok(s) => s,
            Err(_) => {
                dt += step_s;
                continue;
            }
        };
        let mut at = arr_start;
        while at <= arr_end + 1.0 {
            if at <= dt {
                at += step_s;
                continue;
            }
            let arr_state = match fetch_state(&arr_body, at) {
                Ok(s) => s,
                Err(_) => {
                    at += step_s;
                    continue;
                }
            };

            let tof = at - dt;
            let mut branch_results = Vec::new();

            if cli.long_path {
                if let Some(res) = evaluate_branch(
                    &origin,
                    &destination,
                    rpark_dep,
                    rpark_arr,
                    &dep_state,
                    &arr_state,
                    tof,
                    false,
                ) {
                    branch_results.push(res);
                }
            } else {
                if let Some(res) = evaluate_branch(
                    &origin,
                    &destination,
                    rpark_dep,
                    rpark_arr,
                    &dep_state,
                    &arr_state,
                    tof,
                    true,
                ) {
                    branch_results.push(res);
                }
                if let Some(res) = evaluate_branch(
                    &origin,
                    &destination,
                    rpark_dep,
                    rpark_arr,
                    &dep_state,
                    &arr_state,
                    tof,
                    false,
                ) {
                    branch_results.push(res);
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

            let depart_utc = ephemeris::format_epoch(dt).unwrap_or_else(|_| "".to_string());
            let arrive_utc = ephemeris::format_epoch(at).unwrap_or_else(|_| "".to_string());
            let tof_days = (at - dt) / 86_400.0;

            writeln!(
                writer,
                "{},{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{},{},{:.3},{:.3}",
                dt,
                at,
                depart_utc,
                arrive_utc,
                tof_days,
                best.c3,
                best.vinf_dep,
                best.vinf_arr,
                best.dv_dep,
                best.dv_arr,
                best.dv_total,
                path,
                if feasible { "true" } else { "false" },
                dep_body,
                arr_body,
                rpark_dep,
                rpark_arr,
            )?;

            at += step_s;
        }
        dt += step_s;
    }

    writer.flush()?;

    Ok(())
}

fn fetch_state(target: &str, et: f64) -> Result<StateVector, ephemeris::EphemerisError> {
    ephemeris::state_vector_et(target, "SUN", "ECLIPJ2000", "NONE", et)
}

fn find_body<'a>(planets: &'a [PlanetConfig], name: &str) -> anyhow::Result<PlanetConfig> {
    let upper = name.to_uppercase();
    planets
        .iter()
        .find(|p| p.name.to_uppercase() == upper)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Body '{}' not found in scenarios", name))
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
            path: "none",
        }
    }
}

fn evaluate_branch(
    origin: &PlanetConfig,
    destination: &PlanetConfig,
    rpark_dep: f64,
    rpark_arr: f64,
    dep_state: &StateVector,
    arr_state: &StateVector,
    tof: f64,
    short: bool,
) -> Option<BranchResult> {
    let lam = lambert::solve(
        dep_state.position_km,
        arr_state.position_km,
        tof,
        MU_SUN,
        short,
    )
    .ok()?;

    let (v1_lam, v2_lam) = lam;
    let vinf_dep = norm3(&[
        v1_lam[0] - dep_state.velocity_km_s[0],
        v1_lam[1] - dep_state.velocity_km_s[1],
        v1_lam[2] - dep_state.velocity_km_s[2],
    ]);
    let vinf_arr = norm3(&[
        v2_lam[0] - arr_state.velocity_km_s[0],
        v2_lam[1] - arr_state.velocity_km_s[1],
        v2_lam[2] - arr_state.velocity_km_s[2],
    ]);
    let c3 = vinf_dep * vinf_dep;

    let v_circ_dep = (origin.mu_km3_s2 / rpark_dep).sqrt();
    let v_esc = (vinf_dep * vinf_dep + 2.0 * origin.mu_km3_s2 / rpark_dep).sqrt();
    let dv_dep = (v_esc - v_circ_dep).max(0.0);

    let v_circ_arr = (destination.mu_km3_s2 / rpark_arr).sqrt();
    let v_cap = (vinf_arr * vinf_arr + 2.0 * destination.mu_km3_s2 / rpark_arr).sqrt();
    let dv_arr = (v_cap - v_circ_arr).max(0.0);
    let dv_total = dv_dep + dv_arr;

    Some(BranchResult {
        c3,
        vinf_dep,
        vinf_arr,
        dv_dep,
        dv_arr,
        dv_total,
        path: if short { "short" } else { "long" },
    })
}
