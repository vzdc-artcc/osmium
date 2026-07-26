# Workflow APIs

## Purpose

Cover the new backend-owned workflow domains that previously lived in website actions and cron routes.

## Response Timezones

Timestamped workflow responses for LOAs, solo certifications, staffing requests, SUA requests, controller lifecycle operations, and job runs follow the shared response-timezone contract.

- default: authenticated user's stored timezone
- fallback: UTC/Zulu for anonymous or non-user contexts
- override header: `X-Response-Timezone: user|zulu|<IANA timezone>`

All paginated workflow list routes now use the shared envelope with canonical `page` and `page_size` inputs. `limit` and `offset` remain accepted as compatibility aliases.

## Main Routes

Self-service routes:

- `GET /api/v1/loa/me`
- `POST /api/v1/loa/me`
- `PATCH /api/v1/loa/{loa_id}`
- `POST /api/v1/loa/{loa_id}/cancel`
- `GET /api/v1/users/{cid}/solo-certifications`
- `GET /api/v1/users/{cid}/certifications`
- `POST /api/v1/users/{cid}/certifications`
- `GET /api/v1/staffing-requests/me`
- `POST /api/v1/staffing-requests/me`
- `GET /api/v1/sua/me`
- `POST /api/v1/sua/me`
- `GET /api/v1/sua/{mission_id}` (public, no auth required — `mission_id` may be the internal id or the human-readable `mission_number`)
- `DELETE /api/v1/sua/{mission_id}`
- `GET /api/v1/sua/upcoming` (public, no auth required)

Administrative routes:

- `GET /api/v1/admin/jobs`
- `GET /api/v1/admin/jobs/{job_name}`
- `POST /api/v1/admin/jobs/{job_name}/run`
- `GET /api/v1/admin/loa`
- `PATCH /api/v1/admin/loa/{loa_id}/decision`
- `POST /api/v1/admin/loa/expire-run`
- `GET /api/v1/admin/solo-certifications`
- `POST /api/v1/admin/solo-certifications`
- `PATCH /api/v1/admin/solo-certifications/{solo_id}`
- `DELETE /api/v1/admin/solo-certifications/{solo_id}`
- `GET /api/v1/admin/certification-types`
- `POST /api/v1/admin/certification-types` (create when `id` omitted, else update; replaces the allowed-option set)
- `PATCH /api/v1/admin/certification-types/order`
- `DELETE /api/v1/admin/certification-types/{id}`
- `GET /api/v1/admin/staffing-requests`
- `DELETE /api/v1/admin/staffing-requests/{request_id}`
- `GET /api/v1/admin/sua`
- `PATCH /api/v1/admin/users/{cid}/controller-lifecycle`
- `GET /api/v1/admin/roster/purge-candidates`

## LOA Notes

- LOA create, update, and self-cancel all require `auth.profile.update`
- LOA start and end must be valid future ranges and meet the backend minimum duration policy
- self-service update (`PATCH /loa/{loa_id}`) works for `PENDING`, `APPROVED`, or `DENIED` LOAs — it resets the LOA back to `PENDING` (clearing any prior decision), matching "editing an approved LOA cancels the approval". It is rejected (`404`) once the LOA is `INACTIVE`.
- self-service cancel (`POST /loa/{loa_id}/cancel`) closes the caller's own LOA (sets it `INACTIVE`) from any non-`INACTIVE` status, with no body — this is the only way a plain user can withdraw their own LOA; it does not touch `decided_at`/`decided_by_actor_id` since it isn't a staff decision
- admin LOA filters support `limit`, `offset`, `status`, `cid`, and `display_name` (contains, case-insensitive) against the LOA owner
- LOA decisions (admin-only, via `PATCH /admin/loa/{loa_id}/decision`) accept `APPROVED`, `DENIED`, or `INACTIVE` — `EXPIRED` is not a settable decision value (corrected here: it was previously listed as one, but `normalize_loa_admin_status` never accepts it and automatic expiration sets `INACTIVE`, not `EXPIRED`)
- manual expiration runs (`POST /admin/loa/expire-run`) set expired `APPROVED` LOAs to `INACTIVE` and record a durable job run and audit entry

Example create body:

```json
{
  "start": "2026-05-20T00:00:00Z",
  "end": "2026-05-30T00:00:00Z",
  "reason": "Travel"
}
```

## Certification Type & Grid Notes

- `GET /api/v1/admin/certification-types` returns every certification type with its allowed `CertificationOption` set (`certification_options`, one of `NONE`, `UNRESTRICTED`, `DEL`, `GND`, `TWR`, `APP`, `CTR`, `TIER_1`, `CERTIFIED`, `SOLO`)
- `POST /api/v1/admin/certification-types` upserts a type: omit `id` to create, include it to update. `name` is 1–20 chars; `certification_options` fully replaces the allowed set. Updating is rejected (`409 Conflict`) if a lesson roster change still grants an option being removed
- `PATCH /api/v1/admin/certification-types/order` accepts `{ "items": [{ "id", "order" }] }` and rewrites `sort_order`
- `POST /api/v1/users/{cid}/certifications` bulk-saves a controller's grid: `{ "certifications": [{ "certification_type_id", "certification_option" }], "dossier_message" }`. Each entry upserts on `(user_id, certification_type_id)`; a non-empty `dossier_message` is required and written to the controller's dossier
- roster sync auto-grants `UNRESTRICTED` for every `auto_assign_unrestricted` type to controllers rated S1+ who do not already hold a non-`NONE` option for it

## Solo Certification Notes

- self-service reads allow the owner to inspect their own active and expired solo records
- admin list filters support `limit`, `offset`, and optional `cid`
- create requires `user_id`, `certification_type_id`, `position`, and future `expires`
- updates can change `certification_type_id`, `position`, and `expires`
- delete removes the certification and can trigger the existing solo notification flow

Example create body:

```json
{
  "user_id": "user_uuid",
  "certification_type_id": "cert_type_uuid",
  "position": "DCA_GND",
  "expires": "2026-06-01T00:00:00Z"
}
```

## Certification Notes

- `GET /api/v1/users/{cid}/certifications` returns one row per certification type (`org.certification_types`), ordered by the type's configured sort order
- ungranted types are returned explicitly with `certification_option: "NONE"` rather than being omitted, so a client can render every type's chip without a separate catalog fetch
- read-only; certifications are still granted indirectly via the training-session roster-change flow, not through this endpoint
- same self/`users.directory.read` access split as solo certifications, unpaginated (bounded by the certification-type catalog)

## Staffing Request Notes

- staffing request create requires non-empty `name` and `description`
- self-service listing returns the current user only
- admin listing supports `limit`, `offset`, `cid` (exact match), and `display_name`
  (contains, case-insensitive) against the submitting user
- `StaffingRequestItem` includes the submitting user's denormalized `cid`,
  `display_name`, and `email`
- admin delete is the current resolution flow
- admin list/delete are gated by dedicated `org.staffing_requests.read`/`.delete`
  permissions (not the general Users-directory permissions used elsewhere in this
  file), granted to both `STAFF` and `EVENT_STAFF`

Example create body:

```json
{
  "name": "More mentors for tower prep",
  "description": "Need additional coverage for evening sessions."
}
```

## SUA Request Notes

- SUA requests require `afiliation`, `start_at`, `end_at`, `details`, and at least one airspace block
- validation enforces future windows, minimum (30 min) and maximum (12h) duration, and the per-user active request limit (2)
- `bottom_altitude`/`top_altitude` must each be exactly 3 ASCII digits (a flight level with no `FL` prefix, e.g. `"000"` for the surface, `"180"` for FL180) — anything else is a `400`
- admin listing supports `limit`, `offset`, and optional `cid`
- self-service delete is restricted to the original owner
- `GET /api/v1/sua/{mission_id}` is a public, unauthenticated single-mission
  lookup matching either the internal id or `mission_number` — mission
  details were never gated to the owner on the legacy site either (only
  deleting was), since a controller who only has the mission number needs to
  be able to look it up. Response omits `user_id`.
- `GET /api/v1/sua/upcoming` is a public, unauthenticated feed of missions
  starting within the next 2 hours (limit 10, newest-starting first), for
  external clients (e.g. controller plugins) that poll for active/upcoming
  airspace blocks. Each call also expires (deletes) any mission more than an
  hour past its `end_at` — the same "expire on read" behavior the legacy
  website route had, not a scheduled job. Items omit `user_id`.

Example create body:

```json
{
  "afiliation": "CAP",
  "start_at": "2026-05-20T14:00:00Z",
  "end_at": "2026-05-20T16:00:00Z",
  "details": "Training sortie",
  "airspace": [
    {
      "identifier": "R-6608A",
      "bottom_altitude": "000",
      "top_altitude": "180"
    }
  ]
}
```

## Controller Lifecycle Notes

- lifecycle updates centralize controller status changes, ARTCC parity, cleanup on demotion, and operating-initial assignment
- request body uses `controller_status`, optional `artcc`, and optional `cleanup_on_none`
- `NONE` transitions can remove training assignments, assignment requests, and LOAs in one backend-owned operation
- transitioning to `NONE` (a purge) additionally requires the `users.controller_status.delete` permission (ATM/DATM-only by default) on top of the base `users.controller_status.update` every caller needs — a purge is far more destructive than an ordinary status change
- a `NONE` transition also removes the controller from VATUSA's real facility roster (the DELETE counterpart of the visitor-application approval flow's `manageVisitor` sync) before any local DB mutation — if VATUSA's API call fails, the whole request fails and nothing local changes; skipped only when the membership is already `NONE`
- a best-effort `roster.removed` email is sent to the purged controller once the transition commits

Example body:

```json
{
  "controller_status": "NONE",
  "artcc": null,
  "cleanup_on_none": true
}
```

`GET /api/v1/admin/roster/purge-candidates?year=&start_month=&end_month=` returns per-controller activity (controlling hours, training hours given/received, open broadcast count, join date, and whether the controller currently has an active `APPROVED` LOA) for every `HOME`/`VISITOR` roster member, bounded to the given zero-indexed month range within one year — powers the roster Purge Assistant's selection table. `start_month`/`end_month` must both be `0`-`11` with `start_month <= end_month`. Controlling hours are drawn from the `live` network environment only (never sweatbox/training time). Excludes OBS-rated controllers with a pending training assignment request (they're never purge candidates). `has_active_approved_loa` reflects an LOA that is approved *right now* — not "has ever had one approved" (a correction from the legacy site's equivalent check, which effectively never re-included someone once any past LOA had been approved).

## Jobs Notes

- jobs expose durable run state with `last_started_at`, `last_finished_at`, `last_success_at`, `last_result_ok`, and `last_error`
- current manual runs cover the backend-owned timed workflows such as LOA expiration, solo expiration, event automation, and roster post-processing
- job-run responses include the persisted run record plus any summarized result payload
