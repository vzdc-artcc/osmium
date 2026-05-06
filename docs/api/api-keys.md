# API Keys API

## Purpose

Provides user-managed API keys for machine access to Osmium.

These keys authenticate through the normal bearer-token middleware, but they are created and managed through a human-authenticated API surface.

## Main Routes

- `GET /api/v1/api-keys`
- `GET /api/v1/api-keys/{key_id}`
- `POST /api/v1/api-keys`
- `PATCH /api/v1/api-keys/{key_id}`
- `DELETE /api/v1/api-keys/{key_id}`

## Data Model

An API key is stored as:

- an `access.service_accounts` row with `kind = 'api_key'`
- one `access.service_account_credentials` row holding the hashed secret metadata
- explicit `access.service_account_permissions` rows for the key's granted permissions

The plaintext secret is never stored. Only its SHA-256 hash is persisted.

## Visibility and Ownership

- users can always list and manage keys they created
- users with `api_keys.read` can list keys created by other users
- users with `api_keys.update` can update keys created by other users
- users with `api_keys.delete` can revoke keys created by other users
- creating a key requires `api_keys.create`

Service accounts cannot create API keys.

## Create Behavior

`POST /api/v1/api-keys` requires:

- `name`
- `permissions`

Optional fields:

- `description`
- `expires_at`

Behavior notes:

- `name` is trimmed and must not be empty
- `description` is trimmed; empty text is treated as `null`
- `permissions` must normalize into at least one canonical permission
- non-admin users may grant only a subset of their own effective permissions
- server admins may grant any current permission
- the response returns both the created key metadata and the plaintext `secret`
- the plaintext `secret` is returned exactly once

The secret format currently starts with `osm_`. The stored display metadata also includes a public `prefix` and `last_four`.

## Update and Revoke Behavior

`PATCH /api/v1/api-keys/{key_id}` supports partial updates for:

- `name`
- `description`
- `permissions`

Behavior notes:

- omitted fields are unchanged
- `description: null` clears the description
- providing `permissions` replaces the full permission set
- replacement permissions are validated with the same subset rule used at creation time

`DELETE /api/v1/api-keys/{key_id}` revokes the underlying credential and marks the service account status as `disabled`.

## Auth Model

- key-management routes require a valid human session
- bearer-token callers use the created secret as `Authorization: Bearer <secret>`
- machine access from a created key resolves through the normal service-account auth path

## Response Shape

`GET /api/v1/api-keys` now uses the shared pagination envelope with canonical `page` and `page_size` inputs plus compatibility aliases for `limit` and `offset`.

List response `items` expose metadata such as:

- `id`
- `key`
- `name`
- `description`
- `status`
- `prefix`
- `last_four`
- `created_by_user_id`
- `created_by_display_name`
- `created_at`
- `last_used_at`
- `expires_at`
- `revoked_at`

Detail responses add grouped `permissions`.
