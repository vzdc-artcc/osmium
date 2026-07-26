# Broadcasts API

## Purpose

Site-wide "what's new" change-broadcast notices — an admin posts a title/description (with an optional linked file) to an explicitly chosen set of recipients, and each recipient individually tracks whether they've seen and agreed to it.

## Response Timezones

Timestamped broadcast responses (`timestamp`, `updated_at`, `seen_at`, `agreed_at`) follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

Admin routes:

- `GET /api/v1/admin/broadcasts`
- `GET /api/v1/admin/broadcasts/{broadcast_id}`
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

## Recipient targeting

- `POST /admin/broadcasts` requires `recipient_groups: string[]` — one or more named cohorts this broadcast is addressed to, resolved to a concrete set of users **server-side, at creation time**. Valid values: `ALL` (all rostered controllers), `HOME_OBS`, `HOME_S1`, `HOME_S2`, `HOME_S3`, `HOME_C1_C3` (home-facility controllers by rating tier), `VISITING` (visiting controllers), `INSTRUCTORS`, `MENTORS`, `ALL_TRAINING_STAFF` (instructors + mentors). Any other value is a `400`. Every group is implicitly scoped to users who currently hold an active controller status (`HOME` or `VISITOR`) and have email notifications enabled, mirroring the live site's own group computation.
- The resolved recipient set is a **snapshot, immutable after creation** — `PATCH /admin/broadcasts/{id}` only updates `title`/`description`/`file_id`/`exempt_staff`, never recipients, and later membership changes (a user's rating changing, a role being granted/revoked) do not retroactively add or remove recipients from an already-posted broadcast.
- `GET /broadcasts/me` only returns broadcasts the caller is an explicit recipient of — it is no longer a global list.
- `POST /broadcasts/{id}/seen` and `POST /broadcasts/{id}/agree` return `404` if the caller isn't a recipient of that broadcast (rather than a distinguishing `401`/`403`, so non-recipients can't probe which broadcast ids exist).
- `exempt_staff` on create adds every user holding the `STAFF` role as an *additional* recipient (on top of whichever `recipient_groups` were chosen) and immediately marks them as agreed — "exempt" means exempt from having to act, not invisible; staff still see it in their own broadcast history, just pre-checked off. This only happens once, at creation; it is not retroactively applied to staff who gain the role later, and toggling `exempt_staff` on an update does not re-run it.

## Notes

- `GET /admin/broadcasts` list items include `seen_count` and `agreed_count` (aggregate, not per-user) for an admin table view.
- `GET /admin/broadcasts/{broadcast_id}` returns the full detail plus every recipient's `cid`/`name`/`seen_at`/`agreed_at` (both null if not yet interacted) — this is what powers an admin "who has/hasn't reviewed" view.
- `GET /broadcasts/me` returns the caller's own recipient broadcasts with `seen_at`/`agreed_at` (both `null` if never interacted), most recent broadcast first.
- `POST /broadcasts/{id}/seen` and `POST /broadcasts/{id}/agree` are idempotent — calling either again after the state is already set does not clear or overwrite an earlier timestamp. Agreeing implies having seen it.
- update bumps `timestamp` to the current time (same as create), which is what re-surfaces an edited broadcast as "new" in a most-recent-first list.
- **not implemented**: the "broadcast posted" email notification and a scheduled stale-broadcast cleanup job — both are separate features from the CRUD surface this API covers, not gaps in these routes.

Example create body:

```json
{
  "title": "New training progression rolled out",
  "description": "See the training team channel for details.",
  "file_id": null,
  "exempt_staff": true,
  "recipient_groups": ["HOME_S1", "HOME_S2", "VISITING"]
}
```
