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
    /// FK: Reference frame definitions for body-fixed frames.
    Fk,
}

impl KernelKind {
    /// Returns the lowercase kernel kind identifier used by SPICE utilities.
    pub fn kdata_kind(self) -> &'static str {
        match self {
            Self::Spk => "spk",
            Self::Lsk => "lsk",
            Self::Pck => "pck",
            Self::Fk => "fk",
        }
    }

    /// Returns a human-readable label for this kernel type.
    pub fn label(self) -> &'static str {
        match self {
            Self::Spk => "SPK (ephemeris)",
            Self::Lsk => "LSK (leap seconds)",
            Self::Pck => "PCK (planetary constants)",
            Self::Fk => "FK (reference frames)",
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
        filename: "jup365.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/jup365.bsp",
        kind: KernelKind::Spk,
        description: "Jupiter system satellites (Galilean moons and select inner moons) ephemeris (1965–2055).",
    },
    KernelDescriptor {
        filename: "sat455.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/sat455.bsp",
        kind: KernelKind::Spk,
        description: "Saturn system satellites ephemeris (major moons over modern epochs).",
    },
    KernelDescriptor {
        filename: "mar099.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/mar099.bsp",
        kind: KernelKind::Spk,
        description: "Mars satellites ephemeris (Phobos and Deimos).",
    },
    KernelDescriptor {
        filename: "plu060.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/plu060.bsp",
        kind: KernelKind::Spk,
        description: "Pluto system ephemeris (Pluto and Charon barycentric states).",
    },
    KernelDescriptor {
        filename: "nep095.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/nep095.bsp",
        kind: KernelKind::Spk,
        description: "Neptune system satellites ephemeris (includes Triton).",
    },
    KernelDescriptor {
        filename: "codes_300ast_20100725.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/asteroids/codes_300ast_20100725.bsp",
        kind: KernelKind::Spk,
        description: "Asteroid ephemeris covering the 300 largest main-belt bodies (Ceres, Vesta, Pallas, etc.).",
    },
    KernelDescriptor {
        filename: "codes_300ast_20100725.tf",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/asteroids/codes_300ast_20100725.tf",
        kind: KernelKind::Fk,
        description: "Reference frame definitions for the 300 largest asteroids (orientation metadata referenced by codes_300ast_20100725.bsp).",
    },
    KernelDescriptor {
        filename: "tnosat_v001_20000617_jpl082_20230601.bsp",
        url: "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/tno/tnosat_v001_20000617_jpl082_20230601.bsp",
        kind: KernelKind::Spk,
        description: "Trans-Neptunian object ephemeris (TNO centroids and satellites for Eris, Haumea, Makemake, etc.).",
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
