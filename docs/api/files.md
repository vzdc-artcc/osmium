# Files API

## Purpose

Manage file assets, file metadata, content replacement, signed URLs, and file audit logs.

## Response Timezones

Timestamped file and file-audit responses follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `GET /api/v1/files`
- `POST /api/v1/files`
- `POST /api/v1/files/import`
- `GET /api/v1/files/{file_id}`
- `PATCH /api/v1/files/{file_id}`
- `DELETE /api/v1/files/{file_id}`
- `GET /api/v1/files/{file_id}/content`
- `PUT /api/v1/files/{file_id}/content`
- `GET /api/v1/files/{file_id}/signed-url`
- `GET /api/v1/admin/files/audit`
- `GET /cdn/{file_id}`

## Notes

- `GET /api/v1/files` and `GET /api/v1/admin/files/audit` now use the shared pagination envelope
- upload uses raw request bytes
- `POST /api/v1/files/import` fetches an image from a caller-supplied URL server-side and stores it as a normal file asset (same response shape and query params as `POST /api/v1/files`); it requires the same `files.assets.create` + `files.content.create` permissions
- import rejects non-`http(s)` URLs, hosts that resolve to a private/loopback/link-local/multicast address (SSRF guard), responses whose `Content-Type` isn't `image/*`, and payloads over `FILE_MAX_UPLOAD_BYTES`
- import does not follow redirects
- signed URLs depend on `FILE_SIGNING_SECRET`
- the CDN route can be used for public files or signed-token access
- publications reuse file assets by storing a linked `file_id` and exposing public CDN URLs instead of duplicating blob storage
- `GET /api/v1/files` is for authenticated users and returns only files visible to the caller unless they have elevated file-management access
- default `USER` access includes file browsing, not file upload
- upload, mutation, and policy-management routes require elevated file permissions
