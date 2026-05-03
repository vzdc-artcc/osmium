# Osmium

Osmium is the shared backend and API platform for vZDC applications, bots, publications/downloads, files, training workflows, events, feedback, and statistics.

## What It Owns

- user identity and sessions
- access control and permissions
- roster and controller-state data
- training assignments and requests
- event staffing and lifecycle data
- publication categories and download metadata
- controller feedback
- files, metadata, and CDN delivery
- statistics and sync status
- service-account auth for internal machine clients

## Quick Local Start

```bash
cp .env.example .env
docker compose up -d postgres
cargo run
```

To bootstrap the singleton server admin in Docker, set `OSMIUM_SERVER_ADMIN_CID` before the matching user logs in:

```bash
OSMIUM_SERVER_ADMIN_CID=1234567
docker compose up -d
```

Then log in as CID `1234567` and verify `GET /api/v1/me` returns `role: "SERVER_ADMIN"` with the full grouped permission set.

If you previously ran the pre-reset schema and get `VersionMissing(20260329120000)` or a similar startup migration error, reset the old dev volume:

```bash
docker compose down -v
docker compose up -d postgres
```

Verify:

```bash
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/docs/health
```

## Local VATSIM OAuth

The example env is configured for VATSIM dev hosts by default.

Important local rules:

- use `http://127.0.0.1:3000` consistently
- do not mix `localhost` and `127.0.0.1`
- keep `COOKIE_SECURE=false` on plain local HTTP
- use `VATSIM_CLIENT_AUTH_METHOD=post` with `auth-dev.vatsim.net`

If OAuth callback fails with `oauth callback missing state cookie`, the browser usually started login on a different origin than the configured `VATSIM_REDIRECT_URI`.

## Docs Entry Points

- Docs home: `GET /docs`
- Interactive API reference: `GET /docs/api/v1`
- OpenAPI JSON: `GET /docs/api/v1/openapi.json`

## High-Level Architecture

- Axum API application
- shared auth middleware for users and service accounts
- Postgres with multiple schemas by domain
- repo-backed query layer
- markdown docs served by the app
- generated OpenAPI for route reference

## Common Commands

```bash
docker compose up -d postgres
cargo fmt
cargo test
cargo run
```

## Git and Image Flow

- `master` and `develop` both auto-publish Docker images on push
- `master` publishes:
  - `ghcr.io/<owner>/<repo>:master`
  - `ghcr.io/<owner>/<repo>:master-<sha>`
- `develop` publishes:
  - `ghcr.io/<owner>/<repo>:develop`
  - `ghcr.io/<owner>/<repo>:develop-<sha>`

Recommended local validation before pushing:

```bash
cargo fmt --all -- --check
cargo check
cargo test --all-targets
docker build -f Dockerfile .
```

## Read More

- Local development: [docs/getting-started/local-development.md](docs/getting-started/local-development.md)
- Configuration: [docs/getting-started/configuration.md](docs/getting-started/configuration.md)
- Architecture overview: [docs/architecture/overview.md](docs/architecture/overview.md)
- Platform architecture plan: [docs/platform-architecture-plan.md](docs/platform-architecture-plan.md)
