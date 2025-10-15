//! Propulsion and vehicle capability descriptors shared across mission phases.

/// Simple propulsion mode enumeration. Additional parameters can be layered on per mode.
#[derive(Debug, Clone)]
pub enum PropulsionMode {
    /// Instantaneous impulsive burn (e.g., chemical engine, upper stage).
    Impulsive { max_delta_v_km_s: f64 },
    /// Continuous thrust with bounded acceleration and specific impulse.
    Continuous {
        max_thrust_newtons: f64,
        isp_seconds: f64,
        max_acceleration_m_s2: Option<f64>,
    },
    /// Hybrid strategies (e.g., impulsive departure, continuous cruise). Placeholder for future modelling.
    Hybrid,
}

/// Basic vehicle definition used to check feasibility across mission legs.
#[derive(Debug, Clone)]
pub struct Vehicle {
    pub name: String,
    pub dry_mass_kg: f64,
    pub propellant_mass_kg: f64,
    pub propulsion: PropulsionMode,
}

impl Vehicle {
    /// Convenience accessor for total initial mass.
    pub fn initial_mass_kg(&self) -> f64 {
        self.dry_mass_kg + self.propellant_mass_kg
    }
}
