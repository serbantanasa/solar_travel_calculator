use std::path::{Path, PathBuf};

/// Location where the project's helper expects SPICE kernels to live.
pub const LOCAL_SPICE_DIR: &str = "data/spice";

/// Enumerates the SPICE kernel families we currently ship helpers for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelKind {
    /// SPK: Solar System ephemerides (positions and velocities).
    Spk,
    /// LSK: Leap seconds kernel (UTC ↔ TDB/ET conversions).
    Lsk,
    /// PCK: Planetary constants kernel (body orientation, radii, etc.).
    Pck,
}

impl KernelKind {
    /// Returns the lowercase kernel kind identifier used by SPICE utilities.
    pub fn kdata_kind(self) -> &'static str {
        match self {
            Self::Spk => "spk",
            Self::Lsk => "lsk",
            Self::Pck => "pck",
        }
    }

    /// Returns a human-readable label for this kernel type.
    pub fn label(self) -> &'static str {
        match self {
            Self::Spk => "SPK (ephemeris)",
            Self::Lsk => "LSK (leap seconds)",
            Self::Pck => "PCK (planetary constants)",
        }
    }
}

/// Metadata describing a SPICE kernel we expect to manage.
#[derive(Debug, Clone, Copy)]
pub struct KernelDescriptor {
    pub filename: &'static str,
    pub url: &'static str,
    pub kind: KernelKind,
    pub description: &'static str,
}

impl KernelDescriptor {
    /// Returns the on-disk path where the kernel should reside.
    pub fn local_path(self) -> PathBuf {
        Path::new(LOCAL_SPICE_DIR).join(self.filename)
    }
}

/// Canonical kernel set used to bootstrap the calculator.
pub const KERNEL_CATALOG: &[KernelDescriptor] = &[
    KernelDescriptor {
        filename: "de440s.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/de440s.bsp",
        kind: KernelKind::Spk,
        description: "JPL DE440 short ephemeris: barycentric positions/velocities for Sun, planets, and Pluto (1550–2650).",
    },
    KernelDescriptor {
        filename: "naif0012.tls",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/lsk/naif0012.tls",
        kind: KernelKind::Lsk,
        description: "NAIF leap seconds kernel: UTC↔TDB conversion table with historical and predicted leap seconds.",
    },
    KernelDescriptor {
        filename: "pck00011.tpc",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/pck00011.tpc",
        kind: KernelKind::Pck,
        description: "Planetary constants kernel: body orientation models, radii, and physical constants for the Sun, planets, and select moons.",
    },
];
