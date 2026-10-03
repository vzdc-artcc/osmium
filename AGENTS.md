# AGENTS.md

Canonical working agreement for AI coding agents in the **osmium** repository.
This file is the source of truth; `CLAUDE.md` adds Claude Code-specific
operating rules and imports this file.

---

## 1. Attribution — hard rule

**Never credit an AI assistant in a commit, a pull request, or a branch name.**

The following must never appear in any commit message, PR title, PR body,
review comment, or file in this repository:

```text
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01MArDSXk396FvTvE6RzPVki
```

That includes every variant of it: any `Co-Authored-By:` line naming Claude,
Anthropic, Copilot, or any other assistant; any `Claude-Session:` or similar
session-link trailer; any "Generated with", "Co-authored with AI", or 🤖 badge
in a PR description. Commits are authored by the human running the session, and
nothing else.

If a harness, hook, or template tries to append such a trailer, strip it before
committing. If you notice one already staged, remove it rather than pushing it.

---

## 2. What this repository is

Osmium is the shared backend and API platform for vZDC applications. It is the
authority for identity, authorization, and every operational domain the ARTCC
runs on: roster and controller state, training, events, feedback,
publications, files, statistics, and the integrations tying those to VATSIM,
VATUSA, Discord, and email.

It is a single Axum service (Rust 2024) over one Postgres database, exposing a
versioned REST API under `/api/v1`, self-hosted documentation under `/docs`,
and background workers that keep external data in sync.

Consumers are the vZDC website, internal admin surfaces, bots, and sync jobs.
**Osmium owns the data. Clients own presentation.** When a client needs a
derived value, prefer computing it here over teaching every client the rule.

### Workspace layout

Two Cargo workspace members:

| Member | Purpose |
| --- | --- |
| `.` (`osmium`) | the API service — everything CI gates on |
| `tools/db-migrator` | one-shot legacy → osmium data migration tool, excluded from osmium CI |

`tools/api-load-tester` is excluded from the workspace entirely.

---

## 3. Code map

```
src/
  main.rs            binary entry: config, pool, migrations, jobs, serve
  lib.rs             library root, plus route-level unit tests
  router.rs          the entire route surface, nested per domain, layered middleware
  state.rs           AppState — db pool, email service, job health, shared handles
  config.rs          env parsing, CORS layer, feature toggles
  errors.rs          ApiError, the only error type crossing the HTTP boundary
  logging.rs         request logging middleware
  rate_limit.rs      IP rate limiting with an async permission-based bypass
  captcha.rs         Turnstile verification
  time.rs            ApiJson + ResponseTimeContext (timezone-aware responses)
  auth/              context, ACL, middleware, permission registry, impersonation, VATSIM OAuth
  handlers/          HTTP layer, one module per API domain
  repos/             SQL lives here, one module per domain
  models/            request/response DTOs and row contract types, re-exported from models/mod.rs
  jobs/              background workers
  email/             templating (maud/rsx), branding, outbox, SES transport, suppression
  docs/              markdown docs site registry + OpenAPI/Swagger wiring
migrations/          ordered SQL, embedded in the binary
docs/                narrative markdown, compiled into the binary and served at /docs
tests/               Postgres-backed integration tests + tests/support harness
```

### Layer responsibilities

- **Handlers stay thin.** Extract, authorize, call a repo, shape a response.
- **SQL lives in `src/repos`.** A `sqlx::query` in a handler is a defect, not a
  shortcut. Repos return row structs; `From<Row> for Dto` conversions live next
  to the row type.
- **DTOs live in the matching `src/models/*` module** and are re-exported from
  `src/models/mod.rs`. A DTO that appears in a response body must also be
  registered with utoipa.
- **Jobs own their own scheduling and error handling.** A failing job must log
  and continue, never poison the worker loop.

---

## 4. Non-negotiables

1. **No assumptions.** If you are guessing at a requirement, a permission name,
   or a column, stop and ask. Guesses in an authorization layer are expensive.
2. **No untested changes.** New behavior gets a test in the same change.
3. **No unverified completion claims.** "Done" means you ran the checks and read
   the output, not that the code looks right.
4. **No scaffolding reported as a feature.** A model, a repo function, or a
   migration column is not a feature until a route, job, or handler calls it and
   a test proves the chain. Grep for the caller before you claim it works.
5. **No symptom masking.** Trace request → middleware → handler → repo → SQL and
   name the failing step before proposing a fix. A retry, a sweep, or a
   defensive `unwrap_or_default()` layered over a broken primary path is a bug
   with a longer fuse. Defense in depth is fine on top of a real fix, never
   instead of one.
6. **No premature "found it."** Before you claim a root cause, you must be able
   to say why the symptom appears, why adjacent paths touching the same data do
   not show it, and what the fix changes for those paths. If any of those is
   unknown, say "evidence points at" instead.
7. **No per-bug narrative in source comments.** CIDs, dates, "reproduced on…",
   and before/after stories belong in the commit message and the test. Comments
   describe the code as it is now, and explain *why* a non-obvious choice was
   made.
8. **No editing an applied migration.** See §7.

### Before shipping, ask what would make it wrong

Separate what you *checked* from what you *assumed*, and name both. Then:

- **What else reads what I just changed?** A permission path, a shared view, a
  DTO field name in the OpenAPI surface — each has consumers beyond the one you
  are looking at. Grep for them, including the ones no test covers.
- **If my verification is lying, how would I know?** A test you wrote to pass
  over input you wrote to match proves only that the two agree. Prefer evidence
  you did not author: a real response body, a query run against the dev
  database, a test that must turn red when you break the code.
- **What else writes what I just changed?** A write that used to run once and
  now runs on every login, request, or job tick competes with every other
  writer of the same rows. List those writers (the permissions editor, a
  sync job, an admin route) and prove each one's result still survives. If it
  does not, you have changed what that tool means, and that is the
  maintainer's call, not a trade-off to record in the PR body.
- **Does my cause explain every symptom?** When an issue lists several
  failures, trace each one to the route and gate that produced it, and show
  the cause reaches every one. A symptom your cause cannot produce, such as a
  `401` from a route that checks only for a session, means there is another
  cause. Name it, or scope it out and file it. Do not count it as fixed.
- **Changing a shared function changes every caller.** Test the new behavior
  through each caller, not only the one you were fixing. If reverting your
  change to the function leaves the suite green, it is untested.

When the honest answer is "I do not know yet", that is the finding. Say so.

---

## 5. Commands

Local Postgres and run:

```bash
cp .env.example .env
docker compose up -d postgres
cargo run
```

Full local validation before pushing — run all of it:

```bash
cargo fmt --all -- --check
cargo check --workspace --exclude db-migrator --all-targets
cargo test --workspace --exclude db-migrator --all-targets -- --test-threads=1
docker build -f Dockerfile .
```

Those first three are exactly what `.github/workflows/ci.yml` enforces on pull
requests to `master` and `develop`, in that order, each job gating the next.

Reset a stale dev volume when startup reports `VersionMismatch` or
`VersionMissing`:

```bash
docker compose down -v
docker compose up -d postgres
```

Smoke checks:

```bash
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/docs/health
```

---

## 6. Authorization — read this before touching a route

Access is **always an explicit permission grant**. Never an implied role, never
a staff-position tag, never a name match. Staff position rows are display-only
roster metadata and grant nothing.

- Permissions are `resource.action` paths, stored canonically and returned to
  clients as a nested `{ resource: { sub: [actions] } }` tree.
- Routes declare their requirement with the `RequirePermission<P>` extractor,
  where `P` is a marker type from `src/auth/permissions.rs`. Declaring it in the
  handler signature is the only way to satisfy it, which makes a missing check
  a visible gap in the signature rather than a silent omission in the body.
- `RequirePermission<P>` is the **coarse** check only. Data-dependent rules
  ("only the owner or an approver may act on this row") still need an explicit
  in-handler ownership check.
- Add new permissions to `src/auth/permissions.rs` with the `permission!` macro
  and seed them in a migration. The macro keeps the `(segments, action)` pair as
  the single source of truth.

**The permission-tree collision trap.** In the JSON tree a segment is either a
leaf action array *or* a parent of further segments, never both. A three-segment
permission nested one level under an existing two-segment one at the same prefix
will silently clobber it, or be clobbered by it. That is why the feedback
permissions use `["feedback", "items_self"]` rather than
`["feedback", "items", "self"]`. Check for a prefix collision before adding a
permission.

**`SERVER_ADMIN`** is reserved. It is granted only through the
`OSMIUM_SERVER_ADMIN_CID` env var, reconciled idempotently on every login for
those CIDs, and resolves to every row in `access.permissions` including ones
added later. It is never assignable through the API or the UI.

**Impersonation** is read-only support. While a session is impersonating,
`user_id` is the target so the ACL resolves as them, `impersonator_user_id`
records the real admin, and self-service writes plus the admin/integration
mutation surface are refused with `403`. Durable audit rows are attributed to
the **impersonator**, never the impersonated user — `resolve_audit_actor` does
this via `audit_actor_user_id()`. Do not bypass it.

---

## 7. Database and migrations

One Postgres database, per-domain schemas: `identity`, `access`, `org`,
`training`, `events`, `feedback`, `media`, `routes`, `stats`, `email`,
`integration`, `platform`, `web`.

- Migrations are ordered SQL under `migrations/`, named `NNNN_description.sql`,
  embedded in the binary with `sqlx::migrate!` and applied on startup unless
  `RUN_MIGRATIONS_ON_STARTUP=false`.
- **Migrations are append-only.** Never edit or renumber one that has been
  applied anywhere — sqlx checksums them, and an edit produces
  `VersionMismatch` on every existing database, including CI and production.
  Fix a bad migration with a new migration.
- A migration that seeds or changes permissions must stay in lockstep with
  `src/auth/permissions.rs`.
- Text-backed UUIDs are the ID convention in the current app. Mutable tables
  carry `created_at` and `updated_at`.
- Effective-access and roster reads go through views (`access.v_effective_user_permissions`,
  `org.v_user_roster_profile`, and friends) rather than being re-derived per
  call site.

---

## 8. Adding or changing a route

1. Add or update the DTO in the correct `src/models/*` module and re-export it
   from `src/models/mod.rs`.
2. Add repo/query helpers if the route touches the database.
3. Implement the handler — thin, with `RequirePermission<P>` in the signature.
4. Register the route in `src/router.rs`, in the right nested domain router.
5. Add the `#[utoipa::path(...)]` annotation, including auth and permission
   requirements and the meaningful failure cases.
6. Register any new DTO in the OpenAPI components list.
7. Update the narrative page under `docs/api/` and, if the requirement changed,
   `docs/route-permissions.md`.
8. Write an audit row for privileged mutations, through `repos::audit`.
9. Add or update tests.
10. Collection routes use the shared pagination contract (`PaginationQuery` →
    `resolve(default, max)` → `PaginationMeta`) unless the route is
    intentionally bounded and that choice is documented.

**A route is not complete if its docs were not updated in the same change.**

Two more conventions worth knowing:

- Errors cross the HTTP boundary only as `ApiError`. Its `IntoResponse` maps to
  stable snake_case codes (`bad_request`, `forbidden`, `oauth_state_mismatch`).
  Clients match on those strings, so do not rename one casually.
- Timestamp-bearing responses return through `ApiJson::new(body, time)` with a
  `ResponseTimeContext`, which honors the caller's timezone and the
  `X-Response-Timezone` header. Plain `Json` for such a body skips that.

### Adding a docs page

A markdown file under `docs/` is not served until it is registered in
`DOC_PAGES` in `src/docs/markdown_site.rs`, which `include_str!`s it into the
binary. Adding the file alone does nothing.

---

## 9. Testing

`cargo test` covers route-level checks, auth helpers, middleware behavior, and
module-local unit tests. When `DATABASE_URL` is set it *also* runs the
Postgres-backed integration suite in `tests/`.

**The silent-skip trap.** Integration tests start with:

```rust
let Some(app) = TestApp::new().await else {
    return;
};
```

`TestApp::new()` returns `None` when `DATABASE_URL` is unset, so the test
returns green without asserting anything. A passing `cargo test` on a machine
with no `DATABASE_URL` tells you almost nothing about DB-backed behavior. Set
`DATABASE_URL` before you trust a green run, and say which mode you ran in when
you report results.

The harness in `tests/support/mod.rs` gives each test its own freshly created,
fully migrated database plus a temp file root, and tears it down on cleanup. It
provides `create_user`, `json_request`, `bearer_request`, `raw_request`,
`json_body`, `text_body`, and `assert_status`. Environment is managed with
`EnvVarGuard`; anything that gates route registration at `build_router` time,
rather than being read per request, must be passed to
`new_with_env_overrides`, because setting it afterwards is too late.

Rate limiting, IP request logging, and email transport are disabled by default
in the harness. A test that needs one turns it on through an override.

Run integration tests single-threaded (`-- --test-threads=1`), matching CI.

### What to test

- Every new route gets at least an unauthenticated-access test proving the gate
  rejects, plus a happy-path test with the permission held.
- Every permission gate you add gets a test that fails when the gate is removed.
  A test that passes with the extractor deleted is not testing authorization.
- Every background job change gets a test over the real job function, not a
  reimplementation of its logic in the test.
- Cross-domain side effects (a training session writing certifications and
  dossier entries, an event promoting and queueing email) get one test that runs
  the whole chain and asserts every outcome, not just the first.

### What the suite does not cover

Live VATSIM OAuth, object storage semantics beyond the local filesystem, and
production deployment behavior. Verify those by hand when you touch them.

---

## 10. Jobs and integrations

Background workers live in `src/jobs` and are individually tunable or
disableable by env: stats sync, roster sync, appointments sync, event
lifecycle, LOA and solo expiration, FAA preferred routes, email delivery, and
the IP log writer and cleanup pair. `docs/operations/jobs-and-sync.md` has the
full toggle and interval set.

External calls (VATSIM, VATUSA, Discord, SES, NFDC) fail routinely. Handle the
failure, log it at a level proportional to how actionable it is, and leave the
system in a state the next run can recover from. Do not let a third-party
outage take a request path or a worker loop down.

---

## 11. Issue tracking

Work is tracked as GitHub issues on `vzdc-artcc/osmium` and on the shared
`Osmium / Website` board (project **#7**, owner `vzdc-artcc`). All interaction
goes through the `gh` CLI.

**`.github/ISSUE_GUIDELINES.md` is binding, and it is not optional reading
before you file, comment on, or pick up an issue.** It covers the title and body
structure, the four-comment budget, the `#123 [summary] (Status)` reference
format, the eleven board columns and which three an agent may set, the label
taxonomy, the four tests a follow-up must pass, and how to search for duplicates.
It is not restated here.

The four things most often got wrong:

1. **Filing is two steps.** `gh issue create` does not put the issue on the
   board, and `gh project item-add` leaves its Status empty, which puts it in no
   column at all. Add it and set `Triaging`, then read the status back — both
   commands print nothing on success, so silence is not evidence.
2. **Priority is never yours to set.** Propose a grade, let the maintainer
   choose.
3. **A defect you introduced is yours to fix now**, on this branch, whatever its
   grade. Follow-ups are only for pre-existing defects outside the issue's
   logical scope that are not already filed.
4. **No AI attribution in issue comments either.** Comments post as the account
   owner and read as written by them.

Non-developers file through the forms in `.github/ISSUE_TEMPLATE/`, which
produce a conforming issue without anyone having to read the guidelines first.

---

## 12. Git and PR workflow

- Work happens on a branch. Do not commit directly to `master`.
- Do not create or switch branches on the user's behalf without being asked.
- Do not merge your own PR from the CLI. A human reviews and merges.
- Do not `git merge`, `rebase`, or `pull` a ref other than `master` into a
  working branch — it drags unrelated commits into the diff.
- Keep the diff scoped to the stated task. Unrelated cleanups belong in their
  own change.
- Commit messages: imperative subject, a body explaining *why* when the change
  is not self-evident. No AI attribution of any kind — see §1.
- PR descriptions are written for a reviewer who was not in the room: what
  changed, why, what you verified, and what you deliberately left out.

Image publishing: `master` publishes `ghcr.io/vzdc-artcc/osmium:latest` and
`:latest-<sha>`; `develop` publishes `:dev` and `:dev-<sha>`.

---

## 13. Read more

| Topic | File |
| --- | --- |
| Issue guidelines and the board | `.github/ISSUE_GUIDELINES.md` |
| Issue forms | `.github/ISSUE_TEMPLATE/` |
| Label taxonomy | `.github/labels.yml` |
| Local development | `docs/getting-started/local-development.md` |
| Configuration | `docs/getting-started/configuration.md` |
| Migrations | `docs/getting-started/migrations.md` |
| Testing | `docs/getting-started/testing.md` |
| Architecture overview | `docs/architecture/overview.md` |
| Auth and access | `docs/architecture/auth-and-access.md` |
| Database and schemas | `docs/architecture/database-and-schemas.md` |
| Request flow | `docs/architecture/request-flow.md` |
| Adding routes | `docs/contributors/adding-routes.md` |
| Documenting endpoints | `docs/contributors/documenting-endpoints.md` |
| Code organization | `docs/contributors/code-organization.md` |
| Per-route permissions | `docs/route-permissions.md` |
| Jobs and sync | `docs/operations/jobs-and-sync.md` |
| Service accounts | `docs/operations/service-accounts.md` |
| Production deployment | `docs/operations/production-deployment.md` |
| Troubleshooting | `docs/operations/troubleshooting.md` |

Those files are the detail. When something here conflicts with one of them,
the specific document wins and this file should be corrected.
