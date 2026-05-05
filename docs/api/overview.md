# API Overview

The generated API reference lives at `/docs/api/v1`. This page explains how to read the API as a platform consumer.

## Main Route Groups

- auth
- api-keys
- users
- admin
- workflows
- training
- events
- incidents
- integrations
- feedback
- files
- publications
- stats

## Publications Coverage

The publications group covers:

- public downloads/catalog reads
- publication category listing
- staff CRUD for publication metadata
- staff CRUD for publication categories
- CDN-oriented file linkage through existing `media.file_assets`

## Training Coverage

The training group now includes:

- assignment management
- lesson lookup for session submission
- lesson CRUD
- assignment-request and trainer-interest workflows
- trainer release requests
- training session CRUD
- nested ticket and rubric score submission
- session performance-indicator snapshots

## Auth Patterns

- session cookie auth for human clients
- bearer token auth for service accounts
- bearer token auth also covers user-created API keys because they resolve through the same machine-auth path

The auth group now also owns self-service profile mutation and TeamSpeak UID management:

- `PATCH /api/v1/me`
- `GET /api/v1/me/teamspeak-uids`
- `POST /api/v1/me/teamspeak-uids`
- `DELETE /api/v1/me/teamspeak-uids/{identity_id}`

The API-keys group owns human-managed machine credentials:

- `GET /api/v1/api-keys`
- `GET /api/v1/api-keys/{key_id}`
- `POST /api/v1/api-keys`
- `PATCH /api/v1/api-keys/{key_id}`
- `DELETE /api/v1/api-keys/{key_id}`

The backend replacement work also added:

- workflow APIs for LOA, solo certifications, staffing requests, SUA requests, controller lifecycle, and job operations
- incident reporting and admin closure APIs
- training admin/config APIs for progressions, performance indicators, assignments, and dossier reads
- event ops-plan, TMI, preset-position, and lock/unlock APIs
- Discord identity/config APIs and outbound notification jobs

## Error Patterns

Common error values:

- `bad_request`
- `unauthorized`
- `service_unavailable`
- `internal_error`

## Route Prefix

Business routes live under:

```text
/api/v1
```
