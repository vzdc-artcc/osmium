# 003 — Model / DTO / Row Separation

**Depends on**: 002 (repo layer migration) landing first — this only makes sense once data access is behind `src/repos/`.

## Problem

Structs across `src/models/{users,events,training,access,feedback,media,web}/mod.rs` and `src/models/email.rs` derive both `sqlx::FromRow` and `Serialize`/`ToSchema` on the same type (e.g. `RosterUserRow` in `src/models/users/mod.rs`). Database row shape and public wire/API shape are coupled: adding a DB column changes the API response unless someone remembers to add `#[serde(skip)]`, and internal-only columns (soft-delete flags, internal foreign keys, etc.) can leak into API responses by default rather than by explicit choice.

## Goal

Structs that are reused across multiple endpoints, or whose DB columns include fields that shouldn't be public, have a private `*Row` type (FromRow-only, lives in the repo module that queries it) and a separate public DTO (Serialize/ToSchema-only, lives in `src/models/`), connected by an explicit `From<Row> for Dto` or mapping function.

This is **not** a 100%-coverage pass. Scope it to:
- Structs used by more than one handler/endpoint (higher risk of accidental coupling)
- Structs where the underlying table has columns that are DB-internal (audit/soft-delete/internal-linkage columns) and currently either leak into the API or require ad hoc `#[serde(skip)]` sprinkled on the combined struct

Structs that are 1:1 with a single endpoint and expose exactly the columns they select can stay combined — splitting those would be pure ceremony.

## Approach

1. Follow the naming convention already used in `src/repos/users.rs` (`LoginUserRow`, `LoginMembershipRow`): the `Row` suffix marks FromRow-only types.
2. For each struct in scope: rename the FromRow version to `<Name>Row`, keep it private to the repo module that queries it (not exported via `src/models/mod.rs`), define the existing public name as a DTO with only `Serialize`/`Deserialize`/`ToSchema`, and add a `From<XRow> for X` (or a named `fn from_row(...)`) that the repo function calls before returning `Result<X, ApiError>` to the handler.
3. Do this per-struct as a follow-up pass once its call sites are already going through the repo layer (002) — the repo function's return type is the one place this conversion needs to happen, so it's a localized, low-risk change per struct.

## Affected files/patterns

- Repo modules created in 002 (e.g. `src/repos/org/loas.rs`, `src/repos/training/sessions.rs`) — `Row` types live here
- `src/models/<domain>/mod.rs` — DTOs stay here, now without `FromRow`
- Prioritize first: any struct flagged during 002's migration as reused across ≥2 endpoints, or flagged as having DB-internal columns

## Ordering

No fixed order — pick structs opportunistically as 002 completes each domain, prioritizing reused/shared structs over single-use ones. Not a blocking, all-or-nothing migration.

## Verification

- `cargo check` after each struct split (the `From` impl surfaces any field mismatch immediately).
- `cargo test` — confirm no wire-format change for the DTO (this should be a pure internal refactor, response JSON stays identical).
- Spot-check the OpenAPI schema (`src/docs.rs` generated output) for the affected endpoint to confirm `ToSchema` still reflects only the intended public fields.
