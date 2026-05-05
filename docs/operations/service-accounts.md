# Service Accounts

Service accounts are machine actors for bots, app clients, and internal integrations.

User-created API keys are represented internally as service accounts with `kind = 'api_key'`.

## Auth Model

- bearer token presented by client
- API hashes the raw bearer token
- hash matched against `access.service_account_credentials.secret_hash`
- matching credential must be active and not expired

For user-created API keys:

- the raw secret is returned once from `POST /api/v1/api-keys`
- later reads expose only display metadata such as `prefix` and `last_four`
- revoking a key sets the credential as revoked and disables the backing service-account row

## Current Route Support

- `GET /api/v1/auth/service-account/me`
- `GET /api/v1/api-keys`
- `GET /api/v1/api-keys/{key_id}`
- `POST /api/v1/api-keys`
- `PATCH /api/v1/api-keys/{key_id}`
- `DELETE /api/v1/api-keys/{key_id}`

`GET /api/v1/auth/service-account/me` remains the canonical way to verify bearer-token identity and effective access after a key is created.

## Least Privilege

Assign only the roles required for the client’s actual responsibilities.

## Future Expansion

More routes can be opened to service accounts once the handler-level actor and audit assumptions are generalized further.
