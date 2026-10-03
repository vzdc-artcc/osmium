# Integrations API

## Purpose

Expose backend-owned integration and notification orchestration surfaces.

## Response Timezones

Timestamped Discord config and outbound-job responses follow the shared response-timezone contract via `X-Response-Timezone`.

## Discord Identity Routes

- `GET /api/v1/me/discord`
- `POST /api/v1/me/discord/link/start` — body `{ "return_url": "<website page to return to>" }`; returns `{ auth_url }`. The browser is sent to `auth_url`; Discord's `redirect_uri` is osmium's own callback below.
- `GET /api/v1/me/discord/link/callback` — Discord redirects the browser here with `?code&state`. osmium exchanges the code server-side, links the identity, and 302s back to the caller's `return_url` with `?discord_linked=1&discord_username=…` on success or `?discord_error=<reason>` on failure. Public (bound to the user via the `state` token).
- `POST /api/v1/me/discord/unlink`

## Admin Integration Routes

- `GET /api/v1/admin/integrations/discord/configs`
- `GET /api/v1/admin/integrations/discord/features`
- `PATCH /api/v1/admin/integrations/discord/features`
- `GET /api/v1/admin/integrations/discord/guilds`
- `GET /api/v1/admin/integrations/discord/guilds/{guild_id}/discovery`
- `POST /api/v1/admin/integrations/discord/configs`
- `PATCH /api/v1/admin/integrations/discord/configs/{config_id}`
- `POST /api/v1/admin/integrations/discord/channels`
- `PATCH /api/v1/admin/integrations/discord/channels/{channel_id}`
- `DELETE /api/v1/admin/integrations/discord/channels/{channel_id}`
- `POST /api/v1/admin/integrations/discord/roles`
- `PATCH /api/v1/admin/integrations/discord/roles/{role_id}`
- `DELETE /api/v1/admin/integrations/discord/roles/{role_id}`
- `POST /api/v1/admin/integrations/discord/categories`
- `PATCH /api/v1/admin/integrations/discord/categories/{category_id}`
- `DELETE /api/v1/admin/integrations/discord/categories/{category_id}`

## Notification Orchestration Routes

- `POST /api/v1/admin/notifications/announcements`
- `POST /api/v1/events/{event_id}/publish/discord`
- `GET /api/v1/admin/integrations/outbound-jobs`
- `POST /api/v1/admin/integrations/outbound-jobs/run`

## Access

- self Discord identity routes require `auth.profile.read`
- Discord config and outbound-job routes use the integrations admin permission path
- event publish to Discord also uses the integrations admin permission path

## Request Shapes

Discord link start:

```json
{
  "return_url": "http://127.0.0.1:3000/profile/overview"
}
```

`auth_url` in the response is `null` when Discord OAuth is not configured (see `DISCORD_*` in the configuration guide).

Announcement queue:

```json
{
  "title": "Training Freeze",
  "body_markdown": "Training is paused for maintenance tonight.",
  "details_url": "https://example.test/announcements/training-freeze",
  "send_email": true,
  "send_discord": true,
  "channel": "announcements"
}
```

`channel` is the logical Discord config channel name to post to; it defaults to
`announcements`. Event promos pass `event_announcements` to route to a dedicated
channel instead of the general announcements one.

Event publish:

```json
{
  "ping_users": true
}
```

Outbound-job list query parameters:

- `status`
- `page`
- `page_size`
- `limit`
- `offset`

## Notes

- Discord delivery is durable and queue-backed through `integration.outbound_jobs`
- outbound-job list responses now use the shared pagination envelope
- `link/start` now creates a stored OAuth state record and `link/complete` exchanges the code with Discord and finalizes the identity mapping
- announcement fan-out can use both the email platform and Discord outbound jobs from one backend request
- Discord config bundle responses return configs, channels, roles, and categories together
- multiple logical channel names may map to the same Discord `channel_id` (e.g. `announcements` and `event_announcements` on one channel). Names are still unique within a config (`discord_config_id, name`); the `channel_id` uniqueness constraint was dropped in migration 0068
- `discord/features` GET/PATCH manage runtime on/off toggles for bot segments (staffup, commands, announcements, event postings, scheduled events, audit log, role sync, break board, impromptu selector). The canonical list is `BOT_FEATURES` in code; the `integration.bot_feature_flags` table (migration 0069) stores overrides, defaulting missing features to enabled. The bot reads these on its config sync (~60s) and gates each segment
- the `discord/guilds` and `discord/guilds/{guild_id}/discovery` routes proxy live data from the Discord bot (via `BOT_API_BASE_URL` / `BOT_API_SECRET_KEY`) so configuration UIs can pick guilds, channels, categories, and roles from dropdowns instead of pasting snowflake ids; they return `503` when the bot is unreachable
