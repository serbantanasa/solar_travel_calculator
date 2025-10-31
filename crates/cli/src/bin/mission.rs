use clap::{Parser, ValueEnum};
use solar_travel_calculator::config::{load_planets, load_vehicle_configs, PlanetConfig};
use solar_travel_calculator::ephemeris;
use solar_travel_calculator::transfer::vehicle as transfer_vehicle;
use solar_travel_calculator::transfer::{
    AerobrakingOption, ArrivalConfig, DepartureConfig, InterplanetaryConfig, MissionConfig,
    plan_mission,
};

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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let planets = load_planets("configs/bodies")?;
    let vehicle_catalog = load_vehicle_configs("configs/vehicles")?;

    let origin = find_body(&planets, &cli.from)?;
    let destination = find_body(&planets, &cli.to)?;
    let vehicle = transfer_vehicle::select(&vehicle_catalog, cli.vehicle.as_deref())?;

    let departure_cfg = DepartureConfig {
        origin_body: origin.spice_name.clone(),
        parking_altitude_km: cli
            .origin_altitude
            .unwrap_or(origin.default_parking_altitude_km),
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
        target_parking_altitude_km: cli
            .dest_altitude
            .unwrap_or(destination.default_parking_altitude_km),
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
        vehicle,
        origin,
        destination,
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
