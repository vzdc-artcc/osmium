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

## Notes

- `artcc`, `history`, and `totals` support an `environment` query with `live`, `sweatbox1`, or `sweatbox2`; default is `live`
- controller stats now track online session time separately from active facility-bucket time
- `controller-events` is intended for bot/service-account consumers and requires integration permissions
- readiness uses live-feed job staleness to reflect stats sync health
