# Jobs and Sync

Osmium currently includes a stats sync worker, a roster sync worker, and startup migration behavior that matter operationally.

## Startup Migrations

If `RUN_MIGRATIONS_ON_STARTUP=true`, the app attempts to apply migrations before serving requests.

## Stats Sync

The stats worker polls the live, sweatbox1, and sweatbox2 controller feeds, persists ZDC controller sessions and activation spans, and exposes health information through the readiness endpoint.

Important fields:

- per-environment `last_started_at`
- per-environment `last_finished_at`
- per-environment `last_success_at`
- per-environment `last_result_ok`
- per-environment `last_error`
- per-environment `processed`
- per-environment `online`
- per-environment `source_updated_at`

## Staleness

Readiness uses `STATS_SYNC_STALE_SECS` against the `live` feed only to decide whether job health is stale enough to mark the app as degraded. Sweatbox health is reported but does not fail readiness by itself.

## Roster Sync

The roster worker polls VATUSA for the configured facility roster, refreshes matching local users, and demotes local users who are no longer present on the external roster.

Important fields:

- `enabled`
- `last_started_at`
- `last_finished_at`
- `last_success_at`
- `last_result_ok`
- `last_error`
- `processed`
- `matched`
- `updated`
- `demoted`
- `skipped`

This worker is reported through `/ready`, but it does not currently affect readiness status.
