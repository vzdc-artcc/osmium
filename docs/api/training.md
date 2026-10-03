# Training API

## Purpose

Manage training assignments, appointments, assignment requests, trainer-release requests, trainer interest workflows, and full training session submission.

## Response Timezones

Timestamped training and training-admin responses follow the shared response-timezone contract via `X-Response-Timezone`.

Assignment routes now cover:

- assignment create, read (list + single detail), update, and delete
- a `primary_trainer_id` plus a full-replace `other_trainer_ids` list (delete-then-reinsert on every update, same pattern as appointment/session additional trainers)
- update validates `other_trainer_ids` don't duplicate each other or the primary trainer, and that every referenced trainer id exists
- approving a trainer-release request (`PATCH .../trainer-release-requests/{id}` with `status: APPROVED`) deletes the assignment for that request's student as part of the same decision — there would be nothing left for the request to have accomplished otherwise; a missing assignment (already removed some other way) is not an error
- `TrainingAssignment` is denormalized for list/detail UIs: `student_cid`/`student_name`/`student_controller_status`, `primary_trainer_cid`/`primary_trainer_name`, and a structured `other_trainers: [{id, cid, name}]` alongside the original `other_trainer_ids` (kept for compatibility/validation use)
- `TrainingAssignmentRequest` and `TrainerReleaseRequest` are similarly denormalized with `student_cid`/`student_name`/`student_controller_status`
- `TrainingAssignmentRequest` also carries `interested_trainers: [{id, cid, name}]` (joined from the interested-trainers table) — there's no separate endpoint to list interest, it's always inlined on the request
- assignment-request creation (`POST /training/assignment-requests`) accepts an optional `student_id` + `submitted_at`: omit both for the common self-request case (needs only `training.assignment_requests_self.request`), or set `student_id` to log a manual/backdated request on another student's behalf (needs `.create`, the broader permission — see below)
- release-request creation (`POST /training/trainer-release-requests`) accepts an optional `student_id`, same pattern as assignment-request creation: omit it for the common self-request case (needs only `training.release_requests_self.request`), or set it to submit a release request on another student's behalf — e.g. a trainer releasing their own assigned student (needs `training.release_requests.create`, the broader permission)

OTS recommendation routes now cover:

- recommendation list, create, assign or unassign, and delete
- one active recommendation per student
- compatibility with pass-triggered automatic OTS recommendation creation

Training session routes now cover:

- session create, update, delete, list, and detail reads
- nested training tickets
- rubric score submission per ticket
- performance-indicator snapshots per session
- pass-triggered release-request, roster, dossier, and OTS side effects
- additional trainers (secondary trainer + free-text description, replace-on-update)

Lesson routes now cover:

- lesson lookup for session submission
- lesson create, update, and delete
- lesson rubric authoring (criteria and cells)
- progression CRUD
- progression-step CRUD
- performance-indicator template/category/criteria CRUD
- manual progression assignment and removal
- per-controller progression status and force-completion
- dossier reads by CID

All training list routes that can grow large now use the shared pagination envelope. Canonical query params are `page` and `page_size`, with `limit` and `offset` still accepted for compatibility.

Training appointment routes now cover:

- appointment create, update, delete, list, and detail reads
- student, trainer, and combined user filtering
- trainer ownership derived from the authenticated user on create
- estimated duration and estimated end time computed from linked lesson durations
- a free-text `notes` field (uppercased, 50-char cap)
- additional trainers (secondary trainer + free-text description, uppercased, replace-on-update)
- list items now include a denormalized `lessons: [{id, identifier, name, location, duration}]` alongside `lesson_count`, and `additional_trainers: [{trainer_id, trainer_cid, trainer_name, description}]` alongside `additional_trainer_count` (both correlated `json_agg`s, same pattern used for `TrainingAssignment.other_trainers`) — added so the appointments admin table/calendar can show lesson-identifier chips and additional-trainer info per row without an extra request per appointment

## Main Routes

- `/api/v1/training/assignments`
- `/api/v1/training/assignments/{assignment_id}`
- `/api/v1/training/ots-recommendations`
- `/api/v1/training/ots-recommendations/{recommendation_id}`
- `/api/v1/training/lessons`
- `/api/v1/training/lessons/{lesson_id}`
- `/api/v1/training/lessons/{lesson_id}/rubric`
- `/api/v1/training/lessons/{lesson_id}/rubric-criteria`
- `/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}`
- `/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells`
- `/api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells/{cell_id}`
- `/api/v1/training/appointments`
- `/api/v1/training/appointments/{appointment_id}`
- `/api/v1/training/sessions`
- `/api/v1/training/sessions/{session_id}`
- `/api/v1/training/assignment-requests`
- `/api/v1/training/assignment-requests/{request_id}`
- `/api/v1/training/assignment-requests/{request_id}/interest`
- `/api/v1/training/trainer-release-requests`
- `/api/v1/training/trainer-release-requests/{request_id}`
- `/api/v1/admin/training/progressions`
- `/api/v1/admin/training/progression-steps`
- `/api/v1/admin/training/performance-indicators/templates`
- `/api/v1/admin/training/performance-indicators/categories`
- `/api/v1/admin/training/performance-indicators/criteria`
- `/api/v1/admin/training/progression-assignments`
- `/api/v1/users/{cid}/progression` (GET status)
- `/api/v1/users/{cid}/progression/complete` (POST force-complete)
- `/api/v1/users/{cid}/dossier`

## Permissions

Permissions are granular and per-resource (`training.<resource>.<action>`), not a single umbrella grant — `training.manage`/`training.read`/`training.create`/`training.update` existed as coarse permission names historically but were fully replaced by the fine-grained set below in a one-time data migration; they no longer exist in the permission catalog. Whichever role held `training.manage` at that time (currently just `STAFF`) was expanded into direct grants of every leaf permission listed here.

- `training.assignments.read` / `.create` / `.update` / `.delete` — assignment CRUD
- `training.ots_recommendations.read` / `.create` / `.update` / `.delete`
- `training.lessons.read` / `.create` / `.update` / `.delete` — also covers lesson rubric routes (`training.lessons.read` for the `GET .../rubric` read, `training.lessons.update` for criteria/cell create and update, `training.lessons.delete` for criteria/cell delete), progression/progression-step/performance-indicator/progression-assignment CRUD, and dossier reads of another user (`training.lessons.read`) — none of these have a dedicated permission namespace of their own
- `training.appointments.read` / `.create` / `.update` / `.delete`
- `training.sessions.read` / `.create` / `.update` / `.delete` — note `GET /training/sessions/{session_id}` is data-dependent: the session's own **student** may read it with just `auth.profile.read` (backs the controller's `/profile/training/[id]` self-view); anyone else needs `training.sessions.read`
- `training.assignment_requests.read`, `.create` (submit a manual/backdated request on another student's behalf), `.decide` (approve/deny), `.delete` (admin cancel/remove any request); `training.assignment_requests_self.request` (self-submit); `training.assignment_requests_interest.request` / `.delete`
- `training.release_requests.read`, `.decide`, `.delete` (admin cancel/remove any request); `training.release_requests_self.request` (self-submit)
- `training.dossier.create`, `training.dossier_confidential.read` (confidential dossier entries)
- a normal authenticated user can create their own assignment or release requests and mark trainer interest without any of the permissions above
- **self-cancel is a data-dependent exception, not a separate permission**: the student who submitted an assignment-request or release-request can `DELETE` it themselves while it's still `PENDING` with no permission check at all; deleting anyone else's request, or a request that's already been decided, requires the corresponding `.delete` permission

## Training Admin Notes

- progression routes return the current progression catalog and allow create, update, and delete operations
- progression step routes manage lesson ordering within a progression
- performance-indicator template, category, and criteria routes are the backend-owned config surface for scoring policy
- progression assignment routes bind users to progressions using `user_id` and `progression_id`
- `GET /api/v1/users/{cid}/progression` returns the controller's assigned progression and, per step, whether the most recent training ticket for that step's lesson passed (self needs `auth.profile.read`; viewing another controller needs `training.lessons.read`)
- `POST /api/v1/users/{cid}/progression/complete` force-advances the controller to their progression's `next_progression_id` (or unassigns them when there is none) once every non-optional step has passed, emailing `progression.assigned` / `progression.removed`. Self-completion needs `auth.profile.update` and honors the `no_force_progression_finish` opt-out flag; staff completing another controller's progression need `training.lessons.update`
- progressions also advance automatically: right after a training session is saved (non-optional gate) and during the periodic roster sync (stricter all-steps gate, matching the legacy `updateProgressionCompletions`). Roster sync additionally hands new home OBS the `auto_assign_new_home_obs` starter progression once (guarded by `flag_auto_assign_single_pass`)
- dossier reads are exposed through `GET /api/v1/users/{cid}/dossier`; `POST /api/v1/users/{cid}/dossier` requires `training.dossier.create` and accepts an optional `confidential` flag (default `false`) — confidential entries are hidden from reads unless the caller holds `training.dossier_confidential.read`, a permission separate from the base read gate above and granted to `ATM`/`DATM`/`TA` by default

Example progression create body:

```json
{
  "name": "Tower Progression",
  "next_progression_id": null,
  "auto_assign_new_home_obs": true,
  "auto_assign_new_visitor": false
}
```

Example progression-step create body:

```json
{
  "progression_id": "progression_uuid",
  "lesson_id": "lesson_uuid",
  "sort_order": 10,
  "optional": false
}
```

Example performance-indicator template create body:

```json
{
  "name": "Tower Rubric v2"
}
```

Example category create body:

```json
{
  "template_id": "template_uuid",
  "name": "Coordination",
  "sort_order": 10
}
```

Example criteria create body:

```json
{
  "category_id": "category_uuid",
  "name": "Completes handoff on time",
  "sort_order": 10
}
```

Example progression-assignment create body:

```json
{
  "user_id": "user_uuid",
  "progression_id": "progression_uuid"
}
```

## Appointment Notes

- Use `GET /api/v1/training/lessons` to discover lesson IDs before creating or updating an appointment.
- Appointment create accepts `student_id`, `start`, `lesson_ids`, optional `environment`, optional `notes`, and optional `additional_trainers`.
- Appointment create ignores trainer selection from the client; `trainer_id` is always the authenticated user submitting the request.
- Appointment update preserves the original `trainer_id`.
- Appointment list supports pagination plus optional `trainer_id`, `student_id`, and `user_id` filters.
- `user_id` matches appointments where the user is either the trainer or the student.
- Appointment detail returns linked lesson summaries and `additional_trainers`.
- `estimated_duration_minutes` is the sum of linked lesson durations.
- `estimated_end` is computed as `start + estimated_duration_minutes`.
- Empty or duplicate lesson ID payloads are rejected.
- Appointment deletes remove lesson links through database cascades.
- `notes` is trimmed, uppercased, and capped at 50 characters.
- `additional_trainers` is a list of `{trainer_id, description}`; each `description` is trimmed and uppercased. The full list is replaced on every create/update (delete-then-reinsert), not merged.
- An additional trainer cannot be the request's acting user (the appointment's own trainer), cannot be duplicated within the same payload, and must reference an existing user — violating any of these returns `bad_request`.

## Session Notes

- Use `GET /api/v1/training/lessons` to discover lesson IDs before creating a training session.
- `lesson_id` inside each ticket is the primary key of an existing row in `training.lessons`.
- Lesson IDs are generated by the backend when lessons are created; session creation does not generate new lessons or new lesson IDs.
- Lesson create requests do not include an `id`; the backend generates it automatically.
- Session list supports pagination, sorting, and training-grid style filtering by student, instructor, or lesson.
- Session list items also include a denormalized `tickets: [{id, lesson_id, lesson_identifier, passed}]` alongside `ticket_count` (a correlated `json_agg`, same pattern used elsewhere) — added so the sessions admin table can show pass/fail lesson chips per row without an extra request per session.
- Session detail returns nested tickets, rubric scores, and performance-indicator snapshots when present.
- Session create and update accept nested training tickets. **Tickets, additional trainers, and the performance-indicator snapshot are all full-replace on every create/update, not merge-patch** — the caller must resend everything it wants to keep, not just what changed. This applies to performance indicators too: if the first ticket's lesson has a `performance_indicator_template_id`, an update that omits `performance_indicator` entirely is a validation error (`"You must fill out all performance indicators to submit this ticket."`), not a silent no-op — a client that wants to leave PI unchanged on an otherwise-unrelated edit (e.g. fixing a typo in comments) must resend the session's current PI snapshot as-is, fetched from the prior `GET`/mutation response.
- Ticket payloads include lesson id, pass/fail state, and rubric scores.
- Common mistakes are intentionally not accepted by Osmium even though the legacy site supported them.
- Performance-indicator payloads are only allowed when the first submitted lesson requires them (`(None, Some(_))` — a PI snapshot submitted for a lesson with no template — is also a validation error).
- `PATCH /api/v1/training/sessions/{session_id}` returns the same `{session: null, release: null, roster_updates: [], ots_recommendation: null, errors: [...]}` body shape with a `400` on validation failure as `POST` does (both delegate to the same internal upsert) — the client-facing error contract is identical for create and update.
- Passing lessons can create release requests, update certifications, remove solo certifications, write dossier entries, and create or remove OTS recommendations.
- Manual OTS recommendation creation requires `student_id` and non-empty `notes`.
- A student can only have one active OTS recommendation at a time.
- Assigning or unassigning an OTS recommendation updates `assigned_instructor_id`.
- Automatic OTS creation from a passed lesson does nothing when the student already has an active recommendation; it preserves the existing notes and assignment.
- Session deletes remove nested ticket and score data through database cascades.
- Session create and update accept an optional `additional_trainers` list of `{trainer_id, description}` (description is trimmed but **not** uppercased, unlike appointments). The full list is replaced on every create/update.
- An additional trainer cannot be the session's student or its instructor (the acting user), cannot be duplicated within the same payload, and must reference an existing user.

## Lesson Rubric Notes

- A lesson's rubric is created implicitly by its **first** rubric criteria — there is no separate "create rubric" call. `POST /api/v1/training/lessons/{lesson_id}/rubric-criteria` creates the lesson's rubric on first use and attaches subsequent criteria to the same rubric.
- `GET /api/v1/training/lessons/{lesson_id}/rubric` returns the full structure (rubric id plus every criteria with its nested cells). Returns `not_found` if the lesson has no rubric yet.
- Criteria fields: `criteria` (name), `description`, `max_points`, `passing`. All required, `criteria` capped at 255 characters.
- Cell fields: `points`, `description`. `points` must be between `0` and the parent criteria's `max_points` — validated against the criteria's actual stored value server-side, not a client-supplied bound.
- Cell `points` must be unique within a criteria; a duplicate is rejected with `bad_request` (matching the intent behind the legacy site's "add an OR to the description" guidance rather than allowing ambiguous duplicate point values).
- Criteria and cell mutation routes are nested under their owning `lesson_id`/`criteria_id` for path consistency, but IDs alone are sufficient to resolve the resource; a mismatched parent in the path returns `not_found`.
- Deleting a criteria cascades to its cells (and any historical rubric scores referencing them) at the database level.

Example criteria create body (`POST /api/v1/training/lessons/{lesson_id}/rubric-criteria`):

```json
{
  "criteria": "Handoff timing",
  "description": "Hands off traffic within acceptable range of the sector boundary.",
  "max_points": 10,
  "passing": 7
}
```

Example cell create body (`POST /api/v1/training/lessons/{lesson_id}/rubric-criteria/{criteria_id}/cells`):

```json
{
  "points": 10,
  "description": "Handed off within 2 minutes of boundary, no coordination issues."
}
```
