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

## Contributing
Development is just beginning—feel free to open issues or propose enhancements as the modeling and tooling take shape.

## Acknowledgements
- This project uses NASA's [Navigation and Ancillary Information Facility (NAIF) SPICE system](https://naif.jpl.nasa.gov/naif/) for ephemeris data; see the NAIF site for usage rules and credit guidance.

## License
Code in this repository is released under the [Unlicense](LICENSE); you may use it without restriction. NASA's SPICE toolkit and kernels retain their original terms—see [docs/LICENSING.md](docs/LICENSING.md) for details.
