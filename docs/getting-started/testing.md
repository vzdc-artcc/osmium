# Testing

This page documents the current testing baseline for Osmium.

## Main Command

```bash
cargo test
```

This always covers route-level checks, auth helpers, middleware behavior, and basic non-DB expectations for several endpoints.

When `DATABASE_URL` is set, `cargo test` also runs the Postgres-backed integration suite under `tests/`.

## Recommended Local Validation

- `cargo test`
- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace --all-targets -- --test-threads=1`
- `docker build -f Dockerfile .`
- open `/docs`
- open `/docs/api/v1`
- hit `/health`
- hit `/ready`
- verify one authenticated flow with a dev session

## CI Validation

GitHub Actions runs `.github/workflows/ci.yml` on pushes and pull requests for `master` and `develop`.

It enforces:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace --all-targets -- --test-threads=1`

Separately, the Docker publish workflow still publishes:

- `master` as `ghcr.io/vzdc-artcc/osmium:latest` and `:latest-<sha>`
- `develop` as `ghcr.io/vzdc-artcc/osmium:dev` and `:dev-<sha>`

Recommended validation before pushing:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace --all-targets -- --test-threads=1`
- `docker build -f Dockerfile .`

## Automated Coverage

Current automated coverage is split into:

- route/unit tests in `src/lib.rs` and module-local test blocks
- DB-backed integration tests in `tests/` for sessions, API keys, files/publications, and an end-to-end event workflow

## Docs-Specific Checks

For this documentation system, the important checks are:

- docs index loads
- registered docs pages load
- OpenAPI JSON route loads
- Swagger UI route loads
- docs health route loads

## What Tests Do Not Guarantee

The standard test suite does not fully validate:

- live OAuth behavior against VATSIM
- object storage semantics beyond local filesystem behavior
- DB-backed correctness when `DATABASE_URL` is unset
- production deployment behavior

## Manual Scenarios Worth Running

- Dev login, then `/api/v1/me`
- Dev login, then `PATCH /api/v1/me` with a valid timezone and verify the updated `profile` block
- Dev login, then `PATCH /api/v1/me` with an invalid timezone and verify `bad_request`
- Add, list, and delete a TeamSpeak UID through `/api/v1/me/teamspeak-uids`
- Confirm first login produced a unique `profile.operating_initials`
- File upload and signed URL generation
- Publication category create/update/delete flow
- Publication draft to published flow and public visibility
- Public download access through `/cdn/{file_id}` for a published public publication
- Training request create/approve flow
- Event creation and position publish flow
- Service-account introspection route with a valid bearer token
- API key create/list/detail/update/revoke flow, including one-time secret capture and bearer-token introspection
