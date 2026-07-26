# Incidents API

## Purpose

Capture and resolve incident reports involving controllers.

## Response Timezones

Timestamped incident responses follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `POST /api/v1/incidents`
- `GET /api/v1/incidents`
- `GET /api/v1/admin/incidents`
- `GET /api/v1/admin/incidents/{incident_id}`
- `PATCH /api/v1/admin/incidents/{incident_id}`

## Access

- create requires `feedback.items.create`
- self list requires `feedback.items_self.read`
- admin list, detail, and closure require `feedback.items.decide`
- `POST /api/v1/incidents` rejects self-reported incidents (`reportee_cid` resolving to the reporter) with `400`

## Request Shapes

Create:

```json
{
  "reportee_cid": 1234567,
  "timestamp": "2026-05-04T20:30:00Z",
  "reason": "Observed coordination issue on frequency.",
  "reporter_callsign": "DAL123",
  "reportee_callsign": "PCT_APP"
}
```

Admin update:

```json
{
  "closed": true,
  "resolution": "Reviewed with the controller and closed."
}
```

List query parameters (`GET /api/v1/incidents` and `GET /api/v1/admin/incidents`):

- `page`
- `page_size`
- `limit`
- `offset`
- `closed`
- `reporter_cid` — exact match on the reporting user's CID
- `reporter_name` — case-insensitive substring match on the reporting user's display name
- `reportee_cid` — exact match on the reported (reviewed) user's CID
- `reportee_name` — case-insensitive substring match on the reported user's display name

## Notes

- incident creation is user-driven and requires an authenticated session; `reportee_cid` is resolved to the internal user id server-side (matching the `target_cid` pattern used by feedback)
- self-service incident reads return incidents where the caller is either the reporter or the reportee
- incident list responses now use the shared pagination envelope
- admin updates currently focus on closure workflow and can trigger the existing `incident.closed` email template
- incident records are stored in `feedback.incident_reports`
- repeated close attempts are rejected when the incident is already closed
