# Migration Test Stack

This stack is for local destructive migration validation. For production cutover and steady-state deployment, use `docker-compose.prod.yml` and [docs/operations/production-deployment.md](../../docs/operations/production-deployment.md).

This stack is for testing `db-migrator` against:

- a legacy Prisma/public source Postgres database named `prod`
- a fresh target Osmium Postgres database named `osmium`
- the Osmium API running against the target database

`db-migrator` now migrates from the legacy prod/public dump into the new Osmium schema.

## What You Need

1. A local `.env` file at the repo root.
2. A plain SQL dump file at `dev-data/mock-prod/prod.sql`.
3. Run commands from the repo root: `/Users/vainnor/Programing/osmium`

## Important Build Context Rule

Do not run:

```bash
docker build -f Dockerfile ..
```

That uses the parent directory as the Docker build context, so Docker cannot see this repo's `src/`, `migrations/`, `docs/`, or `tools/` directories.

Use one of these instead:

```bash
docker build -f Dockerfile .
```

or from the parent directory:

```bash
docker build -f osmium/Dockerfile osmium
```

## Dump File

Expected path:

```text
dev-data/mock-prod/prod.sql
```

If that file is missing, the seed step fails immediately and the migration test stack is not usable.

## Start The Stack

This does a destructive reset every time:

1. removes the migration-test containers and volumes
2. starts both Postgres containers
3. restores `dev-data/mock-prod/prod.sql` into the mock prod database
4. starts the Osmium API against a fresh target database

Run:

```bash
scripts/migration-test/up.sh
```

`up.sh` also rebuilds the Compose-managed `api` and `migrator` images so the stack uses your current local code instead of a stale previously-built image.

## Stop The Stack

This removes the migration-test containers and volumes:

```bash
scripts/migration-test/down.sh
```

## Run The Migrator

Use the wrapper script so the correct compose file is always used:

```bash
scripts/migration-test/migrator.sh plan
scripts/migration-test/migrator.sh migrate
scripts/migration-test/migrator.sh migrate --domain users
scripts/migration-test/migrator.sh migrate --domain stats
scripts/migration-test/migrator.sh verify
scripts/migration-test/migrator.sh verify --domain stats
scripts/migration-test/migrator.sh reset-run --run-id <run-id>
```

Run `plan` first. It is the supported preflight step for the legacy dump shape.

`migrator.sh` rebuilds the Compose-managed migrator image before running it.

Legacy controller stats are migrated into `stats.controller_monthly_rollups` for `environment = 'live'`. This is enough for the existing stats API to return historical hours. It does not backfill old `stats.controller_sessions` or `stats.controller_activations`, so `last_activity_at` may still be `null` for legacy-only history.

## Service Endpoints

- Osmium API: `http://127.0.0.1:3000`
- Target DB: `127.0.0.1:5432`
- Mock prod DB: `127.0.0.1:5433`

## Quick Verification

After `scripts/migration-test/up.sh`, check:

```bash
curl -s http://127.0.0.1:3000/health
docker compose -f docker-compose.migration-test.yml ps
```

You should see:

- `mock-prod-postgres` running
- `osmium-postgres` running
- `api` running
- the health endpoint returning `ok`

## Common Problems

### Docker build fails with missing `tools` or `src`

Cause:

```bash
docker build -f Dockerfile ..
```

Fix:

```bash
docker build -f Dockerfile .
```

### Manual `docker build` succeeds but `migrator.sh` still runs old code

Cause:

- `docker build -f Dockerfile .` creates a standalone image
- `docker compose run migrator ...` uses the Compose-managed image for the `migrator` service

Fix:

```bash
scripts/migration-test/up.sh
```

or:

```bash
docker compose -f docker-compose.migration-test.yml build api migrator
```

### Seed step fails

Check:

- `dev-data/mock-prod/prod.sql` exists
- the file is a plain SQL dump
- the SQL inside the dump is valid for Postgres 16

### API starts but migrator fails

Check:

- `mock-prod-postgres` is healthy
- `osmium-postgres` is healthy
- the source dump actually restored into `prod`
- the dump is a legacy Prisma/public dump, not a current Osmium-schema database

## Recommended Workflow

From the repo root:

```bash
cp .env.example .env
mkdir -p dev-data/mock-prod
# place your dump at dev-data/mock-prod/prod.sql
docker build -f Dockerfile .
scripts/migration-test/up.sh
scripts/migration-test/migrator.sh plan
scripts/migration-test/migrator.sh migrate
scripts/migration-test/migrator.sh migrate --domain stats
scripts/migration-test/migrator.sh verify
```
