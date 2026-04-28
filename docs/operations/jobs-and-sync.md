# Jobs and Sync

Osmium currently includes a stats sync worker and startup migration behavior that matter operationally.

## Startup Migrations

If `RUN_MIGRATIONS_ON_STARTUP=true`, the app attempts to apply migrations before serving requests.

## Stats Sync

The stats worker updates controller-hour data and exposes health information through the readiness endpoint.

Important fields:

- `last_started_at`
- `last_finished_at`
- `last_success_at`
- `last_result_ok`
- `last_error`
- `processed`
- `online`

## Staleness

Readiness uses `STATS_SYNC_STALE_SECS` to decide whether job health is stale enough to mark the app as degraded.
