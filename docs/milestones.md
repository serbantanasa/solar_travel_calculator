# Milestones

| Milestone | Status | Notes |
|-----------|--------|-------|
| M0 - Workspace skeleton | ✅ | Workspace crates established, docs/spec.md initialised. |
| M1 - SPICE ingestion | ✅ | Ephemeris loader + importer crate (`solar_importer`) in place. |
| M2 - Parking orbits & patched conics | ✅ | `solar_orbits`, `solar_impulsive` provide escape/capture and Lambert/Hohmann helpers. |
| M3 - Mission planner CLI | ✅ | CLI binaries moved under `crates/cli`; mission orchestrator in `solar_transfer`. |
| M4 - Config revamp | ✅ | Body/vehicle catalogs moved to TOML directories with kernel dependency inference. |
| M5 - Future planning/viz crates | ⏳ | Grid search/visualisation helpers to be introduced once implementations land. |
| M6 - High-thrust modelling | ⏳ | Decide whether to extend `solar_propulsion` or add dedicated crate. |
| M7 - Entry & landing arrival mode | 📝 | Model direct-entry/landing workflows in addition to parking-orbit circularisation. |
| M8 - Impulsive propellant tracking | 📝 | Apply the rocket equation after each impulsive burn so mission mass/prop usage stay consistent. |
