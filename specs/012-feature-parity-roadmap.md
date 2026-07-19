# 012 — Feature Parity & Product Roadmap

Osmium is the replacement backend for the current vZDC website (`~/Programing/website`, a Next.js app with Prisma/Postgres and inline server actions acting as its own backend today). This spec is not an implementation plan for one change — it's an audit of where osmium has fallen behind the live site, plus the roadmap for closing that gap.

## Methodology

Rather than diffing ~60 individual `actions/*.ts` files against 19 handler files by hand, three higher-signal comparisons were used and cross-checked against each other:

1. **Data model diff**: `website/prisma/schema.prisma` (`model ...` blocks, current production schema) vs. `osmium/migrations/*.sql` (`create table ...`, current osmium schema). This is the most reliable signal — a missing table means a feature has no home yet, full stop.
2. **Route/handler diff**: `website/actions/*.ts` exports vs. `osmium/src/router.rs` + `pub async fn` in `src/handlers/*.rs`. This catches cases where a table *exists* in osmium's schema (migrated proactively) but no handler/route was ever wired to it — a narrower, cheaper gap than #1.
3. **`git log` on the website repo** (last ~150 commits, back to ~March 2026) to catch recently-shipped features that predate any osmium schema work at all.

Every gap below was verified by grep against current osmium source (not inferred from memory or spec history), so file/table names are accurate as of this session.

---

## Tier 1 — Recently shipped on the live site, zero osmium footprint

These are the most urgent: shipped in the last few weeks, and osmium has neither the schema nor the routes.

### 1.1 Training session/appointment "additional trainers" — ✅ done

Website added `TrainingSessionAdditionalTrainer` and `TrainingAppointmentAdditionalTrainer` (each: session/appointment id, trainer id, free-text `description`, unique per session-trainer or appointment-trainer pair). Shipped in PRs #146/#147 (commits `6fb15ab`, `5fb2d24`, `d6a89af`, `c3f4537`, `affec75`, `1e45b6a` — the most recent activity on the site, days old).

Osmium had an unrelated, older table (`training.training_assignment_other_trainers`, for *assignments* not sessions/appointments) — not to be confused with this.

**Implemented**: `migrations/0033_training_additional_trainers.sql` adds `training.training_session_additional_trainers`, `training.training_appointment_additional_trainers`, and a `notes` column on `training.training_appointments`. Wired into `create_training_session`/`update_training_session` and `create_training_appointment`/`update_training_appointment` in `src/handlers/training.rs`, including the live site's exact validation quirks (session additional trainers can't be the student or instructor; appointment additional trainers can't be the acting caller; appointment `notes`/description are uppercased and capped at 50 chars, session descriptions are not). New `AdditionalTrainerRequest`/`AdditionalTrainerDetail` models, repo CRUD, and `additional_trainer_count` on list items.

**Not yet done**: the DB-backed integration tests (`tests/routes_and_openapi.rs`, `tests/permission_gates.rs`) haven't been run against a live Postgres in this environment (no local DB running) — worth a pass once a dev DB is available, along with a Bruno collection entry for the new fields.

### 1.2 Event statistics (admin per-controller online-position history) — ✅ verified + gaps closed

Turned out to be a bigger composite view than the name suggests: `EventStatisticsInformation.tsx` (the actual PR #138 component) pulls from *five* different domains for one admin controller-profile dashboard — user basic info, feedback stats, published event-position history, certification/roster status, and online-position hour totals. Verified each against osmium's existing endpoints rather than assuming a gap:

- **Feedback stats** — already covered by `GET /users/{cid}/feedback`.
- **Solo certification** — already covered by `GET /users/{cid}/solo-certifications`.
- **Online-position hour totals** (all-time, and an approximable last-60-days via summing recent months) — already covered by `stats::get_controller_totals`/`get_controller_history`.
- **Certifications (non-solo)** — **confirmed real gap, now closed.** `org.certification_types`/`org.user_certifications` were written to (training-session roster-change logic already updates them) but nothing ever read them back. New `GET /api/v1/users/{cid}/certifications` (`src/repos/org/certifications.rs`) — one row per certification type, left-joined against the user's grant so ungranted types show `'NONE'` explicitly, matching the live site's per-type chip display.
- **Published event positions across all events** — **confirmed real gap, now closed.** `list_event_positions` only ever queried one event at a time; there was no "this user's history across every event" query, and the existing `EventPosition` model doesn't even expose the `final_position`/`final_start_time`/`final_end_time` columns the live site's hours math needs (they exist on `events.event_positions`, just unread). New `GET /api/v1/users/{cid}/event-positions` (`src/repos/events.rs::fetch_user_published_event_positions`, new `UserEventPositionItem` model) returns exactly those fields, most recent event first, published-only (matching the live site's hardcoded filter).

Both new endpoints reuse the same data-dependent "self via `auth.profile.read`, otherwise `users.directory.read`" authorization already established by `org::get_user_solo_certifications` — not a new permission, and not a single static `RequirePermission<P>` (ownership check needs the request's `cid` compared against the caller).

---

## Tier 2 — Confirmed gaps: no schema, no routes

These features exist on the live site with real Prisma models and no osmium counterpart at all (checked against the full `create table` list in `migrations/*.sql`):

- **ATC booking proxy** (`actions/atcBooking.ts`, `app/bookings/calendar`) — proxies `atc-bookings.vatsim.net` with a bearer secret (`ATC_BOOKING_TOKEN`). Needs a thin osmium passthrough endpoint so the secret isn't exposed client-side. **Deferred** — explicit product decision: hold until osmium is out of dev, not dropped. Revisit when asked.
- **Captcha verification proxy** (`actions/captcha.ts`) — ✅ done. Live site is Google reCAPTCHA v3 (score-based, `checkCaptcha` rejects below 0.7), used client-side by the Staffing Request and Feedback forms as a pre-submit gate — not bound to the actual form-submit request server-side, just a client-side check the way the live site already does it, so osmium mirrors that shape rather than inventing tighter server-side enforcement. New standalone `POST /api/v1/captcha/verify` (`src/handlers/captcha.rs`, `src/captcha.rs`, `src/models/captcha.rs`) — public/unauthenticated (matches the live site's action having no auth check either — it's a bot gate, not a permission gate), proxies to Google's siteverify endpoint using a server-held `GOOGLE_CAPTCHA_SECRET_KEY` (same env var name as the live site, so the existing secret can be reused). Returns `{success, score}` unchanged from Google's response shape.
- **Welcome messages** (`WelcomeMessages` model: home/visitor welcome text + per-user "seen" flag on `User.showWelcomeMessage`) — ✅ done. Turned out osmium had already proactively migrated *both* pieces of storage — `identity.user_profiles.show_welcome_message` (the per-user flag) and a `web.site_settings` row seeded with `key = 'welcome_messages'` (the home/visitor text, as `{"homeText":"...","visitorText":"..."}` jsonb) — and had *already wired* the flag-setting side: `org::controller_lifecycle::enable_welcome_message` fires from `update_controller_lifecycle` when a user's first becomes an active controller, matching the live site's roster-sync behavior. What was actually missing: (1) content CRUD, (2) a way for a user to read their own state, (3) an acknowledge endpoint, (4) the visitor-application-approval path didn't flip the flag (the live site's `addVisitor` does). New `src/handlers/welcome_messages.rs` + `src/repos/welcome_messages.rs`: admin `GET/PATCH /admin/welcome-messages` (new `WebWelcomeMessagesRead/Update` permissions, seeded in `migrations/0036_welcome_message_permissions.sql`); self-service `GET /welcome-message` (returns `{show, text}` — server-side resolves home-vs-visitor text from the user's `controller_status` rather than making the client fetch both texts and pick, unlike the live site's dialog) and `POST /welcome-message/ack` (reusing `AuthProfileRead`/`AuthProfileUpdate` like broadcasts.rs does). Also added `disable_welcome_message` to `controller_lifecycle.rs` and wired `enable_welcome_message` into `activate_visitor_membership` (`src/repos/users.rs`) to close gap (4).
- **Change broadcasts** (site-wide "what's new" banner system) — ✅ done. New `src/handlers/broadcasts.rs` + `src/repos/broadcasts.rs`: admin CRUD at `GET/POST /admin/broadcasts`, `PATCH/DELETE /admin/broadcasts/{id}` (new `WebBroadcastsRead/Create/Update/Delete` permissions, seeded in `migrations/0035_broadcast_permissions.sql`), plus self-service `GET /broadcasts/me`, `POST /broadcasts/{id}/seen`, `POST /broadcasts/{id}/agree` (gated on the existing `AuthProfileRead`/`AuthProfileUpdate` self-service permissions, matching how `org.rs`'s `/loa/me` does it). Two deliberate departures from the live site, both driven by how osmium's schema (unlike Prisma's `unseenBy`/`seenBy`/`agreedBy` `User[]` relations) already normalizes this into a single `change_broadcast_user_state(broadcast_id, user_id, seen_at, agreed_at)` row: (1) broadcasts are global to all users — there's no `unseenBy`-style initial targeting set, "unseen" is simply "no state row yet"; (2) `exempt_staff` auto-inserts `agreed_at` rows for every `STAFF`-role user at creation time (via `access.user_roles`), same effect as the live site's per-staff-member `handleAgreeBroadcast` loop. **Not ported**: the "broadcast posted" email notification and the 6-month stale-broadcast cleanup job (`deleteStaleBroadcasts`) — both are separate features from the CRUD gap this tier is closing, not schema/route gaps.
- **Site pages** (`web.pages`) — exists, unreferenced. Unclear current live-site consumer; confirm against `app/` static pages (privacy/license/credits) before building anything — may be dead schema rather than a real gap.
- **Site settings** (`web.site_settings`) — exists, unreferenced. Same caveat as above — audit intended use before implementing.

## Tier 3 — Schema exists, routes/handlers don't

Osmium migrated these tables ahead of the corresponding feature work, but nothing reads or writes them yet:

- **Lesson rubric authoring** (`training.lesson_rubrics`, `lesson_rubric_criteria`, `lesson_rubric_cells`) — ✅ done. Osmium already *read* these (joined during session rubric-score validation, `src/repos/training/sessions.rs:428-430`) but had no create/update endpoints. New endpoints in `src/handlers/training.rs` (repo layer in `src/repos/training/rubrics.rs`): `GET/POST /training/lessons/{lesson_id}/rubric-criteria`, `PATCH/DELETE .../rubric-criteria/{criteria_id}`, `POST .../rubric-criteria/{criteria_id}/cells`, `PATCH/DELETE .../cells/{cell_id}`, plus `GET /training/lessons/{lesson_id}/rubric` for reading the full structure. Mirrors the live site's `lessonRubricCriteria.ts`/`lessonCriteriaCell.ts` semantics: a lesson's rubric is auto-created on its first criteria (no standalone "create rubric" step), cell points must be unique per criteria and capped at the criteria's `max_points`. One deliberate improvement over the source: cell-points validation is checked against the criteria's actual DB-stored `max_points`, not a client-supplied value (the live site trusts a form field for this bound).
- **Statistics prefixes** (`stats.statistics_prefixes`) — ✅ done. Table already existed (seeded with a fixed singleton row, `id = 'default'`, migration `0013_seed_reference_data.sql`), but `src/handlers/stats.rs` had no CRUD for it. New `GET`/`PATCH /api/v1/admin/stats/prefixes` (repo layer in `src/repos/stats.rs`), gated behind new `StatsPrefixesRead`/`StatsPrefixesUpdate` permissions (`stats.prefixes.read`/`stats.prefixes.update`, seeded in `migrations/0034_statistics_prefixes_permissions.sql` and granted to `STAFF`). Deliberately simpler than the live site's `statisticsPrefixes.ts`, which treats the row as a fresh cuid each update (`deleteMany()` then `upsert()`) — osmium instead upserts onto the fixed `'default'` id the schema already seeds, so there's no client-supplied `id` field in the request at all. Note: the other four `stats.rs` endpoints remain intentionally public/unauthenticated — this is the only permission-gated one in the file.

## Tier 4 — Logic-only / low backend risk — ✅ all verified, zero backend work needed

- `classifyPosition.ts` — **confirmed pure client-side logic, no backend involvement at all.** Read the actual source: it's a callsign-string classifier (`_GND`/`_TWR`/`_RMP` → Local, `_CTR` → Enroute, `_APP` → Terminal, etc.), used in exactly one place (`OpsPlanView.tsx`) to sort position labels into display columns. Never called from a server action, never persisted, doesn't touch `create_event_position` or any other write path. Osmium already serves the raw position/preset data this reads (`events.event_positions`, `event_position_presets`) unchanged — the frontend can keep or reimplement this classifier in TypeScript with zero osmium changes.
- `mjml.ts` — email-template rendering helper. Superseded already: osmium replaced MJML with its own RSX-based email templates ([[osmium-repo-migration-pattern]] / spec 006, and further built out this session by spec 013's branding work). No action needed.
- Static content pages with no data dependency (`credits`, `license`, `privacy`, `misc/AvDr`, `teamspeak`) — confirmed no `fetch`/`prisma`/action calls in these page components. Likely need no backend work unless a future decision moves them into the (currently-unreferenced) `web.pages`/`web.site_settings` tables from Tier 2.
- `app/web-system/*` (webmaster discord-configs + overview admin pages) — **confirmed already covered.** Discord-config pages map directly to the already-implemented `integrations::list_discord_configs`/create/update/delete family. The overview page's two data needs — recent audit log entries and per-sync-job last-run status — are covered by `admin::list_audit_logs` and `org::list_jobs`/`get_job` respectively; the latter is a strict superset of the live site's single `SyncTimes` row (osmium's job-runs framework from spec 007 tracks status/history per job, not just a bare timestamp) — no separate `sync_times` table/endpoint needs porting, the newer system already supersedes it.

---

## New feature (not on the live site today): self-hosted FAA preferred-routes data

**Deferred** — explicit product decision: come back to this later, not dropped.

The site's old PRD page (`app/prd/`, removed in `0855eb0`) worked by proxying a third-party service (`api.aviationapi.com/v1/preferred-routes/search`) at request time — no data ownership, and it broke/was cut once that dependency became a liability. The replacement is not a re-proxy: **osmium should download the FAA's own preferred-route data, own a copy of it, and serve search over that copy from the API.**

**Source**: the FAA publishes preferred IFR routes as part of the National Flight Data Center's 28-day NASR subscription (the same AIRAC-cycle data source used for navaids/airports/procedures), historically as a fixed-width `PFR.txt`-style file. Confirm the current exact download URL and file format against FAA NFDC (`nfdc.faa.gov`) before implementing — the source location and format are the one part of this item not verified in this session and should not be assumed stable.

**Approach**, following patterns already established elsewhere in osmium:
- New schema: a `routes` (or similar) domain with an `preferred_routes` table — origin, destination, route string, altitude, aircraft type, hours/flow/sequence fields, area, ARTCC boundaries — mirroring the fields the old UI displayed (see `app/prd/page.tsx`'s table columns: Origin, Destination, Route, Hours 1-3, Type, Area, Altitude, Aircraft, Flow, Sequence, Departure/Arrival ARTCC).
- Ingestion as a background job, not a request-time fetch: reuse the existing jobs abstraction (spec 007; `src/jobs/`, `platform.job_runs`, `org::list_jobs`/`run_job`) to add a scheduled "sync FAA preferred routes" job that downloads the current NASR cycle file, parses it, and upserts into the new table — same shape as the existing `roster_sync` job, replacing stale data wholesale or diffing per AIRAC cycle (28 days) rather than per-request.
- New read endpoint(s): a search route (e.g. `GET /api/v1/routes/preferred?origin=&destination=`) backed by the local table instead of an outbound call — the API becomes the source of truth and stops taking a runtime dependency on a third party.
- Note: airport/route-practice data (`Airport`/`Runway`/`RunwayInstruction`) was previously flagged here as a natural pairing, since both are FAA/NASR-flavored reference data with the same "download once, serve locally" shape — that item has since been dropped from scope (see Non-goals), so this feature is scoped standalone.

## New feature (not on the live site today): customizable emails

The website will get an email builder — customizable color, text, font, header, and formatting, with a live preview of the full rendered email before sending. Osmium needs the backend to support this.

**✅ Done** — see **[013 — Customizable Email Branding & Live Preview](013-customizable-email-branding.md)**. Expanded from the original 5-field draft to 18 fields (10 individually-configurable colors, logo, 2 font-family roles, font-size scale, corner style) per explicit "as customizable as possible" direction, layered on top of the existing 23 rsx templates from spec 006 as one global brand config — not per-template overrides, not a user-authored template replacement. The existing `POST /emails/preview` endpoint was extended with an optional draft-branding override so the builder's live preview works against unsaved edits.

---

## Already-planned, not-yet-started infra work

Three specs already exist in this directory but haven't been executed (verified: `auth.rs`/`admin.rs`/`dev.rs`/`emails.rs`/`health.rs` still call `sqlx::query*` directly; `Cargo.toml` has no `tower_governor`). Note: migration `0033` is now taken (by Tier 1.1 above) — 011's "next available migration number: 0033" note is stale; it'll need `0034` when implemented.

- **[009 — Final Handler Layering Cleanup](009-final-handler-layering-cleanup.md)**: migrate the last 5 handler files to the repo-layer + `RequirePermission<P>` pattern.
- **[010 — IP-Based Rate Limiting](010-ip-rate-limiting.md)**: no rate limiting exists anywhere today; adds `tower_governor` with a permission-based bypass.
- **[011 — Durable IP Request Tracking](011-ip-request-tracking.md)**: depends on 010's IP-extraction helper; persists per-request IP metadata for admin auditing.

These are orthogonal to the feature-parity gaps above (pure hardening/cleanup, no user-facing feature), but should be sequenced into the same roadmap since they're the next queued work.

---

## Suggested next steps, in order

1. ~~**Tier 1.1 (additional trainers)**~~ — done.
2. ~~**Tier 3: rubric authoring**~~ — done.
3. ~~**Tier 3: statistics prefixes CRUD**~~ — done. Tier 3 is now fully closed.
4. ~~**Tier 2: change broadcasts**~~ — done. Versions/changelog (the other item originally bundled into this step) was dropped from scope by product decision — see Non-goals. All completed items above still need a DB-backed integration test run once a dev Postgres is available (none of it has run against a live database in this environment).
5. ~~**Tier 2: welcome messages, captcha proxy**~~ — done. **ATC booking proxy deferred** (explicit product decision — revisit once osmium is out of dev). Tier 2 is otherwise fully closed.
6. **Self-hosted FAA preferred-routes data** — **deferred** (explicit product decision — revisit later).
7. ~~**Customizable emails**~~ — done, see spec 013.
8. ~~**Tier 1.2 and Tier 4**~~ — done. Tier 1.2 turned out to be a 5-domain composite dashboard; 3 of 5 already covered, 2 genuine gaps found and closed (user certifications, user event-position history). All 4 Tier 4 items confirmed to need zero backend work (verified against actual source, not assumed).
9. Interleave **specs 009-011** (handler cleanup, rate limiting, IP tracking) wherever convenient — they don't block or get blocked by the feature-parity work above, but 009 should land before any of Tiers 1-3 touch `admin.rs`/`emails.rs` to avoid migrating the same handler twice.

**Remaining open work, all deliberately deferred (not gaps I missed):** ATC booking proxy, self-hosted FAA preferred-routes data, and specs 009-011. Every feature-parity item that was actually in scope is now closed.

## Non-goals

- Re-proxying the removed third-party aviation-charts/PRD integration (`actions/charts.ts`, the old `actions/prd.ts`'s call to `api.aviationapi.com`) — the live site deleted this itself (`0855eb0`) and it's not a gap to restore as-is. A *self-hosted* FAA-sourced replacement for preferred routes is a new feature, tracked separately above ("New feature: self-hosted FAA preferred-routes data") — the two are not the same thing.
- Re-deriving the "Financial Committee" roster section — confirmed to be a pure frontend display grouping over existing staff-position data (`app/controllers/staff/page.tsx`), no new backend model.
- **Common mistakes** (`training.common_mistakes`, `training.training_ticket_common_mistakes`) — dropped from scope by product decision; not being ported. The tables remain in the schema (unused) but no handler/route work is planned for them.
- **Version / changelog** (`web.versions`, `web.version_change_details`) — dropped from scope by product decision; the live site stopped using this feature. The tables remain in the schema (unused) but no handler/route work is planned for them.
- **Airport / route-practice data** (`Airport`, `Runway`, `RunwayInstruction` models; `actions/airports.ts`, `app/airports/`, `app/routepractice/`) — dropped from scope by product decision; not being ported. Distinct from the *removed* aviation-charts/PRD integration (`actions/charts.ts`, `actions/prd.ts`) mentioned above — that one the live site deleted itself; this one is still live on the site but osmium isn't picking it up.
