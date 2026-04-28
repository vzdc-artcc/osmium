# Auth API

## Purpose

Provides user session login/logout and service-account introspection.

## Main Routes

- `GET /api/v1/me`
- `GET /api/v1/auth/service-account/me`
- `GET /api/v1/auth/vatsim/login`
- `GET /api/v1/auth/vatsim/callback`
- `POST /api/v1/auth/logout`

## Notes

- `me` requires a valid session cookie
- `service-account/me` requires a valid bearer token
- `me` requires `auth.read`
- logout only revokes the current session token
- logout requires `auth.delete`
- auth and service-account introspection responses expose grouped permissions
- for local VATSIM OAuth, `auth-dev.vatsim.net` with `VATSIM_CLIENT_AUTH_METHOD=post` is the recommended setup
- the login origin and `VATSIM_REDIRECT_URI` must match exactly to preserve the OAuth state cookie
