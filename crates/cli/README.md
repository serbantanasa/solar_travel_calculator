# cli crate (planned)

Placeholder for the thin command-line frontend once the workspace split is complete.

## TODO
- Move binaries under `src/bin/` into a dedicated crate that depends on the library stack.
- Provide modular subcommands that delegate to the new crates.
- Keep the crate free of business logic; focus on argument parsing and orchestration.
