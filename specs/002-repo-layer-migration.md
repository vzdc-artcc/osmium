# 002 — Repo Layer Migration

## Problem

`src/repos/` has only 4 files (`access.rs`, `api_keys.rs`, `audit.rs`, `users.rs`, ~64 total `sqlx::query*` call sites) covering identity/access/audit. Everything else — training, org, events, files, publications, integrations, stats, feedback, incidents — writes raw SQL directly inside `src/handlers/*.rs`: 305 `sqlx::query*` call sites across 20 handler files. Worst offenders: `training.rs` (3752 lines, 81 queries, mixing assignments/OTS-recommendations/lessons/appointments/sessions/assignment-requests/release-requests), `org.rs` (2651 lines, 49 queries, mixing LOAs/solo-certifications/staffing-requests/SUA-requests/controller-lifecycle/background-job-admin), `training_admin.rs` (37), `integrations.rs` (27), `events.rs` (18), `publications.rs` (17), `files.rs` (16).

Handlers also define request/response DTOs locally instead of using `src/models/` — `org.rs` has 32 local struct definitions, `training.rs` has 14 — duplicating or drifting from the shared model types.

This is the root architectural inconsistency the rest of the findings cascade from: it's why pagination got re-implemented per-file (001), why permission checks are convention-based rather than structural (004), and why DB row types and wire DTOs are the same struct (003).

## Goal

All handler files call into a `src/repos/<domain>/<subdomain>.rs` layer for data access, matching the pattern already established in `src/repos/users.rs` (functions take `&PgPool` or a transaction, return `Result<T, ApiError>`, are called from the corresponding handler). No `sqlx::query*` calls remain directly inside `src/handlers/*.rs`. Handler-local DTOs move into `src/models/<domain>/`.

## Approach

- **Unit of migration**: one repo submodule per sub-domain, not one repo file per handler file. `training.rs` and `org.rs` are already too large because they mix unrelated sub-domains — extracting by sub-domain (which is already visible from how functions are grouped/ordered in the source) avoids recreating the same problem one layer down. Example target structure: `src/repos/org/{loas,solo_certs,staffing_requests,sua_requests,controller_lifecycle,jobs}.rs`, `src/repos/training/{assignments,ots,lessons,appointments,sessions,assignment_requests,release_requests}.rs`.
- **Two-phase split, not one flag day**:
  - Phase A (this spec): extract each query verbatim into a repo function returning the *same struct the handler already used* — even if that struct still derives both `sqlx::FromRow` and `Serialize`/`ToSchema`. The handler swaps its inline `sqlx::query!(...)` for a call to the new repo function. Zero wire-format or schema changes. Move the handler's local DTOs into `src/models/<domain>/` in the same pass (don't defer this — moving structs without moving the queries just churns imports twice).
  - Phase B (separate spec, 003): split FromRow-only rows from Serialize-only DTOs, once everything is behind the repo layer.
- **Transactions**: several handlers currently do fetch → mutate → audit-log → email-send as one implicit sequential unit inside a single async function. When splitting these into repo calls, don't naively pass a bare `&PgPool` to each — repo functions covering a multi-step write should accept an explicit `&mut Transaction<'_, Postgres>` (or a generic executor) so the atomicity that currently holds "by accident" (because it's all sequential against the same pool in one function) doesn't silently become non-atomic once split across multiple pool-acquiring calls.

## Migration order

Smallest/most isolated first, to prove the pattern cheaply before tackling the two large files:

1. `stats.rs` (9 queries, read-only)
2. `feedback.rs` (8)
3. `incidents.rs` (7)
4. `files.rs` (16, self-contained)
5. `publications.rs` (17)
6. `events.rs` (18) + `event_ops.rs` (12, same domain, migrate together)
7. `integrations.rs` (27)
8. `training_admin.rs` (37)
9. `org.rs` (49) — split by its 6 internal sub-domains; migrate LOAs first (cleanest), controller-lifecycle/background-jobs admin last (touches multiple domains)
10. `training.rs` (81) — split by its 7 internal sub-domains, last

Each file/sub-domain is its own PR, not one giant PR for the whole migration.

## Affected files/patterns

- New: `src/repos/<domain>/` submodules as listed above
- Reference pattern to follow: `src/repos/users.rs`, called from `src/handlers/users.rs`
- All 20 handler files listed in the migration order above
- Corresponding `src/models/<domain>/mod.rs` files gain the relocated DTOs

## Verification (per file/sub-domain migrated)

1. `cargo check` after each function extraction.
2. `git diff` the extracted SQL text against the original to confirm zero query-text changes (mechanical extraction, not a rewrite).
3. `cargo test` — full suite, including `tests/*.rs` (integration tests hit the real router over HTTP via `TestApp`) and the route/OpenAPI assertions in `src/lib.rs`. Confirm `TestApp` actually requires and uses a live Postgres in CI (it returns `Option` and some tests short-circuit if unavailable) rather than silently skipping DB-backed assertions.
4. One manual smoke test against a real Postgres instance for that domain's happy path (create/list/update/delete as applicable).
5. Track progress with `grep -rn "sqlx::query" src/handlers | wc -l` trending toward 0 (excluding any handlers intentionally kept simple, if that exception is later decided).
