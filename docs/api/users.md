# Users API

## Purpose

Expose roster, user detail, visitor membership, visitor application, and user feedback views.

## Response Timezones

Timestamped user-domain responses such as visitor applications, solo certifications, feedback entries, and refresh metadata follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `GET /api/v1/users`
- `GET /api/v1/users/{cid}`
- `GET /api/v1/users/{cid}/solo-certifications`
- `GET /api/v1/users/{cid}/dossier`
- `POST /api/v1/users/refresh-vatusa`
- `GET /api/v1/users/visitor-application`
- `POST /api/v1/users/visitor-application`
- `POST /api/v1/users/visit-artcc`
- `GET /api/v1/users/{cid}/feedback`

## Access Rules

- all routes require an authenticated user session
- viewing private fields depends on `users.read`, `users.update`, or self-access
- user detail responses expose grouped effective permissions

## Notes

- `GET /api/v1/users` and `GET /api/v1/users/{cid}/feedback` now use the shared pagination envelope.
- `POST /api/v1/users/visitor-application` is the primary visitor workflow and upserts one current application per user
- `GET /api/v1/users/visitor-application` returns the caller's current application or `null` when none exists
- `POST /api/v1/users/visit-artcc` remains available as a legacy/manual compatibility shortcut
- `POST /api/v1/users/refresh-vatusa` refreshes the caller from VATUSA using the same single-user membership rules as roster sync, including off-roster demotion
- roster detail responses now include stored membership parity fields such as `membership_status`, `join_date`, `home_facility`, `visitor_home_facility`, and `is_active` when full profile access is allowed
- `GET /api/v1/users/{cid}/solo-certifications` is self-readable for the matching user and staff-readable through `users.directory.read`
- `GET /api/v1/users/{cid}/dossier` is self-readable for the matching user and otherwise requires the training read path
