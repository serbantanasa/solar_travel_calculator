# Intra-Solar Transit Calculator — Design

This document captures the streamlined architecture vision for the Solar Travel Calculator.  
It focuses on modular crates, reproducible numerics, and a configuration-driven workflow so the CLI can stay thin while the libraries remain reusable in other binaries or services.

## 0) Objectives & Scope
- Compute and compare heliocentric transfer trajectories between arbitrary solar-system bodies.
- Cover impulsive (Lambert/Hohmann) and continuous-thrust (SEP, fusion-class) options under a shared configuration model.
- Produce machine-consumable outputs (JSON/CSV) plus artifacts that can drive external visualization tools.
- Maintain a documented, testable codebase with a living roadmap and CI coverage.

## 1) Workspace Architecture

```
intrasolar/
  crates/
    core/          # Units, math, time, frames, reusable numerics
    ephem_spice/   # SPICE kernel management & state sampling
    importer/      # Offline kernel import/download helpers
    orbits/        # Vector helpers, patched-conic escape/capture utilities
    propulsion/    # Engine & power models shared by solvers
    impulsive/     # Lambert solver + impulsive transfer estimators
    lowthrust/     # Continuous-thrust analytical helpers
    transfer/      # Mission orchestration facade (delegates to the crates above)
    config/        # Config parsing/validation, schema helpers
    export/        # JSON/CSV writers for downstream tooling
    cli/           # Thin binary crate using the libraries only
  data/spice/      # User-managed kernels, manifest metadata
  configs/
    bodies/        # Individual body TOML descriptors
    vehicles/      # Individual vehicle TOML descriptors
    runs/          # Scenario manifests
  docs/            # Living specification & milestone notes
  tests/           # Workspace-level integration tests
```

**Guiding principles**
- *Library first*: All logic lives under `crates/*`; binaries are orchestration only.
- *Composable*: Each crate exposes small, unit-tested functions that return typed results.
- *Deterministic*: Document tolerances, kernel sets, and solver settings so runs are reproducible.
- *Explicit units and frames*: No bare scalars—types encode meters, seconds, Newtons, frames, and timescales.

## 2) Core Concepts (`crates/core`)
- Types: `Epoch`, `Duration`, `Vector3`, `StateVector`, `Mass`, `Thrust`, `Isp`, `Frame`, `TimeScale`.
- Utilities: unit conversions, time-standard transforms (UTC↔TT↔TDB), interpolation helpers, numerical tolerances.
- Error taxonomy shared across crates (e.g., `EphemerisGap`, `InvalidConfig`, `InfeasibleTransfer`).

## 3) Ephemerides & Constants (`crates/ephem_spice`, `crates/importer`)
- SPICE kernel manifest loader: validates presence of SPK/TPC/PCK/LSK and their coverage windows.
- Sampling API: `state_of(target_id, epoch_tdb, frame) -> StateVector`.
- Caching/interpolation policies for repeated access inside grid searches.
- Helpers to down-select kernel sets (full vs “quick look”) without changing calling code.
- Importer CLI helper (`solar_importer`) downloads the default kernel catalog and can be reused by other tooling.

## 4) Time, Frames, Units
- Default dynamical frame: J2000 (ECLIPJ2000); provide transforms to body-fixed frames for parking orbits.
- All state epochs expressed in TDB; CLI accepts UTC and converts centrally.
- Scalar wrappers enforce SI units; conversions performed via explicit helper functions.

## 5) Configuration Model (`crates/config`)
- **Bodies (`configs/bodies/*.toml`)**: NAIF IDs, frame, default parking orbit, optional inertial start states.
- **Vehicles (`configs/vehicles/*.toml`)**: dry/prop mass, propulsion model, throttle limits, power scaling.
- **Runs (`configs/runs/*.toml`)**: origin/destination, vehicle, ephemeris manifest, window grids, policy hooks.
- Parser accepts directories of TOML files or legacy YAML and returns strongly typed structs with validation diagnostics (missing kernels, unsupported propulsion modes, etc.).

## 6) Orbits & Impulsive Planning
- Parking orbit builders convert named policies into inertial `StateVector`s at a given epoch.
- Patched-conic helpers compute escape/capture Δv from parking orbit given `v_inf`.
- Lambert solver (universal variables) supports prograde/retrograde and multi-rev branches.
- Hohmann planner provides near-circular quick looks and regression baselines.
- Porkchop sampler scans `(depart, tof)` grids, computing `Δv`, `C3`, `v_inf` budgets; exports raw grids plus valley annotations.

## 7) Continuous-Thrust Planning (`crates/lowthrust`)
- Mass-flow helpers: `mdot(thrust, isp)` and simple throttle envelopes bounded by power/acceleration limits.
- Current solver: 1D bang-bang integration aligned with the chord between departure and arrival states (forward Euler with gravity projection and mass depletion).
- Intended evolution: upgrade to higher-order integrators and allow scripted guidance/steering envelopes once physics modules mature.
- Outputs: time-stamped telemetry (position along chord, velocity, mass), propellant usage, peak speed, TOF.

## 8) Propulsion Models (`crates/propulsion`)
- Chemical impulsive engines (Isp/thrust pairs for patched conics).
- Solar-electric power-limited models (1/r² scaling, efficiency curves).
- High-Isp constant-thrust “futuristic” envelope reusing low-thrust propagation.
- Shared validation for throttle bounds, power availability, and mass budgets.

## 9) Search & Optimization (future)
- Grid samplers for porkchops and low-thrust feasibility sweeps will migrate into a dedicated crate once implementations land.
- Constraint evaluation helpers (max `v_inf`, propellant remaining, power draw).
- Optional local refiners (Nelder–Mead / coordinate search) over departure epoch and throttle schedules.

## 10) I/O & Visualization (`crates/export`)
- JSON schemas for trajectory series, porkchop grids, and low-thrust samples (see §13) emitted by `export`.
- CSV exporters for quick inspection and interoperability with Python notebooks.
- Visualization prep hooks will move into a dedicated crate once plotting utilities are factored out of the CLI/tests.

## 11) CLI (`crates/cli`)
- Entry point: `cargo run -p solar_cli --bin <command> [...]`.
- `fetch_spice`: download/import the default kernel catalog.
- `mission`: plan a point-to-point mission using the TOML catalogs.
- `porkchop`: produce impulsive transfer grids (CSV) and annotate Lambert branches.
- `porkchop_plot`: render contour heatmaps from porkchop CSV output.
- CLIs perform no business logic; they delegate to the library crates.

## 12) Testing Strategy
- **Unit tests** (crate-local): time conversions, SPICE sampling against reference values, orbit constructors, Lambert canonical cases, mass-flow invariants, optimizer constraints.
- **Property/regression tests**: ensure porkchop minima drift stays within tolerances, continuous-thrust integrator energy drift when thrust=0, power-scaling invariants.
- **Integration tests** (`tests/`): end-to-end Earth→Mars scenarios for both impulsive and SEP vehicles; fusion envelope quick run; JSON schema compliance.
- **Golden artifacts**: archive representative JSON outputs and assert numeric drift within documented tolerances (e.g., Δv within 0.1 %, arrival `v_inf` within 0.05 km/s).
- CI runs `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`, unit + integration tests, optional `criterion` benchmarks on demand.

## 13) Output Schemas (JSON)
- **Trajectory**: metadata (run name, kernel versions, frame/time scale) and per-entity state arrays with optional throttle/mass telemetry, plus summary metrics (TOF, Δv, prop usage, arrival `v_inf`).
- **Porkchop grid**: axes arrays (`depart_utc`, `tof_days`), nested cell metrics (`dv_kms`, `c3_km2s2`, `vinf_arr_kms`, feasibility flags) and optional valley picks.
- **Low-thrust map**: samples annotated with feasibility, propellant usage, arrival mismatch, heuristic score; can be filtered for plotting favorability maps.
- JSON writers include schema version tags so downstream tools can validate compatibility.

## 14) Documentation & Milestones
- `docs/spec.md`: living design (this document).
- `docs/milestones.md`: roadmap checkpoints—use “M0 skeleton” through “M8 polishing” structure with acceptance criteria and links to reference tests/artifacts.
- Each crate maintains a focused `README.md` summarizing scope, main APIs, and key tests.
- ADR-style notes captured in `docs/decisions/` for major trade-offs (e.g., integrator choices, kernel policy, optimizer selection).

## 15) Migration Notes (current repository → workspace)
- Break the existing monolithic crate into the workspace layout incrementally:
  1. Extract unit/epoch/math utilities into `crates/core`; re-export from the main crate temporarily.
  2. Move SPICE loading code into `ephem_spice`; update call sites.
  3. Introduce `transfer` (phase-A umbrella) while keeping public APIs stable via a top-level facade; split into `impulsive`, `lowthrust`, `propulsion`, and `orbits` once APIs solidify.
  4. Wire search policies into a dedicated crate (future), configs into `config`, and writers into `export`, then cut the CLI over to the new crates.
- Maintain compatibility layers during the transition (feature flag or re-exports) to avoid breaking existing tests/clients.

## 16) Extensibility
- Gravity assists: future `crates/flybys` adding patched-conic swing-by targeting and B-plane calculations.
- Plane-change budgeting: extend porkchops to 3-D grids with inclination penalties.
- Atmospheric capture: plug-in aerobrake/aerocapture estimators once atmospheric models are available.
- Uncertainty analysis: Monte-Carlo sampling wrappers for ephemeris and propulsion dispersions.
- GUI front-end: optional `egui`/`wgpu` viewer consuming the exported JSON without polluting solver crates.

## 17) References
1. Lambert's problem — https://en.wikipedia.org/wiki/Lambert%27s_problem  
2. Hohmann transfer orbit — https://en.wikipedia.org/wiki/Hohmann_transfer_orbit  
3. NAIF SPICE documentation — https://naif.jpl.nasa.gov  
4. Sims-Flanagan low-thrust transcription — NASA Technical Report (2001-210866)  
5. Project Rho fusion drive survey — https://www.projectrho.com/public_html/rocket/enginelist3.php
