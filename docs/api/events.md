# Events API

## Purpose

Create, update, delete, and staff events — including position signup/assignment,
named reusable position-preset bundles, ops plans (TMIs, free text, attached
files), and lifecycle automation (auto-lock, auto-archive).

## Response Timezones

Timestamped event responses such as `starts_at`, `ends_at`, `created_at`, `updated_at`, and event-ops timestamps follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

Events:

- `/api/v1/events`
- `/api/v1/events/{event_id}`
- `/api/v1/events/{event_id}/positions`
- `/api/v1/events/{event_id}/positions/{position_id}`
- `/api/v1/events/{event_id}/positions/publish`
- `/api/v1/events/{event_id}/positions/lock`
- `/api/v1/events/{event_id}/positions/unlock`
- `/api/v1/users/{cid}/event-positions`
- `/api/v1/events/{event_id}/publish/discord`

Ops plan / TMIs / preset positions (single-event scoped):

- `/api/v1/events/{event_id}/ops-plan`
- `/api/v1/events/{event_id}/tmis`
- `/api/v1/events/{event_id}/tmis/{tmi_id}`
- `/api/v1/events/{event_id}/preset-positions`
- `/api/v1/events/{event_id}/ops-plan/files`
- `/api/v1/events/{event_id}/ops-plan/files/{file_id}`

Named event-position-preset bundles (standalone, not scoped to one event):

- `/api/v1/event-position-presets`
- `/api/v1/event-position-presets/{preset_id}`

## Access

- event list/get, position list, ops-plan get, TMI list, preset-positions get, and ops-plan-files list are public
- `POST /events` requires `events.items.create`; `PATCH /events/{event_id}` and every `event-ops` mutation (ops-plan, TMIs, preset-positions, lock/unlock) require `events.items.update`; `DELETE /events/{event_id}` requires `events.items.delete`
- event position self-signup (`POST /events/{event_id}/positions`) requires an authenticated session with `events.positions.self.request`
- `PATCH /events/{event_id}/positions/{position_id}` (reassign/finalize/publish-toggle/status) requires `events.positions.assign`; `DELETE` requires `events.positions.delete`; bulk `POST .../positions/publish` requires `events.positions.publish`
- named event-position-preset bundles (`/event-position-presets*`) are an admin-only tool — even listing requires `events.presets.read`, plus `.create`/`.update`/`.delete` for mutation
- ops-plan file attachments: listing is public (the published ops-plan page shows them), `POST`/`DELETE` require `events.ops_plan_files.create`/`.delete`
- `GET /api/v1/users/{cid}/event-positions` is a user-scoped view (not per-event): every **published** position the user has ever held across all events, most recent event first, with `final_position`/`final_start_time`/`final_end_time` included — self-readable for the matching user, otherwise requires `users.directory.read`; uses the shared pagination envelope (`page`/`page_size`, default 25, max 200) so a profile can ask for just the latest few
- event Discord publish (`POST /events/{event_id}/publish/discord`) queues an outbound integration job (`discord.event_positions_published` in `integration.outbound_jobs`) rather than calling a bot directly, and is gated by `integrations.stats.update` (an integrations-domain permission, not an events one) since it lives in `handlers::integrations`
- create Discord scheduled event (`POST /events/{event_id}/discord-event`, body `{ location? }`) queues a `discord.scheduled_event` outbound job; the bot creates a native Discord scheduled event (Events tab) from the event's title/description/start/end as an External event at `location` (default `vatsim.net`). Same `integrations.stats.update` gate; unrelated to the event's positions

List routes for events, event positions, event TMIs, and event-position presets use the shared pagination envelope with canonical `page` and `page_size` inputs plus compatibility aliases for `limit` and `offset`.

## Event fields

Beyond the identity/scheduling fields, `Event` exposes:

- `banner_asset_id` — optional, settable on create and update (`null` clears it on update; omit the field to leave it untouched)
- `hidden` — visibility toggle, settable on update
- `positions_locked` — read-only here; toggled via the dedicated lock/unlock routes
- `manual_positions_open` — settable on update; overrides the auto-lock job (see below) for this event
- `archived_at` — read-only; set/cleared via `UpdateEventRequest.archived: bool` (`true` archives if not already archived, `false` un-archives) rather than writing the timestamp directly

## Event positions

`EventPosition` exposes the full request/assignment lifecycle: `requested_position`, `requested_secondary_position`, `notes`, `requested_start_time`/`requested_end_time`, `final_position`, `final_start_time`/`final_end_time`, `final_notes`, `controlling_category`, the `is_instructor`/`is_solo`/`is_ots`/`is_tmu`/`is_cic` classification flags, `published`, and `status` (`OPEN`/`REQUESTED`/`ASSIGNED`/`PUBLISHED`/`CANCELLED`). `callsign`/`requested_slot`/`assigned_slot` are legacy columns kept for backward compatibility — `callsign` is auto-set to `requested_position` on create, and neither is required in the request body. Also denormalized (left-joined, `null` when unassigned or unavailable): `user_cid`, `user_name` (from `identity.users`), `user_rating` (from `org.memberships`), and `user_discord_id` (linked Discord id from `integration.external_sync_mappings`) — added so the positions list, and the Discord event-posting embed, can show who holds a position with their rating and @mention without extra roster/link lookups.

- `POST /events/{event_id}/positions` is normally self-service (submits `status: REQUESTED` for the caller). Passing `user_id` for a *different* user requires `events.positions.assign` on top of the base self-request permission — this is the admin "manual add" path, and immediately sets `status: ASSIGNED` plus any `final_*`/classification fields supplied in the same request, bypassing the request/finalize round-trip.
- `PATCH /events/{event_id}/positions/{position_id}` is a general-purpose update: reassign the user (or clear it with `user_id: null`), set any `final_*` field, toggle the classification flags, flip `published` for a single position (no separate single-position publish route — use this instead), or set `status` explicitly. Assigning a real `user_id` without an explicit `status` in the same request defaults `status` to `ASSIGNED` automatically.

`EventOpsPlanItem` (`GET`/`PATCH .../ops-plan`) similarly denormalizes `ops_planner_cid`/`ops_planner_name` alongside `ops_planner_id` (left-joined, `null` when unassigned).

## Named event-position-preset bundles

`events.event_position_presets` — reusable, named lists of position callsigns (distinct from the single freeform `preset_positions` array stored directly on an event, which `PUT /events/{event_id}/preset-positions` still manages). An admin picks a bundle in the UI and its `positions` array gets copied onto the target event's own `preset_positions`; the bundle itself isn't referenced by id anywhere else.

## Ops plan file attachments

`events.ops_plan_files` — files attached to an event's ops plan (shown on the public ops-plan page once published). `asset_id` optionally references a `media.file_assets` row uploaded via the general file-upload endpoint; `url`/`file_type` can instead point at an external link. Not linked to any particular storage backend — the caller decides how to get bytes onto `asset_id`/`url` before attaching them here.

## Lifecycle automation

A background job (`event_automation`, `src/jobs/event_lifecycle.rs`) runs on an interval (`EVENT_LIFECYCLE_INTERVAL_SECS`, default 300s; disable with `EVENT_LIFECYCLE_ENABLED=false`) and:

- locks positions (`positions_locked = true`) for any event starting within the next 24 hours, unless `manual_positions_open` is set on that event
- archives (`archived_at`, `hidden = true`, `positions_locked = true`, `manual_positions_open = false`, `banner_asset_id = null`, `status = 'ARCHIVED'`) any event that ended more than 24 hours ago and isn't already archived

It reuses the same underlying sweep (`org::jobs::lock_events_near_start`/`archive_ended_events`) as the pre-existing on-demand `POST /api/v1/admin/jobs/event_automation/run` endpoint, so automatic and manually-triggered runs share one history — see `GET /api/v1/admin/jobs/event_automation` for status and recent runs.

## Request Shapes

Create event:

```json
{
  "title": "Fly-In at DCA",
  "event_type": "STANDARD",
  "host": "vZDC",
  "description": "Join us for a fly-in.",
  "banner_asset_id": null,
  "starts_at": "2026-05-20T15:00:00Z",
  "ends_at": "2026-05-20T18:00:00Z"
}
```

Update event (hide + archive are separate toggles, both optional):

```json
{
  "hidden": true,
  "manual_positions_open": false,
  "archived": true
}
```

Create event position (self-service signup):

```json
{
  "requested_position": "PCT_TRACON_APP",
  "requested_secondary_position": "PCT_APP",
  "notes": "First time at this position.",
  "requested_start_time": "2026-05-20T15:00:00Z",
  "requested_end_time": "2026-05-20T18:00:00Z"
}
```

Update event position (finalize + assign in one call):

```json
{
  "user_id": "user_uuid",
  "final_position": "PCT_APP",
  "final_start_time": "2026-05-20T15:00:00Z",
  "final_end_time": "2026-05-20T18:00:00Z",
  "controlling_category": "TERMINAL",
  "is_instructor": true,
  "published": true
}
```

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

Preset positions update (the freeform list on an event itself):

```json
{
  "preset_positions": ["DCA_GND", "DCA_TWR", "PCT_APP"]
}
```

Named event-position-preset bundle create:

```json
{
  "name": "Standard DCA/PCT",
  "positions": ["DCA_GND", "DCA_TWR", "PCT_APP"]
}
```

Ops plan file attach:

```json
{
  "asset_id": "file_asset_uuid",
  "filename": "ops-notes.pdf",
  "url": null,
  "file_type": "application/pdf"
}
```
