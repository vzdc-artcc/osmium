# 005 — API Error Taxonomy

## Problem

`ApiError` (`src/errors.rs:9-24`) is genuinely used pervasively as the single unified error type (279+ call sites across handlers and repos map to it, including through the repo layer, e.g. `src/repos/users.rs`) — a real strength worth preserving. But the enum only has: `BadRequest`, 3 OAuth-specific 400 variants (`OAuthLoginOriginMismatch`, `OAuthStateCookieMissing`, `OAuthStateMismatch`), `Unauthorized`, `ServiceUnavailable`, `Internal`.

There is no `NotFound`, `Conflict`, or `Forbidden` variant. Confirmed concrete misuse: `src/handlers/users.rs:119` returns `ApiError::BadRequest` (HTTP 400) when `get_user` can't find a user by CID — a true 404 case. Its own `#[utoipa::path]` doc block only documents `400`/`401`, so the OpenAPI spec is self-consistent but describes the wrong HTTP semantics to API consumers. Zero occurrences of `StatusCode::NOT_FOUND` exist anywhere in `src/handlers/*.rs`, confirming this isn't an isolated case.

## Goal

`ApiError` has variants for the common REST semantics it's currently missing, and call sites that are semantically 404/403/409 return the correct variant instead of `BadRequest`.

## Approach

1. Add to `src/errors.rs`:
   - `NotFound` → `StatusCode::NOT_FOUND`, `"not_found"`
   - `Conflict` → `StatusCode::CONFLICT`, `"conflict"`
   - `Forbidden` → `StatusCode::FORBIDDEN`, `"forbidden"`
2. Audit existing `ApiError::BadRequest` call sites for ones that are actually "resource doesn't exist" (→ `NotFound`), "user is authenticated but not allowed to act on this specific resource" (→ `Forbidden`, distinct from the existing `Unauthorized` which should stay reserved for "not authenticated at all" or "lacks the permission entirely" per how `ensure_permission` uses it today), or "valid request but state conflict" (e.g. double-booking, already-decided request) (→ `Conflict`). Start with `src/handlers/users.rs:119` as the confirmed example; grep `ApiError::BadRequest` across `src/handlers/` for the same pattern (fetch returns `None` → `BadRequest`).
3. Update the corresponding `#[utoipa::path]` response-code annotations for each changed call site so the generated OpenAPI spec matches actual behavior.
4. This can land as one PR (it's additive to the enum) with the call-site audit as a checklist, or be split into "add variants" + "fix call sites" if the audit turns out large — decide based on how many call sites the grep turns up.

## Affected files/patterns

- `src/errors.rs` (enum + `IntoResponse` match arms + existing `#[cfg(test)]` module, which should gain equivalent tests for the 3 new variants matching the existing OAuth-variant test pattern)
- Handler call sites returning `ApiError::BadRequest` for not-found/forbidden/conflict cases — confirmed: `src/handlers/users.rs:119`; audit remaining handlers via `grep -rn "ApiError::BadRequest" src/handlers/`
- `#[utoipa::path]` annotations on each corrected handler

## Ordering

Do early (Phase 0) — foundational, low risk, and both 002 (repo migration) and 004 (permission extractor) benefit from having correct error variants available as they land, rather than propagating the `BadRequest`-for-everything pattern into newly extracted repo functions.

## Verification

- `cargo test` — add unit tests for the 3 new variants mirroring the existing `oauth_state_cookie_missing_maps_to_specific_bad_request`-style tests in `src/errors.rs`.
- For each corrected call site: existing integration tests in `tests/*.rs` that hit that endpoint with a not-found/conflict/forbidden case should be updated to assert the new status code (add a test if none currently covers that path).
- Confirm the generated OpenAPI doc (`src/docs.rs`) reflects the corrected response codes for each changed endpoint.
