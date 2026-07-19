# 009 — Final Handler Layering Cleanup

**Depends on**: 002 (repo layer migration) and 004 (structural permission enforcement) — this closes the last gap in both.

## Problem

Five handler files were never migrated to the repo-layer (spec 002) + `RequirePermission<P>` (spec 004) pattern now used everywhere else: `src/handlers/{auth,admin,dev,emails,health}.rs`. They still call `sqlx::query*` directly and use manual `ensure_permission(...)` instead of the typed extractor.

Confirmed inline SQL / permission-check sites (verified against current source):

| File | Lines | Inline SQL sites | Permission checks | Notes |
|------|-------|---|---|---|
| `auth.rs` | 859 | 3 (session INSERT in `vatsim_callback`, session INSERT in `login_as_cid`, session DELETE in `logout`) | 6 static | `vatsim_login`/`vatsim_callback`/`login_as_cid` are intentionally permission-free (OAuth/dev-only) |
| `admin.rs` | 833 | 3 (`count(*)` pagination companions in `list_audit_logs`, `list_visitor_applications`, `list_users`) | 10 static | Most real logic already delegates to existing repo calls |
| `dev.rs` | 331 | 12 (all INSERTs in `seed_data`) | 0 | Route-gated by `dev_seed_enabled()` in `router.rs`, not handler-level |
| `emails.rs` | 285 | 1 (`count(*)` in `list_outbox`) | 6 static | `get_preferences`/`update_preferences` are public unsubscribe-token routes |
| `health.rs` | 255 | 1 (`select 1` in `ready`) | 0 | Public liveness/readiness checks |

No `FromRow`+`ToSchema` hybrid structs exist in any of these 5 files, so no spec-003-style DTO/row split work is needed here.

## Goal

Zero `sqlx::query*` call sites left in these 5 files. All statically-checkable permission checks converted to `RequirePermission<P>`. `grep -rn "sqlx::query" src/handlers | wc -l` and `grep -rn "ensure_permission" src/handlers | wc -l` both trend further toward zero (excluding documented data-dependent exceptions already established in prior specs).

## Approach

Same mechanical pattern as specs 002/004: extract each SQL site verbatim into a repo function, then swap static `ensure_permission(...)` calls for `RequirePermission<P>` extractor arguments.

**Reuse existing permission markers — do not redefine.** `AuthProfileRead`, `AuthProfileUpdate`, `UsersControllerStatusUpdate` already exist in `src/auth/permissions.rs`. Only add markers for genuinely new paths:

```rust
// auth.rs
permission!(AuthTeamspeakUidsRead, ["auth", "teamspeak_uids"], Read);
permission!(AuthTeamspeakUidsCreate, ["auth", "teamspeak_uids"], Create);
permission!(AuthTeamspeakUidsDelete, ["auth", "teamspeak_uids"], Delete);
permission!(AuthSessionsDelete, ["auth", "sessions"], Delete);

// admin.rs
permission!(AccessSelfRead, ["access", "self"], Read);
permission!(AccessUsersRead, ["access", "users"], Read);
permission!(AccessUsersUpdate, ["access", "users"], Update);
permission!(AccessCatalogRead, ["access", "catalog"], Read);
permission!(AuditLogsRead, ["audit", "logs"], Read);
permission!(UsersVatusaRefreshRequest, ["users", "vatusa_refresh"], Request);
permission!(UsersVisitorApplicationsRead, ["users", "visitor_applications"], Read);
permission!(UsersVisitorApplicationsDecide, ["users", "visitor_applications"], Decide);
permission!(UsersDirectoryPrivateRead, ["users", "directory_private"], Read);

// emails.rs
permission!(EmailsTemplatesRead, ["emails", "templates"], Read);
permission!(EmailsPreviewCreate, ["emails", "preview"], Create);
permission!(EmailsSendCreate, ["emails", "send"], Create);
permission!(EmailsOutboxRead, ["emails", "outbox"], Read);
permission!(EmailsSuppressionsUpdate, ["emails", "suppressions"], Update);
```

Verify exact segment spelling against the same strings already hardcoded in `auth.rs::ensure_user_login_access` before adding markers, to avoid a typo'd permission path that silently never matches the seeded ACL rows.

**New/extended repo modules:**

- `src/repos/auth.rs` (new) — `insert_session`, `delete_session`. Keep the session insert as a separate pool-level call exactly as today (issued after `bootstrap_login_user`'s transaction has already committed) — do not silently change atomicity as part of this extraction.
- Count-query companions for `admin.rs`/`emails.rs` — colocate each new `count_*` function beside whichever existing repo file already owns the sibling `list_*` query, rather than defaulting to a brand new file per handler.
- `src/repos/dev.rs` (new) — one function per seeded entity/table, taking a generic executor or `&mut Transaction<'_, Postgres>`. Check whether `seed_data` is currently wrapped in one transaction; if it isn't, that's a pre-existing atomicity gap — note it explicitly in the PR rather than silently fixing or silently leaving it unaddressed.
- `src/repos/health.rs` (new) — `is_database_ready(pool) -> bool`, swallowing the error internally (matching today's `.is_ok()` behavior). Extract even though it's a one-liner, to keep "no inline SQL in handlers" a total, greppable invariant rather than one with a quiet exception.

## Migration order

1. `health.rs` — smallest, proves the "extract even the trivial one" call cheaply.
2. `emails.rs` — 1 SQL site, 5 permission conversions.
3. `admin.rs` — 3 SQL sites (pagination companions to already-repo'd list calls), 10 conversions. Largest file in this batch but the most mechanical.
4. `auth.rs` — touches session/OAuth flow; do this once the extractor-conversion pattern is well-rehearsed from the previous three.
5. `dev.rs` — dev-only, lowest risk, but last so any transaction-safety question raised during extraction is considered with full context of the codebase's established multi-insert seeding patterns.

**Leave untouched — do not add a permission check where none exists today:**
- `vatsim_login`, `vatsim_callback`, `login_as_cid` (OAuth/dev-only flows)
- `service_account_me` — has no `ensure_permission` call today unlike its siblings; note this during migration but don't silently add a check as scope creep into this spec
- `seed_data` (router-level `dev_seed_enabled()` gate only)
- `get_preferences`, `update_preferences` (public unsubscribe-token routes)
- `health`, `ready`

## Affected files

- `src/handlers/auth.rs`, `src/handlers/admin.rs`, `src/handlers/dev.rs`, `src/handlers/emails.rs`, `src/handlers/health.rs`
- New: `src/repos/auth.rs`, `src/repos/dev.rs`, `src/repos/health.rs`; count-query additions to whichever existing repo files own the sibling list queries
- `src/auth/permissions.rs` — new marker block(s) above

## Verification

- `cargo check` after each file's conversion; full `cargo test` after each file.
- `git diff` each extracted query's SQL text against the original — zero query-text changes.
- `grep -rn "sqlx::query" src/handlers | wc -l` → 0 for these 5 files.
- `grep -rn "ensure_permission" src/handlers | wc -l` → 0 for these 5 files.
- `tests/permission_gates.rs`, `tests/auth_sessions.rs` continue passing unchanged.
- Manual smoke test: VATSIM OAuth login/callback (or dev impersonation locally), logout, admin audit log listing, email outbox listing, `/health` and `/ready`.
