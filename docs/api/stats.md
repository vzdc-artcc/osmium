# Stats API

## Purpose

Expose ARTCC summary and controller-level statistics.

## Main Routes

- `GET /api/v1/stats/artcc`
- `GET /api/v1/stats/controller/{cid}/history`
- `GET /api/v1/stats/controller/{cid}/totals`

## Notes

- these routes are currently public within the running API surface
- they still require a configured database
- readiness uses job staleness to reflect stats sync health
