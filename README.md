# Osmium API (Axum + SQLx bootstrap)

Initial implementation for the Osmium backend using `axum` and raw SQL via `sqlx`.

## What's included

- App bootstrap with tracing and env-based config
- Router with versioned API prefix (`/api/v1`)
- Health endpoints (`/health`, `/ready`)
- Auth/session skeleton:
  - `GET /api/v1/auth/vatsim/login`
  - `GET /api/v1/auth/vatsim/callback`
  - `POST /api/v1/auth/logout`
  - `GET /api/v1/me`
- Request middleware to resolve the current user from `osmium_session` cookie
- First SQLx migration with core tables (`users`, `sessions`, `events`, `event_positions`)
- Basic integration-like test for health endpoint
- Container support via `Dockerfile` and `docker-compose.yml`

## Local development (host)

1. Copy env file:

```bash
cp .env.example .env
```

2. Start Postgres:

```bash
docker compose up -d postgres
```

3. Run migrations (requires `sqlx-cli`):

```bash
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/osmium
sqlx database create
sqlx migrate run
```

4. Run the API:

```bash
cargo run
```

5. Verify:

```bash
curl -s http://127.0.0.1:3000/health
```

## Build and run with Docker

### Build image

```bash
docker build -t osmium-api:local .
```

`Dockerfile` uses `rust:1.94-bookworm` (dependencies currently require Rust >= 1.88).

If you previously built with an older base image tag, rebuild without cache:

```bash
docker build --no-cache -t osmium-api:local .
```

The container defaults to `RUN_MIGRATIONS_ON_STARTUP=true`, so migrations in `migrations/` are applied automatically when `DATABASE_URL` is set.

### Run container (API only)

Use a DB URL that is reachable from inside the container.

```bash
docker run --rm -p 3000:3000 \
  -e BIND_ADDR=0.0.0.0:3000 \
  -e DATABASE_URL=postgres://postgres:postgres@host.docker.internal:5432/osmium \
  -e RUST_LOG=info,tower_http=debug \
  osmium-api:local
```

If you need to skip startup migrations:

```bash
docker run --rm -p 3000:3000 \
  -e DATABASE_URL=postgres://postgres:postgres@host.docker.internal:5432/osmium \
  -e RUN_MIGRATIONS_ON_STARTUP=false \
  osmium-api:local
```

### Run app + DB with Compose

`docker-compose.yml` loads `.env` into the API container (`env_file`), so keep OAuth values there.

```bash
docker compose up -d --build
```

Check service status:

```bash
docker compose ps
```

Check health endpoint:

```bash
curl -s http://127.0.0.1:3000/health
```

Stop services:

```bash
docker compose down
```

## Build and test (Rust)

```bash
cargo fmt -- --check
cargo test
```

## GitHub Actions Docker publish

A workflow at `.github/workflows/docker-publish.yml` builds the API image and publishes it to GitHub Container Registry:

- Image: `ghcr.io/<owner>/osmium`
- Push to `main`: publishes `main`, `sha-<commit>`, and `latest`
- Push tag `v*`: publishes matching version tags plus `sha-<commit>`
- Pull requests to `main`: build only (no push)

No extra secret is required for the default setup; publishing uses `GITHUB_TOKEN` with `packages: write` permission.

## Next implementation slice

- Implement full VATSIM OAuth callback and session creation
- Add RBAC helpers and protected user/event endpoints
- Replace text role field with DB enum types aligned to Prisma schema
- Add CI checks (`fmt`, `clippy`, `test`, `migration validation`)

## OAuth setup (VATSIM)

Set these values in `.env` before testing login:

```bash
API_DEV_MODE=false
VATSIM_DEV_MODE=false
VATSIM_CLIENT_ID=your-client-id
VATSIM_CLIENT_SECRET=your-client-secret
VATSIM_REDIRECT_URI=http://127.0.0.1:3000/api/v1/auth/vatsim/callback
VATSIM_AUTHORIZE_URL=https://auth.vatsim.net/oauth/authorize
VATSIM_TOKEN_URL=https://auth.vatsim.net/oauth/token
VATSIM_USERINFO_URL=https://auth.vatsim.net/api/user
VATSIM_SCOPE=full_name email vatsim_details country
VATSIM_CLIENT_AUTH_METHOD=basic
COOKIE_SECURE=false
```

+## File CDN env setup
+
+Set these values in `.env` for file upload/download, signed links, and CDN URL generation:
+
+```bash
+FILE_STORAGE_ROOT=./storage/files
+FILE_MAX_UPLOAD_BYTES=26214400
+FILE_SIGNING_SECRET=change-me-in-production
+FILE_ENCRYPTION_KEY_HEX=
+CDN_BASE_URL=http://127.0.0.1:3000
+```
+
+Notes:
+
+- `FILE_STORAGE_ROOT`: filesystem location for stored blobs.
+- `FILE_MAX_UPLOAD_BYTES`: max raw upload size in bytes (default 25 MB).
+- `FILE_SIGNING_SECRET`: required to mint/validate signed CDN tokens (`/api/v1/files/{file_id}/signed-url`, `/cdn/{file_id}`); use a long random secret in production.
+- `FILE_ENCRYPTION_KEY_HEX`: optional 64-char hex key enabling AES-256-GCM encryption for blobs at rest (including Docker volumes). If unset, files are stored plaintext.
+- `CDN_BASE_URL`: base URL embedded in generated signed links.
+
 `VATSIM_DEV_MODE=true` switches default OAuth endpoints to `auth-dev.vatsim.net` when `VATSIM_AUTHORIZE_URL`, `VATSIM_TOKEN_URL`, and `VATSIM_USERINFO_URL` are not explicitly set. If these vars are set to the standard production defaults (`https://auth.vatsim.net/...`), they are also remapped to `auth-dev.vatsim.net` in dev mode.

`API_DEV_MODE=true` enables a local shortcut route `GET /api/v1/auth/login/as/{cid}` that creates/reuses a user by CID and issues `osmium_session` without VATSIM OAuth.

`VATSIM_CLIENT_AUTH_METHOD` controls how the token request authenticates the client:

- `basic` (default): send `client_id`/`client_secret` via HTTP Basic auth header
- `post`: send `client_id`/`client_secret` in form body

If VATSIM returns `invalid_client`, verify all of these match exactly: `VATSIM_CLIENT_ID`, `VATSIM_CLIENT_SECRET`, and `VATSIM_REDIRECT_URI` in both `.env` and your VATSIM app registration.

For local development clients provisioned in VATSIM Connect dev, use the dev host values:

```bash
VATSIM_AUTHORIZE_URL=https://auth-dev.vatsim.net/oauth/authorize
VATSIM_TOKEN_URL=https://auth-dev.vatsim.net/oauth/token
VATSIM_USERINFO_URL=https://auth-dev.vatsim.net/api/user
VATSIM_CLIENT_AUTH_METHOD=post
```

Login flow endpoints:

- `GET /api/v1/auth/vatsim/login` redirects to VATSIM and sets OAuth state cookie
- `GET /api/v1/auth/vatsim/callback` exchanges code, upserts user, sets `osmium_session`
- `GET /api/v1/auth/login/as/{cid}` logs in directly as a CID when `API_DEV_MODE=true` (or `VATSIM_DEV_MODE=true`)
- `GET /api/v1/me` reads authenticated user session, including role and effective permissions
- `GET /api/v1/admin/acl` returns effective ACL permissions for the current user (staff-only)
- `GET /api/v1/admin/access/catalog` returns assignable roles and permissions for admin tools (staff-only)
- `GET /api/v1/admin/users/{cid}/access` returns a specific user's role and effective permissions (staff-only)
- `POST /api/v1/admin/users/{cid}/access` updates roles and/or direct permissions and returns effective permissions (staff-only)

Example access update payload:

```json
{
  "roles": ["STAFF"],
  "permissions": [
    { "name": "manage_users", "granted": true },
    { "name": "dev_login_as_cid", "granted": false }
  ]
}
```
