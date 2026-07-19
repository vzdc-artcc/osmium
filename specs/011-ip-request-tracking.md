# 011 — Durable IP Request Tracking

**Depends on**: 010 (IP-based rate limiting) — reuses its unified `client_ip()` helper (`src/auth/ip.rs`). Deliberately does **not** share a table or in-memory mechanism with 010's rate limiter — see Approach.

## Problem

IP addresses are persisted today only for specific authenticated write actions:
- `identity.sessions.ip_address` (login)
- `access.audit_logs.ip_address` (via `audit_repo::record_audit`, called from most write-handlers)
- `media.file_audit_logs.ip_address` (file operations)

Nothing records *every* request — reads, unauthenticated traffic, failed requests — durably. `src/logging.rs::log_requests` already extracts and logs `client_ip` per request, but only to `tracing` (structured stdout), never to the database. There is no `rate_limit_events`/`request_log`/`ip_request_log` table. Next available migration number: `0033`.

## Goal

Every request's IP address is durably recorded (metadata only — method, matched route, status, actor, timestamp — not bodies/headers/query strings), queryable per-user by admins via a new endpoint, with bounded storage growth via a retention/cleanup job. The write path must not add a synchronous per-request database write to the hot path.

## Approach

### Relationship to spec 010 (rate limiting) — kept separate, by design

Both specs need per-IP request data, but they serve different purposes and have different performance requirements:

- **Spec 010's rate limiter** needs sub-millisecond, in-memory, per-request checks (`tower_governor`'s in-memory keyed state). It's fine for this state to reset on restart — a brief post-restart window of laxer enforcement is an acceptable, common tradeoff.
- **Spec 011's audit table** needs durable, append-only history that must survive restarts, with a retention policy and indexes suited to occasional forensic/admin queries — not a hot, frequently-reset counter.

Forcing one table to serve both would pull it toward "hot write-heavy counter table" (bad for retention/indexing) or "slow durable-write table" (bad for the rate limiter's hot path). They share only the `client_ip()` extraction helper from spec 010.

This assumes osmium runs as a single instance today (confirmed during planning: no replica/orchestration config found in `docker-compose*.yml`/`Dockerfile`). If osmium is deployed as multiple instances behind a load balancer in the future, spec 010's in-memory limiter would under-count real per-IP traffic (limit × instance count) — that's a reason to revisit spec 010 toward a distributed store, not a reason to merge specs 010/011's tables.

### Schema

New migration `migrations/0033_ip_request_log.sql`:

```sql
create table access.ip_request_log (
    id bigserial primary key,
    ip_address inet not null,
    method text not null,
    matched_path text not null,   -- route template (e.g. "/api/v1/users/{cid}"), not the raw
                                    -- path, so aggregation by endpoint doesn't explode on path params
    status_code smallint not null,
    actor_type text,               -- 'user' | 'service_account' | null, mirrors logging.rs::auth_mode
    actor_id text references access.actors(id) on delete set null,
    created_at timestamptz not null default now()
);

create index ip_request_log_ip_created_idx on access.ip_request_log (ip_address, created_at);
create index ip_request_log_actor_created_idx on access.ip_request_log (actor_id, created_at);
```

`actor_id` is the join point for the new per-user endpoint (below). Deliberately excludes request/response bodies, query strings, and headers — this is a narrower, higher-volume-tolerant table than `access.audit_logs` (which keeps its `before_state`/`after_state` JSON for specific write actions; unaffected by this spec).

### Write path — batched, not synchronous

Writing one row per request synchronously (in `log_requests` or a sibling middleware) would add a DB write to every single request, including 404s and health checks — unacceptable write volume on the hot path. Instead:

- A new sibling middleware (or an extension of `src/logging.rs`, reusing its existing `actor_summary`/`auth_mode` helpers) pushes a lightweight entry onto a bounded `tokio::sync::mpsc` channel after each request. If the channel is full, drop the entry and log a warning — never back-pressure real traffic.
- A new background job drains the channel and bulk-inserts every N seconds or M rows, whichever comes first, implementing the existing `Job` trait (`src/jobs/mod.rs`) and registered via `spawn()`, exactly like `src/jobs/{roster_sync,stats_sync,email_delivery}.rs`.

This keeps the request path's added cost to a single non-blocking channel `send`.

### Retention / cleanup job

`src/jobs/ip_log_cleanup.rs`, implementing `Job`, ticking on an interval (e.g. daily), deleting rows older than a configurable retention window:

```
IP_REQUEST_LOG_RETENTION_DAYS   default e.g. 30
```

Register it in `main.rs`/`lib.rs` alongside the existing three job spawns. Consider wiring it into the existing `POST /api/v1/admin/jobs/{job_name}/run` endpoint (`org.rs`'s job registry, used by `stats_sync`/`roster_sync` today) so it can be triggered manually the same way — follow that file's existing job-name-lookup pattern if included.

### New per-user admin endpoint

Per the confirmed decision, this is scoped to *viewing a specific user's* IP history, not a generic "browse all IPs" admin page:

```
GET /api/v1/admin/users/{cid}/ip-history
```

Nested exactly like the existing `/admin/users/{cid}/{access,controller-status,controller-lifecycle,refresh-vatusa,solo-certifications,dossier}` routes already in `src/router.rs`. Returns a paginated list (reusing `PaginationMeta`/`PaginationQuery`, per spec 001) of `{ip_address, method, matched_path, status_code, created_at}` rows for that user's `actor_id`.

Permission: add `permission!(UsersIpHistoryRead, ["users", "ip_history"], Read)`, or reuse `UsersDirectoryPrivateRead`/`AccessUsersRead` (added in spec 009) if their scope already covers "admin viewing sensitive per-user data" well enough — decide at implementation time by checking which existing marker's semantics fit best, to avoid an unnecessary new permission.

### New repo module

`src/repos/ip_request_log.rs`:
- `insert_batch(pool, entries: &[IpRequestLogEntry]) -> Result<(), ApiError>` — bulk insert (e.g. via multi-row `VALUES` or `UNNEST`), called by the draining job.
- `delete_older_than(pool, cutoff: DateTime<Utc>) -> Result<u64, ApiError>` — called by the cleanup job.
- `list_for_actor(pool, actor_id, page_size, offset) -> Result<Vec<IpRequestLogItem>, ApiError>` and `count_for_actor(pool, actor_id) -> Result<i64, ApiError>` — for the new endpoint.

## Affected files

- New: `migrations/0033_ip_request_log.sql`
- New: `src/repos/ip_request_log.rs`
- New: `src/jobs/ip_log_cleanup.rs`
- `src/jobs/mod.rs` — register the new job module (`pub mod ip_log_cleanup;`)
- `src/logging.rs` (or a new sibling, e.g. `src/request_tracking.rs`) — the buffering write path; reuses `src/auth/ip.rs` from spec 010
- `src/state.rs` — new mpsc sender field, plus a job-health record following the existing `JobHealth` pattern
- `src/router.rs` — new `GET /api/v1/admin/users/{cid}/ip-history` route
- `src/handlers/admin.rs` — new handler for the above
- `src/auth/permissions.rs` — new marker (or reuse, per above)
- `main.rs`/`lib.rs` — spawn the buffer-drain task and the cleanup job alongside the existing three
- `src/config.rs` — new env vars (retention days, flush interval/batch size)

## Verification

- `cargo check`/`cargo build`; migration applies cleanly against a scratch DB.
- New `tests/ip_request_log.rs` (needs live Postgres, following `tests/support/mod.rs`'s `TestApp` pattern): issue a handful of requests, force-flush the buffer via a test-only synchronous-flush hook (check whether `email_delivery`'s job already exposes a similar "tick now" hook to follow the same approach), then query `access.ip_request_log` directly via the test's pool handle to confirm rows landed with the right `ip_address`/`matched_path`/`status_code`/`actor_id`.
- Test `delete_older_than` directly: seed synthetic old + new rows, run cleanup, assert only the new rows remain.
- Test `GET /api/v1/admin/users/{cid}/ip-history`: correctly permission-gated, returns paginated results scoped to the right `actor_id` only.
- Confirm `log_requests`'s existing stdout/tracing behavior is unchanged — this spec adds a durable sink alongside it, not a replacement.
- Manual smoke test: hit a few endpoints locally, confirm rows appear in `access.ip_request_log` after the flush interval, confirm the cleanup job removes aged-out rows, confirm the new admin endpoint returns a real user's IP history.
