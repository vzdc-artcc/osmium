# Admin API

## Purpose

Administrative access and roster-control operations.

## Main Routes

- `GET /api/v1/admin/acl`
- `GET /api/v1/admin/access/catalog`
- `GET /api/v1/admin/visitor-applications`
- `PATCH /api/v1/admin/visitor-applications/{application_id}`
- `GET /api/v1/admin/users/{cid}/access`
- `POST /api/v1/admin/users/{cid}/access`
- `PATCH /api/v1/admin/users/{cid}/controller-status`
- `GET /api/v1/admin/publications`
- `GET /api/v1/admin/publications/{publication_id}`
- `POST /api/v1/admin/publications`
- `PATCH /api/v1/admin/publications/{publication_id}`
- `DELETE /api/v1/admin/publications/{publication_id}`
- `GET /api/v1/admin/publications/categories`
- `POST /api/v1/admin/publications/categories`
- `PATCH /api/v1/admin/publications/categories/{category_id}`
- `DELETE /api/v1/admin/publications/categories/{category_id}`

## Permissions

Most admin routes on this page currently require `users.update`.

Publication and publication-category management requires `web.update`.

## Permission Payloads

- access responses return grouped permissions such as `{ "users": ["read", "update"] }`
- `POST /api/v1/admin/users/{cid}/access` accepts grouped `permissions` and grouped `permission_overrides`
- legacy flat permission overrides are still accepted for compatibility during migration
- visitor application review supports `PENDING`, `APPROVED`, and `DENIED` workflow states
- visitor application approval is further restricted to users with one of the explicit approver roles: `ATM`, `DATM`, `TA`, or `ATA`
- approving a visitor application also calls the VATUSA `manageVisitor` endpoint with the configured `VATUSA_API_KEY`; if that external call fails, the local approval does not complete
