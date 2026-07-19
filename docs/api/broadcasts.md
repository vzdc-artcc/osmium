# Broadcasts API

## Purpose

Site-wide "what's new" change-broadcast notices — an admin posts a title/description (with an optional linked file), and every user individually tracks whether they've seen and agreed to it.

## Response Timezones

Timestamped broadcast responses (`timestamp`, `updated_at`, `seen_at`, `agreed_at`) follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

Admin routes:

- `GET /api/v1/admin/broadcasts`
- `POST /api/v1/admin/broadcasts`
- `PATCH /api/v1/admin/broadcasts/{broadcast_id}`
- `DELETE /api/v1/admin/broadcasts/{broadcast_id}`

Self-service routes:

- `GET /api/v1/broadcasts/me`
- `POST /api/v1/broadcasts/{broadcast_id}/seen`
- `POST /api/v1/broadcasts/{broadcast_id}/agree`

## Permissions

- admin CRUD requires `web.broadcasts.read` / `web.broadcasts.create` / `web.broadcasts.update` / `web.broadcasts.delete`
- self-service routes require only `auth.profile.read` (the `GET /broadcasts/me` list) or `auth.profile.update` (the `seen`/`agree` actions) — the same self-service permissions every "current user" route in the API reuses, not broadcast-specific permissions

## Notes

- broadcasts are global — every broadcast is visible to every user. There is no per-broadcast targeted-audience concept; a user's "unseen" state is simply the absence of a state row for that broadcast, not membership in some initial recipient set.
- `GET /admin/broadcasts` list items include `seen_count` and `agreed_count` (aggregate, not per-user) for an admin table view — it does not return the full list of who has/hasn't seen a broadcast.
- `GET /broadcasts/me` returns every broadcast with this user's `seen_at`/`agreed_at` (both `null` if never interacted), most recent broadcast first.
- `POST /broadcasts/{id}/seen` and `POST /broadcasts/{id}/agree` are idempotent — calling either again after the state is already set does not clear or overwrite an earlier timestamp. Agreeing implies having seen it.
- `exempt_staff` on create automatically marks the broadcast as agreed (not just seen) for every user holding the `STAFF` role at creation time — this only happens once, at creation; it is not retroactively applied to staff who gain the role later, and toggling `exempt_staff` on an update does not re-run it.
- update bumps `timestamp` to the current time (same as create), which is what re-surfaces an edited broadcast as "new" in a most-recent-first list.
- **not implemented**: the "broadcast posted" email notification and a scheduled stale-broadcast cleanup job — both are separate features from the CRUD surface this API covers, not gaps in these routes.

Example create body:

```json
{
  "title": "New training progression rolled out",
  "description": "See the training team channel for details.",
  "file_id": null,
  "exempt_staff": true
}
```
