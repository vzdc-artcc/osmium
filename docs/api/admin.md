# Admin API

## Purpose

Administrative access and roster-control operations.

## Response Timezones

Timestamped admin responses follow the shared response-timezone contract via `X-Response-Timezone`.

- default: authenticated user's stored timezone
- fallback: UTC/Zulu when no authenticated human timezone exists
- audit `before_state` and `after_state` snapshots remain stored JSON and are not rewritten

## Main Routes

- `GET /api/v1/admin/acl`
- `GET /api/v1/admin/access/catalog`
- `GET /api/v1/admin/visitor-applications`
- `PATCH /api/v1/admin/visitor-applications/{application_id}`
- `GET /api/v1/admin/jobs`
- `GET /api/v1/admin/jobs/{job_name}`
- `POST /api/v1/admin/jobs/{job_name}/run`
- `GET /api/v1/admin/loa`
- `PATCH /api/v1/admin/loa/{loa_id}/decision`
- `POST /api/v1/admin/loa/expire-run`
- `GET /api/v1/admin/solo-certifications`
- `POST /api/v1/admin/solo-certifications`
- `PATCH /api/v1/admin/solo-certifications/{solo_id}`
- `DELETE /api/v1/admin/solo-certifications/{solo_id}`
- `GET /api/v1/admin/staffing-requests`
- `DELETE /api/v1/admin/staffing-requests/{request_id}`
- `GET /api/v1/admin/sua`
- `GET /api/v1/admin/incidents`
- `GET /api/v1/admin/incidents/{incident_id}`
- `PATCH /api/v1/admin/incidents/{incident_id}`
- `GET /api/v1/admin/training/progressions`
- `POST /api/v1/admin/training/progressions`
- `PATCH /api/v1/admin/training/progressions/{progression_id}`
- `DELETE /api/v1/admin/training/progressions/{progression_id}`
- `GET /api/v1/admin/training/progression-steps`
- `POST /api/v1/admin/training/progression-steps`
- `PATCH /api/v1/admin/training/progression-steps/{step_id}`
- `DELETE /api/v1/admin/training/progression-steps/{step_id}`
- `GET /api/v1/admin/training/performance-indicators/templates`
- `POST /api/v1/admin/training/performance-indicators/templates`
- `PATCH /api/v1/admin/training/performance-indicators/templates/{template_id}`
- `DELETE /api/v1/admin/training/performance-indicators/templates/{template_id}`
- `GET /api/v1/admin/training/performance-indicators/categories`
- `POST /api/v1/admin/training/performance-indicators/categories`
- `PATCH /api/v1/admin/training/performance-indicators/categories/{category_id}`
- `DELETE /api/v1/admin/training/performance-indicators/categories/{category_id}`
- `GET /api/v1/admin/training/performance-indicators/criteria`
- `POST /api/v1/admin/training/performance-indicators/criteria`
- `PATCH /api/v1/admin/training/performance-indicators/criteria/{criteria_id}`
- `DELETE /api/v1/admin/training/performance-indicators/criteria/{criteria_id}`
- `GET /api/v1/admin/training/progression-assignments`
- `POST /api/v1/admin/training/progression-assignments`
- `DELETE /api/v1/admin/training/progression-assignments/{user_id}`
- `GET /api/v1/admin/integrations/discord/configs`
- `POST /api/v1/admin/integrations/discord/configs`
- `PATCH /api/v1/admin/integrations/discord/configs/{config_id}`
- `POST /api/v1/admin/integrations/discord/channels`
- `PATCH /api/v1/admin/integrations/discord/channels/{channel_id}`
- `DELETE /api/v1/admin/integrations/discord/channels/{channel_id}`
- `POST /api/v1/admin/integrations/discord/roles`
- `PATCH /api/v1/admin/integrations/discord/roles/{role_id}`
- `DELETE /api/v1/admin/integrations/discord/roles/{role_id}`
- `POST /api/v1/admin/integrations/discord/categories`
- `PATCH /api/v1/admin/integrations/discord/categories/{category_id}`
- `DELETE /api/v1/admin/integrations/discord/categories/{category_id}`
- `GET /api/v1/admin/integrations/outbound-jobs`
- `POST /api/v1/admin/integrations/outbound-jobs/run`
- `POST /api/v1/admin/notifications/announcements`
- `GET /api/v1/admin/users/{cid}/access`
- `POST /api/v1/admin/users/{cid}/access`
- `PATCH /api/v1/admin/users/{cid}/controller-status`
- `PATCH /api/v1/admin/users/{cid}/controller-lifecycle`
- `GET /api/v1/admin/users/{cid}/flags`
- `PATCH /api/v1/admin/users/{cid}/flags`
- `PATCH /api/v1/admin/users/{cid}/operating-initials`
- `POST /api/v1/admin/users/{cid}/refresh-vatusa`
- `GET /api/v1/admin/users/{cid}/ip-history`
- `GET /api/v1/admin/users/{cid}/sessions`
- `DELETE /api/v1/admin/users/{cid}/sessions/{session_id}`
- `DELETE /api/v1/admin/users/{cid}/sessions`
- `GET /api/v1/admin/data-export/roster`
- `POST /api/v1/admin/users/{cid}/staff-positions/{position}`
- `DELETE /api/v1/admin/users/{cid}/staff-positions/{position}`
- `GET /api/v1/admin/publications`
- `GET /api/v1/admin/publications/{publication_id}`
- `POST /api/v1/admin/publications`
- `PATCH /api/v1/admin/publications/{publication_id}`
- `DELETE /api/v1/admin/publications/{publication_id}`
- `GET /api/v1/admin/publications/categories`
- `POST /api/v1/admin/publications/categories`
- `PATCH /api/v1/admin/publications/categories/{category_id}`
- `DELETE /api/v1/admin/publications/categories/{category_id}`

## Permissions

Most admin routes on this page currently require `users.update`.

Publication and publication-category management requires `web.update`.

Manual VATUSA refresh requires `users.vatusa_refresh.request`.

Integration and outbound-job operations use the existing integrations management permission path.

Assigning or revoking a staff position tag requires `users.staff_positions.update`.

Reading another user's self-service opt-out flags requires `users.flags.read`; updating them requires `users.flags.update` and a non-empty `reason` (recorded as a dossier entry, matching the access-editing endpoint's audit pattern). Reassigning a controller's operating initials requires `users.operating_initials.update`. All three are granted to the `STAFF` role.

## Workflow Notes

- admin list routes that can grow large now use the shared pagination envelope
- `GET /api/v1/admin/jobs` and `GET /api/v1/admin/jobs/{job_name}` expose persisted job-run state for backend automations
- `PATCH /api/v1/admin/users/{cid}/controller-lifecycle` is the backend-owned controller transition and cleanup endpoint
- `identity.user_flags` (no-request-LOAs/no-event-signup/no-edit-profile/no-request-training-assignments/no-request-trainer-release/no-force-progression-finish/excluded-from-roster-sync/hidden-from-roster) backs `GET`/`PATCH .../flags`; a row is created on first write, defaults to all-`false` if none exists yet
- `PATCH .../operating-initials` and the self-service `operating_initials` field on `PATCH /api/v1/me` share the same manual-reassignment repo function (`reassign_operating_initials`) — distinct from the deterministic, collision-retried auto-generation that runs once at first login (`ensure_operating_initials`)
- LOA, solo-certification, staffing-request, SUA, visitor-application, audit, admin-user, and outbound-job admin list routes all support backend-native pagination fields instead of website grid semantics
- training admin routes are grouped under `/api/v1/admin/training/*` and use read or update variants of the training lesson permission path
- staff position tags (`ATM`/`DATM`/`TA`/`EC`/`WM`/`FE`/`AEC`/`AWM`/`AFE`/`EP`/`TMU`/`FC`/`INS`/`MTR`) are purely display-only — they never grant permissions, unlike `access.user_roles`. Roster sync auto-assigns the 8 VATUSA-recognized codes (everything except `AEC`/`AWM`/`AFE`/`EP`/`TMU`/`FC`, which have no VATUSA equivalent and are always manual); a manual assign/revoke via this endpoint marks the row `source = 'manual'` and roster sync will never overwrite it again until a human changes it. Read via `GET /api/v1/users/{cid}/staff-positions` (public, same policy as `GET /api/v1/users/{cid}`).

## Permission Payloads

- access responses return grouped permissions such as `{ "users": ["read", "update"] }`
- `POST /api/v1/admin/users/{cid}/access` accepts grouped `permissions` and grouped `permission_overrides`
- `SERVER_ADMIN` is reserved for env-driven bootstrap and is not assignable through `POST /api/v1/admin/users/{cid}/access`
- `POST /api/v1/admin/users/{cid}/access` requires a non-empty `reason` string, recorded as a dossier entry on the *target* user's log (in addition to the existing `USER_ACCESS` audit entry)
- non-`SERVER_ADMIN` callers may only add or remove permissions they themselves currently hold effectively — this is a diff against the target's existing *direct* grants, not a check against the whole submitted set, so permissions the target already has from elsewhere (role membership, a previous admin's grant) are left untouched as long as the save doesn't actually change them; violating this returns `403`
- legacy flat permission overrides are still accepted for compatibility during migration
- visitor application review supports `PENDING`, `APPROVED`, and `DENIED` workflow states
- both listing and deciding visitor applications require `users.visitor_applications.read`/`.decide` respectively (currently granted to `STAFF` — corrected here: this previously claimed a narrower `ATM`/`DATM`/`TA`/`ATA`-only restriction, but no such role check exists in `decide_visitor_application`, and none of those roles hold the permission)
- approving a visitor application also calls the VATUSA `manageVisitor` endpoint with the configured `VATUSA_API_KEY`; if that external call fails, the local approval does not complete
- approving a visitor application activates the applicant's membership (`controller_status: VISITOR`, `membership_status: ACTIVE`, `visitor_home_facility` set from the application) and enables their welcome message, in the same transaction as the decision
- `POST /api/v1/admin/users/{cid}/refresh-vatusa` refreshes one local user against the configured VATUSA facility rosters and applies the same membership upsert or off-roster demotion rules as roster sync
- `GET/DELETE /api/v1/admin/users/{cid}/sessions` + `DELETE .../sessions/{session_id}` (user session manager) list and revoke a user's active osmium auth sessions over `identity.sessions`, gated by `users.sessions.read`/`users.sessions.delete` (SERVER_ADMIN only, kept out of the assignable catalog). The list is metadata only — created/expires time, IP (if captured), and impersonation state — and the raw `session_token` is never returned. Revokes soft-revoke (`revoked_at`), take effect immediately, and are each recorded in the audit log (`USER_SESSION` / `REVOKE` / `REVOKE_ALL`). Distinct from impersonation start/stop
- `GET /api/v1/admin/users/{cid}/ip-history` (spec 011) returns paginated durable request-IP metadata (`ip_address`, `method`, `matched_path`, `status_code`, `created_at`) for that user's actor, newest first, gated by `users.directory_private.read`. Records are written by a batched off-hot-path pipeline (a bounded channel drained by a background job) and pruned after `IP_REQUEST_LOG_RETENTION_DAYS`; the write path adds no synchronous per-request database cost

If a user is the current `SERVER_ADMIN`, the normal access endpoints still return that role and the full grouped effective permission set.
