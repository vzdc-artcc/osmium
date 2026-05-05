# API Load Tester

Standalone Rust tool for timing Osmium API routes, running burst/load tests, and exercising realistic multi-user flows without becoming part of the main application build or Docker image.

## Why It Lives Here

This crate is kept under `tools/api-load-tester/` and is not referenced by the root `Cargo.toml` or main Dockerfile. The production image currently copies only the backend app sources, so this tool stays out of the runtime build.

## Prerequisites

- Osmium API running locally or in a non-prod environment
- For local realistic persona flows:
  - `API_DEV_MODE=true` or `VATSIM_DEV_MODE=true`
  - working database
- For local seeded scenarios:
  - `/api/v1/dev/seed` available

## Default Local Usage

```bash
cargo run --manifest-path tools/api-load-tester/Cargo.toml -- discover
cargo run --manifest-path tools/api-load-tester/Cargo.toml -- run
cargo run --manifest-path tools/api-load-tester/Cargo.toml -- sweep --include-tags events,auth
cargo run --manifest-path tools/api-load-tester/Cargo.toml -- load --burst-requests 300 --burst-concurrency 25
```

The default base URL is `http://127.0.0.1:3000`.

## Auth Modes

- `dev-login`: uses seeded dev personas and `/api/v1/auth/login/as/{cid}`
- `bearer`: uses bearer tokens from environment variables
- `hybrid`: tries dev-login first, then bearer fallback per persona

Supported bearer env vars:

- `API_LOAD_BEARER_STAFF`
- `API_LOAD_BEARER_STUDENT`
- `API_LOAD_BEARER_TRAINER`
- `API_LOAD_BEARER_ADMIN`

## Safety Model

The tool skips unsafe or unsupported mutations by default. If the target is not a local dev URL, write-heavy scenario and mutation routes require `--unsafe-mutations`.

## Reports

Each run writes a JSON report under `tools/api-load-tester/reports/` by default, unless `--json-out` is provided.
