use clap::{Parser, ValueEnum};
use solar_travel_calculator::mission::arrival::AerobrakingOption;
use solar_travel_calculator::mission::departure::DepartureConfig;
use solar_travel_calculator::mission::interplanetary::InterplanetaryConfig;
use solar_travel_calculator::mission::{MissionConfig, plan_mission};
use solar_travel_calculator::scenario::{load_planets, load_vehicles};

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

    /// Vehicle name from scenarios (defaults to first continuous vehicle)
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
}

#[derive(Copy, Clone, ValueEnum, Debug)]
enum AerobrakeMode {
    None,
    Partial,
    Full,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let planets = load_planets("data/scenarios/planets.yaml")?;
    let vehicles = load_vehicles("data/scenarios/vehicles.yaml")?;

    let origin = find_body(&planets, &cli.from)?;
    let destination = find_body(&planets, &cli.to)?;
    let vehicle = choose_vehicle(&vehicles, cli.vehicle.as_deref())?;

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

    let arrival_cfg = solar_travel_calculator::mission::arrival::ArrivalConfig {
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

    println!("=== Mission Profile ===");
    println!(
        "Departure: Δv = {:.3} km/s, v_inf = {:.3} km/s",
        profile.departure.delta_v_required, profile.departure.hyperbolic_excess_km_s
    );
    println!(
        "Cruise: TOF = {:.1} days, propellant used = {:.1} kg",
        profile.cruise.time_of_flight_days,
        profile.cruise.propellant_used_kg.unwrap_or(0.0)
    );
    println!("Arrival: Δv = {:.3} km/s", profile.arrival.delta_v_required);

    Ok(())
}

fn find_body<'a>(
    planets: &'a [solar_travel_calculator::scenario::PlanetConfig],
    name: &str,
) -> anyhow::Result<solar_travel_calculator::scenario::PlanetConfig> {
    let upper = name.to_uppercase();
    planets
        .iter()
        .find(|p| p.name.to_uppercase() == upper)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Planet/moon '{}' not found in scenarios", name))
}

fn choose_vehicle(
    vehicles: &[solar_travel_calculator::mission::propulsion::Vehicle],
    requested: Option<&str>,
) -> anyhow::Result<solar_travel_calculator::mission::propulsion::Vehicle> {
    if let Some(name) = requested {
        let upper = name.to_uppercase();
        vehicles
            .iter()
            .find(|v| v.name.to_uppercase().contains(&upper))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Vehicle '{}' not found", name))
    } else {
        vehicles
            .iter()
            .find(|v| {
                matches!(
                    v.propulsion,
                    solar_travel_calculator::mission::propulsion::PropulsionMode::Continuous { .. }
                )
            })
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No continuous-thrust vehicle in scenarios"))
    }
}
