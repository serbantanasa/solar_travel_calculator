# Solar Travel Calculator Specification

## 1. Mission and Scope
- Deliver an open-source Rust toolkit that computes minimum-time trajectories for travel within the Solar System under a variety of propulsion models.
- Begin with impulsive maneuvers (e.g., Lambert, Hohmann) as a correctness baseline, then extend toward continuous-thrust brachistochrone profiles inspired by the classical fastest-descent problem in gravity fields.[1]
- Target fast, local execution with zero external runtime dependencies so the app can be installed via `cargo install` or distributed as a standalone binary.

## 2. Primary Use Cases
- **Mission study**: Analysts compare travel times, delta-v budgets, and departure windows across multiple origin/destination pairs.
- **Vehicle concept validation**: Engineers evaluate whether a propulsion system (e.g., high-Isp fusion concepts) satisfies mission timelines and constraints.[5]
- **Education and outreach**: Students explore orbital mechanics numerically, with built-in scenario templates mirroring textbook examples and realistic benchmarks.
- **Science fiction plausibility checks**: Authors test the feasibility of storylines against hard-physics models, leveraging the same data the aerospace community uses.[4][5]

## 3. Physics and Modeling Requirements
### 3.1 Trajectory Classes
- **Two-impulse transfers**: Provide Lambert solver support, returning feasible transfer orbits for a given time of flight.[2]
- **Hohmann and bi-elliptic approximations**: Offer quick-look estimates for nearly-coplanar, circular orbits to validate solver output and enable regression tests.[3]
- **Low-thrust / brachistochrone-style paths**: Model thrust-limited trajectories using continuous integration of dynamics, with configurable thrust magnitude and mass flow; treat the classical brachistochrone as a limiting case for infinite thrust ratio.[1]
- **Patched-conic multi-leg itineraries**: Chain individual legs (planetary flybys or staging burns) with constraints on sphere-of-influence transitions.

### 3.2 Dynamics and Forces
- Start with two-body Keplerian dynamics for each leg; extend to include zonal harmonics (J2) or solar radiation pressure if needed for accuracy.
- Include gravitational parameters (μ) for Sun, planets, and selectable dwarf planets or moons to support missions beyond inner planets.[4]
- Define reference frames (heliocentric ecliptic, planet-centered inertial) and provide transformation utilities.

### 3.3 Constraints and Optimization
- Enforce thrust magnitude, propellant limits, allowable acceleration (crew g-load), and arrival/departure windows.
- Support objective functions: minimize time of flight, minimize propellant, or maximize arrival mass.
- Provide hooks for optimizers (gradient-free search, direct transcription) to iterate on departure dates and thrust profiles.

## 4. Data and Ephemerides
- **Ephemeris Kernels**: Use NASA NAIF SPICE kernels for high-precision state vectors; allow toggling between full kernels (e.g., DE440) and simplified analytic models for quick runs.[4]
- **Constants**: Bundle CODATA values for astronomical unit, gravitational constants, planetary radii, and rotation rates. Allow overrides via config files.
- **Scenario Definitions**: Store YAML/TOML templates describing mission endpoints, vehicle capabilities, and solver tolerances under `data/`.
- **Time Standards**: Support conversion between UTC, TDB, and TAI as required by SPICE and mission designs.

## 5. Software Architecture
- **Core Library (`src/lib.rs`)**
  - `astro::bodies`: Enumerations and data structs for celestial bodies, including GM, ephemeris identifiers, and frame metadata.
  - `astro::ephemeris`: Trait-based loader supporting SPICE binary kernels and analytic approximations.
  - `dynamics`: State propagation utilities (Keplerian propagation, Lambert solver wrapper, thrust integration).
  - `optimizer`: Abstractions for search algorithms (grid scan, particle swarm, direct collocation).
  - `mission`: High-level orchestration tying vehicle models, trajectories, constraints, and output products. Current scaffold sequences departure, interplanetary, and arrival legs so we can plug in impulsive or continuous propulsion models incrementally.
- **CLI (`src/main.rs` & `src/bin/`)**
  - Command groups: `plan` (single transfer), `scan` (window analysis), `simulate` (time-stepped propagation), `export` (trajectory to CSV/JSON).
  - Flags to inject alternative ephemeris sets, solver tolerances, and output formatting.
- **Data Layer (`data/`)**
  - Example mission configs (Earth→Mars Hohmann, Earth→Saturn brachistochrone).
  - Scenario catalogs for planets, moons, dwarf planets, and vehicle presets (including speculative drives).
  - Placeholder ephemeris data for offline development.
- **Testing (`tests/` and inline unit tests)**
  - Integration tests that execute CLI scenarios via `assert_cmd`.
  - Unit tests validating analytic solutions and invariants (energy conservation for Kepler problem).

## 6. External Dependencies (Initial)
- `nalgebra` or `ndarray` for vector math and matrices.
- `clap` for CLI parsing.
- `serde` + `serde_yaml`/`toml` for scenario serialization.
- `thiserror` for structured error handling.
- Optional: `argmin` for optimization routines, `approx` for tolerance-based float comparisons.

## 7. Testing and Validation Strategy
- **Analytic baselines**: Validate Lambert solver output against known Earth↔Mars Hohmann transfer times and delta-v (patched-conic solution).[3]
- **Regression fixtures**: Save reference trajectories (state vectors over time) derived from SPICE data and ensure solver matches within tolerance.
- **Property tests**: Assert time-reversal symmetry for Keplerian propagation and monotonic fuel consumption under thrust-limited integration.
- **Cross-source comparison**: Compare generated state vectors against NASA/JPL trajectory browser or GMAT outputs when available.
- **Continuous integration**: Run `cargo fmt`, `cargo clippy`, and `cargo test --all-features`; stage SPICE-dependent tests behind feature flags to keep CI lightweight.

## 8. Roadmap
1. **v0.1 — Impulsive Core (in progress)**
   - [x] Implement celestial body catalog (scenario YAML) and analytic helpers for SPICE access.
   - [x] Add Lambert solver and mission CLI (`cargo run --bin mission`).
   - [ ] Write regression tests for Earth↔Mars / Earth↔Venus impulsive (Lambert) transfers.
2. **v0.2 — SPICE Integration (partially complete)**
   - [x] Download baseline SPICE kernels and expose `state_vector`.
   - [x] Add ephemeris-driven integration tests.
   - [ ] Implement time conversions and window scanning utilities.
3. **v0.3 — Continuous-Thrust Prototype**
   - [x] Add thrust profile integrator (RK-based) with mass-flow coupling.
   - [ ] Validate continuous solutions against analytic brachistochrone cases.
   - [x] Integrate vehicle propulsion models (chemical, ion, nuclear) with mission phases.
4. **v0.4+ — Advanced Optimization**
   - [ ] Search over multi-leg itineraries and planetary flybys.
   - [ ] Expose user-defined objectives/constraints and possibly GUI/web frontends.

## 9. Open Questions and Risks
- **Ephemeris licensing and distribution**: Determine which SPICE kernels can be redistributed or whether the app should download them on demand.
- **Heat and power modeling for high-thrust concepts**: Decide how far to model vehicle subsystems when evaluating speculative drives (e.g., Epstein-class fusion engines).[5]
- **Performance vs. accuracy**: Balance precision of SPICE-driven propagation with the need for responsive, interactive calculations.
- **User extensibility**: Define plugin or scripting interfaces (e.g., Lua, Python) without bloating the Rust binary.
- **Validation data**: Identify authoritative datasets (e.g., NASA GMAT cases) to ground continuous-thrust solutions.

## 10. Prior Art Review
- The open-source calculators by jveigel provide quick brachistochrone estimates but rely on coarse orbital heuristics (e.g., linearized distance metrics and constant acceleration assumptions) that mis-represent true transfer requirements.[6] Use them only as qualitative inspiration; this project should ground its math in verifiable astrodynamics references and expose unit tests that fail for the oversimplifications observed there.

## References
1. Brachistochrone curve — Wikipedia. https://en.wikipedia.org/wiki/Brachistochrone_curve
2. Lambert's problem — Wikipedia. https://en.wikipedia.org/wiki/Lambert%27s_problem
3. Hohmann transfer orbit — Wikipedia. https://en.wikipedia.org/wiki/Hohmann_transfer_orbit
4. NAIF/JPL SPICE System Overview. https://naif.jpl.nasa.gov/naif/aboutspice.html
5. Project Rho, "The Expanse's Epstein Drive". https://www.projectrho.com/public_html/rocket/enginelist3.php#section_id--Fusion--(_Epstein_Drive_
6. jveigel, "brachistochrone-calculators" (GitHub repository). https://github.com/jveigel/brachistochrone-calculators
