# Welcome Messages API

## Purpose

First-visit welcome message content (separate text for home controllers vs. visitors) plus a per-user flag for whether that user should currently be shown one.

## Main Routes

Admin routes:

- `GET /api/v1/admin/welcome-messages`
- `PATCH /api/v1/admin/welcome-messages`

Self-service routes:

- `GET /api/v1/welcome-message`
- `POST /api/v1/welcome-message/ack`

## Permissions

- admin content read/update requires `web.welcome_messages.read` / `web.welcome_messages.update`
- self-service routes require only `auth.profile.read` (the `GET /welcome-message` state read) or `auth.profile.update` (the acknowledge action)

## Notes

- the per-user "should I see a welcome message" flag (`identity.user_profiles.show_welcome_message`) is set automatically, not through this API: it flips on when a user first becomes an active controller (fires from the admin controller-lifecycle-update path) and when a visitor application is approved. This API only lets a user *read* their current state and *clear* it — it does not expose a way to set it.
- `GET /welcome-message` resolves which text to show server-side rather than returning both texts for the client to pick: `{"show": true, "text": "..."}` when the flag is set (text chosen from the user's `controller_status` — home text for `HOME`, visitor text for `VISITOR`, `null` if the status matches neither), or `{"show": false, "text": null}` when it isn't.
- `POST /welcome-message/ack` clears the flag. It does not take a body and does not distinguish "acknowledged" from "dismissed" — there's one flag, not a history of interactions (unlike broadcasts, which track `seen_at`/`agreed_at` separately per broadcast).
- the admin content (`home_text`/`visitor_text`) is a single global row, not versioned or per-role.

Example admin update body:

```json
{
  "home_text": "Welcome back! Here's what's changed since your last session...",
  "visitor_text": "Welcome to vZDC as a visiting controller..."
}
```

Example self-service response when a message should be shown:

```json
{
  "show": true,
  "text": "Welcome to vZDC as a visiting controller..."
}
```
