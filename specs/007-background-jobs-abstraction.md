# 007 — Background Jobs Abstraction

## Problem

`src/jobs/mod.rs` is just three `pub mod` declarations — no shared trait or interface. Each background worker is bespoke:

- `roster_sync.rs`: uses `tokio::time::interval` + `set_missed_tick_behavior`, writes to `state.job_health.roster_sync` (`RosterSyncHealth`, `src/state.rs:44`).
- `stats_sync.rs`: also uses `state.job_health`, but a different sub-field/shape (`StatsSyncHealth`, `src/state.rs:24`, itself split per-environment via `environment_mut`/`environment`), 1360 lines total.
- `email_delivery.rs`: uses `tokio::time::sleep` in a loop (not `interval`), writes to a *separate* struct entirely — `state.email_health.worker` (`EmailWorkerHealth`, defined in `src/email/mod.rs:14`, wrapped by `EmailHealth` in `src/state.rs:18`).

All three hand-roll the same scaffolding: record a start time, run the tick, record success/failure and timestamps, log via `tracing`. `JobHealth` and `EmailHealth` are two differently-shaped health-tracking structs for what's conceptually the same concern (is this background worker healthy, when did it last run, did it succeed).

## Goal

One shared abstraction for "a periodic background job with health tracking," used by all three workers, with a single consistent health-reporting shape.

## Approach

1. Define a small trait in `src/jobs/mod.rs`, e.g.:
   - `fn interval(&self) -> Duration`
   - `async fn tick(&self, state: &AppState) -> Result<TickOutcome, SomeError>` (or similar — exact signature decided at implementation time per the "high-level, no code sketches" scope of this spec)
2. Define one generic health-tracking shape (fields roughly matching what `StatsSyncEnvironmentHealth`/`RosterSyncHealth`/`EmailWorkerHealth` already track in common: `last_started_at`, `last_finished_at`, `last_success_at`, `last_error`, `last_result_ok`, plus a job-specific metrics payload) and one generic runner function that: records start, calls `tick`, records outcome, logs.
3. `stats_sync.rs`'s per-environment health (`live`/`sweatbox1`/`sweatbox2`) is a genuine structural difference (one "job" conceptually running 3 sub-instances) — the shared abstraction should accommodate this as 3 job instances of the same job type, each with its own health record, rather than special-casing it inside a single health struct.
4. Consolidate `state.job_health` and `state.email_health` into one map/registry of job health records using the new shared shape, or keep them as separate `AppState` fields if that's simpler, but make their *internal shape* consistent even if the top-level fields stay separate.

## Affected files/patterns

- `src/jobs/mod.rs` — new trait/runner
- `src/jobs/roster_sync.rs`, `src/jobs/stats_sync.rs`, `src/jobs/email_delivery.rs` — convert to implement the shared trait, remove hand-rolled loop/health-tracking scaffolding
- `src/state.rs` — `JobHealth`, `EmailHealth`, `StatsSyncHealth`, `StatsSyncEnvironmentHealth`, `RosterSyncHealth` structs; consolidate shapes
- `src/email/mod.rs` — `EmailWorkerHealth` struct
- Any handler reading job health for a status/health endpoint (check `src/handlers/health.rs` and `src/handlers/admin.rs` for consumers of `state.job_health`/`state.email_health` before changing field names)

## Ordering

Independent of the other items — can run any time. Suggested as Phase 1 (after the Phase 0 quick wins, before the large repo migration) since it's medium effort and fully self-contained.

## Verification

- `cargo check`/`cargo test` after each job is converted, one at a time (start with `roster_sync.rs`, it's the simplest).
- Confirm `src/handlers/health.rs`'s `/ready` or similar endpoint (and any admin job-status endpoint) still reports correct, equivalent health information after the shape consolidation — add/update a test asserting the health endpoint's JSON shape if one doesn't already exist.
- Manually run the app locally (or via existing dev-data fixtures) and confirm all 3 jobs still start on schedule and update health state as expected, since this touches long-running background loops that aren't easily covered by fast unit tests.
