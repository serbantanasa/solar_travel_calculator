# solar_core

Core units, constants, and foundational types shared across the Solar Travel Calculator workspace.

## Scope
- Define common physical constants (e.g., standard gravity) and numeric wrappers that other crates depend on.
- Provide shared helpers for units, time scales, and error taxonomy as they are carved out of the legacy crate.

## Status
The crate currently exposes a minimal constant set while the migration from the monolithic crate is underway. Future milestones (see `docs/spec.md`) will move unit-safe types and time/frames utilities here.
