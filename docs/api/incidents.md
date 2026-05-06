# Incidents API

## Purpose

Capture and resolve incident reports involving controllers.

## Main Routes

- `POST /api/v1/incidents`
- `GET /api/v1/incidents`
- `GET /api/v1/admin/incidents`
- `GET /api/v1/admin/incidents/{incident_id}`
- `PATCH /api/v1/admin/incidents/{incident_id}`

## Access

- create requires `feedback.items.create`
- self list requires `feedback.items.self.read`
- admin list, detail, and closure require `feedback.items.decide`

## Request Shapes

Create:

```json
{
  "reportee_id": "user_uuid",
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

List query parameters:

- `page`
- `page_size`
- `limit`
- `offset`
- `closed`

## Notes

- incident creation is user-driven and requires an authenticated session
- self-service incident reads return incidents where the caller is either the reporter or the reportee
- incident list responses now use the shared pagination envelope
- admin updates currently focus on closure workflow and can trigger the existing `incident.closed` email template
- incident records are stored in `feedback.incident_reports`
- repeated close attempts are rejected when the incident is already closed
