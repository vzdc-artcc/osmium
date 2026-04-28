# Auth and Access

Osmium supports both human and machine authentication.

## Human Authentication

Primary human auth flow:

- `GET /api/v1/auth/vatsim/login`
- `GET /api/v1/auth/vatsim/callback`
- `POST /api/v1/auth/logout`
- `GET /api/v1/me`

The callback upserts the user and creates a session row in `identity.sessions`.

For local development, the code still supports `auth-dev.vatsim.net` when `VATSIM_DEV_MODE=true`. In that mode, `post` client authentication should be used, and the login origin must exactly match `VATSIM_REDIRECT_URI` so the OAuth state cookie survives the round trip.

## Dev Login

When `API_DEV_MODE=true`, dev login is available at:

```text
GET /api/v1/auth/login/as/{cid}
```

This is for local development only.

## Service-Account Authentication

Service accounts authenticate with:

```text
Authorization: Bearer <raw-secret>
```

The API hashes the incoming bearer token and matches it against `access.service_account_credentials.secret_hash`.

Current machine-facing introspection route:

```text
GET /api/v1/auth/service-account/me
```

## Access Model

- roles define default capabilities
- permissions are stored canonically as `resource.action`
- API access payloads group them as `{ resource: [action, ...] }`
- direct overrides are rare exceptions
- machine actors also receive roles and effective permissions

## Important Permissions

- `auth.read`
- `auth.delete`
- `users.read`
- `users.update`
- `training.update`
  This currently gates assignment management, release-request moderation, and all training-session CRUD routes.
- `feedback.update`
- `files.create`
- `files.update`
- `events.update`

## Human-Only vs Machine-Ready

Machine-ready today:

- auth introspection
- ACL evaluation
- service-account permission lookup

Human-oriented handlers still dominate the current app behavior because some writes still assume a current human actor for ownership or audit context.
