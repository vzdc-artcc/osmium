# Testing

This page documents the current testing baseline for Osmium.

## Main Command

```bash
cargo test
```

This covers route-level checks, auth helpers, middleware behavior, and basic non-DB expectations for several endpoints.

## Recommended Local Validation

- `cargo test`
- `cargo fmt --all -- --check`
- `cargo check`
- `cargo test --all-targets`
- `docker build -f Dockerfile .`
- open `/docs`
- open `/docs/api/v1`
- hit `/health`
- hit `/ready`
- verify one authenticated flow with a dev session

## Branch Build Flow

- pushes to `master` publish:
  - `ghcr.io/<owner>/<repo>:master`
  - `ghcr.io/<owner>/<repo>:master-<sha>`
- pushes to `develop` publish:
  - `ghcr.io/<owner>/<repo>:develop`
  - `ghcr.io/<owner>/<repo>:develop-<sha>`

Recommended validation before pushing:

- `cargo fmt --all -- --check`
- `cargo check`
- `cargo test --all-targets`
- `docker build -f Dockerfile .`

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
- full DB-backed correctness when `DATABASE_URL` is unset
- production deployment behavior

## Manual Scenarios Worth Running

- Dev login, then `/api/v1/me`
- File upload and signed URL generation
- Publication category create/update/delete flow
- Publication draft to published flow and public visibility
- Public download access through `/cdn/{file_id}` for a published public publication
- Training request create/approve flow
- Event creation and position publish flow
- Service-account introspection route with a valid bearer token
