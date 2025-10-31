# Core Logic Architecture and Mathematical Foundations

This document is a deep-dive into the math and implementation behind Solar Travel Calculator. It is written for a technical audience to enable formal validation of the algorithms, units, assumptions, and numerical behavior against the source code.

The content below mirrors the codebase and references concrete locations for each claim so future readers can confirm implementation parity.

## 0. Units, Symbols, Conventions

- Base units: kilometres (km), seconds (s), kilograms (kg). Velocities in km/s, accelerations in km/s^2 unless explicitly noted. Gravitational parameter μ in km^3/s^2.
- Time standards: user-supplied epochs are parsed by SPICE (`str2et_c`) into ephemeris seconds past J2000 (ET/TDB). Rendering to UTC uses `et2utc_c`.
- Frames: heliocentric state vectors use `ECLIPJ2000` (mean ecliptic of J2000). No aberration corrections are applied (`NONE`). See boundary state acquisition in `crates/transfer/src/mission/interplanetary.rs:77` and `crates/transfer/src/mission/interplanetary.rs:90`.
- Central body: the interplanetary legs are referenced to the Sun with the constant `MU_SUN = 1.32712440018e11 km^3/s^2` (see `crates/transfer/src/mission/departure.rs:10`, `crates/transfer/src/mission/arrival.rs:10`, `crates/transfer/src/mission/interplanetary/continuous.rs:7`).
- Vectors: bold for vectors (r, v), hats for unit vectors (r̂), subscripts 1/2 for departure/arrival boundary states.

## 1. Ephemerides and State Vectors

- SPICE kernel set: DE440 short SPK, NAIF leap seconds, PCK constants; see catalog in `crates/ephem_spice/src/kernels.rs:53`–`crates/ephem_spice/src/kernels.rs:73`. Local path is `data/spice/` (`crates/ephem_spice/src/kernels.rs:3`–`crates/ephem_spice/src/kernels.rs:4`).
- Kernel load/validation: paths validated then furnished to SPICE; error mode set to RETURN; see `crates/ephem_spice/src/lib.rs:154`–`crates/ephem_spice/src/lib.rs:168` and `crates/ephem_spice/src/lib.rs:203`–`crates/ephem_spice/src/lib.rs:213`.
- State query: `state_vector(target, observer, frame, ab, epoch)` wraps `spkezr_c` and returns `[r, v, lt]` (km, km/s, s) in the requested frame; see `crates/ephem_spice/src/lib.rs:77`–`crates/ephem_spice/src/lib.rs:119`.
- Epoch parsing/formatting: `epoch_seconds` and `format_epoch` use `str2et_c` and `et2utc_c`; see `crates/ephem_spice/src/lib.rs`.

Implementation invariant:
- Boundary states for cruise are sampled as
  r1,v1 = state_vector(departure_body,"SUN","ECLIPJ2000","NONE", t1)
  r2,v2 = state_vector(destination_body,"SUN","ECLIPJ2000","NONE", t2)
  with t2 = provided arrival epoch or t2 = t1 (placeholder) — see `crates/transfer/src/mission/interplanetary.rs`.

Note on barycenters: scenario catalogs mix body centers and barycenters (e.g., `EARTH` vs `MARS BARYCENTER`). SPICE resolves these consistently; the solver treats all boundary states as Sun-centered in the same inertial frame.

## 2. Scenario Catalogs and Vehicle Models

- Body catalog (TOML per body): `mu_km3_s2`, `radius_km`, `soi_radius_km`, `surface_gravity_m_s2`, `mass_kg`, and optional atmosphere descriptors; see `configs/bodies/`. Kernel dependencies are inferred automatically if omitted.
- Vehicles catalog (TOML per vehicle): `dry_mass_kg`, `propellant_mass_kg`, and propulsion mode parameters (impulsive or continuous); runtime conversion lives in `crates/transfer/src/facade.rs`.
- Vehicle helper: `initial_mass_kg = dry + propellant` (`crates/propulsion/src/lib.rs`).

## 3. Lambert Problem — Mathematics and Implementation

We solve the classical two-point boundary value problem under a Keplerian central potential to obtain velocities at r1 and r2 that achieve a specified time of flight Δt. The implementation uses the “Bate, Mueller & White” universal variable formulation via the `lambert_bate` crate.

3.1. Universal variable formulation
- Semi-parameter definitions:
  - c = ||r2 − r1||, s = (||r1|| + ||r2|| + c)/2, λ = √(1 − c/s)
  - z = α x^2 with α = 1/a for conic parameterization; Stumpff functions C(z), S(z) are
    C(z) = (1 − cos√z)/z for z>0; C(0)=1/2; C(z) = (cosh√−z − 1)/−z for z<0
    S(z) = (√z − sin√z)/z^(3/2) (z>0); S(0)=1/6; S(z) = (sinh√−z − √−z)/−z^(3/2) (z<0)
- Time of flight equation F(x) = 0 has the canonical form (one of several equivalent forms):
  Δt √μ = x^3 S(z) + λ x^2 C(z) + (1 − λ^2) x
  where z = α x^2 and the sign of λ encodes short/long path selection.
- Newton or Halley iterations solve for x with tolerance ε and iteration cap N.

3.2. Returned velocities
- Once x (or equivalently z) converges, Lagrange f, g functions yield
  v1 = (r2 − f r1)/g,   v2 = (ġ r2 − r1)/g
  where f = 1 − (x^2/||r1||) C(z), g = Δt − x^3/√μ S(z), and ġ analogous.

3.3. Code binding and tolerances
- Wrapper: `solve(r1, r2, tof, mu, short)` delegates to `lambert_bate::get_velocities(r1,r2,tof,mu,short, 1e-8, 500)` with tolerance 1e−8 and max 500 iterations; see `crates/impulsive/src/lambert.rs:10`–`crates/impulsive/src/lambert.rs:19`.
- Unit test sanity check: quarter-orbit transfer on a 1 AU circle reproduces the circular speed √(μ/a) to within 0.5 km/s, with tangential velocity directions verified; see `tests/lambert.rs:6`–`tests/lambert.rs:30`.

3.4. Numerical robustness
- The mission phases call the Lambert solver and fall back to a +1 km perturbation of r₂ if the solver fails, mitigating collinear/degenerate geometry (see `crates/transfer/src/mission/departure.rs` and `crates/transfer/src/mission/arrival.rs`).

## 4. Patched-Conic Mission Phasing

High-level orchestration: `plan_mission` constructs the interplanetary plan, then uses it to compute departure and arrival manoeuvres (see `crates/transfer/src/mission/mod.rs`).

### 4.1 Departure — Escape from Parking Orbit

Inputs
- Parking radius rpark = Rbody + hpark.
- Planetary GM μp and heliocentric planet state (r1, v1) at t1.
- Lambert departure velocity v1^LAM for the heliocentric transfer.

Equations (patched conics)
- Hyperbolic excess relative to the origin body:
  v∞ = v1^LAM − v1.
- Circular speed at rpark: vcirc = √(μp / rpark).
- Hyperbolic periapsis speed: vhyp = √(v∞^2 + 2 μp / rpark).
- Required impulsive burn (tangential approximation): Δvdep = max(0, vhyp − vcirc).

Implementation
- r₁,v₁ from SPICE; v₁^LAM from Lambert over [r₁,r₂,Δt] (`crates/transfer/src/mission/departure.rs`).
- v∞ magnitude computed from difference of `lambert_v1` and `planet_velocity` (`crates/transfer/src/mission/departure.rs`).
- v_circ and v_hyp from vis-viva identities; Δv reported in the departure plan (`crates/transfer/src/mission/departure.rs`).

Remarks
- Current implementation assumes a circular parking orbit (no argument of periapsis) and an impulsive escape aligned with local tangential direction; finite burn and steering losses are not yet modeled.
- If Lambert fails, `required_v_infinity` from config seeds v∞ (defaults to 0).

### 4.2 Interplanetary — Cruise Integration

Two regimes are supported by the API; the impulsive one is currently a placeholder while the continuous one performs a dynamical integration with simplified steering.

4.2.1 Continuous-thrust solver (bang-bang with gravity)

Physics
- Thrust T (N), specific impulse Isp (s) imply mass flow ṁ = −T/(Isp g₀). Here g₀ = 9.80665 m/s² (CODATA) — `crates/transfer/src/mission/interplanetary/continuous.rs`.
- Instantaneous thrust-limited acceleration is a_T = T/m (m/s²). A user/vehicle limit a_max may further cap acceleration (see same module).
- Converted to km/s² by division by 1000 in the solver.
- Solar gravity is modeled as a(r) = − μ⊙ r / ||r||³ (km/s²); projection along the line-of-flight direction d̂ uses a⋅d̂.

Steering and kinematics (1D along chord)
- The solver constructs the displacement vector Δr = r2 − r1 and its unit direction d̂. State evolved is scalar position x(t) along d̂ and scalar speed v(t) along d̂.
- Bang-bang thrust direction: +a until x reaches half the distance, then −a to brake.
- Integration is explicit forward Euler for v and x with fixed Δt determined from the constant-acceleration brachistochrone estimate: total time ≈ 2√(D/a) where D = ||Δr|| and a = a_max (both in km/s²).
- Mass is reduced linearly with time by ṁ, clamped at dry mass.

Outputs and metrics
- Time of flight: t = ∑ Δt over steps; converted to days before emitting the plan.
- Propellant used: min(m₀ − m_final, propellant_mass), non-negative.
- Peak speed along d̂ is tracked as an indicator of velocity scale; not a 3D magnitude.

Limitations (important for validation)
- 1D straight-line dynamics ignore the actual orbital motion of the destination during the transfer; the end states r1 and r2 are sampled at t1 and t2, but the integrated trajectory does not propagate 3D heliocentric motion.
- Euler integration is first order; accuracy depends on dt; the scheme uses at least 10,000 steps and targets ~10 s step size for long cruises but can still incur drift.
- Thrust always aligned with d̂; no lateral steering, no gravity-turn style guidance.
- Gravity is projected along d̂ only; orthogonal components are neglected.

4.2.2 Impulsive placeholder
- For non-continuous propulsion modes, the planner currently returns a fixed 150-day TOF with no propellant usage.

### 4.3 Arrival — Capture to Parking Orbit

Inputs and boundary conditions mirror departure, using Lambert arrival velocity v2^LAM and the destination’s state (r2, v2).

Equations
- v∞ = v2^LAM − v2.
- vcirc = √(μp / rpark), vhyp = √(v∞^2 + 2 μp / rpark).
- Δvcap = max(0, vhyp − vcirc).
- Aerobraking modifiers (placeholder): multiply Δv by 0.5 for partial and 0.1 for full (`crates/transfer/src/mission/arrival.rs`).

Implementation references
- Time of flight for Lambert constructed from either provided arrival epoch or the continuous-leg result.
- Lambert velocity calculation plus fallback and patched-conic capture inside `crates/transfer/src/mission/arrival.rs`.

## 5. Cross-Checks and Invariants

These statements should be true if the implementation matches the math:

- Frame and units consistency
  - SPICE `state_vector` returns km and km/s; the solver never mixes m and km without explicit division by 1000. Check thrust/acceleration conversion in `crates/transfer/src/mission/interplanetary/continuous.rs:63` and `crates/transfer/src/mission/interplanetary/continuous.rs:77`–`crates/transfer/src/mission/interplanetary/continuous.rs:79`.
  - All GM values are km^3/s^2 (scenario file and MU_SUN constants). Departure/arrival vis-viva use the same units.

- Lambert sanity
  - For circular 1 AU endpoints and Δt = π/2 √(a^3/μ), the magnitudes of v1, v2 are ≈ √(μ/a) and near-tangential; verified in `tests/lambert.rs:6`–`tests/lambert.rs:30`.
  - Perturbation fallback is triggered only if the universal solver fails to converge; the fallback should not alter well-posed cases.

- Departure/arrival Δv
  - If v∞ = 0, Δvdep = max(0, √(2 μp/rpark) − √(μp/rpark)) = (√2 − 1) √(μp/rpark). Code path follows this when `required_v_infinity`=0 and Lambert fails.
  - Aerobraking: enabling “Full” reduces Δvcap tenfold relative to purely propulsive capture (placeholder model).

- Continuous solver mass use
  - m(t) = m0 − (T/(Isp g0)) t, until mdry. Reported propellant usage ∈ [0, propellant_mass]. Verified in `tests/phases.rs:137`–`tests/phases.rs:147`.

## 6. Mathematical Derivations and Identities (Reference)

Vis-viva (two-body energy)
- v^2 = μ (2/r − 1/a). For hyperbolic escape or capture at radius r = rpark with hyperbolic excess v∞, the speed is v = √(v∞^2 + 2 μ/r). This identity underlies Δvdep and Δvcap.

Sphere-of-influence and patched conics
- The solver presently neglects SOI radii explicitly in computations, treating the departure/arrival burns at parking radii instantaneously; future work will apply v∞ at SOI boundary, then convert to parking orbit Δv with turning-angle constraints.

Classical brachistochrone in constant gravity vs. interplanetary case
- The constant-acceleration estimate T ≈ 2√(D/a) used to size the integrator step arises from a 1D bang-bang profile with no gravity; in heliocentric travel, solar gravity modifies the profile, but the estimate remains a practical upper bound for step sizing.

Universal variable/Stumpff details
- C(z) and S(z) are analytic continuations that avoid singularities across elliptic/parabolic/hyperbolic regimes; the universal variable approach solves a single scalar equation for time of flight across all regimes.

## 7. Tests and What They Prove

- `tests/ephemeris.rs`
  - Kernel presence and indexability (`kernel_summaries`); see `tests/ephemeris.rs:29`–`tests/ephemeris.rs:50`.
  - Earth heliocentric distance near 1 AU and speed near 30 km/s; light-time matches c within 1 s (`tests/ephemeris.rs:52`–`tests/ephemeris.rs:92`).

- `tests/phases.rs`
  - Departure Δv > 0 and v∞ > 0 for Earth→Mars example (`tests/phases.rs:84`–`tests/phases.rs:102`).
  - Aerobraking reduces capture Δv (`tests/phases.rs:104`–`tests/phases.rs:135`).
  - Continuous solver consumes propellant but not beyond available (`tests/phases.rs:137`–`tests/phases.rs:147`).

- `tests/mission.rs`
  - End-to-end planning runs and returns positive quantities with SPICE kernels installed; see `tests/mission.rs`.

## 8. Known Limitations and Roadmap (Physics Fidelity)

- Interplanetary continuous solver
  - Upgrade integrator from Euler to RK4 or higher-order adaptive schemes; introduce full 3D propagation of the spacecraft while the destination moves on its ephemeris.
  - Replace chord-based kinematics with integration in inertial coordinates, including lateral steering and targeting logic (shooting method) to meet moving boundary conditions.
  - Couple thrust vector to guidance laws (e.g., Edelbaum spirals, SEP steering) and support variable thrust/Isp.

- Impulsive solver
  - Implement an impulsive Lambert leg with TOF minimization subject to launch/arrival windows and plane-change costs; replace the 150-day placeholder.

- Planetary operations
  - Model ascent/landing losses, plane changes, and inclination/RAAN alignment in departure/arrival phases; support elliptical parking orbits and finite burn arcs.
  - Replace aerobraking scalars with atmospheric flight-path angle, density model (exponential with scale height), and heat load constraints.

- Frames and aberrations
  - Allow `J2000`/`ECLIPJ2000` selection and add light-time and stellar aberration corrections as toggles.

## 9. How To Independently Validate The Implementation

- Reproduce boundary states with SPICE Toolkit or NAIF WebGeocalc for the same epochs, frame, and observer.
- Solve Lambert externally (e.g., PyKEP, pykep.lambert_problem) with r1, r2, Δt and confirm v1/v2 match within numerical tolerance of `lambert_bate`.
- Compute v∞, vcirc, vhyp and Δv for departure/arrival using the equations in §4.1/§4.3 and confirm equality to reported values within floating-point roundoff.
- For the continuous solver, integrate a 1D bang-bang trajectory with gravity term using an independent script and compare TOF and propellant used. Expect agreement within Euler truncation error and mass-flow discretization.

## 10. CLI Outputs and Physical Interpretation

- The CLI (`crates/cli/src/bin/mission.rs`) reports:
  - Departure Δv and v∞ (km/s) from the patched-conic calculation.
  - Cruise TOF (days) and propellant used (kg) from the continuous solver.
  - Peak heliocentric speed estimate along the 1D line-of-flight (km/s) and its fraction of c; see reporting at `crates/cli/src/bin/mission.rs:65`–`crates/cli/src/bin/mission.rs:117`.
  - Arrival Δv with optional aerobraking factor.

These map directly to the quantities defined in §4 and §4.2.

## 11. Data Sources and Constants

- μ values for planets and bodies are supplied via scenarios and should match NAIF kernels to first order. Sun’s μ is a constant in code; any mismatch with SPICE’s internal μ only influences Lambert through μ, which is provided explicitly (μ⊙), not sourced from SPICE in our calls.
- g0 uses 9.80665 m/s^2 (standard gravity). Conversions to km/s^2 are explicit only where needed.

## 12. Glossary

- Δt: time of flight between boundary states.
- v∞: hyperbolic excess speed relative to a planet/moon at SOI.
- vcirc: circular orbit speed at a given radius.
- vhyp: speed on a hyperbolic trajectory at periapsis.
- Isp: specific impulse; T: thrust; ṁ: propellant mass flow rate.

## 13. Traceability Matrix (Code ↔ Math)

- Lambert solve inputs/outputs ↔ §3: `crates/impulsive/src/lambert.rs:10`–`crates/impulsive/src/lambert.rs:19`.
- Departure v∞, vcirc, vhyp, Δv ↔ §4.1: `crates/transfer/src/mission/departure.rs:58`–`crates/transfer/src/mission/departure.rs:119`.
- Arrival v∞, vcirc, vhyp, Δv ↔ §4.3: `crates/transfer/src/mission/arrival.rs:64`–`crates/transfer/src/mission/arrival.rs:132`.
- Continuous solver physics ↔ §4.2.1: `crates/transfer/src/mission/interplanetary/continuous.rs:6`–`crates/transfer/src/mission/interplanetary/continuous.rs:125`.
- Boundary state sampling ↔ §1: `crates/transfer/src/mission/interplanetary.rs:74`–`crates/transfer/src/mission/interplanetary.rs:98`.

End of document.
