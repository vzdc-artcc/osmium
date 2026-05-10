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
- user-managed API keys for machine clients

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
- Narrative API keys docs: `GET /docs/api/api-keys`

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
  - `ghcr.io/<owner>/<repo>:latest`
  - `ghcr.io/<owner>/<repo>:latest-<sha>`
- `develop` publishes:
  - `ghcr.io/<owner>/<repo>:dev`
  - `ghcr.io/<owner>/<repo>:dev-<sha>`

Recommended local validation before pushing:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets -- --test-threads=1
docker build -f Dockerfile .
```

## Migration Test Stack

The migration test stack provisions:

- a mock source Postgres database at `127.0.0.1:5433`
- a fresh Osmium target Postgres database at `127.0.0.1:5432`
- the Osmium API at `http://127.0.0.1:3000`
- an on-demand `db-migrator` container

`db-migrator` now migrates from the legacy Prisma/public prod dump into the new Osmium schema.

Place the source dump at `dev-data/mock-prod/prod.sql`. That directory is intentionally gitignored except for a tracked `.gitkeep`.

Start the stack with a full reset:

```bash
scripts/migration-test/up.sh
```

Stop and delete both Postgres volumes:

```bash
scripts/migration-test/down.sh
```

Run migration tool commands against the stack:

```bash
scripts/migration-test/migrator.sh plan
scripts/migration-test/migrator.sh migrate
scripts/migration-test/migrator.sh migrate --domain stats
scripts/migration-test/migrator.sh verify --domain stats
scripts/migration-test/migrator.sh verify
```

Run `plan` first as the preflight check for the legacy source dump. The supported startup flow always destroys and recreates both databases before seeding the mock prod database and starting Osmium.

Legacy controller stats are migrated into `stats.controller_monthly_rollups` for `environment = 'live'`. This backfills historical hours for the current stats API without synthesizing old controller sessions or activations, so `last_activity_at` may remain `null` for legacy-only controllers.

Detailed instructions: [scripts/migration-test/README.md](scripts/migration-test/README.md)

## Production Deployment

Production deployment now has a separate compose path for:

- steady-state `postgres` + `api`
- one-time legacy dump cutover with a temporary source DB and `db-migrator`

Quick start:

```bash
cp .env.cutover.example .env.cutover
cp .env.prod.example .env.prod
# edit both env files
# place the legacy dump at ${OSMIUM_DUMP_DIR}/prod.sql
docker compose --env-file .env.cutover -f docker-compose.prod.yml config
docker compose --env-file .env.cutover -f docker-compose.prod.yml --profile cutover config
scripts/prod/cutover.sh
scripts/prod/up.sh
```

Full step-by-step setup instructions: [docs/operations/production-deployment.md](docs/operations/production-deployment.md)

PRs against `master` and `develop` are enforced by `.github/workflows/ci.yml`.

## Read More

- Local development: [docs/getting-started/local-development.md](docs/getting-started/local-development.md)
- Configuration: [docs/getting-started/configuration.md](docs/getting-started/configuration.md)
- Architecture overview: [docs/architecture/overview.md](docs/architecture/overview.md)
- Testing: [docs/getting-started/testing.md](docs/getting-started/testing.md)
