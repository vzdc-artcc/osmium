# Users API

## Purpose

Expose roster, user detail, visitor membership, visitor application, and user feedback views.

## Main Routes

- `GET /api/v1/user`
- `GET /api/v1/user/{cid}`
- `GET /api/v1/user/visitor-application`
- `POST /api/v1/user/visitor-application`
- `POST /api/v1/user/visit-artcc`
- `GET /api/v1/user/{cid}/feedback`

## Access Rules

- all routes require an authenticated user session
- viewing private fields depends on `users.read`, `users.update`, or self-access
- user detail responses expose grouped effective permissions

## Notes

- `POST /api/v1/user/visitor-application` is the primary visitor workflow and upserts one current application per user
- `GET /api/v1/user/visitor-application` returns the caller's current application or `null` when none exists
- `POST /api/v1/user/visit-artcc` remains available as a legacy/manual compatibility shortcut
- roster detail responses now include stored membership parity fields such as `membership_status`, `join_date`, `home_facility`, `visitor_home_facility`, and `is_active` when full profile access is allowed
