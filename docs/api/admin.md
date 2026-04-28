# Admin API

## Purpose

Administrative access and roster-control operations.

## Main Routes

- `GET /api/v1/admin/acl`
- `GET /api/v1/admin/access/catalog`
- `GET /api/v1/admin/users/{cid}/access`
- `POST /api/v1/admin/users/{cid}/access`
- `PATCH /api/v1/admin/users/{cid}/controller-status`

## Permissions

These routes currently require `users.update`.

## Permission Payloads

- access responses return grouped permissions such as `{ "users": ["read", "update"] }`
- `POST /api/v1/admin/users/{cid}/access` accepts grouped `permissions` and grouped `permission_overrides`
- legacy flat permission overrides are still accepted for compatibility during migration
