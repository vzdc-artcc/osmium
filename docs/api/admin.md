# Admin API

## Purpose

Administrative access and roster-control operations.

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
- `POST /api/v1/admin/users/{cid}/refresh-vatusa`
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

## Workflow Notes

- admin list routes that can grow large now use the shared pagination envelope
- `GET /api/v1/admin/jobs` and `GET /api/v1/admin/jobs/{job_name}` expose persisted job-run state for backend automations
- `PATCH /api/v1/admin/users/{cid}/controller-lifecycle` is the backend-owned controller transition and cleanup endpoint
- LOA, solo-certification, staffing-request, SUA, visitor-application, audit, admin-user, and outbound-job admin list routes all support backend-native pagination fields instead of website grid semantics
- training admin routes are grouped under `/api/v1/admin/training/*` and use read or update variants of the training lesson permission path

## Permission Payloads

- access responses return grouped permissions such as `{ "users": ["read", "update"] }`
- `POST /api/v1/admin/users/{cid}/access` accepts grouped `permissions` and grouped `permission_overrides`
- `SERVER_ADMIN` is reserved for env-driven bootstrap and is not assignable through `POST /api/v1/admin/users/{cid}/access`
- legacy flat permission overrides are still accepted for compatibility during migration
- visitor application review supports `PENDING`, `APPROVED`, and `DENIED` workflow states
- visitor application approval is further restricted to users with one of the explicit approver roles: `ATM`, `DATM`, `TA`, or `ATA`
- approving a visitor application also calls the VATUSA `manageVisitor` endpoint with the configured `VATUSA_API_KEY`; if that external call fails, the local approval does not complete
- `POST /api/v1/admin/users/{cid}/refresh-vatusa` refreshes one local user against the configured VATUSA facility rosters and applies the same membership upsert or off-roster demotion rules as roster sync

If a user is the current `SERVER_ADMIN`, the normal access endpoints still return that role and the full grouped effective permission set.
