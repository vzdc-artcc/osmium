# Integrations API

## Purpose

Expose backend-owned integration and notification orchestration surfaces.

## Discord Identity Routes

- `GET /api/v1/me/discord`
- `POST /api/v1/me/discord/link/start`
- `POST /api/v1/me/discord/link/complete`
- `POST /api/v1/me/discord/unlink`

## Admin Integration Routes

- `GET /api/v1/admin/integrations/discord/configs`
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
  "redirect_uri": "http://localhost:3000/discord/callback"
}
```

Discord link complete:

```json
{
  "code": "oauth_code",
  "state": "oauth_state_token",
  "redirect_uri": "http://localhost:3000/discord/callback"
}
```

Announcement queue:

```json
{
  "title": "Training Freeze",
  "body_markdown": "Training is paused for maintenance tonight.",
  "details_url": "https://example.test/announcements/training-freeze",
  "send_email": true,
  "send_discord": true
}
```

Event publish:

```json
{
  "ping_users": true
}
```

Outbound-job list query parameters:

- `status`
- `limit`
- `offset`

## Notes

- Discord delivery is durable and queue-backed through `integration.outbound_jobs`
- `link/start` now creates a stored OAuth state record and `link/complete` exchanges the code with Discord and finalizes the identity mapping
- announcement fan-out can use both the email platform and Discord outbound jobs from one backend request
- Discord config bundle responses return configs, channels, roles, and categories together
