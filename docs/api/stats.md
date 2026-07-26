# Stats API

## Purpose

Expose ARTCC summary and controller-level statistics.

## Response Timezones

Timestamped stats responses such as controller event `occurred_at` and dataset refresh timestamps follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `GET /api/v1/stats/artcc`
- `GET /api/v1/stats/controller/{cid}/history`
- `GET /api/v1/stats/controller/{cid}/totals`
- `GET /api/v1/stats/controller/{cid}/positions`
- `GET /api/v1/stats/controller-events`
- `GET /api/v1/admin/stats/prefixes`
- `PATCH /api/v1/admin/stats/prefixes`

## Notes

- `artcc`, `history`, `totals`, and `positions` support an `environment` query with `live`, `sweatbox1`, or `sweatbox2`; default is `live`
- controller stats now track online session time separately from active facility-bucket time
- `controller/{cid}/positions` returns the individual online-position sessions behind those aggregates — one row per `stats.controller_activations` record (`position_name`, `facility_name`, `is_primary`, `started_at`, `ended_at`, `active_seconds`), most recent first, using the shared pagination envelope. `ended_at`/`active_seconds` are null for a still-open position. Optional `year`/`month` restrict to a calendar year or a specific month within it (`month` requires `year`; both are `bad_request` if `month` isn't 1-12); omit both for all-time.
- `artcc`'s `monthly` field is a 12-entry, ARTCC-wide (summed across every controller) monthly breakdown — only populated for the full-year view (`all_time=false` and no `month` filter); `null` for a single-month view or `all_time=true`, since there's nothing to break down further in either case
- `controller-events` is intended for bot/service-account consumers and requires integration permissions
- readiness uses live-feed job staleness to reflect stats sync health
- `artcc`, `controller-events`, `controller/{cid}/history`, `controller/{cid}/totals`, and `controller/{cid}/positions` are intentionally public/unauthenticated — no permission check. `admin/stats/prefixes` is the only permission-gated route in this file (`stats.prefixes.read` / `stats.prefixes.update`)

## Statistics Prefixes Notes

- a singleton config row (callsign prefixes that count as this ARTCC's own controllers for stats attribution) — `GET` always returns the one current row, `PATCH` replaces it wholesale
- prefixes are normalized server-side: trimmed, upper-cased, de-duplicated
- reject an empty string after trimming any individual prefix with `bad_request`

Example update body:

```json
{
  "prefixes": ["ZDC", "PCT"]
}
```
