# Solar Travel Calculator

This project explores brachistochrone-style optimal trajectories within the Solar System. The goal is to build a calculator that determines minimum-time transfer paths between planetary bodies while accounting for realistic mission constraints.

## Project Vision
- Model the classical brachistochrone problem for interplanetary travel.
- Incorporate solar gravity and planetary orbits to assess real mission feasibility.
- Provide an interactive interface for configuring origin, destination, and mission parameters.

## Early Objectives
1. Define the mathematical model and underlying assumptions.
2. Prototype numerical solvers for the governing equations.
3. Validate solutions against known optimal trajectories or benchmark datasets.

## Recent Highlights
- Aerobrake modelling now integrates drag through the atmosphere, reports peak loads, and optimises periapsis while respecting configurable dynamic-pressure and g-force limits.
- Mission CLI surfaces per-phase Δv totals so propulsive burns and aerothermal energy removal are easy to distinguish.
- Porkchop hints select the true low-energy valley beyond a requested departure date instead of echoing the same launch epoch.

## Example: Earth→Mars 2026 Window with Aerobrake Capture
```bash
cargo run -p solar_cli --bin mission -- \
  --from Earth --to Mars \
  --depart "2026-10-31T00:00:00" \
  --arrive "2027-09-07T00:00:00" \
  --vehicle "Starship V4 Concept" \
  --aerobrake full --estimate-hohmann
```
Output excerpt:
```
=== Mission Profile ===
Departure epoch : 2026-10-31T00:00:00
Arrival epoch   : 2027 SEP 07 00:00:00.000
Departure burn : Δv = 3.59 km/s, v_inf = 3.04 km/s
Cruise         : TOF = 311.00 days (311d 0h 0m), propellant used = 0.0 kg
Speeds         : start = 29.995 km/s, peak = 29.995 km/s (0.010005% c), arrival = 24.212 km/s
Arrival burn   : Δv = 1.39 km/s
Δv budget      : propulsive = 4.99 km/s, aerobrake = 4.29 km/s, total = 9.28 km/s
Aerobrake      : Δv_drag = 4.29 km/s, v_inf_post = 0.00 km/s
               : peak q = 9.70 kPa, peak accel = 38.35 m/s², periapsis = 38.7 km
Hohmann est.   : Δv_total = 5.66 km/s (dv1=2.98, dv2=2.68), TOF = 256.98 days
```

## Contributing
Development is just beginning—feel free to open issues or propose enhancements as the modeling and tooling take shape.

## Acknowledgements
- This project uses NASA's [Navigation and Ancillary Information Facility (NAIF) SPICE system](https://naif.jpl.nasa.gov/naif/) for ephemeris data; see the NAIF site for usage rules and credit guidance.

## License
Code in this repository is released under the [Unlicense](LICENSE); you may use it without restriction. NASA's SPICE toolkit and kernels retain their original terms—see [docs/LICENSING.md](docs/LICENSING.md) for details.
