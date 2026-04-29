# Resource-Action Permission Model

## Summary

Replace the current flat permission catalog such as `manage_users`, `manage_training`, and `upload_files` with a canonical resource-action model.

Canonical representation:

```json
{
  "events": ["read", "create", "update", "delete"],
  "files": ["read", "create", "update", "delete"],
  "users": ["read", "update"],
  "training": ["read", "create", "update", "manage"],
  "feedback": ["read", "update"]
}
```

This should be canonical in both the database model and the API surface, with a short compatibility period where old flat permission names are still accepted on write and translated internally. Action names should use a shared verb vocabulary by default.

## Goals

- Make permissions legible and predictable by grouping them by resource.
- Remove the current drift between seeded DB permission names and the Rust `Permission` enum.
- Give handlers a consistent way to express checks like `events.update` instead of reusing unrelated permissions such as `manage_users`.
- Preserve role-based defaults and user-specific overrides.

## Non-Goals

- Do not redesign the role concept.
- Do not introduce row-level or scope-level authorization in this change.
- Do not change auth/session/service-account flows beyond permission representation and checks.
- Do not solve file viewer-role access in this change; that remains a separate resource-visibility mechanism.

## Canonical Model

### Resources

Initial resource set:

- `auth`
- `system`
- `users`
- `training`
- `feedback`
- `files`
- `events`
- `stats`
- `integrations`
- `web`

### Shared actions

Canonical shared action vocabulary:

- `read`
- `create`
- `update`
- `delete`
- `manage`

Rules:

- Prefer the smallest verb set that accurately expresses current route behavior.
- `manage` is allowed, but only as an explicit catalog entry, not as an implied wildcard in code.
- No resource-specific verbs in this phase. Existing concepts like `upload_files` should map to `files.create`. Existing concepts like `publish_events` should map to `events.update` for now, because publishing is currently an event state mutation.

### Permission identity

Internally, each permission is identified by the string:

```text
<resource>.<action>
```

Examples:

- `users.read`
- `users.update`
- `training.read`
- `training.create`
- `training.update`
- `training.manage`
- `files.create`
- `files.update`
- `events.update`

This dotted string is the canonical DB value and the canonical runtime key. The grouped JSON object is the canonical API representation for admin read/write surfaces.

## Database Changes

### Keep existing tables

Keep these tables:

- `access.permissions`
- `access.role_permissions`
- `access.user_permissions`

Do not introduce separate `resources` and `actions` tables in this phase. The repo already stores generic permission names, so storing canonical dotted strings is the least disruptive migration.

### Migrate permission names

Replace current seeded names with canonical dotted names.

Target mapping:

- `read_own_profile` -> `auth.read`
- `logout` -> `auth.delete`
- `read_system_readiness` -> `system.read`
- `view_all_users` -> `users.read`
- `manage_users` -> `users.update`
- `manage_training` -> `training.manage`
- `manage_feedback` -> `feedback.update`
- `upload_files` -> `files.create`
- `manage_files` -> `files.update`
- `dev_login_as_cid` -> `auth.manage`
- `manage_events` -> `events.update`
- `publish_events` -> `events.update`
- `manage_stats` -> `stats.manage`
- `manage_integrations` -> `integrations.manage`
- `manage_web_content` -> `web.update`

Notes:

- `publish_events` collapses into `events.update` in this phase.
- `manage_events` also collapses into `events.update`.
- Duplicate mappings are acceptable; the migration should deduplicate role assignments and user overrides after translation.

### Migration plan

Create a new SQL migration that:

1. Inserts all canonical dotted permissions into `access.permissions`.
2. Rewrites `access.role_permissions.permission_name` from legacy names to canonical names.
3. Rewrites `access.user_permissions.permission_name` from legacy names to canonical names.
4. Deletes now-unused legacy permission rows from `access.permissions`.
5. Deduplicates any rows that collapse to the same `(role_name, permission_name)` or `(user_id, permission_name)`.
6. Seeds any missing canonical permissions needed by code, even if no role currently uses them.

Do not change the effective-permissions views; they already operate on opaque permission strings.

## Runtime and Type Changes

### Replace `Permission` enum

The current Rust enum is already out of sync with the DB seed. Replace it with a typed resource-action representation.

Introduce:

- `PermissionResource` enum
- `PermissionAction` enum
- `PermissionKey` struct or enum-backed value object

Recommended runtime shape:

```rust
struct PermissionKey {
    resource: PermissionResource,
    action: PermissionAction,
}
```

Required methods:

- parse from canonical dotted string
- render to canonical dotted string
- serialize to API-friendly grouped structures
- compare/hash for set membership

Avoid keeping a manually enumerated flat permission enum. That is the current failure mode.

### Authorization helper

Replace:

```rust
ensure_permission(..., Permission::ManageUsers)
```

with:

```rust
ensure_permission(..., PermissionKey::new(Resource::Users, Action::Update))
```

Keep the middleware interface otherwise unchanged.

### Effective permission loading

Change permission fetch/parsing so unknown seeded values are not silently dropped. Current `filter_map` behavior hides mismatches.

Required behavior:

- DB permission strings must parse into `PermissionKey`.
- Parse failure should return `ApiError::Internal` and log the invalid permission value.
- No silent permission loss.

## API Changes

### Read endpoints

Change admin and introspection responses from flat arrays to grouped objects.

Current shape:

```json
{
  "roles": ["STAFF"],
  "permissions": ["manage_users", "manage_training"]
}
```

New shape:

```json
{
  "roles": ["STAFF"],
  "permissions": {
    "users": ["read", "update"],
    "training": ["update"],
    "files": ["create", "update"]
  }
}
```

Apply this to:

- `GET /api/v1/admin/acl`
- `GET /api/v1/admin/access/catalog`
- `GET /api/v1/admin/users/{cid}/access`
- `GET /api/v1/me`
- `GET /api/v1/auth/service-account/me`
- any user overview/admin overview payloads that expose permissions

### Write endpoint

Change `POST /api/v1/admin/users/{cid}/access` to accept grouped permissions.

New request shape:

```json
{
  "roles": ["STAFF"],
  "permissions": {
    "users": ["read", "update"],
    "events": ["read", "update"],
    "files": ["create", "update"]
  }
}
```

Direct grant/deny overrides should remain supported, but the payload needs to become explicit about polarity.

Recommended request shape:

```json
{
  "roles": ["STAFF"],
  "permission_overrides": {
    "grant": {
      "events": ["update"],
      "files": ["create"]
    },
    "deny": {
      "users": ["update"]
    }
  }
}
```

Rules:

- `roles` remains a full replacement list.
- `permission_overrides.grant` and `.deny` are each full replacement grouped maps for direct user overrides.
- A permission must not appear in both `grant` and `deny`; reject with `400`.
- Empty payload is still `400`.

### Compatibility window

For one release window, accept legacy write payloads too:

- old `permissions: [{ "name": "manage_users", "granted": true }]`
- new grouped `permission_overrides`

Behavior:

- Normalize both into canonical dotted permission keys.
- Responses always use the new grouped shape.
- After the compatibility window, remove legacy write support.

## Route-to-Permission Mapping

Use these permissions at handlers:

- `GET /api/v1/me` -> `auth.read`
- `POST /api/v1/auth/logout` -> `auth.delete`
- readiness routes -> `system.read` if protected
- admin ACL/catalog/user access/admin user list/controller status -> `users.update`
- user list/private full roster access -> `users.read`
- event create/update/delete/position management -> `events.update`
- event public reads -> no permission if public; otherwise `events.read`
- training reads -> `training.read`
- training creates -> `training.create`
- training standard edits -> `training.update`
- training moderation and destructive actions -> `training.manage`
- feedback moderation/decision -> `feedback.update`
- file upload -> `files.create`
- file metadata/content replacement/delete/admin list/audit -> `files.update`
- stats sync/admin operations -> `stats.manage`
- integration management -> `integrations.manage`
- web content management -> `web.update`

Important correction during implementation:

- Event handlers currently check `Permission::ManageUsers`. That should become `events.update`, not a users permission.

## Role Seed Mapping

Seed roles should continue to assign default canonical permissions.

Initial role mapping:

- `USER`
  - `auth.read`
  - `auth.delete`
  - `files.create`

- `STAFF`
  - `auth.read`
  - `auth.delete`
  - `system.read`
  - `users.read`
  - `users.update`
  - `training.read`
  - `training.create`
  - `training.update`
  - `training.manage`
  - `feedback.update`
  - `files.create`
  - `files.update`
  - `auth.manage`
  - `events.update`
  - `stats.manage`
  - `integrations.manage`
  - `web.update`

- `BOT`
  - `integrations.manage`

- `SERVICE_APP`
  - `integrations.manage`

Do not expand specialty staff roles in this change unless they are already used elsewhere in code. Keep the existing simple assignment model.

## Handler and Model Refactor

### Models

Update API models in `src/models/access/mod.rs` and related user/auth models so:

- `permissions: Vec<Permission>` becomes `permissions: BTreeMap<String, Vec<String>>` at the serialized API boundary, or a dedicated typed wrapper that serializes that way.
- Access catalog returns grouped permissions, not a flat string array.

Recommended serialized type:

```rust
type GroupedPermissions = BTreeMap<String, Vec<String>>;
```

Use sorted output for stable tests and docs.

### Parsing helpers

Add normalization helpers:

- grouped API object -> canonical dotted strings
- canonical dotted strings -> grouped API object
- legacy flat names -> canonical dotted strings

All normalization should deduplicate and sort.

## Validation Rules

Reject with `400` when:

- resource name is unknown
- action name is unknown
- same permission appears in both grant and deny
- payload contains empty resource keys
- payload contains empty action strings
- payload is semantically empty after normalization

## Testing

### Unit tests

Add tests for:

- parsing canonical dotted permission strings
- rendering resource/action keys back to strings
- grouping flat dotted keys into API shape
- normalizing grouped API input into canonical dotted strings
- legacy flat-name translation to canonical keys
- rejection of unknown resources/actions
- rejection of duplicate permission across grant and deny
- failure on invalid DB permission string instead of silent drop

### Integration tests

Add or update route tests for:

- `GET /api/v1/admin/access/catalog` returns grouped permissions
- `GET /api/v1/admin/acl` returns grouped effective permissions
- `POST /api/v1/admin/users/{cid}/access` accepts grouped overrides
- legacy write payload still accepted during compatibility window
- event mutation routes require `events.update`
- a user with `users.read` but not `users.update` can read user data but cannot mutate admin access
- a user with `files.create` can upload but cannot perform file-admin updates
- a user with `files.update` can manage file metadata
- effective permission collapse works when both legacy event permissions mapped to `events.update`

### Regression tests

Specifically cover the current drift:

- seeded DB permission present but missing from code should fail fast in tests
- no handler should rely on `users.update` for event management after refactor

## Documentation Changes

Update:

- `docs/architecture/auth-and-access.md`
- `docs/architecture/request-flow.md`
- `docs/api/admin.md`
- `docs/api/auth.md`
- `docs/api/users.md`
- any generated API docs / Bruno collections that send or display permission payloads

Document the canonical format as:

- storage/runtime key: `resource.action`
- API read/write grouped object: `{ resource: [action, ...] }`

## Rollout Sequence

1. Add canonical permission types and conversion helpers.
2. Add grouped API serializers/deserializers plus legacy-write compatibility.
3. Add DB migration to rewrite legacy permission names to canonical dotted names.
4. Update seed data to canonical dotted names.
5. Refactor handlers to use resource/action checks.
6. Update docs and Bruno examples.
7. Remove any tests that assert old flat permission arrays.
8. In a later cleanup release, remove legacy write payload support.

## Assumptions and Defaults

- Canonical layer is both DB and API, not API-only.
- Shared verbs are the default vocabulary.
- No resource-specific verbs are introduced in this phase.
- Dotted permission strings are the canonical stored identifier.
- Grouped JSON objects are the canonical external admin/introspection representation.
- `publish_events` is not preserved as a separate action in this phase; it maps to `events.update`.
- `logout` maps to `auth.delete` because it destroys a session.
- `dev_login_as_cid` maps to `auth.manage` as an elevated auth capability.
- Role model remains unchanged.
- Scope-aware permissions are out of scope for this change.
