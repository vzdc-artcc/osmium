# 010 — IP-Based Rate Limiting with Permission Bypass

**Depends on**: 004 (structural permission enforcement) for the bypass check; no dependency on 009, but should land before 011 (durable IP tracking), which reuses this spec's unified IP-extraction helper.

## Problem

No rate limiting exists anywhere in the API today. Every endpoint — authenticated or not — can be hit at unbounded frequency from a single source IP. `Cargo.toml` has no rate-limiting crate (current stack: axum 0.8.9, tower-http 0.6.6, tower 0.5.2 dev-dependency only).

## Goal

All requests are rate-limited per source IP. A caller (session user or service account) holding a new `SystemRateLimitBypass` permission is exempt. The common case (an IP under its limit) adds **zero database round-trips** to the request path.

## Approach

### Crate

Use **`tower_governor` 0.8.0** rather than hand-rolling middleware around the bare `governor` crate — `tower_governor` already ships first-class axum integration, per-key/IP limiting, and is actively maintained (confirmed current via `cargo info tower_governor` — has explicit `axum` and `tonic` feature flags).

```toml
tower_governor = { version = "0.8", features = ["axum"] }
```

Use a custom `KeyExtractor` — not the crate's default `SmartIpKeyExtractor`/`PeerIpKeyExtractor` — since this app has no `ConnectInfo<SocketAddr>` wired in anywhere and relies entirely on `X-Forwarded-For`/`X-Real-IP` headers. The custom extractor calls the shared `client_ip()` helper (below) so the rate-limit key exactly matches what's already used for audit logging.

### Unify duplicated IP extraction first

Two near-duplicate `client_ip()` implementations exist today:
- `src/repos/audit.rs` — reads `X-Forwarded-For` then `X-Real-IP`, validates via `.parse::<std::net::IpAddr>()`.
- `src/logging.rs` — same header logic, but does **not** validate.

Move the validated version to a new `src/auth/ip.rs`; update both `audit.rs` and `logging.rs` to import from there. This is a small, explicitly-called-out fix bundled into this spec (`logging.rs` gains validation it previously lacked) — not silent scope creep, call it out in the PR description.

### New permission marker

```rust
// src/auth/permissions.rs
permission!(SystemRateLimitBypass, ["system", "rate_limit"], Update);
```

`PermissionAction` (`src/auth/acl.rs`) has no `Bypass` variant (current set: `Read, Create, Update, Delete, Publish, Assign, Decide, Request, Approve, Deny`) — use `Update` rather than growing the enum for one marker. Do **not** reuse `SystemRead` (`["system"], Read`, already defined) — rate-limit bypass is a distinct, more sensitive grant from generic system-read access.

**Bypass policy (confirmed with user): the permission is required for both session users and API-key/service-account callers.** A valid API key alone does not bypass rate limiting — a service account must additionally hold `SystemRateLimitBypass`. This means existing API keys do not bypass by default; an admin must explicitly grant the permission to whichever service accounts need it.

### Bypass check placement — avoid hot-path DB cost

`ensure_permission()` performs a DB query per call. Running it on every request (even ones nowhere near their limit) would add a DB round-trip to the hot path for all traffic, including anonymous reads. Instead:

1. Extract IP via `client_ip()`.
2. Check `tower_governor`'s in-memory limiter for this IP — no DB involved.
3. Under limit → allow, done. Zero DB cost.
4. Over limit → **only now** call `ensure_permission(&state, user, service_account, SystemRateLimitBypass::path())`.
   - Pass → allow, and do not count this request against the limiter (bypass means truly exempt).
   - Fail → return `429 Too Many Requests`.

This keeps the DB permission check confined to the (hopefully small) fraction of traffic that's already over its limit.

### Middleware layer ordering

`src/router.rs` currently (~lines 482-491):

```rust
.layer(middleware::from_fn_with_state(state.clone(), crate::logging::log_requests))
.layer(middleware::from_fn_with_state(state.clone(), resolve_current_user))
.layer(build_cors_layer())
```

axum layers execute outside-in per request, in reverse registration order (the last `.layer()` call registered is the outermost, and runs first). Today's actual execution order: CORS → `resolve_current_user` → `log_requests` → handler.

The new rate-limit layer must be registered **first** (closest to the handler), before `log_requests`:

```rust
.layer(middleware::from_fn_with_state(state.clone(), crate::rate_limit::enforce_rate_limit)) // NEW — innermost
.layer(middleware::from_fn_with_state(state.clone(), crate::logging::log_requests))
.layer(middleware::from_fn_with_state(state.clone(), resolve_current_user))
.layer(build_cors_layer())
```

Resulting execution order: CORS → `resolve_current_user` (populates the `Option<CurrentUser>`/`Option<CurrentServiceAccount>` extensions the bypass check needs) → `log_requests` (wraps the rate limiter, so it still observes and logs the final response, including any `429`s) → rate limiter → handler.

Registering the rate limiter on the *outside* of `log_requests` instead would make throttled requests skip request logging entirely, which is wrong — spec 011 explicitly wants throttled requests tracked too. **Write a small test asserting (a) `CurrentUser`/`CurrentServiceAccount` extensions are populated by the time the rate-limit middleware runs, and (b) a `429` response is still visible to/logged by `log_requests`** — don't rely on layer-ordering intuition alone; axum's layer semantics are easy to get backwards.

### Granularity

Global per-IP, not per-route. The primary threat model (abusive scraping, credential stuffing, brute force from one IP) is about aggregate volume from that IP, not any single endpoint. Mount the layer so it covers the whole router — `/health`, `/ready`, `/docs*`, `/cdn/{file_id}` included, not just `/api/v1/*`. Per-route limits (e.g. a tighter limit specifically on `/api/v1/auth/vatsim/login`) can be layered on later as a follow-up if needed; out of scope here.

### Configuration

Follow `src/config.rs`'s existing env-var parsing style (see `dev_seed_enabled()`/`dev_impersonation_enabled()` for the pattern):

```
RATE_LIMIT_ENABLED           bool, default true
RATE_LIMIT_REQUESTS_PER_MIN  u32,  default TBD — pick based on realistic legitimate-client
                                    rates; check tools/api-load-tester for any existing
                                    baseline numbers before choosing
RATE_LIMIT_BURST             u32,  default TBD — governor's burst/cell capacity
```

`tests/support/mod.rs`'s `EnvVarGuard` pattern should set `RATE_LIMIT_ENABLED=false` by default for the existing test suite, matching how other feature flags are already neutralized there, so unrelated tests don't start tripping the limiter.

### AppState

Add the `tower_governor` limiter instance to `AppState`, constructed once in `AppState::from_env()`. It has no DB dependency, so it should also exist in `without_db()` test contexts (disabled/effectively-infinite quota by default there).

## Affected files

- `Cargo.toml` — add `tower_governor`
- New: `src/rate_limit.rs` — the middleware function and bypass logic
- New: `src/auth/ip.rs` — unified `client_ip()`
- `src/repos/audit.rs`, `src/logging.rs` — updated to import shared `client_ip()`
- `src/auth/permissions.rs` — new `SystemRateLimitBypass` marker
- `src/router.rs` — new layer registration (see exact placement above)
- `src/state.rs` — new rate-limiter field
- `src/config.rs` — new env var parsing functions
- `tests/support/mod.rs` — `RATE_LIMIT_ENABLED=false` added to the standard test env guards

## Verification

- `cargo check`/`cargo build`.
- Unit tests for the unified `client_ip()`, including the "invalid IP in header" case the old `logging.rs` version previously accepted silently.
- New `tests/rate_limiting.rs`, following `tests/permission_gates.rs`'s test-harness pattern:
  - Requests under the configured limit succeed.
  - Requests over the limit return `429` for a caller with no bypass.
  - A service account or session user holding `SystemRateLimitBypass` bypasses; one without it still gets `429`.
  - Two different synthetic `X-Forwarded-For` IPs don't share a counter.
- Manual load test via `tools/api-load-tester` (already a workspace member) to confirm the `429` boundary behaves as configured.
- Confirm `RATE_LIMIT_ENABLED=false` fully disables enforcement end-to-end.
