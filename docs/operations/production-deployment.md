# Production Deployment

This is the production Docker Compose runbook for Osmium.

It covers two distinct modes:

- steady-state production runtime with only `postgres` and `api`
- one-time cutover from a legacy SQL dump into a fresh Osmium database

The production path does not include TLS termination or a reverse proxy. Handle public ingress separately.

## What Gets Created

Steady-state services:

- `postgres`
- `api`

Temporary cutover-only services:

- `target-init`
- `legacy-postgres`
- `legacy-seed`
- `migrator`

Persistent storage:

- a Docker volume for the target Postgres database
- a host directory mounted into `/app/storage/files`

Temporary cutover storage:

- a Docker volume for the temporary legacy source database

## Files You Will Use

- Compose file: `docker-compose.prod.yml`
- Cutover env template: `.env.cutover.example`
- Steady-state env template: `.env.prod.example`
- Main cutover script: `scripts/prod/cutover.sh`
- Steady-state start script: `scripts/prod/up.sh`
- Steady-state stop script: `scripts/prod/down.sh`
- Manual migrator wrapper: `scripts/prod/migrator.sh`

## Before You Start

Have these ready before touching production:

- Docker Engine with Compose enabled
- enough disk space for:
  - the fresh Osmium target database
  - the temporary restored legacy database
  - persistent file storage
- a plain SQL dump of the old website database
- a pinned GHCR image tag or digest for Osmium
- backups and a restore plan for any existing production data

This deployment path does not attempt automatic rollback.

## Required Secrets and Variables

At minimum, fill in:

- `OSMIUM_IMAGE`
- `OSMIUM_POSTGRES_PASSWORD`
- `DATABASE_URL`
- `VATSIM_CLIENT_ID`
- `VATSIM_CLIENT_SECRET`
- `FILE_SIGNING_SECRET`
- `EMAIL_UNSUBSCRIBE_SECRET`
- `VATUSA_API_KEY` if roster sync will be enabled
- SES credentials if email sending will be enabled

Production templates intentionally use placeholders. Replace all of them before deployment.

## Image Pinning

Do not use a floating production tag for cutover.

Preferred:

- `ghcr.io/<owner>/<repo>:latest-<sha>`
- an immutable image digest

Avoid using raw `:latest` for an actual production migration.

## Recommended Host Layout

Recommended defaults:

- `OSMIUM_FILES_DIR=/srv/osmium/files`
- `OSMIUM_DUMP_DIR=/srv/osmium/dumps`

Expected dump path:

```text
${OSMIUM_DUMP_DIR}/prod.sql
```

The cutover script creates the files directory if needed. It does not create the SQL dump.

## Environment Files

Use two env files:

- `.env.cutover`
  - used only during initial cutover
  - keeps background workers disabled
- `.env.prod`
  - used for steady-state production
  - enables your intended background workers

These values must stay aligned between both files:

- `OSMIUM_IMAGE`
- `OSMIUM_POSTGRES_DB`
- `OSMIUM_POSTGRES_USER`
- `OSMIUM_POSTGRES_PASSWORD`
- `DATABASE_URL`
- `OSMIUM_FILES_DIR`
- `OSMIUM_DUMP_DIR`
- public base URLs and OAuth settings

## Step-By-Step Setup

### 1. Prepare the host

From the repo root on the production host:

```bash
mkdir -p /srv/osmium/files /srv/osmium/dumps
cp .env.cutover.example .env.cutover
cp .env.prod.example .env.prod
```

If you use different host paths, update both env files before continuing.

### 2. Fill in `.env.cutover`

Set at least:

- `OSMIUM_IMAGE`
- `OSMIUM_POSTGRES_PASSWORD`
- `DATABASE_URL`
- `CORS_ALLOWED_ORIGINS`
- `VATSIM_CLIENT_ID`
- `VATSIM_CLIENT_SECRET`
- `VATSIM_REDIRECT_URI`
- `FILE_SIGNING_SECRET`
- `CDN_BASE_URL`

During cutover, these should remain disabled:

- `STATS_SYNC_ENABLED=false`
- `ROSTER_SYNC_ENABLED=false`
- `EMAIL_WORKER_ENABLED=false`
- `EMAIL_ENABLED=false`

### 3. Fill in `.env.prod`

Copy the same core connection and origin values from `.env.cutover`, then set the steady-state worker posture you want.

Typical steady-state values:

- `STATS_SYNC_ENABLED=true`
- `ROSTER_SYNC_ENABLED=true`
- `EMAIL_WORKER_ENABLED=true`
- `EMAIL_ENABLED=true`

If SES or VATUSA integration is not ready yet, leave the matching worker disabled until its secrets are actually valid.

### 4. Place the legacy SQL dump

Put the legacy dump at:

```text
${OSMIUM_DUMP_DIR}/prod.sql
```

For the default layout:

```bash
ls -lh /srv/osmium/dumps/prod.sql
```

The dump must be a plain SQL dump that Postgres 16 can restore with `psql -f`.

### 5. Validate the compose config before cutover

```bash
docker compose --env-file .env.cutover -f docker-compose.prod.yml config
docker compose --env-file .env.cutover -f docker-compose.prod.yml --profile cutover config
```

Both commands must succeed before continuing.

### 6. Run the cutover

```bash
scripts/prod/cutover.sh
```

If your env file lives elsewhere:

```bash
scripts/prod/cutover.sh /absolute/path/to/.env.cutover
```

The cutover script does this in order:

1. validates compose interpolation and required env
2. starts target `postgres`
3. starts `target-init` to apply the Osmium SQL migration chain to the fresh target DB
4. stops and removes `target-init`
5. starts temporary `legacy-postgres`
6. runs `legacy-seed` to restore `prod.sql`
7. runs `db-migrator plan`
8. runs `db-migrator migrate`
9. runs `db-migrator verify`
10. starts the public `api`
11. waits for `/health`
12. prints the `/ready` response
13. removes the temporary legacy cutover services and legacy cutover volume

The public API is intentionally not started until migration and verification succeed.

## What Success Looks Like

After a successful cutover:

- the API is running
- the target Postgres container is running
- the temporary legacy services are gone
- the temporary legacy volume is gone
- `/health` returns `ok`
- `/ready` shows the DB as ready

Check:

```bash
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/ready
docker compose --env-file .env.cutover -f docker-compose.prod.yml ps
```

## Move From Cutover Mode To Steady State

Once cutover succeeds:

1. confirm `.env.prod` has the values you want for live operation
2. enable only the workers whose secrets and upstream dependencies are ready
3. recreate the stack using the steady-state env

Start steady-state services:

```bash
scripts/prod/up.sh
```

Or with an explicit env path:

```bash
scripts/prod/up.sh /absolute/path/to/.env.prod
```

Expected steady-state services:

- `postgres`
- `api`

## Normal Operations

### Start production services

```bash
scripts/prod/up.sh
```

### Stop production services without deleting data

```bash
scripts/prod/down.sh
```

This does not remove the target Postgres volume or your file-storage directory.

### Inspect running services

```bash
docker compose --env-file .env.prod -f docker-compose.prod.yml ps
```

### Inspect logs

```bash
docker compose --env-file .env.prod -f docker-compose.prod.yml logs -f api
docker compose --env-file .env.prod -f docker-compose.prod.yml logs -f postgres
```

## Manual Migrator Commands

Use the wrapper so the correct compose file and `cutover` profile are always selected:

```bash
scripts/prod/migrator.sh plan
scripts/prod/migrator.sh migrate
scripts/prod/migrator.sh verify
scripts/prod/migrator.sh reset-run --run-id <run-id>
```

You can also pass an explicit env file first:

```bash
scripts/prod/migrator.sh /absolute/path/to/.env.cutover migrate --domain stats
```

Useful recovery commands after a failed run:

```bash
scripts/prod/migrator.sh verify
scripts/prod/migrator.sh migrate --resume
scripts/prod/migrator.sh reset-run --run-id <run-id>
```

## Rollback Limits

This deployment path does not attempt automatic rollback of the target Osmium database.

If migration fails:

- the target Osmium database is left intact for inspection
- the temporary legacy source database is left intact for inspection
- the script prints rerun guidance for `verify`, `migrate --resume`, and `reset-run`

Before any real production cutover, take your own database backups and confirm your restore procedure.

## Known Validation Result

As of the current repo state, the production cutover infrastructure has been exercised locally against the sample dump and the infrastructure path works through:

- target DB startup
- target schema bootstrap
- legacy dump restore
- migrator startup

The current end-to-end run then fails inside the existing migrator logic with:

```text
legacy controller log ... references unresolved user ...
```

That is a current migrator/data issue, not a compose bootstrapping issue. Until that migrator issue is fixed, expect cutover to stop before API startup and before legacy teardown.

## Troubleshooting

### Missing dump

Check:

- `${OSMIUM_DUMP_DIR}/prod.sql` exists
- the file is a plain SQL dump

### Bad image tag

Check:

- `OSMIUM_IMAGE` is set
- it does not still contain placeholder text
- the host can pull that GHCR image

### `docker compose config` fails

Check:

- `.env.cutover` or `.env.prod` exists
- all required vars are present
- shell-sensitive values are quoted if needed

### Seed failure

Check:

- the legacy dump matches Postgres 16 expectations
- the dump is valid plain SQL
- the cutover profile services are healthy

### Migrate failure

Check:

- `scripts/prod/migrator.sh plan`
- `scripts/prod/migrator.sh migrate --resume`
- migrator logs from the one-shot container

### Verify failure

Check:

- `scripts/prod/migrator.sh verify`
- target DB contents and source dump quality

The cutover script intentionally stops before legacy DB teardown when verification fails.

### Degraded `/ready`

Check:

- `DATABASE_URL`
- startup migration success
- whether workers are intentionally disabled during cutover
- missing runtime secrets for enabled integrations

During cutover, worker-disabled readiness is expected to reflect your chosen disabled state rather than a full steady-state posture.

### Stats sync remains stale after enabling workers

Check:

- `STATS_SYNC_ENABLED=true`
- upstream feed reachability
- `STATS_SYNC_STALE_SECS`
- API logs for stats sync failures

Readiness uses live stats staleness and will remain degraded until live stats succeed again.
