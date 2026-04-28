# Request Flow

This page explains the normal request path through the API.

## HTTP Request Path

1. Request hits the Axum router.
2. `resolve_current_user` middleware checks for:
   - session cookie auth
   - bearer token service-account auth
3. Route handler reads `AppState`.
4. Permission checks call the ACL layer.
5. Repos execute SQL against the configured Postgres pool.
6. Handler returns JSON, file content, redirect, or docs HTML.

## Auth Resolution

Human auth:

- cookie name: `osmium_session`
- session lookup: `identity.sessions`
- user lookup: `identity.users`

Machine auth:

- `Authorization: Bearer <token>`
- credential lookup: `access.service_account_credentials`
- service account lookup: `access.service_accounts`

## Permission Resolution

- user role assignments come from `access.user_roles`
- direct permission overrides come from `access.user_permissions`
- permission names are canonical dotted keys such as `users.update`
- effective permissions are exposed through `access.v_effective_user_permissions`
- service-account effective permissions are exposed through `access.v_effective_service_account_permissions`

## Docs Serving Flow

- `/docs` and `/docs/{section}/{page}` render compiled-in markdown
- `/docs/api/v1` is Swagger UI
- `/docs/api/v1/openapi.json` is generated from `utoipa`

## Failure Modes

- no `DATABASE_URL`: DB-backed routes degrade to `service_unavailable`
- missing session or invalid bearer token: `unauthorized`
- permission mismatch: `unauthorized`
- invalid path or query values: `bad_request`
