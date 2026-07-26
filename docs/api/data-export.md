# Data Export API

## Purpose

GDPR **Article 15 (right of access)** self-service export. Lets an authenticated
user download a single JSON document of all personal data osmium holds on them,
assembled across every domain that links to them. Because a JSON document is a
"structured, commonly used, machine-readable format", it also satisfies the
**Article 20 (data portability)** right.

## Main Routes

- `GET /api/v1/me/data-export` — self-service (one subject: the caller)
- `GET /api/v1/admin/data-export/roster` — admin bulk (every on-roster controller)

## Access

**Self-service** (`GET /api/v1/me/data-export`) — gated by `auth.profile.read` (the
baseline self-read permission every authenticated member holds) and hard-scoped to
the caller's own `user.id`. There is no path here to export another subject's data.

The request itself is logged to the audit trail as a `DATA_EXPORT` / `EXPORT`
entry (Article 5(2) accountability — the org must be able to demonstrate it
handled access requests).

**Admin mass export** (`GET /api/v1/admin/data-export/roster`) — gated by
`users.data_export.read`, which is granted to **no role** (SERVER_ADMIN-only, via
the effective-permissions cross-join) and kept out of the assignable catalog
(`acl.rs` NON_ASSIGNABLE) so facility admins cannot grant it onward. It returns the
full `DataExportDocument` for **every on-roster controller** (the same population
the roster page shows: `controller_status` set and not `NONE`) in one payload,
reusing the exact same per-subject assembler — so each subject's document honours
the same Article 15(4) third-party scoping as the self-service export. Each
subject's `activity_log` is resolved by *their own* actor, not the requesting
admin's. The bulk access is recorded as a single `DATA_EXPORT` / `EXPORT_ALL` audit
entry whose `scope_key` is the subject count. It has its own dedicated rate limiter
(`MASS_DATA_EXPORT_RATE_LIMIT_PER_HOUR`, default 5; burst 2), tighter than the
per-user export, because it is the single most expensive request in the API.

The mass-export response is a `MassDataExportDocument`: `generated_at`,
`subject_count`, a `gdpr_notice`, and `subjects` — an array of per-controller
`DataExportDocument`s ordered by CID.

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
