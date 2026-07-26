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

- **ATC booking proxy** — **done 2026-07-25** (was deferred). New osmium
  passthrough `handlers/bookings.rs` (`GET/POST/PUT/DELETE /api/v1/bookings`)
  holds `ATC_BOOKING_TOKEN` server-side and proxies `atc-bookings.vatsim.net`,
  reusing `stats::fetch_statistics_prefixes` for the callsign check and
  reproducing the ≤2-active / 2h-advance / 72h-max / 2h-duration rules
  (non-training only). Data-dependent write auth: self non-training →
  `auth.profile.update`, otherwise `training.appointments.update`. The website's
  4 bookings pages, homepage widget, and training-appointment sync now use it;
  `actions/atcBooking.ts` + `actions/trainingAppointment.ts` were deleted.
  Verified logic+wiring only (dev has no token → endpoints return 503).
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

> **CONFIRMED (Track B, 2026-07-25):** verified live against `nfdc.faa.gov` during implementation, including downloading and parsing the real file (13,309 routes). Current shape:
> - **URL**: `https://nfdc.faa.gov/webContent/28DaySub/extra/{DD}_{Mon}_{YYYY}_PFR_CSV.zip` — the current 28-day NASR cycle (anchor `2026-05-14`, +28 days; day zero-padded, e.g. `09_Jul_2026`).
> - **Format**: the legacy fixed-width `PFR.txt` is superseded by a CSV bundle. The zip contains `PFR_BASE.csv`, `PFR_SEG.csv`, and **`PFR_RMT_FMT.csv`** — the last is the richest single-file match to the old PRD UI (columns: `Orig, Route String, Dest, Hours1, Type, Area, Altitude, Aircraft, Direction, Seq, DCNTR, ACNTR`) and is what the ingest job parses.
> - **Caveat (still open, non-code):** the FAA has announced a NASR format change effective the **2026-09-03** AIRAC cycle. The parser is header-driven (maps by column name, not position) and the URL is env-overridable to absorb this, but a human should re-confirm the source before the job is **enabled in production** (it is disabled by default — `FAA_PREFERRED_ROUTES_SYNC_ENABLED`).

**Approach**, following patterns already established elsewhere in osmium:
- New schema: a `routes` (or similar) domain with an `preferred_routes` table — origin, destination, route string, altitude, aircraft type, hours/flow/sequence fields, area, ARTCC boundaries — mirroring the fields the old UI displayed (see `app/prd/page.tsx`'s table columns: Origin, Destination, Route, Hours 1-3, Type, Area, Altitude, Aircraft, Flow, Sequence, Departure/Arrival ARTCC).
- Ingestion as a background job, not a request-time fetch: reuse the existing jobs abstraction (spec 007; `src/jobs/`, `platform.job_runs`, `org::list_jobs`/`run_job`) to add a scheduled "sync FAA preferred routes" job that downloads the current NASR cycle file, parses it, and upserts into the new table — same shape as the existing `roster_sync` job, replacing stale data wholesale or diffing per AIRAC cycle (28 days) rather than per-request.
- New read endpoint(s): a search route (e.g. `GET /api/v1/routes/preferred?origin=&destination=`) backed by the local table instead of an outbound call — the API becomes the source of truth and stops taking a runtime dependency on a third party.
- Note: airport/route-practice data (`Airport`/`Runway`/`RunwayInstruction`) was previously flagged here as a natural pairing, since both are FAA/NASR-flavored reference data with the same "download once, serve locally" shape — that item has since been dropped from scope (see Non-goals), so this feature is scoped standalone.

## New feature (not on the live site today): self-service data export (GDPR right of access)

**Not started.** Users can request an export of all personal data osmium holds on them; staff/trainers additionally get instructor-only material and tickets they graded. This is a GDPR compliance feature (vZDC has EU members/visitors, so GDPR applies regardless of where the ARTCC itself is based), not a live-site parity item — no equivalent exists on the current website today.

**Legal basis**: this maps to **GDPR Article 15 (right of access by the data subject)**. A few things Article 15 requires that the "grab all their data" framing above doesn't fully cover on its own — worth deciding on explicitly before implementing, not discovering after:

- **Article 15(1)(a)-(h)**: a compliant response isn't just the raw records — it must also state the *purposes* of processing, the *categories* of data, *recipients* (or categories of recipients, e.g. "training staff", "VATUSA/VATSIM via roster sync"), the *retention period* (or criteria for it), the existence of the rights to rectification/erasure/restriction/objection and to lodge a complaint with a supervisory authority, and the *source* of data not collected directly from the user (e.g. VATSIM/VATUSA roster sync). Most of this is static/boilerplate text (can live in the response alongside the data dump, or in a linked privacy notice) rather than something queried per-request, but it needs to actually be present in what's returned, not just the DB rows.
- **Article 12(3)**: response is normally due within one month of the request (extendable to three for complex/numerous requests, with notice to the requester). If this ships as a synchronous API endpoint this is moot — but worth deciding now whether export is synchronous (small footprint, likely fine given osmium's data volumes) or an async job (reuse the existing jobs framework from spec 007) before committing to an API shape.
- **Article 20 (data portability)**, adjacent but distinct from Article 15: for data the user *provided themselves* (profile fields, self-service form submissions), they additionally have the right to receive it in a "structured, commonly used, machine-readable format." A JSON export satisfies both Article 15 and Article 20 at once, so this doesn't need separate handling as long as the export format is JSON, not e.g. rendered HTML/PDF only.
- **Article 5(2) (accountability)**: the export *request itself* should be logged (who requested it, when, and ideally what was included) via the existing `audit.logs` mechanism (`record_audit_entry`) — the org needs to be able to demonstrate it handled access requests, not just handle them.
- **Identity verification**: satisfied for free here since the request is authenticated via the existing session — no separate identity-proofing step needed, unlike a support-ticket-based DSAR process.

**Open compliance question — flagging, not deciding**: the instruction to strip trainer-only notes from a *regular user's own* export needs a second look before being taken as settled. Article 15(4) does carve out an exception where the right "shall not adversely affect the rights and freedoms of others" — but that exemption is narrow and is generally read as protecting *other data subjects'* personal data or genuine trade secrets, not an institution's internal evaluative commentary written *about* the requester specifically. A trainer's private note on a specific trainee's ticket is arguably still the trainee's own personal data (it's an opinion/assessment concerning them), and several EU data protection authorities and courts have leaned toward disclosure of exactly this kind of internal evaluative material (performance reviews, teacher's notes on a student) being in scope for the data subject it's about, with only narrow, jurisdiction-specific carve-outs (e.g. UK GDPR's exam-script and some reference exemptions, which don't map cleanly here and don't apply EU-wide). **Recommend confirming this filtering decision with whoever owns compliance for vZDC before building it as described** — if trainer notes must legally go to the trainee too, the "regular users get the redacted view" design would need to change to "trainer notes go to the trainee as well, just not attributed/exposed in the normal ticket UI the same way," which is a materially different implementation. Building the staff/trainer path (full notes + mentor-of-record tickets) as described is much less contentious — that's the org accessing its own operational records, not a third party's data.

> **RESOLVED (Track B, 2026-07-25):** decision is **INCLUDE** — trainer/staff evaluative notes written *about* the requester (training-session `trainer_comments`, feedback `staff_comments`, and dossier entries) are returned in that subject's own export as their personal data under Article 15, with the individual staff **authors not identified**. This matches osmium's existing `GET /training/sessions/{id}` self-read (which already returns `trainer_comments` to the student) and the leaning of the analysis above. Applied consistently across all three note surfaces. The "regular users get the redacted view" design in the original ask was **not** built. The resolution and its rationale are recorded in the export's `gdpr_notice` (`evaluative_notes_disclosure`) and in `docs/api/data-export.md`. A human compliance owner should still ratify this on the record, but it is no longer an open blocker.
- **Related but out of scope here — don't scope-creep this item to include them**: **Article 16 (rectification)** and **Article 17 (erasure / "right to be forgotten")** are separate rights or requests this doesn't cover; a full GDPR compliance posture eventually needs those too but they're materially different features (erasure in particular has to reconcile against retention obligations — training records, audit logs — that may need to survive an erasure request under a different legal basis). Worth its own backlog item later, not bundled into this one.

**Implementation shape** (sketch, not a committed design):
- New self-service endpoint, e.g. `GET /api/v1/me/data-export`, returning a single JSON document assembled across every domain that links to the requesting user — identity/profile, training (sessions, appointments, tickets/rubric scores, additional-trainer notes per the access-level rule above, dossier), events (position history), stats (hour totals/history), feedback, incidents, LOA, solo/other certifications, staffing and SUA requests, broadcast seen/agree state, welcome-message state, API keys (metadata only, never secrets), visitor applications, TeamSpeak/Discord identity links, emails sent to them (from `emails.outbox`), and relevant audit log entries.
- Staff/trainer variant of the same endpoint (or a query parameter/permission-gated field) additionally includes: trainer-authored notes on tickets they wrote, and full tickets for sessions where they were the instructor/mentor of record — this is the org's own operational data about *their* training activity, not a third party's personal data, so it's a materially easier compliance case than the trainee-notes question above.
- Needs a careful table-by-table pass for **third-party data leakage** within joined records — e.g. a session record naturally references the *other* party (student sees instructor's name and vice versa, an event position record might list who trained whom) — this is normal and expected (context needed to make the record intelligible), but should be scoped to "identifying context for records about *this* user," not "every full profile of every person mentioned anywhere in a linked record."
- Rate-limit the endpoint (ties into spec 010 once it lands) — a full cross-domain export is one of the more expensive single requests in the API, and a DSAR endpoint is a predictable target for either accidental hammering or abuse.

## New feature (not on the live site today): authenticated user impersonation

**Not started.** Server admins need a way to temporarily act as another human user (support, debugging, reproducing permission bugs) without sharing passwords or using the unauthenticated local-dev login-as shortcut. This is a new product capability — the live site has no equivalent.

### Relationship to today's `login_as_cid`

Osmium already has `GET /api/v1/auth/login/as/{cid}` behind `DEV_LOGIN_AS_CID_ENABLED`. That route is **not** production impersonation:

- it is env-gated and unauthenticated when enabled (no permission check at runtime)
- it creates a normal session for the target with **no real-actor field**
- it can bootstrap synthetic users for arbitrary CIDs
- every subsequent ACL and audit action looks like the target did it

**Shipping this feature retires that route.** Remove `login_as_cid`, `DEV_LOGIN_AS_CID_ENABLED` / `dev_impersonation_enabled()`, related router wiring, docs, Bruno/tests, and the conceptual `auth.dev_login.create` permission. Local “become someone else” uses the real impersonation flow after a normal login (typically as `SERVER_ADMIN`). Do not promote or rename the old endpoint into production.

### Intended design

- **Name / API**: production feature is **impersonation**, not “login as CID”. Sketch:
  - `POST /api/v1/admin/impersonate/{cid}` — start
  - `POST /api/v1/admin/impersonate/stop` — end and restore the admin session
- **Permission**: new `auth.impersonate.create` (name final at implementation). Seed / effectively hold only on `SERVER_ADMIN` — same audience as Website Management. Do **not** grant to facility `STAFF` / ATM-shaped roles by default; treat as non-assignable via the normal facility access UI if needed.
- **Session model**: extend `identity.sessions` with `impersonator_user_id` (nullable FK) plus start metadata (e.g. `impersonation_started_at`, optional reason). Keep `user_id` = effective subject so ACL and UI resolve as the target; keep the real admin on the session for audit and stop.
- **Auth context**: middleware loads both subject and optional impersonator into request context (extend `CurrentUser` or parallel fields). Permissions continue to evaluate for the **target**.
- **`/me`**: expose `impersonating: true` plus enough real-actor identity for a Website Management / global banner and a stop control. Do not leak more than the banner needs.
- **Guards**: refuse nested impersonation; refuse impersonating another `SERVER_ADMIN` (or any target that would escalate beyond the actor); human sessions only — service accounts must not start or be used as impersonation subjects for normal ops.
- **Session lifecycle**: shorter TTL for impersonation sessions than normal 30-day logins; on stop, revoke the impersonation session and restore the prior admin session (or force re-login — prefer restore). Do not leave a durable orphaned “as target” session.
- **Side effects**: prefer blocking or hard-tagging outbound side effects while impersonating (emails, Discord, VATUSA/roster writes) so the target is not silently contacted or mutated via integrations.

### Server-level audit (facility admins must not see these logs)

Impersonation start/stop (and preferably attribution that an action occurred *while* impersonating) is **server-level only**.

- Record with a distinct resource type (e.g. `AUTH_IMPERSONATION`) via the existing `access.audit_logs` / `record_audit` path.
- Today `STAFF` still holds `audit.logs.read` (via the old `audit.read` remap), so a naïve insert into the shared audit feed would leak to facility admins.
- **`list_audit_logs` must filter `AUTH_IMPERSONATION` (and any impersonator fields on other rows) out unless the caller is `SERVER_ADMIN`** (or holds an explicit server-audit permission introduced for this purpose). Facility Admin UIs that call the same endpoint must never see who was impersonated or by whom.
- UI home for these logs: Website Management audit (`/website-management/audit`), not Facility Admin. Website Management is already `SERVER_ADMIN`-gated; this feature depends on that gate remaining the only consumer for server-sensitive audit rows.
- While impersonating, `resolve_audit_actor` must attribute durable audit entries to the **impersonator**, with the target recorded in after-state / scope metadata — never the reverse.

### Security vulnerabilities (must be designed against, not discovered after)

1. **Audit forgery / accountability loss** — swapping only `session.user_id` without storing the impersonator makes every action look like the victim’s. Mitigation: always persist `impersonator_user_id` and resolve it for audit.
2. **Facility-admin visibility leak** — impersonation events in the STAFF-readable audit feed reveal sensitive support/investigation activity. Mitigation: distinct resource type + filter non–server-admin reads (see above).
3. **Privilege elevation** — impersonating another `SERVER_ADMIN` or a higher-priv account is catastrophic. Mitigation: block nested impersonation; refuse `SERVER_ADMIN` / privilege-escalating targets; no service-account start path.
4. **Unauthenticated / env-gated hole (current login-as)** — today’s route is open whenever the flag is on and can bootstrap users. Mitigation: delete it when this ships; never reuse it as the production path.
5. **Durable orphaned sessions** — stop that only clears a cookie can leave a long-lived target session. Mitigation: revoke the impersonation row on stop; restore admin session; shorter impersonation TTL.
6. **Self-service mutation while acting as target** — profile, email preferences, Discord/TeamSpeak linkage, etc. mutate the victim’s account. Mitigation: block sensitive self-service writes during impersonation, or require explicit confirmation plus server-only audit of those writes.
7. **Side-effect channels** — emails, Discord posts, VATUSA/roster writes attributed to the target. Mitigation: block or hard-tag outbound side effects while impersonating (policy above).
8. **Cookie / session fixation** — putting actor IDs in client-readable cookies, or dual insecure cookies, invites tampering. Mitigation: keep a single httpOnly `osmium_session`; actor identity lives in the DB row, not a readable cookie.
9. **Permission sprawl** — granting `auth.impersonate.create` to broad STAFF turns every staff account into a full identity takeover. Mitigation: seed only onto `SERVER_ADMIN`; keep out of facility access-assignment UI.
10. **`/me` disclosure** — the banner needs impersonation state, but over-sharing real-actor details on a compromised or mis-issued session is unnecessary. Mitigation: minimal fields for banner + stop only.

### Implementation sketch (not a committed PR plan)

- Migration on `identity.sessions` (`impersonator_user_id`, start metadata).
- Permission seed for `auth.impersonate.create` (SERVER_ADMIN only).
- Session create/lookup helpers in `src/repos/access.rs`; extend auth middleware / `CurrentUser`.
- Handlers + router for start/stop; OpenAPI + Bruno + docs.
- Audit filter in `list_audit_logs` / `repos/audit.rs`; thread impersonator through `resolve_audit_actor`.
- Website: start control (Website Management), global “impersonating as …” banner + stop, audit rows visible only under `/website-management/audit`.
- Tests: ACL-as-target, audit-as-impersonator, facility caller cannot read `AUTH_IMPERSONATION`, stop restores admin, nested/SERVER_ADMIN target refused, old login-as route gone.
- Delete `DEV_LOGIN_AS_CID_ENABLED` path as part of the same change set.

## New feature (not on the live site today): customizable emails

The website will get an email builder — customizable color, text, font, header, and formatting, with a live preview of the full rendered email before sending. Osmium needs the backend to support this.

**✅ Done** — see **[013 — Customizable Email Branding & Live Preview](013-customizable-email-branding.md)**. Expanded from the original 5-field draft to 18 fields (10 individually-configurable colors, logo, 2 font-family roles, font-size scale, corner style) per explicit "as customizable as possible" direction, layered on top of the existing 23 rsx templates from spec 006 as one global brand config — not per-template overrides, not a user-authored template replacement. The existing `POST /emails/preview` endpoint was extended with an optional draft-branding override so the builder's live preview works against unsaved edits.

---

## ~~TODO: update hand-maintained docs for everything built this session~~ — done

All three hand-maintained doc surfaces are now in sync with everything shipped in this roadmap:

- **`docs/route-permissions.md`** — added `POST /captcha/verify` (public), the full `/broadcasts/*` family, `/welcome-message*` (self-service + admin), `/admin/emails/branding`, `/admin/stats/prefixes`, `/users/{cid}/certifications`, `/users/{cid}/event-positions`, and the `/training/lessons/{lesson_id}/rubric*` family. Also closed a pre-existing gap found in passing: `/users/{cid}/solo-certifications` was missing from this file even though it predates this session.
- **`docs/api/*.md`** — added 3 new files (`broadcasts.md`, `welcome-messages.md`, `captcha.md`) plus targeted updates to `overview.md`, `stats.md`, `emails.md`, `training.md`, `workflows.md`, `users.md`, `events.md`.
- **`src/docs/markdown_site.rs`** — registered the 3 new markdown files as `DocPage` entries so they're actually served at `/docs/api/{slug}` (not auto-discovered from the filesystem).
- **`Bruno/osmium/`** — added new `broadcasts/`, `welcome-messages/`, `captcha/` folders plus new request files in the existing `stats/`, `emails/`, `training/`, `workflows/`, `events/` folders, matching each folder's existing style convention (full params+examples vs. minimal, per folder precedent).

## Already-planned, not-yet-started infra work

Three specs already exist in this directory but haven't been executed (verified: `auth.rs`/`admin.rs`/`dev.rs`/`emails.rs`/`health.rs` still call `sqlx::query*` directly; `Cargo.toml` has no `tower_governor`). Note: migration `0033` is now taken (by Tier 1.1 above) — 011's "next available migration number: 0033" note is stale; it'll need `0034` when implemented.

- **[009 — Final Handler Layering Cleanup](009-final-handler-layering-cleanup.md)**: migrate the last 5 handler files to the repo-layer + `RequirePermission<P>` pattern.
- **[010 — IP-Based Rate Limiting](010-ip-rate-limiting.md)**: no rate limiting exists anywhere today; adds `tower_governor` with a permission-based bypass.
- **[011 — Durable IP Request Tracking](011-ip-request-tracking.md)**: depends on 010's IP-extraction helper; persists per-request IP metadata for admin auditing.

These are orthogonal to the feature-parity gaps above (pure hardening/cleanup, no user-facing feature), but should be sequenced into the same roadmap since they're the next queued work.

---

## Website cutover status & osmium changes it drove (updated 2026-07-24)

The website (`~/Programing/website`) has now cut over to osmium for essentially
every domain, including its **authorization and identity layer** (the hardest
part, tracked in `website/docs/osmium-migration-plan.md` §5 "Phase 8.5"). That
work drove several **additive osmium changes** beyond the parity audit above:

- **`roster_sync` now also writes `access.user_roles`** — auto-grants
  `STAFF`/`INS`/`MTR` from VATUSA facility roles (fold: `ATM/DATM/TA/EC/FE/WM →
  STAFF`, `INS → INS`, `MTR → MTR`), mirroring the staff-position auto-sync and
  respecting `source='manual'` overrides (migration `0053` added the `source`
  column). This is what lets the website gate on osmium roles without locking
  out real staff. `EVENT_STAFF` has no VATUSA equivalent and stays manual.
- **`POST /api/v1/admin/users/{cid}/access` gained an optional `role_names`**
  field (assignable set `STAFF/INS/MTR/EVENT_STAFF`, never `SERVER_ADMIN`) so
  admins can manually grant/revoke coarse roles — actor-scoped like the existing
  permission editor. New `users.flags.*` / `users.operating_initials.update`
  permissions + `GET/PATCH .../flags` and `PATCH .../operating-initials` (migr.
  `0052`) back the admin "user settings" + OI-matrix surfaces.
- **`/api/v1/me` gained `role_names`, `flags`, and `controller_status`** — the
  website's self-service gates need these client-side (server components can't
  read osmium's host-only session cookie). `users.directory.read` was split out
  of the overloaded `users.directory_private.read` and added to the baseline so
  every controller sees roster OI/status without also getting admin access.

**⚠️ Deploy gate**: because website authorization now reads `access.user_roles`,
that table must be populated before the flipped site goes to production or staff
are locked out — set `ROSTER_SYNC_ENABLED=true` + `VATUSA_API_KEY` and let one
sync run, manually grant `EVENT_STAFF`, confirm `OSMIUM_SERVER_ADMIN_CID`.

- **Certifications & Progression is now fully implemented in osmium** (2026-07-24;
  the last deferred domain). All schema pre-existed (`org.certification_types`
  et al. from `0004`, `training.training_progressions` et al. from `0006`); this
  added the missing write/compute/trigger side:
  - **Certification-type admin CRUD** + **bulk user-cert write** (`org.rs`,
    `src/repos/org/certifications.rs`) behind a new `org.certifications.read`/
    `.update` permission pair granted to `STAFF` (migration `0054`). The update
    guard rejects removing an option a `lesson_roster_changes` row still grants.
  - **Progression status** (`GET /users/{cid}/progression`) computes per-step
    pass state from the latest training ticket for each step's lesson, and
    **force-complete** (`POST /users/{cid}/progression/complete`) advances to the
    `next_progression_id` (or unassigns) once non-optional steps pass — self-gated
    by the `no_force_progression_finish` flag.
  - **Auto-advance** fires in three places via one shared helper
    (`training_admin::advance_progression_if_complete`): instantly after a
    training-session save (non-optional gate), during the periodic roster sync
    (stricter all-steps gate), and via the manual endpoint. Roster sync also
    auto-grants `UNRESTRICTED` for `auto_assign_unrestricted` cert types to rated
    controllers and hands new home OBS their starter progression once. The
    `progression.assigned`/`progression.removed` emails (already-built rsx
    templates) are enqueued on every change.

**Two small osmium additions the website needed** (both **built 2026-07-25** to
remove the cert/progression-unblocked NextAuth server-session reads):
1. ~~training-session self-read~~ — **done.** `GET /training/sessions/{id}` is now
   data-dependent: the session's own student reads it via `auth.profile.read`,
   others via `training.sessions.read` (`handlers/training.rs`). Unblocked the
   website's `/profile/training/[id]` self-view.
2. ~~admin edit-another-user's-profile~~ — **done.** New
   `PATCH /api/v1/admin/users/{cid}/profile` (`users.flags.update`) updates
   preferred name / bio / timezone for a target cid (`handlers/admin.rs` +
   `repos/users.rs::admin_update_user_profile`); OI stays on the existing
   operating-initials endpoint, and the event-notification opt-in is left as a
   self-service preference (never clobbered). Unblocked the website's admin
   controller-edit pages, which now use osmium instead of Prisma `updateCurrentProfile`.

**Website Phase 6 (NextAuth retirement) status (2026-07-25):** the
cert/progression-unblocked `getServerSession` sites are now all client-side
osmium (`profile/overview`, `AdminControllerInformation`, both controller-edit
pages + `ProfileEditCard` admin-mode, `profile/training/[id]`; `ProfileCard`
gained a client + osmium-adapter form). **Bookings also done 2026-07-25** (see
the ATC-booking-proxy item above) — the 4 bookings pages are now client-side
osmium too. Remaining `getServerSession`: only the legacy Prisma
`actions/log.ts`/`actions/dossier.ts`/`lib/update.ts` cron/logging (retire at
Prisma cutover). NextAuth deletion + login/logout dual-flow is now the final
step, gated only on those legacy Prisma paths.

## Suggested next steps, in order

1. ~~**Tier 1.1 (additional trainers)**~~ — done.
2. ~~**Tier 3: rubric authoring**~~ — done.
3. ~~**Tier 3: statistics prefixes CRUD**~~ — done. Tier 3 is now fully closed.
4. ~~**Tier 2: change broadcasts**~~ — done. Versions/changelog (the other item originally bundled into this step) was dropped from scope by product decision — see Non-goals. All completed items above still need a DB-backed integration test run once a dev Postgres is available (none of it has run against a live database in this environment).
5. ~~**Tier 2: welcome messages, captcha proxy**~~ — done. **ATC booking proxy deferred** (explicit product decision — revisit once osmium is out of dev). Tier 2 is otherwise fully closed.
6. ~~**Self-hosted FAA preferred-routes data**~~ — **done (Track B, 2026-07-25, pending combined merge)**. New `routes` domain (migration `0060`, `repos/routes.rs`, `handlers/routes.rs` `GET /routes/preferred`, `jobs/faa_preferred_routes.rs`). Source URL/format confirmed live (see the FAA section above); job disabled by default.
7. ~~**Customizable emails**~~ — done, see spec 013.
8. ~~**Tier 1.2 and Tier 4**~~ — done. Tier 1.2 turned out to be a 5-domain composite dashboard; 3 of 5 already covered, 2 genuine gaps found and closed (user certifications, user event-position history). All 4 Tier 4 items confirmed to need zero backend work (verified against actual source, not assumed).
9. Interleave **specs 009-011** (handler cleanup, rate limiting, IP tracking) wherever convenient — they don't block or get blocked by the feature-parity work above, but 009 should land before any of Tiers 1-3 touch `admin.rs`/`emails.rs` to avoid migrating the same handler twice.
10. ~~**Update hand-maintained docs**~~ — done. See the dedicated section above for exactly what changed.
11. ~~**GDPR self-service data export (right of access)**~~ — **done (Track B, 2026-07-25, pending combined merge)**. `GET /me/data-export` (`handlers/data_export.rs`, `repos/data_export.rs`, `models/data_export.rs`). The open compliance question (trainer-note redaction) is **resolved: INCLUDE** — see the RESOLVED note in the dedicated section above.
12. **Authenticated user impersonation** — not started. See the dedicated section above. Depends on server-only audit filtering (facility `audit.logs.read` callers must not see `AUTH_IMPERSONATION`) and Website Management as the log/UI surface; ships with retirement of `GET /auth/login/as/{cid}` / `DEV_LOGIN_AS_CID_ENABLED`.

**Remaining open work (not gaps I missed):** the Track A items — specs 009–011 and authenticated user impersonation (item 12). Track B's items (self-hosted FAA preferred-routes data, item 6; and GDPR data export, item 11) are **done** as of 2026-07-25 and signed off by review, pending the combined merge behind the gate. Every feature-parity item that was actually in scope, including the docs update, is now closed.

**Cutover-plumbing additions the website now needs from osmium** (surfaced by the website's authorization/identity migration — see the "Website cutover status" section above): (a) a training-session self-read path so a controller can read their own `/training/sessions/{id}`, and (b) admin edit-another-user's-profile endpoints. These are small and unblock the last ~9 website `getServerSession` sites; the rest of those sites are blocked on the deferred **Certifications & Progression** domain (the last Users/Roster parity tail — Progression needs osmium auto-advance logic that doesn't exist yet) and the deferred **ATC booking proxy**.

> **Note (supersedes the ordered list above for what's left).** Since that list
> was written, **Certifications & Progression** and the **ATC booking proxy**
> both landed (2026-07-24 / 2026-07-25), and the website's **EventStatistics**
> composite view has been migrated onto the new stats endpoints. The only work
> that remains open is: **specs 009–011**, **GDPR data export**, **impersonation**,
> and **FAA preferred routes**. The section below is the authoritative plan for
> executing that remainder with two parallel workers.

---

## Parallel execution plan — two workers + mandatory review gate

This section reorganizes **all remaining open osmium work** so two agents can run
concurrently without ever editing the same file, and requires an independent
**review worker** to sign off on each track before it merges. It is written to be
read alongside the website side of the same plan in
`website/docs/osmium-migration-plan.md` (§ "Parallel execution plan"), which
covers the front-end halves of these same two workers.

**Baseline assumption:** the in-flight **training-stats** work
(`src/handlers/stats.rs`, `src/repos/stats.rs`, `src/repos/training/stats.rs`,
`src/models/stats/mod.rs`, `Bruno/osmium/stats/`, `docs/api/stats.md`) is
**merged to `master` first**. Neither worker below touches the stats domain;
if training-stats is still open when these workers start, treat every
`stats`-domain file as owned by that third agent and off-limits.

### Track split (file-disjoint by design)

Remaining work divides into two clusters that share almost no source files. The
one cluster that *does* naturally collide — spec 009's handler cleanup and the
impersonation feature both edit `handlers/auth.rs` + `handlers/admin.rs` — is
kept **inside a single worker** so the collision never crosses a track boundary.

#### Worker A — "Auth, Sessions & Infra Hardening"

Owns, exclusively: `src/handlers/auth.rs`, `src/handlers/admin.rs`,
`src/handlers/dev.rs`, `src/handlers/emails.rs`, `src/handlers/health.rs`,
`src/auth/**` (middleware + `CurrentUser`), `src/repos/access.rs`,
`src/repos/audit.rs`, `src/config.rs`, and `Cargo.toml`.

1. **Spec 009 — final handler layering cleanup** *(do first — it rewrites the
   exact handlers impersonation then extends, so doing it first avoids migrating
   them twice)*. Move the direct `sqlx::query*` calls out of the 5 handlers
   above (verified counts: auth 3, admin 3, dev 13, emails 1, health 1) into the
   repo layer + `RequirePermission<P>`. See [009](009-final-handler-layering-cleanup.md).
2. **Spec 010 — IP rate limiting.** Add `tower_governor` (`Cargo.toml`), the
   middleware layer + wiring, and the permission-based bypass. See [010](010-ip-rate-limiting.md).
3. **Spec 011 — durable IP request tracking** *(after 010 — depends on its
   IP-extraction helper)*. New migration + middleware extension + one admin read
   endpoint on `handlers/admin.rs`. See [011](011-ip-request-tracking.md).
4. **Authenticated user impersonation** (the full feature spec'd above). Migration
   on `identity.sessions`, session helpers in `repos/access.rs`, auth-middleware /
   `CurrentUser` extension, start/stop handlers, `/me` `impersonating` field,
   audit filtering in `repos/audit.rs`, and **retirement of the dev
   `login_as_cid` / `DEV_LOGIN_AS_CID_ENABLED` path** (`config.rs`, `handlers/auth.rs`,
   `router.rs`). Follow the security checklist in the impersonation section above
   — that checklist is a hard gate for the review worker, not a suggestion.

#### Worker B — "New Data Domains"

Owns, exclusively: **new** files only — `src/handlers/data_export.rs`,
`src/handlers/routes.rs`, `src/repos/routes.rs`, `src/jobs/faa_preferred_routes.rs`
(names indicative) — plus **additive, read-only** functions in domain repos that
Worker A does *not* own (`repos/training/*`, `repos/events.rs`, `repos/feedback.rs`,
`repos/org/*`, etc.).

1. **GDPR self-service data export** (`GET /me/data-export`). New handler assembling
   one cross-domain JSON document. **Reuse existing repo reads** wherever possible
   — in particular read audit rows via the *existing* `audit_repo::fetch_audit_logs`
   rather than editing `repos/audit.rs` (Worker A owns that file). **Blocker to
   resolve before building:** the open compliance question on trainer-note
   redaction (Article 15(4)) — confirm with whoever owns vZDC compliance first; do
   not assume the redaction design from the original ask. Log the export request
   itself via the existing audit path.
2. **Self-hosted FAA preferred-routes data.** New `routes` domain: migration
   (reserved range below), `repos/routes.rs`, `handlers/routes.rs`
   (`GET /routes/preferred?origin=&destination=`), and a scheduled ingest job on
   the existing jobs framework (spec 007). **Blocker to resolve before building:**
   confirm the current FAA NFDC download URL + file format — the one part of this
   item not verified in-session; do not assume the old fixed-width shape is current.

### Shared-registry coordination protocol (the only files both workers touch)

Backend endpoint work unavoidably appends to a handful of central registries.
These stay append-only, each worker edits only its own domain's block, and the
**review worker resolves the (trivial, non-overlapping) append merges at
integration**. Both workers run on their own branch and never commit to the other's.

- **Migrations — reserved ranges (never reuse):** Worker A takes **0055–0059**,
  Worker B takes **0060–0064**. (Current max is `0054`.) This removes the only
  hard, non-mergeable collision — two files claiming the same number.
- **`src/router.rs`, `src/auth/permissions.rs`, `src/docs/openapi.rs`,
  `src/repos/mod.rs`, `src/handlers/mod.rs`, `src/models/mod.rs`** — append-only;
  add your domain's block, don't reorder or touch the other worker's lines.
- **`docs/route-permissions.md`, `docs/api/*.md`, `Bruno/osmium/**`** — disjoint by
  domain: each worker adds/edits only the files for its own new endpoints.

### Cross-worker dependency edges (none block *starting* in parallel)

- 011 → 010 (internal to Worker A).
- Impersonation → 009 landing first (internal to Worker A, hence same worker).
- **Neither worker depends on the other to begin.** The only cross-track edge is
  at the website layer: the front-end **NextAuth deletion** (Worker A's website
  half) cannot complete until every remaining website Prisma/next-auth consumer is
  gone (Worker B's website half + the deferred Mail domain) — see the website plan.
- **Deferred, in neither track:** the **Mail** domain (osmium custom-email-send
  endpoint + the website mail composer) remains a product-deferred feature. It
  blocks the *final* removal of the website's legacy Prisma logging, but nothing
  in Tracks A or B.

### Mandatory review-worker gate

**A worker's track is not "done" until a separate review worker has reviewed it.**
No track merges to `master` on the implementing agent's say-so.

- **When:** after each worker finishes its track's code (and again after any
  substantial follow-up), before merge.
- **A fresh agent** with no stake in the implementation — not the worker
  reviewing its own diff.
- **Scope, per track:**
  1. **Correctness + security.** For Worker A, the impersonation security checklist
     (audit forgery, facility-admin visibility leak, privilege elevation, orphaned
     sessions, side-effect channels, `/me` disclosure) is a line-by-line gate, and
     the dev `login_as_cid` path must be confirmed *gone*. For Worker B, the GDPR
     export's third-party-data-leakage pass (no leaking other data subjects' full
     profiles through joined records) and the compliance-question resolution.
  2. **Permission gating** — every new route has the right `RequirePermission<P>` /
     data-dependent auth; nothing new is accidentally public.
  3. **No cross-track collisions** — verify the two branches only meet on the
     append-only registries above, and integrate those append merges.
  4. **Migrations apply cleanly** in range order, and the **DB-backed integration
     tests** (`tests/routes_and_openapi.rs`, `tests/permission_gates.rs`) pass
     against a live Postgres — several completed items in this roadmap have *never*
     been run against a real DB in this environment, so the review worker is the
     first place that actually happens.
  5. **Docs/Bruno/OpenAPI** updated for every new endpoint (per the protocol above).
- **Only after review sign-off** does a track merge, and only once *both* tracks
  plus the website domain cleanup have merged does the final **NextAuth-deletion
  gate** open.

## Non-goals

- Re-proxying the removed third-party aviation-charts/PRD integration (`actions/charts.ts`, the old `actions/prd.ts`'s call to `api.aviationapi.com`) — the live site deleted this itself (`0855eb0`) and it's not a gap to restore as-is. A *self-hosted* FAA-sourced replacement for preferred routes is a new feature, tracked separately above ("New feature: self-hosted FAA preferred-routes data") — the two are not the same thing.
- Re-deriving the "Financial Committee" roster section — confirmed to be a pure frontend display grouping over existing staff-position data (`app/controllers/staff/page.tsx`), no new backend model.
- **Common mistakes** (`training.common_mistakes`, `training.training_ticket_common_mistakes`) — dropped from scope by product decision; not being ported. The tables remain in the schema (unused) but no handler/route work is planned for them.
- **Version / changelog** (`web.versions`, `web.version_change_details`) — dropped from scope by product decision; the live site stopped using this feature. The tables remain in the schema (unused) but no handler/route work is planned for them.
- **Airport / route-practice data** (`Airport`, `Runway`, `RunwayInstruction` models; `actions/airports.ts`, `app/airports/`, `app/routepractice/`) — dropped from scope by product decision; not being ported. Distinct from the *removed* aviation-charts/PRD integration (`actions/charts.ts`, `actions/prd.ts`) mentioned above — that one the live site deleted itself; this one is still live on the site but osmium isn't picking it up.
- **Promoting `GET /auth/login/as/{cid}` into production** — not a standalone parity item and not the intended impersonation design. Retiring that env-gated route is part of shipping authenticated user impersonation (item 12), not a separate backlog entry.
