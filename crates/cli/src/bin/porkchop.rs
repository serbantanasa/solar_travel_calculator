use anyhow::anyhow;
use clap::Parser;
use solar_travel_calculator::config::{PlanetConfig, load_planets, load_vehicle_configs};
use solar_travel_calculator::ephemeris;
use solar_travel_calculator::export::porkchop as export_porkchop;
use solar_travel_calculator::propulsion::{PropulsionMode, Vehicle};
use solar_travel_calculator::transfer::mission::porkchop::{
    self as porkchop_calc, PorkchopPath, PorkchopRequest, TimeWindow,
};
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

    let mut writer = export_porkchop::writer_for_path(&cli.output)?;
    export_porkchop::write_header(writer.as_mut())?;

    let departure_window = TimeWindow {
        start_et: dep_start,
        end_et: dep_end,
        step_seconds: step_s,
    };
    let arrival_window = TimeWindow {
        start_et: arr_start,
        end_et: arr_end,
        step_seconds: step_s,
    };

    let request = PorkchopRequest {
        origin_body: &origin,
        origin_parent: origin_parent.as_ref(),
        destination_body: &destination,
        destination_parent: destination_parent.as_ref(),
        vehicle: &vehicle,
        rpark_depart_km: rpark_dep,
        rpark_arrive_km: rpark_arr,
        departure_window,
        arrival_window,
        long_path_only: cli.long_path,
        ignore_vehicle_limits: false,
    };

    let points = porkchop_calc::generate(&request)?;

    for point in points {
        let path_str = match point.lambert_path {
            PorkchopPath::Short => "short",
            PorkchopPath::Long => "long",
            PorkchopPath::None => {
                if cli.long_path {
                    "long"
                } else {
                    "none"
                }
            }
        };

        let record = export_porkchop::Record {
            depart_et: point.depart_et,
            arrive_et: point.arrive_et,
            depart_utc: &point.depart_utc,
            arrive_utc: &point.arrive_utc,
            tof_days: point.tof_days,
            c3: point.c3_km2_s2,
            vinf_dep: point.vinf_depart_km_s,
            vinf_arr: point.vinf_arrive_km_s,
            dv_dep: point.dv_depart_km_s,
            dv_arr: point.dv_arrive_km_s,
            dv_total: point.dv_total_km_s,
            propellant_used_kg: point.propellant_used_kg,
            burn_time_s: point.burn_time_s,
            final_mass_kg: point.final_mass_kg,
            path: path_str,
            feasible: point.feasible,
            origin_body: origin.spice_name.as_str(),
            dest_body: destination.spice_name.as_str(),
            rpark_dep_km: rpark_dep,
            rpark_arr_km: rpark_arr,
        };
        record.write_to(writer.as_mut())?;
    }

    writer.flush()?;

    Ok(())
}

fn find_body<'a>(planets: &'a [PlanetConfig], name: &str) -> anyhow::Result<PlanetConfig> {
    let upper = name.to_uppercase();
    planets
        .iter()
        .find(|p| p.name.to_uppercase() == upper)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Body '{}' not found in catalog", name))
}
