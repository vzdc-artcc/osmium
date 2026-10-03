# Users API

## Purpose

Expose roster, user detail, visitor membership, visitor application, and user feedback views.

## Response Timezones

Timestamped user-domain responses such as visitor applications, solo certifications, feedback entries, and refresh metadata follow the shared response-timezone contract via `X-Response-Timezone`.

## Main Routes

- `GET /api/v1/users`
- `GET /api/v1/users/{cid}`
- `GET /api/v1/users/{cid}/staff-positions`
- `GET /api/v1/users/{cid}/solo-certifications`
- `GET /api/v1/users/{cid}/certifications`
- `GET /api/v1/users/{cid}/event-positions`
- `GET /api/v1/users/{cid}/dossier`
- `POST /api/v1/users/{cid}/dossier`
- `POST /api/v1/users/refresh-vatusa`
- `GET /api/v1/users/visitor-application`
- `POST /api/v1/users/visitor-application`
- `POST /api/v1/users/visit-artcc`
- `GET /api/v1/users/{cid}/feedback`

## Access Rules

- `GET /api/v1/users` and `GET /api/v1/users/{cid}` are public by policy — no session required — matching the live site's roster/profile pages, which have never required login. An anonymous or non-privileged caller only ever sees `basic` fields; `full` (private) fields require self-access or `users.directory_private.read`, same as before.
- every other route in this file still requires an authenticated user session
- viewing private fields depends on `users.read`, `users.update`, or self-access
- user detail responses expose grouped effective permissions
- `GET /api/v1/users/{cid}/feedback` requires `users.directory_private.read` for every caller, including a controller viewing their own feedback: the response carries every status, staff comments, and the submitter's identity

## Notes

- `GET /api/v1/users` and `GET /api/v1/users/{cid}/feedback` now use the shared pagination envelope.
- `GET /api/v1/users` excludes roster-hidden users (`identity.user_flags.hidden_from_roster`) from the listing entirely unless the caller can view the private directory (self-match doesn't apply to the bulk listing) — matching the live site's public roster query, which never showed hidden users. `GET /api/v1/users/{cid}` (direct-by-cid lookup) is unaffected; hidden-from-roster only controls listing visibility, not individual profile access.
- `GET /api/v1/users` accepts `controllers_only=true` to restrict the listing to users whose `controller_status` is `HOME` or `VISITOR` (excludes `NONE`), matching the live site's controller-picker filter (e.g. the feedback submission form).
- `GET /api/v1/users` accepts `role=<ROLE_NAME>` (e.g. `role=INS` for instructors, `role=MTR` for mentors — see the role catalog for the full list) to restrict the listing to users who currently hold that role — checked against a user's full role set via `access.user_roles`, not just the single highest-priority "primary role" the listing otherwise displays as `role` on each item, so a user who is both `STAFF` and `INS` is still matched by `role=INS`. Exists for trainer/instructor pickers across the training domain, which have no other way to filter the roster by role.
- `POST /api/v1/users/visitor-application` is the primary visitor workflow and upserts one current application per user
- `GET /api/v1/users/visitor-application` returns the caller's current application or `null` when none exists
- `GET /api/v1/admin/visitor-applications` supports `limit`, `offset`, `status`, `cid` (exact), `display_name` (contains, case-insensitive), and `home_facility` (contains, case-insensitive)
- `POST /api/v1/users/visit-artcc` remains available as a legacy/manual compatibility shortcut
- `POST /api/v1/users/refresh-vatusa` refreshes the caller from VATUSA using the same single-user membership rules as roster sync, including off-roster demotion
- roster detail responses now include stored membership parity fields such as `membership_status`, `join_date`, `home_facility`, `visitor_home_facility`, and `is_active` when full profile access is allowed
- roster detail responses (both `GET /api/v1/users` list items with full access and `GET /api/v1/users/{cid}`) also include `operating_initials` and `role_names` (the user's complete assigned role set, e.g. `["STAFF", "INS"]` — distinct from the always-present `role` field, which is just the single highest-priority "primary role" shown for every roster row regardless of viewer access) when full profile access is allowed — added for training pickers that need to check role membership or display operating initials without an extra per-user request
- `GET /api/v1/users/{cid}/solo-certifications` is self-readable for the matching user and staff-readable through `users.directory.read`
- `GET /api/v1/users/{cid}/certifications` returns every certification type with the user's granted option (`'NONE'` when ungranted, never omitted) — same self/`users.directory.read` split as solo certifications, unpaginated (bounded by the certification-type catalog)
- `GET /api/v1/users/{cid}/event-positions` returns the user's **published** event-position history only, most recent event first, including the `final_position`/`final_start_time`/`final_end_time` fields set after an event concludes — same self/`users.directory.read` split, unpaginated
- `GET /api/v1/users/{cid}/dossier` is self-readable for the matching user and otherwise requires the training read path; entries with `is_confidential = true` are omitted from the response (and its `total`/pagination count) unless the caller holds `training.dossier_confidential.read` — this applies even to the subject viewing their own dossier, since a confidential entry isn't necessarily meant for them
- `POST /api/v1/users/{cid}/dossier` requires `training.dossier.create` and accepts `{"message": string, "confidential"?: bool}` (`confidential` defaults to `false`); returns the created `DossierEntryItem`
- `PATCH /api/v1/admin/users/{cid}/profile` (`users.flags.update`) is the admin edit of **another** controller's profile — `{preferred_name?, bio?, timezone}` — returning the updated `MeProfileBody`. Operating initials stay on the dedicated `PATCH .../operating-initials` endpoint, the event-notification opt-in stays a self-service preference (`PATCH /me`, never clobbered here), and self-service editing stays on `PATCH /me`; this backs the website's admin controller-edit pages
