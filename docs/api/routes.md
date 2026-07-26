# Preferred Routes API

## Purpose

Search self-hosted FAA preferred IFR routes. Replaces the old PRD page, which
proxied a third-party aviation API (`api.aviationapi.com`) at request time and
broke when that dependency became a liability. Osmium now owns a local copy of the
FAA's own data and serves search over it, taking no runtime dependency on a third
party.

## Main Routes

- `GET /api/v1/routes/preferred?origin=&destination=`

## Access

Authenticated members only — not a specific permission. This is public FAA
reference data any logged-in controller can use for flight planning, but it is
deliberately not exposed unauthenticated (nothing accidentally public). Requests
without a session return `401`.

## Query

At least one of `origin` / `destination` must be supplied (else `400`). Both are
airport identifiers (e.g. `KJFK` or `JFK`) matched case-insensitively — inputs are
upper-cased before comparison against the upper-cased stored identifiers.

```
GET /api/v1/routes/preferred?origin=KJFK&destination=KBOS
```

Response:

```json
{
  "routes": [
    {
      "origin": "JFK",
      "destination": "BOS",
      "route_type": "L",
      "sequence": 1,
      "route_string": "JFK GREKI MERIT ROBER BOS",
      "hours": "",
      "area": "",
      "altitude": "",
      "aircraft": "",
      "direction": "",
      "departure_artcc": "ZNY",
      "arrival_artcc": "ZBW",
      "effective_date": "2026-07-09"
    }
  ],
  "count": 1
}
```

## Data source & ingest

The data is populated by the `faa_preferred_routes` background job (see the Jobs
and Sync operations doc). It downloads the current 28-day NASR cycle's
`PFR_RMT_FMT.csv` from the FAA National Flight Data Center and wholesale-replaces
the `routes.preferred_routes` table.

- Source URL (default, env-overridable):
  `https://nfdc.faa.gov/webContent/28DaySub/extra/{DD}_{Mon}_{YYYY}_PFR_CSV.zip`
- The job is **disabled by default** (`FAA_PREFERRED_ROUTES_SYNC_ENABLED=true` to
  opt in) — it is a new outbound integration, so no environment starts contacting
  the FAA by surprise.
- The CSV parser is header-driven (maps by column name, not position) so it
  tolerates the FAA's announced 2026-09-03 NASR format change rather than silently
  mis-parsing.
- A manual refresh is available to job-runners via
  `POST /api/v1/admin/jobs/faa_preferred_routes/run` (gated by
  `users.controller_status.update`, like the other manual job runs).

### Environment variables

- `FAA_PREFERRED_ROUTES_SYNC_ENABLED` (default `false`)
- `FAA_PREFERRED_ROUTES_INTERVAL_SECS` (default `86400`, min `60`) — the job checks
  daily but only downloads when the current cycle isn't already loaded.
- `FAA_PREFERRED_ROUTES_URL` — full URL override (skips cycle-date computation).
- `FAA_PREFERRED_ROUTES_BASE_URL` — base URL override (default the NFDC path above).
- `FAA_PREFERRED_ROUTES_CSV_NAME` — inner CSV name (default `PFR_RMT_FMT.csv`).
