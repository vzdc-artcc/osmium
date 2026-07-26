# 014 — Legacy Data Migration Tool

## Status note (read first)

A working implementation of this tool **already exists** at `tools/db-migrator/`
(~6,850 lines, a separate crate, not part of the osmium binary). Seven of the
eight domains are implemented, and there is a Docker-based test harness
(`scripts/migration-test/`, `docker-compose.migration-test.yml`) plus a
production cutover path (`scripts/prod/`, `docs/operations/production-deployment.md`).

This spec therefore serves two purposes:

1. **Design of record** — capture *why* the tool is shaped the way it is, since
   it was built without an accompanying spec (001–013 exist; the migrator has none).
2. **Gap roadmap** — enumerate what is deliberately out of scope, what is
   missing, and what must be true before a real production cutover.

Every table/file/entity-type name below was verified by grep against the current
`tools/db-migrator/src/` and `migrations/*.sql`, not inferred from memory.

---

## Problem

Osmium is the replacement backend for the live vZDC website
(`~/Programing/website`), a Next.js app whose Prisma/Postgres database (`public`
schema, ~70 `model` blocks) *is* production today. Osmium's schema is a
domain-partitioned redesign (`identity`, `access`, `org`, `training`, `events`,
`feedback`, `stats`, `media`, `web`, `integration` schemas across
`migrations/0001`–`0062`) with different table names, different primary-key
types (legacy `cuid` `text` ids → osmium `uuid` / natural keys like `cid`),
different enum encodings (legacy string enums → osmium numeric ratings, etc.),
and normalized many-to-many join tables where Prisma used implicit relations.

There is no path today to move real production data from the legacy schema into
osmium's. Without one, cutover means either starting from an empty roster or
hand-writing throwaway SQL — both unacceptable for a live ARTCC with years of
controller hours, training history, and feedback.

## Goal

A standalone, **idempotent, resumable, verifiable** batch tool that reads a
legacy Prisma/`public` Postgres database and writes the equivalent rows into a
fresh osmium-schema Postgres database, preserving referential integrity across
the id-type change, with a dry-run/plan mode, a verification pass, and a
machine-readable report — safe to run repeatedly against the same target without
duplicating or corrupting data.

Non-goal: a live/streaming/CDC sync. This is a one-shot cutover tool (run
repeatedly during rehearsal, once for real), not a steady-state replicator.

## Architecture (as built / of record)

Separate crate at `tools/db-migrator/` so it never links into the API binary and
can depend on `sqlx` against *two* pools at once.

- **Entry / CLI** (`src/main.rs`, `src/cli.rs`, `src/config.rs`): two connection
  URLs (`--source-url`/`SOURCE_DATABASE_URL`, `--target-url`/`TARGET_DATABASE_URL`),
  a `--domain` selector (`all` or one of the eight), `--dry-run`, `--resume`
  (default on), `--strict`, `--abort-on-warning`, `--json`, and four subcommands:
  `migrate`, `plan` (= migrate with `dry_run` forced on), `verify`, `reset-run`.
- **Run state** (`src/target/mod.rs`): a dedicated `migrator` schema in the
  *target* DB, created on demand, holding:
  - `migration_runs` — one row per run (`run_id`, status, dry-run flag, timestamps).
  - `migration_entity_map` — **the heart of the tool**: `unique(entity_type,
    source_id)` maps each legacy `cuid` to its new target id, with a
    `checksum` of the written payload and a business key on each side. This is
    both the id-translation table (so child rows can resolve parent fks across
    the id-type change) *and* the idempotency ledger (re-running finds the
    existing mapping instead of re-inserting).
  - `migration_warnings` — non-fatal per-row issues (unmapped enum, dangling fk,
    duplicate natural key).
  - `migration_checkpoints` — per-domain completion markers enabling `--resume`.
- **Domains** (`src/domains/`): one module per domain, run in a fixed
  dependency order — `reference → users → org → training → feedback → events →
  stats` (see `domains/mod.rs`). Order matters because later domains resolve
  fks through mappings created by earlier ones (e.g. every `user_id` resolves
  through the `users` domain's entity map). `web` is registered but
  `bail!`s as "intentionally not implemented in v1".
- **Mapping / normalization** (`src/mapping/mod.rs`): pure functions translating
  legacy encodings — string VATSIM ratings ↔ numeric (`OBS`→1 … `ADM`→12),
  `ControllerStatus`, `Role`, `StaffPosition`, `EventType` (incl. legacy
  `STANDARD`→`HOME`), `TmiCategory` (incl. `APP`→`TERMINAL`, `CTR`→`ENROUTE`),
  and `CertificationOption`. Unmapped values become warnings, not silent drops.
  Unit-tested.
- **Verify** (`src/verify.rs`): three passes — row-count parity per table
  (`select count(*)` legacy vs. osmium), referential-integrity checks, and
  semantic checks (e.g. legacy `ControllerLogMonth` hours vs. osmium
  `stats.controller_monthly_rollups` seconds).
- **Report** (`src/report.rs`): per-domain planned/created/updated/skipped/
  warnings/errors counts, printed as a table or `--json`.
- **Test harness** (`scripts/migration-test/`): destructive local stack — mock
  legacy Postgres (from a `dev-data/mock-prod/prod.sql` dump) + fresh osmium
  Postgres + the API — with a `migrator.sh plan|migrate|verify|reset-run`
  wrapper. Production path is `scripts/prod/` + the prod compose file.

### Invariants the design guarantees

1. **Idempotent**: `upsert_mapping` keys on `(entity_type, source_id)`; a second
   run resolves the existing target id instead of inserting a duplicate.
2. **Resumable**: `--resume` (default) skips domains already checkpointed; a
   crash mid-run is restartable without `reset-run`.
3. **Fk-safe across id remap**: children resolve parents via `find_mapping`; a
   parent that failed to migrate produces a warning + skip, never a dangling fk.
4. **Dry-run truth**: `plan` writes nothing to target data tables and nothing to
   the `migrator` bookkeeping tables (guarded in `main.rs`).
5. **Non-destructive to target user data**: the tool only creates the `migrator`
   schema itself; it never drops osmium tables.

## Deliberate exclusions (v1)

These have osmium homes or none, and were intentionally left out — a real
cutover must consciously accept each:

- **Auth artifacts** — legacy `Account`, `Session`, `VerificationToken`,
  `DiscordOauthState`. Osmium owns its own auth; these are transient NextAuth
  state and are correctly dropped.
- **Most of the `web` domain** — resolved 2026-07-26 (see "Gaps to close" #2).
  Only `WelcomeMessages` is migrated. `File`/`FileCategory` (manual, per user),
  `ChangeBroadcast`, `StaffingRequest`, `StatisticsPrefixes`,
  `Version`/`ChangeDetail`, Discord config, and the airports/route-practice
  cluster are all deliberately dropped by user decision.
- **Stats depth** — only `stats.controller_monthly_rollups` (`environment =
  'live'`) is backfilled from `ControllerLogMonth`. Legacy has no session-grain
  data, so `stats.controller_sessions`/`controller_activations` stay empty and
  `last_activity_at` may be `null` for legacy-only controllers. This is
  accepted, not a bug, but the API surface should tolerate it.
- **`VatsimUpdateMetadata`** — operational sync timestamp, regenerated by
  osmium's own jobs; not migrated.

## Gaps to close (remaining work)

Ordered by cutover-blocking severity:

1. **Additional trainers not migrated. ✅ done (2026-07-26).**
   `training_session_additional_trainers` and
   `training_appointment_additional_trainers` (schema `migrations/0033`, the live
   site's most recent training feature per spec 012 §1.1) had **no insert** in
   `src/domains/training.rs`. Now migrated: two new `Source*AdditionalTrainer`
   structs, fetched from `public."TrainingSessionAdditionalTrainer"` /
   `public."TrainingAppointmentAdditionalTrainer"`, resolved through the existing
   `training_session`/`training_appointment` + `user` entity maps and upserted
   into the join tables (`on conflict (…, trainer_id) do update set description`),
   with dangling-fk warnings + skip matching the `training_appointment_lessons`
   pattern. Also fixed a sibling `0033` gap in the same pass: the appointment
   `notes` column (added by `0033`, `default ''`) was never migrated — the
   `SourceAppointment` select/insert now carries it. Two row-count parity checks
   added to `verify.rs`. `cargo check` clean.
2. **`web` domain decision. ✅ resolved (2026-07-26).** Scope confirmed with the
   user: the four things that matter — **stats, controller info (incl. feedback),
   training, events** — are already covered by the existing seven domains, so the
   `web` bucket only needed **welcome messages**. Implemented `domains/web.rs`
   (`migrate_welcome_messages`): reads the single legacy `WelcomeMessages` row and
   updates the seeded `web.site_settings` `welcome_messages` jsonb
   (`{"homeText","visitorText"}`); warns + no-ops if legacy has no row. `bail!` in
   `domains/mod.rs` removed; `Web` added to the default `--domain all` run
   (`config.rs`) and runs last.
   - **Event banners removed.** Per the user, banners are deleted post-event, so
     the events domain no longer migrates legacy `bannerKey` → `banner_asset_id`
     (it was writing a raw UploadThing key into a column with no matching
     `media.file_assets` row anyway). `banner_asset_id` now always migrates null.
   - **Deliberately dropped** (user decision — not gaps): `File`/`FileCategory`
     (user migrates files manually; blobs live on UploadThing, out of a DB
     migrator's scope), `ChangeBroadcast`, `DiscordConfig` cluster,
     `StaffingRequest`, `StatisticsPrefixes` (seeded default stands),
     `Version`/`ChangeDetail`, `SyncTimes`, `VatsimUpdateMetadata`, the
     airports/route-practice cluster, and the auth artifacts.
   - **No legacy source** (osmium-native, nothing to migrate): `web.pages`,
     `web.announcements`, `web.publications`.
3. **Verify coverage audit.** Confirm `verify.rs` count checks exist for every
   migrated entity (it currently covers users/org/training/feedback; audit
   events + the join tables). A migrated table with no verify check can silently
   under-migrate.
4. **`--strict` / `--abort-on-warning` semantics.** These flags are parsed and
   threaded into `Config`; confirm they actually gate behavior (fail the run vs.
   log) end-to-end, and document the intended contract.
5. **Cutover runbook completeness.** `docs/operations/production-deployment.md`
   and `scripts/prod/cutover.sh` should encode: take legacy read-only → dump →
   `plan` → review report/warnings → `migrate` → `verify` → seed osmium-only
   prerequisites (notably `access.user_roles` must be populated before the app
   is usable — see the Role[] redesign) → smoke test → flip DNS.

## Affected files / patterns

- Existing: everything under `tools/db-migrator/src/` (extend, don't rewrite).
- New submigration for gap #1: additions to `src/domains/training.rs` + verify
  checks in `src/verify.rs`.
- New for gap #2: `src/domains/web.rs`, wired in `domains/mod.rs` + `config.rs` +
  `cli.rs`'s `DomainArg`.
- Reference pattern for any new domain: `src/domains/feedback.rs` (small,
  self-contained: fetch source rows → normalize → resolve fks via `find_mapping`
  → insert → `upsert_mapping` → count in report).

## Verification

Per gap closed:

1. `cargo check` in `tools/db-migrator/`, and `cargo test` (mapping unit tests).
2. Against the local migration-test stack with a representative
   `dev-data/mock-prod/prod.sql` dump:
   - `migrator.sh plan` — no writes, report shows expected `planned` counts.
   - `migrator.sh migrate` then **`migrate` again** — second run shows
     `created=0, updated≈N` (idempotency proof), zero new duplicate rows.
   - `migrator.sh verify` — all count/integrity/semantic checks pass (or every
     mismatch is a documented, accepted exclusion).
   - Spot-check a controller with training history end-to-end via the osmium API
     against the migrated target DB.
3. For additional-trainers specifically: pick a source session with a known
   additional trainer, migrate, and confirm the row lands in
   `training.training_session_additional_trainers` with the trainer's remapped id.

## Open questions for the user

Both prior open questions are resolved (2026-07-26): the `web` scope is welcome
messages only, and files are migrated manually by the user (blobs on UploadThing,
out of a DB migrator's scope). No open product questions remain — the outstanding
work is the mechanical/operational items in "Gaps to close" (#3–#5).
