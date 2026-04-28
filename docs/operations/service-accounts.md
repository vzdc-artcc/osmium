# Service Accounts

Service accounts are machine actors for bots, app clients, and internal integrations.

## Auth Model

- bearer token presented by client
- API hashes the raw bearer token
- hash matched against `access.service_account_credentials.secret_hash`
- matching credential must be active and not expired

## Current Route Support

- `GET /api/v1/auth/service-account/me`

This route is the current canonical way to verify service-account identity and effective access.

## Least Privilege

Assign only the roles required for the client’s actual responsibilities.

## Future Expansion

More routes can be opened to service accounts once the handler-level actor and audit assumptions are generalized further.
