# Workflow APIs

## Purpose

Cover the new backend-owned workflow domains that previously lived in website actions and cron routes.

All paginated workflow list routes now use the shared envelope with canonical `page` and `page_size` inputs. `limit` and `offset` remain accepted as compatibility aliases.

## Main Routes

Self-service routes:

- `GET /api/v1/loa/me`
- `POST /api/v1/loa/me`
- `PATCH /api/v1/loa/{loa_id}`
- `GET /api/v1/users/{cid}/solo-certifications`
- `GET /api/v1/staffing-requests/me`
- `POST /api/v1/staffing-requests/me`
- `GET /api/v1/sua/me`
- `POST /api/v1/sua/me`
- `DELETE /api/v1/sua/{mission_id}`

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
- `GET /api/v1/admin/staffing-requests`
- `DELETE /api/v1/admin/staffing-requests/{request_id}`
- `GET /api/v1/admin/sua`
- `PATCH /api/v1/admin/users/{cid}/controller-lifecycle`

## LOA Notes

- LOA create and update require `auth.profile.update`
- LOA start and end must be valid future ranges and meet the backend minimum duration policy
- user updates only apply while the LOA is still `PENDING`
- admin LOA filters support `limit`, `offset`, `status`, and `cid`
- LOA decisions accept `APPROVED`, `DENIED`, `INACTIVE`, or `EXPIRED`
- manual expiration runs record a durable job run and audit entry

Example create body:

```json
{
  "start": "2026-05-20T00:00:00Z",
  "end": "2026-05-30T00:00:00Z",
  "reason": "Travel"
}
```

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

## Staffing Request Notes

- staffing request create requires non-empty `name` and `description`
- self-service listing returns the current user only
- admin listing supports `limit`, `offset`, and optional `cid`
- admin delete is the current resolution flow

Example create body:

```json
{
  "name": "More mentors for tower prep",
  "description": "Need additional coverage for evening sessions."
}
```

## SUA Request Notes

- SUA requests require `afiliation`, `start_at`, `end_at`, `details`, and at least one airspace block
- validation enforces future windows, minimum and maximum duration, altitude formatting, and the per-user active request limit
- admin listing supports `limit`, `offset`, and optional `cid`
- self-service delete is restricted to the original owner

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
      "bottom_altitude": "SFC",
      "top_altitude": "FL180"
    }
  ]
}
```

## Controller Lifecycle Notes

- lifecycle updates centralize controller status changes, ARTCC parity, cleanup on demotion, and operating-initial assignment
- request body uses `controller_status`, optional `artcc`, and optional `cleanup_on_none`
- `NONE` transitions can remove training assignments, assignment requests, and LOAs in one backend-owned operation

Example body:

```json
{
  "controller_status": "NONE",
  "artcc": null,
  "cleanup_on_none": true
}
```

## Jobs Notes

- jobs expose durable run state with `last_started_at`, `last_finished_at`, `last_success_at`, `last_result_ok`, and `last_error`
- current manual runs cover the backend-owned timed workflows such as LOA expiration, solo expiration, event automation, and roster post-processing
- job-run responses include the persisted run record plus any summarized result payload
