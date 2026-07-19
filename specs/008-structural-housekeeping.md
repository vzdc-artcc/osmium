# 008 — Structural Housekeeping

This spec bundles several smaller, independent inconsistencies that don't each warrant a standalone spec.

## 1. Route naming: `/api/v1/user` → `/api/v1/users`

**Problem**: Every other collection route is plural (`/events`, `/feedback`, `/files`, `/training`, `/api-keys`, `/publications`, `/staffing-requests`), but `user_routes` is nested at singular `/user` (`src/router.rs:463`), and other user-adjacent routes are already registered directly under plural `/users/{cid}/...` (`src/router.rs:441-447`) — so the *same resource family* is split across singular and plural prefixes.

**Decision**: rename `/user` → `/users` outright (no alias/deprecation period), updating any known consumers of the old path within this repo (e.g. the Bruno collection at `Bruno/osmium`) in the same change.

**Approach**: change the `.nest("/user", user_routes)` call at `src/router.rs:463` to `.nest("/users", user_routes)`. Grep the repo for `/api/v1/user/` (not `/api/v1/users/`) to find any hardcoded references outside `router.rs` (Bruno collection, any scripts under `scripts/`, `docs/`).

## 2. `src/models/email.rs` → `src/models/email/mod.rs`

**Problem**: every other domain (`access`, `events`, `feedback`, `media`, `training`, `users`, `web`) is a directory with `mod.rs`; `email.rs` is the one flat-file exception.

**Approach**: move `src/models/email.rs` to `src/models/email/mod.rs` (pure file move, no content change, no import changes needed since the module path stays `crate::models::email`).

## 3. Extract `src/lib.rs`'s test module

**Problem**: `src/lib.rs` is 1661 lines, but lines 91-1661 (97% of the file) are a single `#[cfg(test)] mod tests` block of ~70 integration-style tests asserting on route status codes, OpenAPI shape, and CORS behavior. The actual bootstrap logic (`run()`, `init_tracing()`, `startup_migrations_enabled()`, `run_startup_migrations()`) is only ~90 lines. This makes `lib.rs` misleading at a glance — it reads as a huge file when the real entry-point surface is small.

**Approach**: move the test module's contents into `tests/` as one or more new files (e.g. `tests/routes_and_openapi.rs`, or split further by concern — route status assertions vs. OpenAPI shape vs. CORS — matching how `tests/api_keys.rs`, `tests/auth_sessions.rs`, etc. are already organized by domain). Reuse whatever test-app bootstrap helper the existing `tests/support` module provides rather than duplicating it. `lib.rs` ends up as just the bootstrap functions.

## 4. Split `src/docs.rs`'s two concerns

**Problem**: `docs.rs` (728 lines) contains two unrelated things in one file: a hand-written markdown documentation site (routes/rendering, roughly lines 8-232) and the generated-OpenAPI `#[openapi(paths(...))]` registry (roughly lines 310-709, listing all 282 documented handler paths).

**Approach**: split into two files, e.g. `src/docs/markdown_site.rs` (or keep the name `docs.rs` for this half) and `src/docs/openapi.rs`, each focused on one concern. Pure code-organization move — no behavior change. Note for whoever executes this: there's currently no compile-time check that every router-registered handler appears in the OpenAPI path list (the existing `lib.rs` test only spot-checks a subset) — worth flagging as a possible follow-up (not in scope for this spec) once the split makes the list easier to audit.

## 5. Note on migrations history (no action)

`migrations/0016_resource_action_permissions.sql` → `0024_training_permissions_split.sql` → `0027_hierarchical_permissions.sql` → `0028_route_permission_split.sql` show the permissions/ACL schema was reworked 4 times, each rebuilding overlapping permission catalogs. This is historical — migrations aren't rewritten after the fact. Documented here only so the current hierarchical-permissions design (0027/0028) is treated as settled, not as a candidate for a 5th rework alongside the other changes in this plan.

## Ordering

All 5 items are independent of each other and of every other spec in this plan. Do as Phase 0 quick wins, in any order; items 2-4 are pure moves with no functional risk, item 1 is a breaking API change (coordinate with any external consumers before merging), item 5 requires no action.

## Verification

- Item 1: `cargo test` — update any test asserting the old `/api/v1/user/...` path; grep-confirm no remaining references to the old path anywhere in the repo (code, Bruno collection, docs).
- Item 2: `cargo check` — a pure file move should produce zero diagnostics.
- Item 3: `cargo test` — all relocated tests must pass unchanged in their new location; confirm `cargo test` still discovers them (they need to be registered as a `tests/*.rs` binary or `mod` included from one).
- Item 4: `cargo check`/`cargo build` — confirm OpenAPI generation (hit `/docs` or the OpenAPI JSON endpoint) still produces identical output after the split.
- Item 5: none required.
