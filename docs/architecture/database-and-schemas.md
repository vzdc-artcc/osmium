# Database and Schemas

Osmium uses one Postgres database with multiple schemas.

## Conventions

- IDs are stored as text-backed UUID values in the current Rust app
- mutable tables generally use `created_at` and `updated_at`
- domain ownership is explicit by schema
- views are used to simplify effective-access and roster reads

## Important Views

- `access.v_user_primary_role`
- `access.v_effective_user_permissions`
- `access.v_effective_service_account_permissions`
- `org.v_user_roster_profile`
- `training.v_active_assignments`
- `events.v_event_staffing_summary`

`org.v_user_roster_profile` now also carries profile-adjacent roster data used by user and staff detail views, including:

- `operating_initials`
- `bio`
- `timezone`
- `new_event_notifications`

## Identity And Membership Notes

Recent self-service profile work relies on these storage points:

- `identity.users.preferred_name`
- `identity.user_profiles.bio`
- `identity.user_profiles.timezone`
- `identity.user_profiles.new_event_notifications`
- `identity.user_identities` with `provider = 'TEAMSPEAK'`
- `org.memberships.operating_initials`

`org.memberships.operating_initials` is protected by a unique partial index so first-login generation can rely on the database to break collisions safely.

## Website Content Tables In Active Use

The website/public-content domain now actively uses:

- `web.pages`
- `web.announcements`
- `web.change_broadcasts`
- `web.site_settings`
- `web.publication_categories`
- `web.publications`

Publication records join to `media.file_assets` for CDN delivery and file metadata while keeping domain ownership in `web`.

## Training Tables In Active Use

The training API now actively reads and writes these training tables:

- `training.training_sessions`
- `training.training_tickets`
- `training.rubric_scores`
- `training.session_performance_indicators`
- `training.session_performance_indicator_categories`
- `training.session_performance_indicator_criteria`
- `training.training_assignments`
- `training.training_assignment_requests`
- `training.trainer_release_requests`

Training session side effects also touch:

- `org.user_certifications`
- `org.user_solo_certifications`
- `feedback.dossier_entries`
- `training.ots_recommendations`

## Why Schemas Instead of Many Databases

This keeps:

- shared user and permission logic centralized
- cross-domain joins simple for admin flows
- local development manageable
- migrations coherent

## Repo Layering

The current preferred layering is:

- models for contracts and row shapes
- handlers for HTTP concerns
- repos for query and persistence logic
- auth modules for identity/access concerns
