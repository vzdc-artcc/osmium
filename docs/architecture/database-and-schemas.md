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
