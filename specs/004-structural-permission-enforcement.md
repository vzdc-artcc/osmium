# 004 — Structural Permission Enforcement

## Problem

`ensure_permission()` (`src/auth/middleware.rs:56`) takes `(&AppState, Option<&CurrentUser>, Option<&CurrentServiceAccount>, PermissionPath)` and returns `Result<(), ApiError>`. It's called manually ~100 times across handler bodies, each preceded by boilerplate that extracts `Extension<Option<CurrentUser>>`/`Extension<Option<CurrentServiceAccount>>` from request extensions. This is convention, not structure: nothing in the type system prevents a new handler from forgetting the call, and it's not visible from the router (`src/router.rs`) which routes actually enforce which permission.

`resolve_current_user` (`src/auth/middleware.rs:17`) is a global Axum middleware, registered once in `src/router.rs`, that inserts `Option<CurrentUser>` and `Option<CurrentServiceAccount>` into request extensions for *every* request — it doesn't know what permission any given route needs; that's left to each handler.

## Goal

A missing permission check becomes a compile error (or at minimum, a structural gap that's mechanically greppable/lintable to zero), without requiring a flag-day rewrite of ~100 call sites or any changes to `router.rs`.

## Approach

Introduce a typed extractor, `RequirePermission<P>`, using Axum's `FromRequestParts`:

- One marker type per permission (e.g. `AuthProfileRead`), each implementing a small `Permission` trait that returns its `PermissionPath`. Generate these via a small declarative macro to avoid hand-writing one impl per permission.
- `RequirePermission<P>::from_request_parts` reads the same `Option<CurrentUser>`/`Option<CurrentServiceAccount>` extensions already inserted by `resolve_current_user`, calls the existing `ensure_permission()` internally (reusing the current logic, not replacing it), and returns `ApiError::Unauthorized` on failure exactly as today.
- Handlers declare `RequirePermission<SomePermission>` as a function argument instead of manually calling `ensure_permission(...)`. Since Axum requires all extractor arguments to be present in the handler signature, the compiler enforces the check is declared — it can't be silently omitted the way a body-level function call can.
- **No `router.rs` changes required.** This is purely a per-handler signature/body change, so old manual `ensure_permission()` calls and the new extractor coexist indefinitely — migrate one handler at a time.

Two known limits to call out explicitly wherever this pattern is applied:

- **Data-dependent authorization**: some routes fold an ownership check into the query itself (e.g. "only the LOA's owner or an approver can update it" — implemented via a `WHERE` clause or a fetch-then-compare in `org.rs`'s LOA update/decide handlers, not solely via `ensure_permission`). The extractor only covers the coarse-grained role/permission check; these routes need an explicit, documented two-layer pattern (extractor for the coarse check + explicit in-handler ownership check) — do not treat "has the extractor" as "fully authorized" for these.
- **Service-account duality**: the extractor must resolve both `CurrentUser` and `CurrentServiceAccount` from extensions (not just user), since service-account-authenticated routes (`api_keys.rs`) rely on the same `ensure_permission` accepting either.

## Migration order

Same file order as 002 (repo layer migration) — migrate a handler file's data access and its permission checks together where practical, since both touch the same functions and this avoids touching each file twice. Within a file, prioritize routes with purely route+user-level authorization (safe, mechanical conversion) before routes with data-dependent authorization (need the documented two-layer pattern).

## Affected files/patterns

- New: a permission-extractor module (e.g. `src/auth/require_permission.rs`) defining the `Permission` trait, `RequirePermission<P>`, and the macro that generates marker types
- `src/auth/middleware.rs` — `ensure_permission` stays as the underlying implementation, reused by the new extractor; not removed until burn-down is complete
- All handler files, migrated in the order specified above

## Verification

- `cargo check`/`cargo build` after each handler's conversion (missing extractor argument is a straightforward compile-time signature mismatch, not a runtime surprise).
- `cargo test` — existing auth/permission tests (`tests/auth_sessions.rs` and similar) must continue passing unchanged, since `RequirePermission<P>` must produce identical `ApiError::Unauthorized` behavior to the manual call for both authenticated and unauthenticated requests.
- Track burn-down via `grep -rn "ensure_permission" src/handlers | wc -l` trending to zero (the extractor's internal use of `ensure_permission` in `src/auth/` doesn't count toward this).
- For each data-dependent-auth route, confirm the two-layer pattern is documented inline (a short comment noting "coarse check via extractor + ownership check below") so it doesn't read as fully covered by the extractor alone.
