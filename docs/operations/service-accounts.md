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
- `GET /api/v1/admin/integrations/discord/configs`
- `GET /api/v1/admin/integrations/discord/features`
- `PATCH /api/v1/admin/integrations/discord/features`
- `GET /api/v1/admin/integrations/discord/guilds`
- `GET /api/v1/admin/integrations/discord/guilds/{guild_id}/discovery`
- `POST /api/v1/admin/integrations/discord/impromptu-claims`
- `GET /api/v1/api-keys`
- `GET /api/v1/api-keys/{key_id}`
- `POST /api/v1/api-keys`
- `PATCH /api/v1/api-keys/{key_id}`
- `DELETE /api/v1/api-keys/{key_id}`

`GET /api/v1/auth/service-account/me` remains the canonical way to verify bearer-token identity and effective access after a key is created.

## Session-Only Integration Routes

The routes below require `integrations.stats.update` and also a signed-in user session. They return `401` to a bearer token, even one that holds `integrations.stats.update`. This is deliberate: the handlers resolve the caller as a user (`ensure_integrations_manage` in `src/handlers/integrations.rs`), and the announcement and event-queue routes record that user on their audit rows and job payloads.

- `POST /api/v1/admin/integrations/discord/configs`
- `PATCH`/`DELETE /api/v1/admin/integrations/discord/configs/{config_id}`
- `POST /api/v1/admin/integrations/discord/channels`
- `PATCH`/`DELETE /api/v1/admin/integrations/discord/channels/{channel_id}`
- `POST /api/v1/admin/integrations/discord/roles`
- `PATCH`/`DELETE /api/v1/admin/integrations/discord/roles/{role_id}`
- `POST /api/v1/admin/integrations/discord/categories`
- `PATCH`/`DELETE /api/v1/admin/integrations/discord/categories/{category_id}`
- `GET /api/v1/admin/integrations/outbound-jobs`
- `POST /api/v1/admin/integrations/outbound-jobs/run`
- `POST /api/v1/admin/notifications/announcements`
- `POST /api/v1/events/{event_id}/publish/discord`
- `POST /api/v1/events/{event_id}/discord-event`

## Least Privilege

Assign only the roles required for the client’s actual responsibilities.

## Future Expansion

More routes can be opened to service accounts once the handler-level actor and audit assumptions are generalized further.
