# Feedback API

## Purpose

Submit and review controller feedback.

## Response Timezones

Timestamped feedback responses follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `GET /api/v1/feedback`
- `GET /api/v1/feedback/{feedback_id}`
- `POST /api/v1/feedback`
- `PATCH /api/v1/feedback/{feedback_id}`

## Access

- authenticated users can submit feedback
- authenticated users can list their own submitted feedback and view their own received feedback
- managers with `manage_feedback` can review and decide feedback state
- feedback submission remains an intentional self-service exception to the otherwise read-mostly default user access
- `POST /api/v1/feedback` rejects self-submitted feedback (`target_cid` resolving to the submitter) with `400`
- `GET /api/v1/feedback/{feedback_id}` is visible to the submitter, the target, or a caller with `feedback.items.read`; anyone else (or a nonexistent id) gets `404` rather than a distinguishing `401`/`403`, so existence isn't leaked to non-owners

## Pagination

`GET /api/v1/feedback` now uses the shared pagination envelope with canonical `page` and `page_size` inputs plus compatibility aliases for `limit` and `offset`.

## Filtering

`GET /api/v1/feedback` accepts:

- `status` — exact match, one of `PENDING`, `RELEASED`, `STASHED`
- `submitter_cid` — exact match on the submitting user's CID
- `submitter_name` — case-insensitive substring match on the submitting user's display name
- `target_cid` — exact match on the target (reviewed) user's CID
- `target_name` — case-insensitive substring match on the target user's display name
- `controller_position` — case-insensitive substring match on the staffed position
- `min_rating` / `max_rating` — inclusive rating range (1-5); pass the same value for both to match an exact rating

## Denormalized identity fields

`FeedbackItem` includes `submitter_cid`, `submitter_name`, `target_cid`, and `target_name` alongside the raw `submitter_user_id`/`target_user_id`, mirroring the pattern used by `IncidentItem`. These are populated on list/find/decide responses (which join `identity.users`) and are `null` on the response returned immediately from `POST /api/v1/feedback`.
