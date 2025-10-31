//! Core units, constants, and shared primitives for the Solar Travel Calculator workspace.

/// Physical constants expressed in SI units (unless stated otherwise).
pub mod constants {
    /// Standard gravity at Earth's surface (m/s²).
    pub const G0: f64 = 9.80665;
    /// Kilometres per astronomical unit.
    pub const AU_KM: f64 = 149_597_870.7;
    /// Seconds per Julian day.
    pub const SECONDS_PER_DAY: f64 = 86_400.0;
}

/// Basic unit conversion helpers.
pub mod units {
    /// Convert kilometres to metres.
    #[inline]
    pub fn km_to_m(v: f64) -> f64 {
        v * 1_000.0
    }

    /// Convert metres to kilometres.
    #[inline]
    pub fn m_to_km(v: f64) -> f64 {
        v / 1_000.0
    }

    /// Convert metres per second to kilometres per second.
    #[inline]
    pub fn ms_to_kms(v: f64) -> f64 {
        v / 1_000.0
    }

    /// Convert kilometres per second to metres per second.
    #[inline]
    pub fn kms_to_ms(v: f64) -> f64 {
        v * 1_000.0
    }
}

/// Lightweight time utilities shared across crates.
pub mod time {
    use super::constants::SECONDS_PER_DAY;

    /// Convert days to seconds.
    #[inline]
    pub fn days_to_seconds(days: f64) -> f64 {
        days * SECONDS_PER_DAY
    }

    /// Convert seconds to days.
    #[inline]
    pub fn seconds_to_days(seconds: f64) -> f64 {
        seconds / SECONDS_PER_DAY
    }
}

/// Minimal vector helpers to avoid ad-hoc `[f64; 3]` math everywhere.
pub mod vector {
    /// Alias for a 3D vector in kilometres or km/s depending on context.
    pub type Vector3 = [f64; 3];

    /// Euclidean norm of a vector.
    #[inline]
    pub fn norm(v: &Vector3) -> f64 {
        dot(v, v).sqrt()
    }

    /// Dot product of two vectors.
    #[inline]
    pub fn dot(a: &Vector3, b: &Vector3) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    /// Vector addition.
    #[inline]
    pub fn add(a: &Vector3, b: &Vector3) -> Vector3 {
        [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
    }

    /// Vector subtraction.
    #[inline]
    pub fn sub(a: &Vector3, b: &Vector3) -> Vector3 {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    /// Scale a vector by a scalar.
    #[inline]
    pub fn scale(v: &Vector3, s: f64) -> Vector3 {
        [v[0] * s, v[1] * s, v[2] * s]
    }
}
