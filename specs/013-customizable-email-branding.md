# 013 — Customizable Email Branding & Live Preview

## Status: ✅ implemented

Implemented as designed below, with 18 configurable fields. Summary of what shipped:

- **Migration** `migrations/0037_email_branding.sql`: `email.branding` table (single seeded row, `id='default'`, matching today's hardcoded look exactly) + `emails.branding.read`/`emails.branding.update` permissions granted to `STAFF`.
- **Rendering** (`src/email/branding.rs`, new): `EmailBranding` model (`src/models/email/mod.rs`), `EmailTheme<'a>` wrapper (branding + resolved logo URL), `stylesheet()` CSS generator, font-stack allow-list, corner-radius/font-size-scale resolution, accent-color-derived callout tint, and the full validator (`validate_branding_input`) — hex-only colors, allow-listed fonts, enum checks, length-capped text fields.
- **Mechanical pass across all 23 rsx templates** (not 22 — corrected the count from the design doc): `RsxTemplate::render` trait gained a `theme: &EmailTheme` parameter, threaded through every template file plus `EmailLayout`/`email_header`/`email_footer`/`render.rs`. `STYLE` (the old hardcoded const) is gone, replaced by `branding::stylesheet()`.
- **Handlers** (`src/handlers/emails.rs`): `GET`/`PATCH /api/v1/admin/emails/branding` (manual `ensure_permission`, matching the rest of this not-yet-`RequirePermission<P>`-migrated file per spec 009's scope note); `POST /api/v1/emails/preview` extended with `branding_override` for the builder's live preview, validated the same way as a real save before rendering.
- **Send-path wiring**: `EmailService::process_pending_batch` fetches branding once per batch (not per recipient); `enqueue_template_send`'s validation-render and `preview_template` both take branding explicitly rather than `EmailService` reaching into the DB itself, keeping it DB-agnostic as before.
- **Logo**: `logo_file_id` references `media.file_assets`, validated `is_public` on write; the CDN URL is resolved once in `render_template` by reusing `unsubscribe_base_url` as a general "this deployment's public base URL" rather than adding a second env var for one field.
- **Test fixup**: `rsx/templates/mod.rs`'s tests now build an explicit `EmailBranding`/`EmailTheme` fixture matching the seeded default instead of asserting a hardcoded `#500e0e` — same assertion, now testing that branding actually flows through rendering instead of testing a constant.

Compiles clean, all 102 lib tests pass, no new clippy warnings, `rustfmt`-clean. Not run against a live Postgres in this environment (matches every other feature from this session — no local DB available).

## Original design (below, unchanged from planning)

The website is getting an email builder: an admin picks colors, text, font, header, and formatting for system emails, with a live preview of the fully rendered email before saving. Osmium needs the backend for this. Today there is nothing to customize — every visual property of every outbound email is a Rust compile-time constant.

## Current architecture (verified against source, this session)

Osmium already consolidated all email rendering onto one system (spec 006): 22 templates in `src/email/rsx/templates/*.rs`, each implementing the `RsxTemplate` trait (`id()`, `render(payload, unsubscribe_link) -> RenderedEmail`), registered in a static slice in `src/email/rsx/templates/mod.rs`.

All 22 templates share one visual envelope, `src/email/rsx/components/`:

- `styles.rs` — `pub const STYLE: &str` — a single hardcoded CSS block. The ARTCC brand color `#500e0e` appears **7 times** in it (header background, panel heading color, links, `strong` color, callout border, button background). Font is hardcoded `Roboto,Arial,Helvetica,sans-serif`.
- `header.rs` — `email_header()` — hardcodes the brand name `"vZDC"` and eyebrow text `"Washington ARTCC"`.
- `footer.rs` — `email_footer(unsubscribe_link)` — hardcodes `"Sent by vZDC."`.
- `layout.rs` — `EmailLayout` — a builder (`subject`, `preheader`, `heading`, `unsubscribe_link`) whose `.render(body, cta)` assembles the full HTML document: `<style>{STYLE}</style>` + `email_header()` + the template's own `body` markup + `email_footer()`. This is the **single call site** that pulls in `STYLE`/`email_header`/`email_footer` — each of the 22 templates calls `EmailLayout::new(subject)....render(body, cta)` itself; the shared CSS/header/footer aren't duplicated per template, they're just invoked once per render from 22 different call sites.

Rendering path: `render.rs::render_template(template: &TemplateDefinition, payload, ...)` → `find_rsx_template(template.id)` → `.render(payload, unsubscribe_link)`. This is a **pure, synchronous, DB-free function** — `EmailService` (`service.rs`) holds only `config: EmailConfig` and `mailer: Arc<SesMailer>`, no `PgPool`.

Two call sites matter for this feature:
- `EmailService::preview_template(template_id, payload)` (sync, no DB) → used by `POST /api/v1/emails/preview`, today only for previewing a template's *content* with sample payload data (not branding).
- `EmailService::process_pending_batch(pool)` (async, has `pool`) → the actual outbox worker, calls `render_template` once per recipient inside a loop.

`emails.rs` is one of the 5 handler files spec 009 flags as not yet migrated to `RequirePermission<P>` — every endpoint in it still uses manual `ensure_permission(&state, ..., PermissionPath::from_segments([...], PermissionAction::X))` with dotted permission names like `emails.templates.read`, `emails.preview.create`.

## Goal

An admin can configure a single, site-wide set of branding properties, see a live preview reflecting unsaved edits before committing, and have every subsequently-sent email (preview, transactional, and outbox-batch) render with the saved branding — with zero visual change for anyone until an admin actually edits it (the seeded default must match today's hardcoded look exactly).

## Scope decision: one global brand, not per-template/per-category

Nothing in the request describes per-template or per-category theming — "an email builder" reads as "configure how vZDC's emails look," singular. Scoping to one global `email.branding` row keeps this a styling layer on top of the existing 22 templates rather than a second templating system. Per-template overrides can be a v2 if actually requested later — don't build for a requirement that hasn't been asked for.

## Data model

User feedback on the first draft: needs to be **as customizable as possible**, not just one primary color. Revised to a real theme model — every distinct color *role* the CSS currently hardcodes gets its own field, plus a logo (osmium already has a public, unauthenticated-servable CDN — `GET /cdn/{file_id}` skips the read-permission check entirely when `media.file_assets.is_public = true`, confirmed by reading `handlers/files.rs::cdn_download_file` — so logo support doesn't need new infrastructure, just a reference to an existing public file asset).

New table, `email.branding`, single seeded row (`id = 'default'`, same fixed-singleton-id pattern as `stats.statistics_prefixes`):

**Identity**
| column | type | notes |
|---|---|---|
| `id` | `text primary key` | always `'default'` |
| `brand_name` | `text not null` | replaces hardcoded `"vZDC"` |
| `tagline` | `text not null` | replaces hardcoded `"Washington ARTCC"` |
| `footer_text` | `text not null` | replaces hardcoded `"Sent by vZDC."` |
| `logo_file_id` | `text references media.file_assets(id) on delete set null` | optional; must reference a row with `is_public = true` (enforced at the handler, not the FK — see Validation). When set, rendered as `<img>` in the header instead of the text brand name; when null, header falls back to `brand_name` text exactly like today. |

**Colors** — every place `#500e0e` (or a fixed neutral) appears in `STYLE` today becomes its own role instead of one shared "primary" (see the file-by-file breakdown below):
| column | replaces (in current `STYLE`) |
|---|---|
| `header_background_color` | `.header{background:#500e0e}` |
| `header_text_color` | `.header{color:#ededf5}` |
| `page_background_color` | `.bg{background:...}` (the outer page background; the current two-stop gradient is dropped in favor of one flat, admin-set color — see Non-goals) |
| `panel_background_color` | `.panel{background:#ffffff}` |
| `text_color` | `body{color:...}`, `.panel p{color:...}` |
| `heading_color` | `.panel h1{color:#500e0e}` |
| `link_color` | `.panel a{color:#500e0e}`, `.footer a{color:#500e0e}` |
| `accent_color` | `.panel strong{color:#500e0e}`, `.callout{background:#f7ecec;border-left-color:#500e0e}` (callout background is derived — see below) |
| `button_background_color` | `.button{background:#500e0e}` |
| `button_text_color` | `.button{color:#ededf5}` |

`.callout`'s background (`#f7ecec`, a pale tint of the brand red) doesn't get its own field — computing a tint from `accent_color` at render time (fixed-alpha overlay, e.g. `color-mix` or a precomputed lighten in Rust) keeps the callout legible against whatever accent an admin picks without adding an 11th color field nobody will tune independently of the accent it's derived from. Border colors on `.panel`/`.footer` (`#d9dce5`) stay fixed — they're structural hairlines, not brand expression.

**Typography**
| column | type | notes |
|---|---|---|
| `heading_font_family` | `text not null` | allow-list key, see Validation |
| `body_font_family` | `text not null` | allow-list key, independent from heading — lets an admin pair a display face for headings with a plainer body face, same as most theme systems |
| `font_size_scale` | `text not null check (in ('small','medium','large'))` | a size *multiplier* preset, not raw px per element — keeps every font-size in the template (brand/h1/body/footer/eyebrow) scaling together and staying legible, instead of exposing 5 independent px fields an admin could set inconsistently |

**Shape**
| column | type | notes |
|---|---|---|
| `corner_style` | `text not null check (in ('sharp','rounded','soft'))` | one dial controlling border-radius consistently across header/panel/footer/button/callout (0 / ~8px / ~18px, the last matching today's look) — not 5 independent radius fields for the same reason as font scale |

**Bookkeeping**
| column | type |
|---|---|
| `updated_by_user_id` | `text references identity.users(id) on delete set null` |
| `updated_at` | `timestamptz not null default now()` |

That's 4 identity fields + 10 colors + 3 typography + 1 shape = 18 configurable fields, all individually validated. Seed migration inserts the row with every value set to exactly what's hardcoded today (`header_background_color='#500e0e'`, `panel_background_color='#ffffff'`, `corner_style='soft'`, etc.) so a day-one `GET` returns today's exact look and nothing visually changes until an admin edits something.

**Why a dedicated table over a `web.site_settings` jsonb row** (the pattern used for welcome messages): these are individually-typed, individually-validated fields (a hex color needs a real constraint, an enum needs a real check constraint) — not a single free-text blob. A typed table gets that validation for free and matches the majority pattern already in this codebase (`stats.statistics_prefixes`, `web.change_broadcasts`) rather than the exception (`site_settings`, used for welcome messages only because that storage already existed pre-seeded).

## Validation — this is the part that actually matters for safety

`STYLE`'s CSS block is spliced into the response via `PreEscaped`, not maud's normal `(value)` text interpolation — maud does **not** HTML-escape it. `brand_name`/`tagline`/`footer_text` go through ordinary `html! { (value) }` interpolation elsewhere (in `header.rs`/`footer.rs`), which *is* escaped, so those three are low-risk from injection as long as they stay in that path. Every color field, if spliced unvalidated into the raw CSS string, is a real CSS/HTML-injection vector via a compromised or malicious admin session (e.g. a color value like `red;} </style><script>...` — email clients strip `<script>`, but a malicious `url()`/`expression()` isn't unheard of, and this is attacker-controlled content going into every future outbound email). With 10 color fields instead of 1, this rule has to be applied uniformly, not ad hoc per field.

Rules to enforce server-side on write (not just client-side form validation):
- **All 10 color fields**: must fully match `^#[0-9a-fA-F]{6}$`. Reject anything else with 400. Apply the same regex check to all of them from one shared validator — don't hand-roll it per field.
- **`heading_font_family` / `body_font_family`**: **not free text** — each validated against the same allow-list of email-safe stacks (e.g. `System Sans` → `-apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif`, `Georgia Serif` → `Georgia,'Times New Roman',serif`, `Monospace` → `'Courier New',monospace`). Confirmed with the user: preset list, not free text — both for this injection reason and because email-client font support is too narrow for free text to be useful.
- **`font_size_scale` / `corner_style`**: enums, enforced by a Postgres `check` constraint *and* validated server-side before insert (belt and suspenders — don't rely on the DB constraint alone to produce a clean 400).
- **`logo_file_id`**: if present, must resolve to an existing `media.file_assets` row with `is_public = true` — validate this explicitly in the handler (not just the FK, which doesn't know about `is_public`) and reject with 400 if the referenced file is private. A private logo would either 403 in every recipient's email client or (worse) leak file existence/metadata to an unauthenticated fetch.
- **`brand_name`/`tagline`/`footer_text`**: trim, reject empty, cap length (100/150/200 chars — generous but bounded, these render in a fixed-width email header/footer).

## Rendering changes

1. New `Branding` struct (plain data, no DB awareness — 18 fields matching the table above) passed by value/reference through the render path.
2. `STYLE` becomes a function, `fn stylesheet(branding: &Branding) -> String`. Each of the 10 color roles substitutes into its specific selector(s) per the mapping table above (not one variable reused 7 times anymore — 10 distinct variables, each used at its own specific selector(s)). `font_size_scale` resolves to a small set of px values (e.g. small=14/26/... medium=16/30/... large=18/34/... for body/heading respectively) computed once in Rust, not shipped as raw CSS `calc()`. `corner_style` resolves to one radius value reused across `.header`/`.panel`/`.footer`/`.button`/`.callout`'s `border-radius` declarations. The callout background tint is computed from `accent_color` in Rust (fixed lightening formula) rather than being a stored field.
3. `email_header(branding: &Branding)` renders either the `logo_file_id` as an `<img src="{cdn_url}" alt="{brand_name}">` (when set) or the `brand_name`/`tagline` text (when not) — `footer.rs` similarly takes `branding: &Branding` for `footer_text`.
4. `EmailLayout::render` takes `&Branding`, passes it to `stylesheet`/`email_header`/`email_footer`.
5. `RsxTemplate::render` trait method gains a `branding: &Branding` parameter. **This touches all 22 template files**, but uniformly and mechanically: each one already calls `EmailLayout::new(subject)...render(body, cta)` itself (the shared envelope isn't centralized above them), so each of the 22 just gains the parameter and passes it straight through to its own `EmailLayout::new(...)` call. No per-template branching logic — this is a signature-and-passthrough change, not 22 bespoke edits, and the field-count increase from 1 color to 18 total fields doesn't change this — it's still one `&Branding` parameter, not 18 separate parameters.

**Rejected alternative**: route branding through a global/thread-local instead of an explicit parameter, to avoid touching 22 files. Rejected because (a) nothing else in this codebase uses global mutable state — every other cross-cutting concern (actor, headers, pool) is passed explicitly, and introducing the first exception here just to save a mechanical diff isn't worth the inconsistency; (b) explicit parameters make `preview_template`'s "render with unsaved draft branding" requirement trivial (just pass a different `Branding` value) — a global would need save/restore or a request-scoped override mechanism to do the same thing, which is more code, not less.

**Rejected alternative**: invert control so templates return content data and a single shared function applies `EmailLayout` — architecturally nicer long-term, but changes what the `RsxTemplate` trait *returns*, not just what it *takes*, which is a materially bigger and riskier refactor than this feature needs. Not worth it for a styling change; revisit only if a future requirement (e.g. per-template branding) actually needs it.

## API surface

Follows `emails.rs`'s existing manual-`ensure_permission` style (not the newer `RequirePermission<P>` extractor) — deliberately, for consistency with the rest of that file. `emails.rs` is one of the five files spec 009 will migrate wholesale; mixing extractor styles within one file before that lands is worse than being uniformly old-style until it does.

- `GET /api/v1/admin/emails/branding` — `emails.branding.read`. Returns the current saved `Branding`.
- `PATCH /api/v1/admin/emails/branding` — `emails.branding.update`. Validates per the rules above, upserts the singleton row, audit-logs before/after (matching the `broadcasts.rs`/`welcome_messages.rs`/`stats.rs` pattern from this session).
- `POST /api/v1/emails/preview` (**existing endpoint, extended**) — `EmailPreviewRequest` gains an optional `branding_override: Option<BrandingInput>` field. When present, render with that draft branding instead of the saved one; when absent, behave exactly as today (saved branding). This is what makes the builder's live preview work — the frontend can call this on every keystroke/color-pick with the in-progress form state, before the admin hits save. No new endpoint needed for this half of "live preview."

New permissions `emails.branding.read` / `emails.branding.update`, seeded via migration and granted to `STAFF` (matching every permission migration from this session — 0034/0035/0036).

## Send-path wiring

`EmailService::process_pending_batch(pool)` currently calls `render_template` once per recipient inside its loop. Fetch `Branding` **once per batch call**, before the loop (not once per recipient — it doesn't vary per-recipient and a DB round-trip per email would be wasteful), and pass it into each `render_template` call. `preview_template` gets an explicit `branding: &Branding` parameter (caller — the handler, which has `state.db` — resolves it: either the saved row or the request's `branding_override`).

## Non-goals for this pass

- **Logo *upload* as part of this feature's own API.** Uploading the logo file itself reuses the existing `POST /files`/asset endpoints (`handlers/files.rs`) — `email.branding.logo_file_id` just references a file asset that was uploaded through the normal file-upload flow and marked public. The branding endpoints only ever store/validate a `file_id`, they don't handle multipart upload themselves.
- **Per-template or per-category branding overrides.** See scope decision above — one global brand, confirmed with the user this session.
- **The page-background gradient.** Today's `.bg` is a two-stop `linear-gradient`; this plan flattens it to one admin-set `page_background_color`. A gradient picker (two colors + direction) is a small further increment if wanted later, but a flat color covers "customizable background" without a second round of color-math in the CSS generator for something cosmetic.
- **Structural content editing** (reordering blocks, adding sections, a drag-and-drop body builder). This spec covers cosmetic branding — color/font/header/footer/logo/formatting *of the shared envelope* — not rewriting what each template's body contains. The 22 templates remain code-defined.
- **A generic/user-authored template system.** Branding is a config layer on top of the existing rsx templates, not a replacement for them.

## Implementation phases (once this design is approved)

1. Migration: `email.branding` table (18 fields + bookkeeping, with `check` constraints on the two enum columns) + seed row matching today's hardcoded values exactly (zero visual diff on day one) + permission catalog rows (`emails.branding.read`/`emails.branding.update`) + `STAFF` grants.
2. Models + repo: `Branding` struct, `fetch_branding`/`upsert_branding` in a new `src/repos/email_branding.rs`, plus the shared color-regex/font-allow-list/enum validators (one function per rule, reused across all 10 color fields and both font fields rather than duplicated).
3. Rendering plumbing: `stylesheet()` function (10 color substitutions + resolved font-size-scale px values + resolved corner-style radius), `email_header`/`email_footer` signature changes (including the logo-vs-text-brand branch — logo renders via the same `/cdn/{file_id}` URL pattern `publications.rs` already uses for file assets), `EmailLayout` signature change, `RsxTemplate` trait + all 22 impls gain `branding: &Branding` — mechanical, do as one focused pass, verify with `cargo check` catching every call site the compiler forces you to update.
4. Handlers: `GET/PATCH /admin/emails/branding` (including the `logo_file_id` → `is_public` existence check); extend `preview_email`/`EmailPreviewRequest` with the optional `branding_override`.
5. Wire `process_pending_batch` to fetch and apply saved branding once per batch.
6. Test fixup: `rsx/templates/mod.rs`'s existing test asserts `result.html.contains("#500e0e")` literally — once color is dynamic, either parameterize that test with the seeded default `Branding` or assert against whatever `Branding::default()`/test-fixture value is used, so it keeps testing something real instead of becoming meaningless.

## Decisions confirmed with the user this session

- Field list: expanded from a single primary color to the full 18-field theme model above (10 colors, logo, 2 font-family roles, font-size scale, corner style) — "as customizable as possible" was the explicit instruction.
- Font customization: preset allow-list, not free text — confirmed.

No open questions remain; ready for implementation.
