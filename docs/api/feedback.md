# Feedback API

## Purpose

Submit and review controller feedback.

## Main Routes

- `GET /api/v1/feedback`
- `POST /api/v1/feedback`
- `PATCH /api/v1/feedback/{feedback_id}`

## Access

- authenticated users can submit feedback
- managers with `manage_feedback` can review and decide feedback state
