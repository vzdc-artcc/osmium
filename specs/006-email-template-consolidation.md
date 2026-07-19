# 006 — Email Template Consolidation

## Problem

Two parallel, redundant systems render the same ~23 email templates:

- `src/email/templates.rs` (868 lines): a `TemplateDefinition` registry with hand-built HTML via `format!()` strings, plus a JSON-schema function per template.
- `src/email/rsx/` (maud-based "RSX" system): `templates/*.rs` (11 files) reimplementing the same templates with `maud::html!` macros plus a `TextBuilder` for plaintext.

`render_template` (`src/email/render.rs:28`) always checks `find_rsx_template` first and only falls through to `template.renderer` if the template ID isn't found in the rsx registry. Since all 23 template IDs exist in both registries, the `templates.rs` renderer path is **dead code for every template that has an rsx equivalent** — confirmed: 19 of 23 `TemplateDefinition.renderer` fields point at `rsx_stub` (`src/email/templates.rs:582`), which unconditionally returns `Err(ApiError::Internal)` and is therefore unreachable in practice. The remaining 4 (`announcements.generic`, `events.position_published`, `events.reminder`, `system.test_email`) have real but currently-dead duplicate HTML-building logic in `templates.rs` that's shadowed by the rsx equivalent.

Additionally, `required_string`/`optional_string` helper functions (`src/email/templates.rs:697-714`) are copy-pasted verbatim into every one of the 11 `rsx/templates/*.rs` files instead of being shared.

## Goal

One template-rendering system (the rsx/maud one) remains; the shadowed `templates.rs` renderer logic is deleted; the copy-pasted validation helpers are shared.

## Approach

1. Confirm (via `render_template`'s dispatch order, already verified) that every `TemplateDefinition` entry has a corresponding rsx template — if any gap exists, port that one template to rsx first before deleting anything.
2. Delete the `renderer` field's HTML-building implementations in `templates.rs` (the 4 real ones plus the `rsx_stub` placeholder and its 19 references) — keep `TemplateDefinition` itself only if it's still needed for template *metadata* (id, category, `is_transactional`, and whatever the admin template-preview/list UI reads via `list_templates`/`preview_email` in `src/handlers/emails.rs`).
3. Keep whichever `payload_schema`/JSON-schema logic is actually consumed by the admin UI (`src/handlers/emails.rs::list_templates`/`preview_email`) — do not delete schema validation that's load-bearing, only the dead HTML-rendering path.
4. Extract `required_string`/`optional_string` into one shared module (e.g. `src/email/rsx/validate.rs` or alongside `src/email/rsx/mod.rs`) and update all 11 `rsx/templates/*.rs` files to import it instead of redefining it locally.

## Affected files/patterns

- `src/email/templates.rs` — remove dead renderer implementations and `rsx_stub`; keep only metadata/schema pieces confirmed still in use
- `src/email/render.rs` — dispatch logic likely simplifies once there's only one registry to check
- `src/email/rsx/templates/*.rs` (11 files) — dedupe `required_string`/`optional_string`
- `src/handlers/emails.rs` — confirm `list_templates`/`preview_email` still work against whatever subset of `templates.rs` remains

## Ordering

Independent of everything else — do early (Phase 0) as a safe, high-value, mechanical deletion. Single PR.

## Verification

- `cargo check`/`cargo build` — deleting dead code should produce no warnings about now-unused imports left behind.
- `cargo test` — any existing email-rendering tests (search `src/email/` for `#[test]`/`#[tokio::test]`) must still pass unchanged, since actual rendering behavior (rsx path) doesn't change.
- Manually exercise `preview_email`/`list_templates` endpoints (or their existing tests) for a sample of the 4 templates that had real duplicate logic, to confirm the rsx-rendered output is what's actually served (it already is today, per the dispatch order — this just removes the now-provably-dead alternative).
