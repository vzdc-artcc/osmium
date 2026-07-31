# Osmium

Osmium is the shared backend and API platform for vZDC applications. It is the
authority for identity, authorization, and every operational domain the ARTCC
runs on — roster and controller state, training, events, feedback,
publications, files, statistics, and the integrations that tie them to VATSIM,
VATUSA, Discord, and email.

It is a single [Axum](https://github.com/tokio-rs/axum) (Rust 2024) service
backed by Postgres, exposing a versioned REST API under `/api/v1`, self-hosted
documentation under `/docs`, and a set of background workers that keep external
data in sync.

## What It Owns

**Identity & access**

- VATSIM OAuth login, sessions, and the current-user profile (`/me`)
- explicit, path-based permissions and role assignments (nothing is implied by
  role name — every capability is a granted permission)
- service accounts for internal machine clients and user-managed API keys
- staff impersonation with self-service write blocking

**Controllers & roster**

- roster and controller-state data, synced from VATUSA
- certifications and progression tracking
- LOAs, solo certifications, and their expiration lifecycles
- display-only staff-position roster tags (never grant permissions)

**Operations**

- training assignments, requests, sessions, and appointment scheduling
- event staffing, positions, and the event lifecycle (reminders, promotion)
- controller feedback and incident reports
- ATC bookings (proxied to VATSIM)
- publication categories and download metadata
- files, metadata, and signed CDN delivery

**Platform**

- statistics and per-environment connection sync
- transactional email (SES) with per-user, per-category preferences
- Discord account linking and bot configuration/feature flags
- FAA preferred-route reference data
- GDPR data export
- broadcasts, welcome messages, and CAPTCHA verification

## API Domains

The versioned API is grouped into these tags (see the interactive reference at
`/docs/api/v1`):

`auth` · `users` · `training` · `events` · `feedback` · `incidents` ·
`stats` · `publications` · `files` · `routes` · `emails` · `broadcasts` ·
`welcome-messages` · `integrations` · `data-export` · `api-keys` ·
`captcha` · `workflows` · `admin`

Per-route permission requirements are listed in
[docs/route-permissions.md](docs/route-permissions.md); each domain has a
narrative page under [docs/api/](docs/api/).

## Quick Local Start

```bash
cp .env.example .env
docker compose up -d postgres
cargo run
```

The example env targets VATSIM dev hosts and binds `127.0.0.1:3000` by default.
Migrations are embedded in the binary and run on startup, so a fresh Postgres is
provisioned to the latest schema automatically.

Verify:

```bash
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/docs/health
```

### Server admins

`OSMIUM_SERVER_ADMIN_CID` bootstraps one or more server admins and is the **only**
way to grant `SERVER_ADMIN`. It accepts a comma-separated list of CIDs; the role
is reconciled idempotently on every login for those users.

```bash
OSMIUM_SERVER_ADMIN_CID=1234567,2345678
docker compose up -d
```

Then log in as one of those CIDs and confirm `GET /api/v1/me` returns
`role: "SERVER_ADMIN"` with the full grouped permission set.

### Dev seeding

Set `DEV_SEED_ENABLED=true` to expose `POST /api/v1/dev/seed`, which builds a
full sample world (a ~24-controller roster with certifications, events, training
sessions, and feedback). It is idempotent and intended for local development
only.

### Resetting a stale dev volume

If startup fails with a migration mismatch (`VersionMismatch` / `VersionMissing`)
after schema changes, the local Postgres volume is out of date — recreate it:

```bash
docker compose down -v
docker compose up -d postgres
```

## Local VATSIM OAuth

The example env is configured for VATSIM dev hosts by default. Local rules that
avoid the common failure modes:

- use `http://127.0.0.1:3000` consistently — do not mix `localhost` and
  `127.0.0.1`
- keep `COOKIE_SECURE=false` on plain local HTTP
- use `VATSIM_CLIENT_AUTH_METHOD=post` with `auth-dev.vatsim.net`

If the callback fails with `oauth callback missing state cookie`, the browser
started login on a different origin than the configured `VATSIM_REDIRECT_URI`.

## Docs Entry Points

- Docs home: `GET /docs`
- Interactive API reference: `GET /docs/api/v1`
- OpenAPI JSON: `GET /docs/api/v1/openapi.json`
- Narrative domain docs: `GET /docs/api/{page}` (e.g. `/docs/api/api-keys`)

## Architecture

- **Axum** API application (Rust 2024), single binary
- **Postgres**, organized into per-domain schemas: `identity`, `access`, `org`,
  `training`, `events`, `feedback`, `media`, `routes`, `stats`, `email`,
  `integration`, `platform`, `web`
- **sqlx** with migrations embedded in the binary and applied on startup
- a **repo-backed query layer** (handlers stay thin; SQL lives in `src/repos`)
- shared auth middleware resolving users, service accounts, and API keys, with
  typed `RequirePermission<P>` extractors enforcing access
- request logging, rate limiting, and CORS layers
- markdown docs and a generated OpenAPI spec served by the app itself

### Background workers

Long-running jobs (`src/jobs`) keep external state in sync and drive time-based
lifecycles. Each can be tuned or disabled via env; intervals shown are defaults:

| Worker | Purpose |
| --- | --- |
| stats sync | pulls per-environment VATSIM connections into controller stats |
| roster sync | reconciles the roster against VATUSA |
| appointments sync | assigns training environments, flags double-bookings |
| event lifecycle | promotes events and sends advance reminders |
| LOA / solo expiration | expires lapsed leaves and solo certifications |
| FAA preferred routes | ingests NFDC preferred-route data (opt-in) |
| email delivery | drains the outbound transactional-email queue |
| IP log writer / cleanup | batches request-IP logging with retention pruning |

See [docs/operations/jobs-and-sync.md](docs/operations/jobs-and-sync.md) for the
full set of toggles and intervals.

## Common Commands

```bash
docker compose up -d postgres
cargo fmt
cargo test
cargo run
```

Recommended local validation before pushing:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets -- --test-threads=1
docker build -f Dockerfile .
```

The workspace has two members: the `osmium` service and `db-migrator`, a
one-shot legacy→osmium data migration tool (excluded from the osmium CI jobs).

## Git and Image Flow

- `master` and `develop` both auto-publish Docker images on push
- `master` publishes:
  - `ghcr.io/vzdc-artcc/osmium:latest`
  - `ghcr.io/vzdc-artcc/osmium:latest-<sha>`
- `develop` publishes:
  - `ghcr.io/vzdc-artcc/osmium:dev`
  - `ghcr.io/vzdc-artcc/osmium:dev-<sha>`

PRs against `master` and `develop` are enforced by `.github/workflows/ci.yml`.

## Migration Test Stack

`db-migrator` migrates a legacy Prisma/`public` production dump into the current
Osmium schema. The migration-test stack provisions a mock source database, a
fresh Osmium target, the API, and an on-demand `db-migrator` container.

Place the source dump at `dev-data/mock-prod/prod.sql` (that directory is
gitignored except for a tracked `.gitkeep`), then:

```bash
scripts/migration-test/up.sh                 # full reset + seed + start
scripts/migration-test/migrator.sh plan      # preflight the source dump
scripts/migration-test/migrator.sh migrate   # migrate all domains
scripts/migration-test/migrator.sh migrate --domain stats
scripts/migration-test/migrator.sh verify    # reconcile all domains
scripts/migration-test/down.sh               # stop + delete both volumes
```

Run `plan` first as the preflight check. The startup flow always destroys and
recreates both databases before seeding. Legacy controller stats are backfilled
into `stats.controller_monthly_rollups` for `environment = 'live'`, so historical
hours are available without synthesizing old sessions (`last_activity_at` may
remain `null` for legacy-only controllers).

Detailed instructions: [scripts/migration-test/README.md](scripts/migration-test/README.md)

## Production Deployment

Production uses a separate compose path for steady-state `postgres` + `api` plus
a one-time legacy-dump cutover. The scripts live in `scripts/prod/` and the
deploy-time compose and env files are provided on the host rather than committed.

Full step-by-step setup:
[docs/operations/production-deployment.md](docs/operations/production-deployment.md)

## Read More

- Local development: [docs/getting-started/local-development.md](docs/getting-started/local-development.md)
- Configuration: [docs/getting-started/configuration.md](docs/getting-started/configuration.md)
- Architecture overview: [docs/architecture/overview.md](docs/architecture/overview.md)
- Jobs and sync: [docs/operations/jobs-and-sync.md](docs/operations/jobs-and-sync.md)
- Service accounts: [docs/operations/service-accounts.md](docs/operations/service-accounts.md)
- Testing: [docs/getting-started/testing.md](docs/getting-started/testing.md)
