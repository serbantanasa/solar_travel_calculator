# config crate (planned)

Future home for configuration parsing, validation, and schema definitions.

## TODO
- Migrate the existing YAML/TOML loaders into this crate and align them with `solar_core` types.
- Emit helpful diagnostics when configs reference missing bodies, vehicles, or kernels.
- Provide JSON schema exports for runs, bodies, and vehicles so downstream tools can validate manifests.
