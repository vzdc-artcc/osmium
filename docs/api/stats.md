# Stats API

## Purpose

Expose ARTCC summary and controller-level statistics.

## Response Timezones

Timestamped stats responses such as controller event `occurred_at` and dataset refresh timestamps follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `GET /api/v1/stats/artcc`
- `GET /api/v1/stats/controller/{cid}/history`
- `GET /api/v1/stats/controller/{cid}/totals`
- `GET /api/v1/stats/controller-events`
- `GET /api/v1/admin/stats/prefixes`
- `PATCH /api/v1/admin/stats/prefixes`

## Notes

- `artcc`, `history`, and `totals` support an `environment` query with `live`, `sweatbox1`, or `sweatbox2`; default is `live`
- controller stats now track online session time separately from active facility-bucket time
- `controller-events` is intended for bot/service-account consumers and requires integration permissions
- readiness uses live-feed job staleness to reflect stats sync health
- `artcc`, `controller-events`, `controller/{cid}/history`, and `controller/{cid}/totals` are intentionally public/unauthenticated — no permission check. `admin/stats/prefixes` is the only permission-gated route in this file (`stats.prefixes.read` / `stats.prefixes.update`)

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
