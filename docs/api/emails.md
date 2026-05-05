# Emails API

## Purpose

Provides template discovery, preview, durable email queueing, outbox inspection, and token-based email preference management for website-driven unsubscribe flows.

## Main Routes

- `GET /api/v1/emails/templates`
- `POST /api/v1/emails/preview`
- `POST /api/v1/emails/send`
- `GET /api/v1/emails/outbox`
- `GET /api/v1/emails/outbox/{id}`
- `GET /api/v1/emails/preferences`
- `POST /api/v1/emails/preferences`
- `POST /api/v1/emails/resubscribe`

## Template Model

Templates are repo-managed and rendered in Rust using maud RSX components.

### Available Templates

**System**
- `system.test_email` - Diagnostic email for SES connectivity

**Announcements**
- `announcements.generic` - Generic formatted announcement email
- `broadcast.posted` - Broadcast announcement notification

**Events**
- `events.position_published` - Event position publication notice
- `events.reminder` - Reminder for an upcoming event

**LOA (Leave of Absence)**
- `loa.approved` - LOA approval notification
- `loa.denied` - LOA denial notification
- `loa.deleted` - LOA deletion notification
- `loa.expired` - LOA expiration notification

**Training**
- `training.appointment_scheduled` - Appointment scheduled notification
- `training.appointment_canceled` - Appointment cancellation notification
- `training.appointment_updated` - Appointment update notification
- `training.appointment_warning` - Appointment reminder notification
- `training.session_created` - Training session recorded notification

**Visitor**
- `visitor.accepted` - Visitor application acceptance
- `visitor.rejected` - Visitor application rejection

**Solo Certifications**
- `solo.added` - Solo certification granted
- `solo.deleted` - Solo certification removed
- `solo.expired` - Solo certification expired

**Feedback**
- `feedback.new` - New feedback received notification

**Incident**
- `incident.closed` - Incident report closure notification

**Progression**
- `progression.assigned` - Training progression assignment
- `progression.removed` - Training progression removal

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

`GET /api/v1/emails/preferences?token=...` and `POST /api/v1/emails/preferences` are public-by-token and do not require auth.

## Unsubscribe Model

Suppressions are category-scoped.

- unsubscribe tokens identify the email address and linked category context
- the website UI reads all category states from the API using the token
- the website UI submits selected subscribed/unsubscribed states back to the API
- transactional categories are returned for display but cannot be modified
- non-transactional categories can be suppressed per email address
- unsubscribe links are HMAC-signed using `EMAIL_UNSUBSCRIBE_SECRET`

## Preference API Examples

### Read Current Preferences

`GET /api/v1/emails/preferences?token=...`

```json
{
  "email": "user@example.com",
  "linked_category": "event_notifications",
  "categories": [
    {
      "id": "transactional",
      "name": "Transactional",
      "description": "Required product and operational mail",
      "is_transactional": true,
      "editable": false,
      "subscribed": true
    },
    {
      "id": "event_notifications",
      "name": "Event Notifications",
      "description": "Event-related notifications that users may opt out of",
      "is_transactional": false,
      "editable": true,
      "subscribed": false
    }
  ]
}
```

### Update Preferences

`POST /api/v1/emails/preferences`

```json
{
  "token": "signed-token",
  "preferences": [
    {
      "category": "event_notifications",
      "subscribed": false
    },
    {
      "category": "announcements",
      "subscribed": true
    },
    {
      "category": "marketing",
      "subscribed": false
    }
  ]
}
```

## Example Payloads

### System

```json
// system.test_email
{
  "message": "Hello from Osmium!",
  "requested_by": "admin@example.com"
}
```

### Announcements

```json
// announcements.generic
{
  "headline": "Facility Update",
  "body_markdown": "Hello team,\n\nThis is an important update.",
  "preheader": "Important facility news",
  "cta_label": "Read More",
  "cta_url": "https://example.com/news"
}

// broadcast.posted
{
  "title": "Monthly Newsletter",
  "body_markdown": "Welcome to the monthly update.\n\nHere's what's new.",
  "preheader": "May 2026 Newsletter",
  "details_url": "https://example.com/newsletter"
}
```

### Events

```json
// events.position_published
{
  "event_title": "ZDC Friday Night Ops",
  "starts_at": "2026-05-10T20:00:00Z",
  "details_url": "https://example.com/events/123",
  "preheader": "Sign up now!"
}

// events.reminder
{
  "event_title": "ZDC Friday Night Ops",
  "starts_at": "2026-05-10T20:00:00Z",
  "details_url": "https://example.com/events/123",
  "location": "KDCA Ground",
  "preheader": "Event starts in 24 hours"
}
```

### LOA

```json
// loa.approved
{
  "controller_name": "John Doe",
  "loa_start": "2026-05-15",
  "loa_end": "2026-06-15"
}

// loa.denied
{
  "controller_name": "John Doe",
  "reason": "Insufficient documentation provided"
}

// loa.deleted
{
  "controller_name": "John Doe",
  "reason": "Activity detected during leave period"
}

// loa.expired
{
  "controller_name": "John Doe"
}
```

### Training

```json
// training.appointment_scheduled
{
  "student_name": "Jane Smith",
  "trainer_name": "John Trainer",
  "appointment_start": "2026-05-12T18:00:00Z",
  "details_url": "https://example.com/training/appointments/456"
}

// training.appointment_canceled
{
  "student_name": "Jane Smith",
  "trainer_name": "John Trainer",
  "appointment_start": "2026-05-12T18:00:00Z",
  "reason": "Trainer unavailable"
}

// training.appointment_updated
{
  "student_name": "Jane Smith",
  "trainer_name": "John Trainer",
  "appointment_start": "2026-05-13T19:00:00Z",
  "details_url": "https://example.com/training/appointments/456"
}

// training.appointment_warning
{
  "student_name": "Jane Smith",
  "trainer_name": "John Trainer",
  "appointment_start": "2026-05-12T18:00:00Z",
  "warning_message": "Please complete your pre-session preparation."
}

// training.session_created
{
  "student_name": "Jane Smith",
  "trainer_name": "John Trainer",
  "session_date": "2026-05-12",
  "position": "DCA_TWR",
  "details_url": "https://example.com/training/sessions/789"
}
```

### Visitor

```json
// visitor.accepted
{
  "user_name": "New Visitor",
  "artcc_name": "Washington ARTCC",
  "details_url": "https://example.com/profile"
}

// visitor.rejected
{
  "user_name": "Applicant Name",
  "artcc_name": "Washington ARTCC",
  "reason": "Does not meet rating requirements"
}
```

### Solo

```json
// solo.added
{
  "controller_name": "Jane Smith",
  "position": "DCA_TWR",
  "expires": "2026-06-12"
}

// solo.deleted
{
  "controller_name": "Jane Smith",
  "position": "DCA_TWR",
  "reason": "Certification complete"
}

// solo.expired
{
  "controller_name": "Jane Smith",
  "position": "DCA_TWR"
}
```

### Feedback

```json
// feedback.new
{
  "controller_name": "John Controller",
  "position": "IAD_APP",
  "rating": "Excellent",
  "details_url": "https://example.com/feedback/101"
}
```

### Incident

```json
// incident.closed
{
  "controller_name": "John Controller",
  "incident_date": "2026-05-01",
  "resolution": "No further action required"
}
```

### Progression

```json
// progression.assigned
{
  "controller_name": "Jane Smith",
  "progression_name": "S1 to S2 Tower Training",
  "details_url": "https://example.com/training/progressions/202"
}

// progression.removed
{
  "controller_name": "Jane Smith",
  "progression_name": "S1 to S2 Tower Training",
  "reason": "Certification achieved"
}
```

## Internal Triggering

Future handlers and jobs should enqueue mail through `EmailService` rather than composing SES requests directly.

Recommended entrypoints:

- `enqueue_template_send`
- `enqueue_to_users`
- `enqueue_to_addresses`
- `enqueue_audience_send`
