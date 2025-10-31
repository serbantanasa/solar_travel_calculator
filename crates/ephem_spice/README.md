# solar_ephem_spice

SPICE kernel management and ephemeris sampling utilities for the Solar Travel Calculator workspace.

## Scope
- Validate and load NAIF kernel sets used throughout the project.
- Provide typed accessors for state vectors, ephemeris-time formatting, and kernel metadata.
- Centralise SPICE error handling so higher-level crates can rely on consistent failure modes.

## Status
This crate is the first extraction from the legacy monolith. Additional helpers (coverage queries, interpolation policies) will land here as described in `docs/spec.md`.
