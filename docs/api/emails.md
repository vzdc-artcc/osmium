# Emails API

## Purpose

Provides template discovery, preview, durable email queueing, outbox inspection, and category unsubscribe flows.

## Main Routes

- `GET /api/v1/emails/templates`
- `POST /api/v1/emails/preview`
- `POST /api/v1/emails/send`
- `GET /api/v1/emails/outbox`
- `GET /api/v1/emails/outbox/{id}`
- `POST /api/v1/emails/unsubscribe`
- `POST /api/v1/emails/resubscribe`

Public-by-token convenience route:

- `GET /api/v1/emails/unsubscribe?token=...`

## Template Model

Templates are repo-managed and rendered in Rust.

Current built-in templates:

- `announcements.generic`
- `events.position_published`
- `events.reminder`
- `system.test_email`

Each template exposes:

- `id`
- `name`
- `category`
- `is_transactional`
- `description`
- `allow_arbitrary_addresses`
- `required_payload_schema`

## Send Behavior

`POST /api/v1/emails/send` accepts:

- `template_id`
- arbitrary JSON `payload`
- optional explicit `recipients`
- optional first-party `audience`
- optional `subject_override`
- optional `reply_to_address`
- optional `dry_run`

Behavior notes:

- at least one of `recipients` or `audience` is required
- recipients are deduplicated by normalized email
- arbitrary raw email addresses are allowed only for templates that declare it
- non-transactional categories respect suppressions
- sends are durable: the API enqueues outbox records and the worker delivers them later
- `dry_run=true` validates payload and recipient resolution without inserting outbox rows
- request `reply_to_address` overrides the env default when provided

## Audience Filters

Current audience fields:

- `roles`
- `artcc`
- `rating`
- `receive_event_notifications`
- `active_only`

Audience filters only resolve first-party user rows with stored email addresses.

## Permissions

- `emails.templates.read`
- `emails.preview.create`
- `emails.send.create`
- `emails.outbox.read`
- `emails.suppressions.update`

`POST /api/v1/emails/unsubscribe` and the matching `GET` token route are public-by-token and do not require auth.

## Unsubscribe Model

Suppressions are category-scoped.

- transactional mail is not unsubscribable
- non-transactional categories can be suppressed per email address
- unsubscribe links are HMAC-signed using `EMAIL_UNSUBSCRIBE_SECRET`

## Internal Triggering

Future handlers and jobs should enqueue mail through `EmailService` rather than composing SES requests directly.

Recommended entrypoints:

- `enqueue_template_send`
- `enqueue_to_users`
- `enqueue_to_addresses`
- `enqueue_audience_send`
