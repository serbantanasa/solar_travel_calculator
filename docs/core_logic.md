# Core Logic Architecture and Mathematical Foundations

This document documents the full computational stack of **Solar Travel Calculator**.  It is intentionally exhaustive; the target reader is assumed to be comfortable with orbital mechanics, numerical integration, and the NAIF SPICE toolkit.

## 1. Coordinate Frames and State Vectors

All heliocentric calculations are performed in the J2000 mean ecliptic frame (`ECLIPJ2000`) using kilometres and seconds.

The SPICE helper `ephemeris::state_vector(target, observer, frame, aberration)` wraps `spkezr_c` and returns an inertial state `\mathbf{X} = [\mathbf{r},\mathbf{v}]` with position `\mathbf{r}` (km) and velocity `\mathbf{v}` (km/s).  Epoch strings in TDB or UTC are converted to ephemeris seconds via `ephemeris::epoch_seconds` using `str2et_c`.

Let `\mathbf{r}_1` and `\mathbf{v}_1` represent the heliocentric state of the origin body at launch epoch `t_1`, and `\mathbf{r}_2`, `\mathbf{v}_2` the state of the destination at encounter epoch `t_2`.

## 2. Scenario Catalogue

`scenario::load_planets` and `scenario::load_vehicles` deserialize YAML catalogues into `PlanetConfig` and `Vehicle`.  Each planet entry contains:

* Standard gravitational parameter `\mu` (km³/s²).
* Mean radius `R` (km).
* Sphere of influence radius `R_{SOI}` for patched conic transitions.
* Surface gravity `g_0` (m/s²) and mass `m` (kg) for launch/landing losses and for computing `\mu = Gm`.
* Atmospheric metadata (presence flag, scale height, surface density) for aerobraking models.

Vehicle definitions include dry/propellant mass and propulsion mode (continuous or impulsive), permitting heterogeneous mission legs.

## 3. Lambert Solver

We use the Bate-style universal variable solver from `lambert-bate` (MIT licence).  Given boundary vectors and time of flight `\Delta t`, the solver returns the departure and arrival velocity vectors `\mathbf{v}_1^{LAM}`, `\mathbf{v}_2^{LAM}` that satisfy Keplerian motion under the central body (the Sun for interplanetary legs).  Numerically we call:

```rust
lambert::solve(r1, r2, tof, MU_SUN, short)
```

where `short = true` selects the prograde short-path solution.  Failures due to collinearity or root-finding issues are mitigated with a `+1 km` perturbation fallback.

## 4. Mission Phasing

The mission module splits the transfer into three phases:

1. **Departure (`mission::departure`)** – compute departure burn from origin parking orbit into heliocentric transfer.
2. **Interplanetary (`mission::interplanetary`)** – propagate from SOI exit to destination SOI entry using either impulsive or continuous propulsion.
3. **Arrival (`mission::arrival`)** – capture from hyperbolic excess into destination parking orbit, optionally reducing delta-v via aerobraking.

`plan_mission` stitches the phases and returns a `MissionProfile` containing `DeparturePlan`, `InterplanetaryPlan`, `ArrivalPlan`.

### 4.1 Departure Phase Details

Inputs:

* Planet config `(\mu_{body}, R_{body}, R_{SOI}, g_0, m_{body})`.
* Parking radius `r_{park} = R_{body} + h_{park}`.
* Target hyperbolic excess velocity `||\mathbf{v}_\infty||` from Lambert solution.

Algorithm:

1. Compute circular velocity in the parking orbit:

   $$ v_{circ} = \sqrt{\frac{\mu_{body}}{r_{park}}} \; . $$

2. Determine Lambert departure velocity `\mathbf{v}_1^{LAM}` and planet heliocentric velocity `\mathbf{v}_1` at epoch `t_1`.

3. Hyperbolic excess vector relative to planet:

   $$ \mathbf{v}_\infty = \mathbf{v}_1^{LAM} - \mathbf{v}_1 . $$

4. Required burn magnitude to enter the hyperbolic escape:

   $$ v_{esc} = \sqrt{v_\infty^2 + \frac{2\mu_{body}}{r_{park}} }, \qquad \Delta v_{dep} = \max(0, v_{esc} - v_{circ}). $$

5. Propulsion-specific constraints (continuous burns, staging) are TODO items; currently the burn is treated as impulsive even for continuous-capable vehicles, but `v_\infty` is physically accurate and feeds the interplanetary leg.

### 4.2 Interplanetary Phase Details

We handle two propulsion regimes:

#### 4.2.1 Continuous-Thrust Integrator

Using a simple 1D steering model aligned with the displacement vector `\hat{d}` from `\mathbf{r}_1` to `\mathbf{r}_2`, we integrate translational motion with solar gravity and thrust acceleration.

Let the initial mass be `m_0`, dry mass `m_{dry}`, thrust `T`, specific impulse `I_{sp}`.  Mass flow is:

$$ \dot{m} = -\frac{T}{I_{sp} g_0}. $$

At each timestep `\Delta t`:

1. Determine thrust direction (accelerate until midpoint, then decelerate) and limit acceleration by user-specified `a_{max}` and by current mass `a_T = T / m`.
2. Compute solar gravity: `\mathbf{a}_g = -\frac{\mu_{\odot}}{r^3} \mathbf{r}` projected along `\hat{d}`.
3. Integrate velocity and displacement along `\hat{d}` using explicit Euler.  This is a simplification; replacing it with RK4 is on the roadmap, but even now we capture variable mass and the coupling between thrust and gravity.
4. Update mass `m(t+\Delta t) = m(t) + \dot{m} \Delta t`, clamped to `m_{dry}`.

Outputs:

* Time of flight `t_f` (days).
* Propellant consumed `m_0 - m(t_f)`.
* Boundary states reused for departure/arrival calculations.

#### 4.2.2 Impulsive Mode

Currently the impulsive case returns analytic placeholders (150-day TOF) until the Lambert-driven impulsive optimiser is implemented.  This is called out in the TODO list.

### 4.3 Arrival Phase Details

Arrival mirrors departure but with capture into parking orbit:

1. Determine Lambert arrival velocity `\mathbf{v}_2^{LAM}`, subtract destination heliocentric velocity `\mathbf{v}_2` to obtain `\mathbf{v}_\infty`.
2. Compute required burn to circularise:

   $$ v_{hyp} = \sqrt{v_\infty^2 + \frac{2\mu_{body}}{r_{park}} }, \qquad \Delta v_{cap} = \max(0, v_{hyp} - v_{circ}). $$

3. Aerobraking options scale `\Delta v_{cap}` (full = 10%, partial = 50%) as a placeholder for future thermal limits.

## 5. Tests

* **Lambert regression** (`tests/lambert.rs`) validates short-path solutions for quarter-orbit transfer.
* **Scenario loading** ensures full catalogs are deserialised and contain key metadata.
* **Phase tests** verify positive departure delta-v, aerobraking effectiveness, and that the continuous solver consumes propellant.
* **Mission integration** ensures end-to-end planning succeeds for default scenarios (Ion Tug between Earth and Mars).

## 6. Pending Work

* Replace Euler integration with higher-order schemes and include full 3D steering.
* Implement impulsive Lambert solver for time-of-flight optimisation.
* Model atmospheric losses and heating explicitly (using scale heights and densities from catalogs).
* Surface gravity and mass are now available to drive ascent/landing energy budgets.

