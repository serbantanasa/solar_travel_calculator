# transfer crate (planned)

Temporary umbrella for impulsive, low-thrust, propulsion, and orbit utilities until the APIs are stable enough to split into their dedicated crates.

## TODO
- Re-export the existing monolithic mission/dynamics code so downstream crates can transition without churn.
- Provide a façade that exposes well-typed planners while the internal layout is refactored.
- Once stabilized, peel off dedicated `impulsive`, `lowthrust`, `propulsion`, and `orbits` crates and keep this crate as a compatibility shim (or retire it).
