# 001 — Pagination Consolidation

## Problem

`src/models/pagination.rs` already defines `PaginationQuery` (`page`/`page_size`/`limit`/`offset` with `resolve()`) and `PaginationMeta` (`total`/`page`/`page_size`/`total_pages`/`has_next`/`has_prev` with `PaginationMeta::new()`), but neither is embedded anywhere. Instead:

- 19 separate `*ListResponse` structs across `src/models/{training,users,events,access,web,feedback,media}/mod.rs` and `src/models/email.rs` each hand-declare the same 6 pagination fields as sibling fields next to `items`, rather than embedding `PaginationMeta`.
- 9 `List*Query` structs (e.g. `ListUsersQuery`, `ListEventsQuery`, `ListTrainingSessionsQuery`) re-declare `page`/`page_size`/`limit`/`offset` instead of `#[serde(flatten)] pagination: PaginationQuery`.
- Handlers then manually destructure `PaginationMeta` back into the 6 individual fields when building each response (e.g. `src/handlers/users.rs:81-90`, `src/handlers/api_keys.rs:83-92`, `src/handlers/admin.rs:226-235`, `src/handlers/events.rs:72-81`).

Zero uses of `#[serde(flatten)]` exist in `src/models/` today. This is pure duplication with no behavior difference — `PaginationMeta::new()` already computes the correct values.

## Goal

Every list response embeds `PaginationMeta` as a field (e.g. `pagination: PaginationMeta`) instead of flattening its 6 fields by hand; every list query struct flattens `PaginationQuery` instead of re-declaring `page`/`page_size`/`limit`/`offset`.

## Approach

1. For each `*ListResponse` struct: replace the 6 individual fields with `#[serde(flatten)] pub pagination: PaginationMeta`, constructed via `PaginationMeta::new(total, page, page_size)` at the call site (already how the data is computed today — this only changes where it's stored). Verified empirically (`UserListResponse`): `serde_json` flattens this correctly at the wire level (response JSON is byte-for-byte identical — `total`, `page`, etc. still appear as top-level siblings of `items`), and utoipa 5.5's `ToSchema` derive represents the flattened field as `allOf: [{$ref: PaginationMeta}, {properties: {items: ...}}]` in the generated OpenAPI schema rather than inlined sibling properties. That's standard, valid OpenAPI composition — semantically equivalent, just structurally different — but it required updating the assertion helper in `src/lib.rs`'s `openapi_paginated_routes_use_envelopes_and_page_params` test to walk `allOf`/`$ref` composition instead of only checking `schema["properties"]` directly (already done; see `collect_schema_properties` in `src/lib.rs`).
2. **Do NOT flatten `List*Query` structs.** Verified empirically this breaks them: axum's `Query<T>` extractor deserializes via `serde_urlencoded`, which does not support `#[serde(flatten)]` (a well-known upstream limitation — flatten requires self-describing map deserialization that `serde_urlencoded` doesn't implement). Adding `#[serde(flatten)] pagination: PaginationQuery` to `ListUsersQuery` made every request fail extraction with 400 before the handler body (and its auth check) ever ran, breaking `user_list_endpoint_requires_session` and any other route relying on default/absent query params. **Leave the 9 `List*Query` structs with their 4 individual fields as-is** — this part of the original finding does not have a safe mechanical fix under the current extractor; it would require switching off axum's `Query` extractor (e.g. to a `serde_html_form`-based custom extractor) to fix properly, which is out of scope for this pass.
3. Net scope of this spec is therefore: **response-side consolidation only** (19 `*ListResponse` structs → `#[serde(flatten)] pagination: PaginationMeta`). The query-side duplication (9 `List*Query` structs) is left as a known, accepted limitation — flag it in the PR description so it isn't rediscovered as an oversight.

## Affected files/patterns

- `src/models/pagination.rs` (no changes needed — already correct)
- Representative response structs: `src/models/users/mod.rs` (`UserListResponse`, `AdminUserListResponse`), `src/models/events/mod.rs` (`EventListResponse`, `EventPositionListResponse`), `src/models/training/mod.rs` (7 list-response structs), `src/models/access/mod.rs` (`ApiKeyListResponse`, `AuditLogListResponse`), `src/models/feedback/mod.rs` (`FeedbackListResponse`), `src/models/media/mod.rs` (`FileAssetListResponse`), `src/models/web/mod.rs` (`PublicationListResponse`), `src/models/email.rs` (`EmailOutboxListResponse`)
- Representative query structs: `src/models/users/mod.rs` (`ListUsersQuery`, `ListVisitorApplicationsQuery`), `src/models/events/mod.rs` (`ListEventsQuery`), `src/models/media/mod.rs` (`ListFilesQuery`), `src/models/web/mod.rs` (`ListPublicationsQuery`), `src/models/training/mod.rs` (`ListTrainingAppointmentsQuery`, `ListTrainingSessionsQuery`), `src/models/access/mod.rs` (`ListAuditLogsQuery`), `src/models/email.rs` (`ListEmailOutboxQuery`)
- Handler call sites building these responses (one file per model above, e.g. `src/handlers/users.rs`, `src/handlers/events.rs`, `src/handlers/training.rs`, `src/handlers/admin.rs`, `src/handlers/api_keys.rs`, `src/handlers/feedback.rs`, `src/handlers/files.rs`, `src/handlers/publications.rs`, `src/handlers/emails.rs`)

## Ordering

Single mechanical pass, one model file (and its handler call sites) at a time; no cross-file dependencies. Suggested order: smallest struct count first (`feedback`, `media`) up to largest (`training`, 7 structs) last, so any surprise (e.g. a utoipa flatten quirk) surfaces early on a small file.

## Verification

- `cargo check` after each model file's structs are updated (catches every call site that still destructures the old fields).
- `cargo test` — existing route/OpenAPI shape assertions in `src/lib.rs` and integration tests in `tests/*.rs` should catch unintended wire-format changes.
- Diff a sample response body (e.g. via a manual `curl` against a running dev server, or the existing test fixtures) before/after to confirm JSON shape is unchanged if `#[serde(flatten)]` is used as recommended above.
- `cargo doc`/OpenAPI generation (`src/docs.rs`) still produces valid schemas for the flattened query params — spot check the generated docs page for one endpoint per model file.
