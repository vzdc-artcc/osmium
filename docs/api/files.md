# Files API

## Purpose

Manage file assets, file metadata, content replacement, signed URLs, and file audit logs.

## Main Routes

- `GET /api/v1/files`
- `POST /api/v1/files`
- `GET /api/v1/files/{file_id}`
- `PATCH /api/v1/files/{file_id}`
- `DELETE /api/v1/files/{file_id}`
- `GET /api/v1/files/{file_id}/content`
- `PUT /api/v1/files/{file_id}/content`
- `GET /api/v1/files/{file_id}/signed-url`
- `GET /api/v1/admin/files/audit`
- `GET /cdn/{file_id}`

## Notes

- upload uses raw request bytes
- signed URLs depend on `FILE_SIGNING_SECRET`
- the CDN route can be used for public files or signed-token access
- `GET /api/v1/files` is for authenticated users and returns only files visible to the caller unless they have elevated file-management access
- default `USER` access includes file browsing, not file upload
- upload, mutation, and policy-management routes require elevated file permissions
