//! Propulsion mode descriptors and vehicle mass properties.

/// Simple propulsion mode enumeration. Additional parameters can be layered on per mode.
#[derive(Debug, Clone)]
pub enum PropulsionMode {
    /// Instantaneous impulsive burn (e.g., chemical engine, upper stage).
    Impulsive {
        max_delta_v_km_s: f64,
        isp_seconds: f64,
        max_thrust_newtons: Option<f64>,
    },
    /// Continuous thrust with bounded acceleration and specific impulse.
    Continuous {
        max_thrust_newtons: f64,
        isp_seconds: f64,
        max_acceleration_m_s2: Option<f64>,
    },
    /// Hybrid strategies (placeholder for future modelling).
    Hybrid,
}

/// Basic vehicle definition used to check feasibility across mission legs.
#[derive(Debug, Clone)]
pub struct Vehicle {
    pub name: String,
    pub dry_mass_kg: f64,
    pub propellant_mass_kg: f64,
    pub propulsion: PropulsionMode,
    pub aero: Option<VehicleAero>,
}

impl Vehicle {
    /// Convenience accessor for total initial mass.
    pub fn initial_mass_kg(&self) -> f64 {
        self.dry_mass_kg + self.propellant_mass_kg
    }

    /// Reference entry mass to use for aerobraking when available.
    pub fn reference_entry_mass_kg(&self) -> f64 {
        self.aero
            .as_ref()
            .and_then(|a| a.entry_mass_ref_kg)
            .unwrap_or_else(|| self.initial_mass_kg())
    }
}

/// Aerodynamic characteristics relevant for atmospheric entry.
#[derive(Debug, Clone)]
pub struct VehicleAero {
    pub attitude: Option<String>,
    pub cd_ref: f64,
    pub ref_area_m2: f64,
    pub ref_diameter_m: Option<f64>,
    pub ballistic_coefficient_kg_m2: Option<f64>,
    pub entry_mass_ref_kg: Option<f64>,
    pub lift_to_drag: Option<f64>,
}

impl VehicleAero {
    /// Compute the ballistic coefficient for a given mass.
    pub fn ballistic_coefficient(&self, mass_kg: f64) -> Option<f64> {
        if let Some(beta) = self.ballistic_coefficient_kg_m2 {
            Some(beta * mass_kg / self.entry_mass_ref_kg.unwrap_or(mass_kg))
        } else if self.cd_ref > 0.0 && self.ref_area_m2 > 0.0 {
            Some(mass_kg / (self.cd_ref * self.ref_area_m2))
        } else {
            None
        }
    }
}
