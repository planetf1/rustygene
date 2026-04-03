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

## Run the Application (New User Guide)

### Prerequisites

- Rust toolchain (stable)
- Node.js 20+ and npm
- Tauri system prerequisites for your OS (WebKit tooling on macOS/Linux)

### 1) Clone and install frontend dependencies

```bash
git clone https://github.com/planetf1/rustygene.git
cd rustygene
cd app
npm install
cd ..
```

### 2) Run the desktop UI (recommended)

This starts the Svelte dev server and launches the Tauri desktop app. The embedded Rust API is started by the app.

```bash
cd app
npm run tauri dev
```

### 3) Optional: run CLI-only workflows

```bash
# Import a GEDCOM file
cargo run -p rustygene-cli -- import --format gedcom testdata/gedcom/kennedy.ged

# Query persons
cargo run -p rustygene-cli -- query person --name "Kennedy"

# Export GEDCOM
cargo run -p rustygene-cli -- export --format gedcom --output out.ged
```

### Troubleshooting

- If `npm run tauri dev` fails with missing Tauri dependencies, install the OS prerequisites and retry.
- If Rust compilation fails, run `cargo build --workspace` once to surface dependency/toolchain issues.
- If UI dependencies are stale, run `cd app && npm install` again.

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
