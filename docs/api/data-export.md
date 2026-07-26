# Data Export API

## Purpose

GDPR **Article 15 (right of access)** self-service export. Lets an authenticated
user download a single JSON document of all personal data osmium holds on them,
assembled across every domain that links to them. Because a JSON document is a
"structured, commonly used, machine-readable format", it also satisfies the
**Article 20 (data portability)** right.

## Main Routes

- `GET /api/v1/me/data-export`

## Access

Self-service only — gated by `auth.profile.read` (the baseline self-read
permission every authenticated member holds) and hard-scoped to the caller's own
`user.id`. There is no path here to export another subject's data.

The request itself is logged to the audit trail as a `DATA_EXPORT` / `EXPORT`
entry (Article 5(2) accountability — the org must be able to demonstrate it
handled access requests).

## Response shape

A single `DataExportDocument`:

- `meta` — Article 15(1) transparency information: `generated_at`, `subject_cid`,
  `subject_user_id`, `format`, and a `gdpr_notice` (legal basis, purposes, data
  categories, recipients, retention, data sources, your rights, and the contact for
  exercising them). This is static boilerplate, present in every export.
- `identity` — profile, membership, flags, roles, staff positions, and linked
  external accounts (Discord/TeamSpeak).
- `training` — sessions as student and as instructor, session tickets, appointments,
  assignment, assignment/release requests, progression, and dossier entries.
- `certifications` — certification grid and solo endorsements.
- `events` — event-position history.
- `feedback` — feedback submitted by, and received about, the subject.
- `incidents` — incident reports filed by, and about, the subject.
- `workflows` — LOA, staffing, and SUA requests.
- `notifications` — broadcast acknowledgements, welcome-message state, and emails
  sent to the subject (metadata only).
- `visitor_application` — the subject's visitor application, if any.
- `activity_log` — audit entries for actions the subject performed (metadata only).

## Data-handling notes

- **Secrets are never exported.** Linked-account OAuth tokens, API-key secrets, and
  email bodies are omitted; only non-secret metadata is included.
- **Evaluative notes are included.** Trainer-only session comments, staff-only
  feedback comments, and dossier entries written *about* the subject are included as
  the subject's own personal data under Article 15. The individual staff authors of
  those notes are **not** identified. (This resolves the open Article 15(4) question
  flagged in spec 012 — confirmed decision: include.)
- **Third-party data is minimised.** For two-party records (feedback, incidents),
  the counterparty is named only when the subject authored the record (their own
  action); when the subject is the passive party, the counterparty's identity is
  omitted. Audit `before`/`after` state bodies are dropped (they can contain other
  subjects' data for staff actions).
- The full cross-domain assembly is one of the heavier single requests in the API;
  it is a candidate for the IP rate limiting in spec 010 once that lands.
