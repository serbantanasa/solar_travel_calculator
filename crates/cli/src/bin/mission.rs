use clap::{Parser, ValueEnum};
use solar_travel_calculator::config::{PlanetConfig, load_planets, load_vehicle_configs};
use solar_travel_calculator::ephemeris;
use solar_travel_calculator::propulsion::{PropulsionMode, Vehicle as PropulsionVehicle};
use solar_travel_calculator::transfer::mission::porkchop::{
    WINDOW_DATASET_VERSION, WindowDataset, WindowError, WindowSuggestion, analyze_departure,
    compute_window_dataset, load_window_dataset, save_window_dataset,
};
use solar_travel_calculator::transfer::vehicle as transfer_vehicle;
use solar_travel_calculator::transfer::{
    AerobrakingOption, ArrivalConfig, DepartureConfig, InterplanetaryConfig, MissionConfig,
    plan_mission,
};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Mission planner CLI (continuous thrust ready)"
)]
struct Cli {
    /// Departure planet name (case-insensitive)
    #[arg(long)]
    from: String,

    /// Destination planet/moon name (case-insensitive)
    #[arg(long)]
    to: String,

    /// Departure epoch (TDB/UTC string accepted by SPICE)
    #[arg(long)]
    depart: String,

    /// Optional arrival epoch (defaults to depart + solver prediction)
    #[arg(long)]
    arrive: Option<String>,

    /// Vehicle name from catalogs (defaults to first continuous vehicle)
    #[arg(long)]
    vehicle: Option<String>,

    /// Aerobraking mode
    #[arg(long, value_enum, default_value_t = AerobrakeMode::None)]
    aerobrake: AerobrakeMode,

    /// Parking altitude at origin in km (defaults to catalog)
    #[arg(long)]
    origin_altitude: Option<f64>,

    /// Parking altitude at destination in km (defaults to catalog)
    #[arg(long)]
    dest_altitude: Option<f64>,

    /// Print coplanar circular Hohmann estimate (Δv, TOF)
    #[arg(long, default_value_t = false)]
    estimate_hohmann: bool,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
enum AerobrakeMode {
    None,
    Partial,
    Full,
}

const WINDOW_CACHE_DIR: &str = "data/windows";
const WINDOW_SPAN_DAYS: f64 = 3_650.0; // 10 years
const WINDOW_STEP_DAYS: f64 = 10.0;
const WINDOW_MIN_TOF_DAYS: f64 = 30.0;
const WINDOW_MAX_TOF_DAYS: f64 = 1_200.0;
const WINDOW_THRESHOLD_FACTOR: f64 = 1.4;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let planets = load_planets("configs/bodies")?;
    let vehicle_catalog = load_vehicle_configs("configs/vehicles")?;

    let origin = find_body(&planets, &cli.from)?;
    let destination = find_body(&planets, &cli.to)?;
    let vehicle = transfer_vehicle::select(&vehicle_catalog, cli.vehicle.as_deref())?;

    let origin_altitude_km = cli
        .origin_altitude
        .unwrap_or(origin.default_parking_altitude_km);
    let destination_altitude_km = cli
        .dest_altitude
        .unwrap_or(destination.default_parking_altitude_km);

    let departure_cfg = DepartureConfig {
        origin_body: origin.spice_name.clone(),
        parking_altitude_km: origin_altitude_km,
        departure_epoch: cli.depart.clone(),
        required_v_infinity: None,
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let cruise_cfg = InterplanetaryConfig {
        departure_body: origin.spice_name.clone(),
        destination_body: destination.spice_name.clone(),
        departure_epoch: cli.depart.clone(),
        arrival_epoch: cli.arrive.clone(),
        propulsion_mode: vehicle.propulsion.clone(),
    };

    let arrival_cfg = ArrivalConfig {
        destination_body: destination.spice_name.clone(),
        target_parking_altitude_km: destination_altitude_km,
        encounter_epoch: cli.arrive.clone().unwrap_or_else(|| cli.depart.clone()),
        propulsion_mode: vehicle.propulsion.clone(),
        aerobraking: Some(match cli.aerobrake {
            AerobrakeMode::None => AerobrakingOption::Disabled,
            AerobrakeMode::Partial => AerobrakingOption::Partial {
                periapsis_altitude_km: destination.default_parking_altitude_km,
            },
            AerobrakeMode::Full => AerobrakingOption::Full {
                periapsis_altitude_km: destination.default_parking_altitude_km,
            },
        }),
    };

    let mission_config = MissionConfig {
        vehicle: vehicle.clone(),
        origin: origin.clone(),
        destination: destination.clone(),
        departure: departure_cfg,
        cruise: cruise_cfg,
        arrival: arrival_cfg,
    };

    let profile = plan_mission(mission_config)?;

    let departure_et = ephemeris::epoch_seconds(&cli.depart)?;
    let arrival_et = if let Some(arrive) = &cli.arrive {
        ephemeris::epoch_seconds(arrive)?
    } else {
        departure_et + profile.cruise.time_of_flight_days * 86_400.0
    };
    let arrival_epoch_str = ephemeris::format_epoch(arrival_et)?;

    let duration_seconds = profile.cruise.time_of_flight_days * 86_400.0;
    let (d, h, m) = format_duration(duration_seconds);

    let depart_speed = vector_norm(&profile.cruise.departure_state.velocity_km_s);
    let arrive_speed = vector_norm(&profile.cruise.arrival_state.velocity_km_s);
    let peak_speed = profile
        .cruise
        .peak_speed_km_s
        .unwrap_or(depart_speed.max(arrive_speed));
    let percent_c = peak_speed / 299_792.458 * 100.0;

    println!("=== Mission Profile ===");
    println!("Departure epoch : {}", cli.depart);
    println!("Arrival epoch   : {}", arrival_epoch_str);
    println!(
        "Departure burn : Δv = {:.3} km/s, v_inf = {:.3} km/s",
        profile.departure.delta_v_required, profile.departure.hyperbolic_excess_km_s
    );
    println!(
        "Cruise         : TOF = {:.2} days ({}d {}h {}m), propellant used = {:.1} kg",
        profile.cruise.time_of_flight_days,
        d,
        h,
        m,
        profile.cruise.propellant_used_kg.unwrap_or(0.0)
    );
    println!(
        "Speeds         : start = {:.3} km/s, peak = {:.3} km/s ({:.6}% c), arrival = {:.3} km/s",
        depart_speed, peak_speed, percent_c, arrive_speed
    );
    println!(
        "Arrival burn   : Δv = {:.3} km/s",
        profile.arrival.delta_v_required
    );

    let total_dv_km_s = profile.departure.delta_v_required + profile.arrival.delta_v_required;
    let rpark_dep_km = origin.radius_km + origin_altitude_km;
    let rpark_arr_km = destination.radius_km + destination_altitude_km;
    if let Some(suggestion) = compute_window_suggestion(
        &planets,
        &origin,
        &destination,
        &vehicle,
        departure_et,
        total_dv_km_s,
        rpark_dep_km,
        rpark_arr_km,
    )? {
        print_window_suggestion(&suggestion, departure_et, &origin.name, &destination.name);
    }

    if cli.estimate_hohmann {
        use solar_travel_calculator::impulsive::transfers::hohmann;
        const MU_SUN: f64 = 1.327_124_400_18e11;
        let r1 = vector_norm(&profile.cruise.departure_state.position_km);
        let r2 = vector_norm(&profile.cruise.arrival_state.position_km);
        let h = hohmann(r1, r2, MU_SUN);
        println!(
            "Hohmann est.   : Δv_total = {:.3} km/s (dv1={:.3}, dv2={:.3}), TOF = {:.2} days",
            h.dv_total_km_s,
            h.dv1_km_s,
            h.dv2_km_s,
            h.tof_seconds / 86_400.0
        );
    }

    Ok(())
}

fn find_body<'a>(planets: &'a [PlanetConfig], name: &str) -> anyhow::Result<PlanetConfig> {
    let upper = name.to_uppercase();
    planets
        .iter()
        .find(|p| p.name.to_uppercase() == upper)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Planet/moon '{}' not found in catalog", name))
}

fn find_body_by_spice<'a>(planets: &'a [PlanetConfig], spice: &str) -> Option<PlanetConfig> {
    let upper = spice.to_uppercase();
    planets
        .iter()
        .find(|p| p.spice_name.to_uppercase() == upper)
        .cloned()
}

fn format_duration(seconds: f64) -> (i64, i64, i64) {
    let total_seconds = seconds.max(0.0);
    let days = (total_seconds / 86_400.0).floor() as i64;
    let remaining = total_seconds - (days as f64 * 86_400.0);
    let hours = (remaining / 3_600.0).floor() as i64;
    let minutes = ((remaining - hours as f64 * 3_600.0) / 60.0).floor() as i64;
    (days, hours, minutes)
}

fn vector_norm(v: &[f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn compute_window_suggestion(
    planets: &[PlanetConfig],
    origin: &PlanetConfig,
    destination: &PlanetConfig,
    vehicle: &PropulsionVehicle,
    departure_et: f64,
    total_dv_km_s: f64,
    rpark_dep_km: f64,
    rpark_arr_km: f64,
) -> anyhow::Result<Option<WindowSuggestion>> {
    if !matches!(vehicle.propulsion, PropulsionMode::Impulsive { .. }) {
        return Ok(None);
    }

    let origin_parent = origin
        .parent_spice
        .as_ref()
        .and_then(|spice| find_body_by_spice(planets, spice));
    let destination_parent = destination
        .parent_spice
        .as_ref()
        .and_then(|spice| find_body_by_spice(planets, spice));

    let cache_path = window_cache_path(origin, destination, departure_et);

    let dataset = if cache_path.exists() {
        match load_window_dataset(&cache_path) {
            Ok(dataset)
                if dataset.version == WINDOW_DATASET_VERSION
                    && departure_et >= dataset.depart_start_et
                    && departure_et <= dataset.depart_end_et =>
            {
                dataset
            }
            _ => compute_and_store_dataset(
                cache_path.as_path(),
                origin,
                origin_parent.as_ref(),
                destination,
                destination_parent.as_ref(),
                vehicle,
                rpark_dep_km,
                rpark_arr_km,
                departure_et,
            )?,
        }
    } else {
        compute_and_store_dataset(
            cache_path.as_path(),
            origin,
            origin_parent.as_ref(),
            destination,
            destination_parent.as_ref(),
            vehicle,
            rpark_dep_km,
            rpark_arr_km,
            departure_et,
        )?
    };

    let suggestion = analyze_departure(
        &dataset,
        departure_et,
        total_dv_km_s,
        WINDOW_THRESHOLD_FACTOR,
    );
    Ok(suggestion)
}

fn compute_and_store_dataset(
    path: &Path,
    origin: &PlanetConfig,
    origin_parent: Option<&PlanetConfig>,
    destination: &PlanetConfig,
    destination_parent: Option<&PlanetConfig>,
    vehicle: &PropulsionVehicle,
    rpark_dep_km: f64,
    rpark_arr_km: f64,
    depart_start_et: f64,
) -> Result<WindowDataset, WindowError> {
    let dataset = compute_window_dataset(
        origin,
        origin_parent,
        destination,
        destination_parent,
        vehicle,
        rpark_dep_km,
        rpark_arr_km,
        depart_start_et,
        WINDOW_SPAN_DAYS,
        WINDOW_STEP_DAYS,
        WINDOW_MIN_TOF_DAYS,
        WINDOW_MAX_TOF_DAYS,
    )?;
    save_window_dataset(path, &dataset)?;
    Ok(dataset)
}

fn print_window_suggestion(
    suggestion: &WindowSuggestion,
    departure_et: f64,
    origin_name: &str,
    destination_name: &str,
) {
    let ratio = suggestion.user_total_dv_km_s / suggestion.baseline.dv_total_km_s;
    let percent = (ratio - 1.0) * 100.0;
    println!(
        "Note: impulsive Δv_total {:.2} km/s is {:.0}% above the best {}→{} window (~{:.2} km/s).",
        suggestion.user_total_dv_km_s,
        percent,
        origin_name,
        destination_name,
        suggestion.baseline.dv_total_km_s
    );
    let delta_days = (suggestion.recommended.depart_et - departure_et) / 86_400.0;
    println!(
        "      Suggestion: depart {} ({}) and arrive {} with Δv_total ≈ {:.2} km/s.",
        suggestion.recommended.depart_utc,
        describe_offset(delta_days),
        suggestion.recommended.arrive_utc,
        suggestion.recommended.dv_total_km_s
    );
}

fn describe_offset(delta_days: f64) -> String {
    let rounded = delta_days.round();
    if rounded.abs() < 1.0 {
        "≈same day".to_string()
    } else if rounded > 0.0 {
        format!("≈{} days later", rounded as i64)
    } else {
        format!("≈{} days earlier", (-rounded) as i64)
    }
}

fn window_cache_path(
    origin: &PlanetConfig,
    destination: &PlanetConfig,
    departure_et: f64,
) -> PathBuf {
    let origin_part = sanitize_filename_component(&origin.spice_name);
    let destination_part = sanitize_filename_component(&destination.spice_name);
    let depart_tag = format!("et{}", departure_et.round() as i64);
    Path::new(WINDOW_CACHE_DIR).join(format!(
        "{origin_part}__{destination_part}__{depart_tag}.json"
    ))
}

fn sanitize_filename_component(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
