# Files and CDN

Osmium treats files as platform assets instead of website-only uploads.

## Metadata and Storage

- metadata lives in Postgres under `media.*`
- bytes live under `FILE_STORAGE_ROOT` in local development
- signed URL behavior is built into the API

Some higher-level domain records reference file assets instead of duplicating file storage. Publications/downloads are the current example: `web.publications` stores the domain metadata while the linked blob still lives in `media.file_assets`.

## Visibility Modes

Current file access decisions consider:

- public visibility
- uploader ownership
- owner user id
- viewer roles
- explicit allowed-user rows
- `files.update` permission

## Signed URLs

Signed download URLs are minted through:

```text
GET /api/v1/files/{file_id}/signed-url
```

The CDN route validates `expires` and `sig` query params:

```text
GET /cdn/{file_id}
```

Public publications do not mint signed URLs in their API payloads. They expose `cdn_url` as `/cdn/{file_id}` and rely on the linked file asset being public.

## Encryption at Rest

If `FILE_ENCRYPTION_KEY_HEX` is configured, local file blobs can be encrypted using AES-256-GCM before being written to disk.

## Audit

File audit records cover events such as:

- upload
- signed URL issuance
- download
- delete
