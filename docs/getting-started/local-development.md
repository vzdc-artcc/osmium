# Local Development

This page covers the normal workflow for running Osmium locally with the current v1 schema layout.

## Required Tools

- Rust toolchain compatible with `Cargo.toml`
- Docker and Docker Compose
- PostgreSQL client tools are helpful but optional
- `sqlx-cli` is optional because the app can run migrations on startup

## Basic Flow

1. Copy the environment file.
2. Start Postgres.
3. Run the API.
4. Verify health and docs routes.

```bash
cp .env.example .env
docker compose up -d postgres
cargo run
```

## Branch and Build Flow

- pushes to `master` automatically publish a `master` image and a `master-<sha>` image
- pushes to `develop` automatically publish a `develop` image and a `develop-<sha>` image

Before pushing, run:

```bash
cargo fmt --all -- --check
cargo check
cargo test --all-targets
docker build -f Dockerfile .
```

If you want your first login to become the singleton server admin, set `OSMIUM_SERVER_ADMIN_CID` before starting the API:

```bash
OSMIUM_SERVER_ADMIN_CID=1234567
docker compose up -d
```

After CID `1234567` logs in, `GET /api/v1/me` should return `role` as `SERVER_ADMIN` and include every grouped permission.

If startup fails with `VersionMissing(20260329120000)` or another missing old migration version, your Postgres volume still has the pre-reset migration ledger. Reset it:

```bash
docker compose down -v
docker compose up -d postgres
```

## Database Setup

Osmium expects a Postgres database named `osmium` with the current fresh-start migration chain under `migrations/0001` through `0022`.

If you are creating the database manually:

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/osmium
sqlx database create
sqlx migrate run
```

If `RUN_MIGRATIONS_ON_STARTUP=true` and `DATABASE_URL` is set, the app will attempt to apply migrations on startup.

## Useful Routes

- `GET /health`
- `GET /ready`
- `GET /docs`
- `GET /docs/api/v1`
- `GET /api/v1/me`
- `GET /api/v1/publications`
- `GET /api/v1/publications/categories`

Quick smoke check:

```bash
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/docs/health
```

## Auth in Local Development

There are two main auth paths:

- VATSIM OAuth login for normal user auth
- Dev login shortcut when `API_DEV_MODE=true`

### Recommended VATSIM Local Setup

Use the VATSIM dev environment for normal local OAuth testing:

```bash
VATSIM_DEV_MODE=true
VATSIM_CLIENT_AUTH_METHOD=post
VATSIM_REDIRECT_URI=http://127.0.0.1:3000/api/v1/auth/vatsim/callback
VATSIM_AUTHORIZE_URL=https://auth-dev.vatsim.net/oauth/authorize
VATSIM_TOKEN_URL=https://auth-dev.vatsim.net/oauth/token
VATSIM_USERINFO_URL=https://auth-dev.vatsim.net/api/user
COOKIE_SECURE=false
```

Important:

- start login from `http://127.0.0.1:3000/api/v1/auth/vatsim/login`
- keep the registered redirect URI exactly equal to `VATSIM_REDIRECT_URI`
- do not mix `localhost` and `127.0.0.1` in the same login flow

If you see `oauth callback missing state cookie`, the login was almost certainly started from a different origin than the callback origin, or the cookie was blocked.

If you see `invalid_client` against `auth-dev.vatsim.net`, verify:

- `VATSIM_CLIENT_ID`
- `VATSIM_CLIENT_SECRET`
- `VATSIM_REDIRECT_URI`
- `VATSIM_CLIENT_AUTH_METHOD=post`

Dev login route:

```text
GET /api/v1/auth/login/as/{cid}
```

This creates or reuses a local user record and issues the `osmium_session` cookie.

## Seed Data

If dev routes are enabled, the API also exposes:

```text
POST /api/v1/dev/seed
```

Use this only for local setup and quick functional testing.

## Storage Notes

Files are stored under `FILE_STORAGE_ROOT` in local development. Signed URL generation and optional encryption behavior are controlled through env vars documented in the configuration page.

The publications/downloads module stores only metadata and linked `file_id` values in `web.*`; publication downloads are still served back through the shared CDN route at `GET /cdn/{file_id}`.
