# Testing

This page documents the current testing baseline for Osmium.

## Main Command

```bash
cargo test
```

This covers route-level checks, auth helpers, middleware behavior, and basic non-DB expectations for several endpoints.

## Recommended Local Validation

- `cargo test`
- open `/docs`
- open `/docs/api/v1`
- hit `/health`
- hit `/ready`
- verify one authenticated flow with a dev session

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
- Training request create/approve flow
- Event creation and position publish flow
- Service-account introspection route with a valid bearer token
