# Auth and Access

Osmium supports both human and machine authentication.

## Human Authentication

Primary human auth flow:

- `GET /api/v1/auth/vatsim/login`
- `GET /api/v1/auth/vatsim/callback`
- `POST /api/v1/auth/logout`
- `GET /api/v1/me`
- `PATCH /api/v1/me`
- `GET /api/v1/me/teamspeak-uids`
- `POST /api/v1/me/teamspeak-uids`
- `DELETE /api/v1/me/teamspeak-uids/{identity_id}`

The callback now bootstraps the user in a single transaction before creating a session row in `identity.sessions`.

Bootstrap steps:

- upsert `identity.users`
- ensure `identity.user_profiles`
- ensure `org.memberships`
- generate `org.memberships.operating_initials` if it is still null

The dev login route follows the same bootstrap path.

## Self-Service Identity Data

The authenticated self-service surface under `/api/v1/me` owns:

- `preferred_name`
- `bio`
- `timezone`
- `receive_event_notifications`
- self-visible TeamSpeak UID linkage

TeamSpeak UIDs are modeled as linked identities in `identity.user_identities` with `provider = 'TEAMSPEAK'`.

## Operating Initials

Operating initials live on `org.memberships.operating_initials`.

- they are generated automatically on first login when absent
- generation is deterministic and two-letter only
- uniqueness is enforced at the database layer
- once present, login does not regenerate or overwrite them

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
- `SERVER_ADMIN` is a reserved singleton human role
- `SERVER_ADMIN` resolves to every current permission in `access.permissions`, including permissions added later
- `SERVER_ADMIN` is claimed or transferred on successful login when `OSMIUM_SERVER_ADMIN_CID` matches that user's CID

## Important Permissions

- `auth.read`
- `auth.delete`
- `files.read`
- `users.read`
- `users.update`
- `training.read`
- `training.create`
- `training.update`
- `training.manage`
  Training routes are now split across read/create/update/manage, with `training.manage` acting as the umbrella training permission.
- `feedback.update`
- `files.create`
  This is no longer part of the default `USER` role. Uploads require elevated access.
- `files.update`
- `events.update`

## Default Human Access

Newly logged-in users receive the baseline `USER` role.

If `OSMIUM_SERVER_ADMIN_CID` matches the logging-in user, Osmium replaces that user's normal human roles and direct permission overrides with the singleton `SERVER_ADMIN` role instead.

- `USER` is read-mostly by default
- `USER` can read its own auth/session info
- `USER` can browse public files and files explicitly visible to that user
- `USER` can submit feedback and view their own feedback surfaces
- `USER` can sign up for event positions as themselves
- `USER` cannot upload files by default

Future VATUSA integration should sync external role data into Osmium roles. Osmium remains responsible for mapping roles to effective permissions.

## Human-Only vs Machine-Ready

Machine-ready today:

- auth introspection
- ACL evaluation
- service-account permission lookup

Human-oriented handlers still dominate the current app behavior because some writes still assume a current human actor for ownership or audit context.
