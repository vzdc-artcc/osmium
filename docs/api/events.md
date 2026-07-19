# Events API

## Purpose

Create, update, delete, and staff events.

## Response Timezones

Timestamped event responses such as `starts_at`, `ends_at`, `created_at`, `updated_at`, and event-ops timestamps follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `/api/v1/events`
- `/api/v1/events/{event_id}`
- `/api/v1/events/{event_id}/positions`
- `/api/v1/events/{event_id}/positions/{position_id}`
- `/api/v1/users/{cid}/event-positions`
- `/api/v1/events/{event_id}/positions/publish`
- `/api/v1/events/{event_id}/ops-plan`
- `/api/v1/events/{event_id}/tmis`
- `/api/v1/events/{event_id}/tmis/{tmi_id}`
- `/api/v1/events/{event_id}/preset-positions`
- `/api/v1/events/{event_id}/positions/lock`
- `/api/v1/events/{event_id}/positions/unlock`
- `/api/v1/events/{event_id}/publish/discord`

## Access

- list and get are public to the API consumer side
- mutation currently requires `events.update`
- event position signup requires an authenticated user session
- event position signup is self-service and stores the requesting user on the position record
- ops-plan and TMI routes are backend-owned event-management surfaces built on the existing `events.events` and `events.event_tmis` tables
- event Discord publish now queues an outbound integration job instead of requiring the website to call the bot directly
- `GET /api/v1/users/{cid}/event-positions` is a user-scoped view (not per-event): every **published** position the user has ever held across all events, most recent event first, with `final_position`/`final_start_time`/`final_end_time` included — self-readable for the matching user, otherwise requires `users.directory.read`; unpaginated (typically bounded to one user's history)

List routes for events, event positions, and event TMIs now use the shared pagination envelope with canonical `page` and `page_size` inputs plus compatibility aliases for `limit` and `offset`.

## Request Shapes

Ops-plan update:

```json
{
  "featured_fields": ["airports", "routes"],
  "preset_positions": ["DCA_GND", "IAD_APP"],
  "featured_field_configs": {
    "airports": ["KDCA", "KIAD"]
  },
  "tmis": "MIT 20 NM north gate",
  "ops_free_text": "Expect heavy departure push.",
  "ops_plan_published": true,
  "ops_planner_id": "user_uuid",
  "enable_buffer_times": true
}
```

TMI create:

```json
{
  "tmi_type": "MIT",
  "start_time": "2026-05-20T18:00:00Z",
  "notes": "Expect traffic compression after 1800Z."
}
```

Preset positions update:

```json
{
  "preset_positions": ["DCA_GND", "DCA_TWR", "PCT_APP"]
}
```
