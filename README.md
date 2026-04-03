# RustyGene

AI-assisted genealogy engine built in Rust. Local-first, assertion-based data model with probabilistic confidence scoring, full GEDCOM 5.5.1 support, and a Tauri desktop app.

## Quick Start

```bash
# Build
cargo build --workspace

# Run tests
cargo test --workspace

# Import a GEDCOM file
cargo run -p rustygene-cli -- import --format gedcom testdata/gedcom/kennedy.ged

# Query persons
cargo run -p rustygene-cli -- query person --name "Kennedy"

# Export
cargo run -p rustygene-cli -- export --format gedcom --output out.ged
```

## Architecture

```
crates/core/       Pure domain model (assertions, entities, validation)
crates/storage/    SQLite persistence (CRUD, audit log, search index)
crates/gedcom/     GEDCOM 5.5.1 import/export pipeline
crates/api/        Axum REST API + OpenAPI spec
crates/cli/        Command-line interface
app/               Tauri 2.x desktop app (Svelte 5 + Cytoscape.js + D3.js)
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for details, [docs/INITIAL_SPEC.md](docs/INITIAL_SPEC.md) for the full technical specification.

## Status

- **Phase 1A** (Core + GEDCOM + CLI): Complete
- **Phase 2** (REST API + Desktop App): Complete
- **Phase 3** (Sandboxes + Agents): Not started

## License

MIT / Apache 2.0 dual license.
