# Auth API

## Purpose

Provides user session login/logout, self-service profile management, TeamSpeak UID management, and service-account introspection.

## Main Routes

- `GET /api/v1/me`
- `PATCH /api/v1/me`
- `GET /api/v1/me/teamspeak-uids`
- `POST /api/v1/me/teamspeak-uids`
- `DELETE /api/v1/me/teamspeak-uids/{identity_id}`
- `GET /api/v1/auth/service-account/me`
- `GET /api/v1/auth/vatsim/login`
- `GET /api/v1/auth/vatsim/callback`
- `POST /api/v1/auth/logout`

## Self-Service Profile Surface

`GET /api/v1/me` returns:

- the current session identity fields
- grouped effective permissions
- a `profile` block with:
  - `first_name`
  - `last_name`
  - `preferred_name`
  - `bio`
  - `timezone`
  - `receive_event_notifications`
  - `operating_initials`
- a self-only `teamspeak_uids` collection

`PATCH /api/v1/me` supports partial updates for:

- `preferred_name`
- `timezone`
- `bio`
- `receive_event_notifications`

Behavior notes:

- omitted fields are unchanged
- `preferred_name: null` clears the stored preferred name
- `bio: null` clears the stored bio
- timezone values must be valid IANA timezone names such as `America/Chicago`
- the public field name is `receive_event_notifications`, while persistence continues to use `new_event_notifications`
- this route does not change `display_name`
- this route cannot change `roles`, `permissions`, or permission overrides
- access changes belong to `POST /api/v1/admin/users/{cid}/access`

## TeamSpeak UID Management

TeamSpeak UIDs are managed only through the auth/self-service surface.

- `GET /api/v1/me/teamspeak-uids` lists the caller's linked UIDs
- `POST /api/v1/me/teamspeak-uids` adds a UID
- `DELETE /api/v1/me/teamspeak-uids/{identity_id}` removes a UID owned by the caller

Behavior notes:

- UIDs are stored in `identity.user_identities` with `provider = 'TEAMSPEAK'`
- UID case is preserved as submitted
- adding the same UID for the same user is idempotent
- adding a UID already linked to another user returns `bad_request`
- TeamSpeak UIDs are intentionally not exposed in the normal CID-based user detail routes

## Notes

- `me` requires a valid session cookie
- `patch me` and TeamSpeak UID management also require a valid session cookie
- `service-account/me` requires a valid bearer token
- `me`, `patch me`, and TeamSpeak UID routes require `auth.read`
- logout only revokes the current session token
- logout requires `auth.delete`
- auth and service-account introspection responses expose grouped permissions
- first login now bootstraps `identity.user_profiles`, `org.memberships`, and `org.memberships.operating_initials` in one transaction
- operating initials are generated as unique two-letter values on first login and then preserved unless changed separately in the future
- for local VATSIM OAuth, `auth-dev.vatsim.net` with `VATSIM_CLIENT_AUTH_METHOD=post` is the recommended setup
- the login origin and `VATSIM_REDIRECT_URI` must match exactly to preserve the OAuth state cookie
- if `OSMIUM_SERVER_ADMIN_CID` matches the logging-in user, that login claims or transfers the singleton `SERVER_ADMIN` role
