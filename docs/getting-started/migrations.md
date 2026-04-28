# Migrations

Osmium now uses a fresh-start ordered migration chain instead of preserving earlier timestamped dev-era migrations.

## Current Migration Model

- One Postgres database
- Multiple schemas by domain
- Ordered SQL files under `migrations/`
- Startup application through SQLx when enabled

Current chain:

- `0001_extensions_and_schemas.sql`
- `0002_identity.sql`
- `0003_access.sql`
- `0004_org.sql`
- `0005_training_core.sql`
- `0006_training_curriculum.sql`
- `0007_events.sql`
- `0008_feedback.sql`
- `0009_stats.sql`
- `0010_media.sql`
- `0011_integration.sql`
- `0012_web.sql`
- `0013_seed_reference_data.sql`
- `0014_seed_roles_permissions.sql`
- `0015_views_and_indexes.sql`

## Philosophy

This repo is still pre-production. The current schema is intended to be the clean v1 baseline, not a compatibility evolution of the earlier schema experiments.

## Local Reset Workflow

If you need a clean reset:

1. Stop the app.
2. Drop or recreate the dev database.
3. Re-run the migration chain.
4. Re-run any dev seed flow if needed.

For Docker Compose users, the normal reset command is:

```bash
docker compose down -v
docker compose up -d postgres
```

If startup logs show `VersionMissing(...)`, that means the database still contains an old `_sqlx_migrations` history from before the fresh-start rewrite.

## Domain Ownership

Each migration file aligns with a clear platform domain:

- identity and sessions
- access and service accounts
- org and roster state
- training
- events
- feedback
- stats
- media/files
- integrations
- web content

## Operational Note

If `RUN_MIGRATIONS_ON_STARTUP=false`, you are responsible for applying migrations manually before starting the API.
