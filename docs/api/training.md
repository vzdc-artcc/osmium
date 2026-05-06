# Training API

## Purpose

Manage training assignments, appointments, assignment requests, trainer-release requests, trainer interest workflows, and full training session submission.

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

Lesson routes now cover:

- lesson lookup for session submission
- lesson create, update, and delete
- progression CRUD
- progression-step CRUD
- performance-indicator template/category/criteria CRUD
- manual progression assignment and removal
- dossier reads by CID

All training list routes that can grow large now use the shared pagination envelope. Canonical query params are `page` and `page_size`, with `limit` and `offset` still accepted for compatibility.

Training appointment routes now cover:

- appointment create, update, delete, list, and detail reads
- student, trainer, and combined user filtering
- trainer ownership derived from the authenticated user on create
- estimated duration and estimated end time computed from linked lesson durations

## Main Routes

- `/api/v1/training/assignments`
- `/api/v1/training/ots-recommendations`
- `/api/v1/training/ots-recommendations/{recommendation_id}`
- `/api/v1/training/lessons`
- `/api/v1/training/lessons/{lesson_id}`
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
- `/api/v1/users/{cid}/dossier`

## Permissions

- read routes require `training.read`
- lesson, assignment, training-appointment, and training-session creation routes require `training.create`
- lesson, training-appointment, and training-session update routes require `training.update`
- OTS recommendation create, assign or unassign, and delete routes require `training.manage`
- moderation and destructive routes require `training.manage`
- `training.manage` is the umbrella training permission and also satisfies the read/create/update checks above
- a normal authenticated user can create their own assignment or release requests and mark interest where allowed

## Training Admin Notes

- progression routes return the current progression catalog and allow create, update, and delete operations
- progression step routes manage lesson ordering within a progression
- performance-indicator template, category, and criteria routes are the backend-owned config surface for scoring policy
- progression assignment routes bind users to progressions using `user_id` and `progression_id`
- dossier reads are exposed through `GET /api/v1/users/{cid}/dossier`

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
- Appointment create accepts `student_id`, `start`, `lesson_ids`, and optional `environment`.
- Appointment create ignores trainer selection from the client; `trainer_id` is always the authenticated user submitting the request.
- Appointment update preserves the original `trainer_id`.
- Appointment list supports pagination plus optional `trainer_id`, `student_id`, and `user_id` filters.
- `user_id` matches appointments where the user is either the trainer or the student.
- Appointment detail returns linked lesson summaries.
- `estimated_duration_minutes` is the sum of linked lesson durations.
- `estimated_end` is computed as `start + estimated_duration_minutes`.
- Empty or duplicate lesson ID payloads are rejected.
- Appointment deletes remove lesson links through database cascades.

## Session Notes

- Use `GET /api/v1/training/lessons` to discover lesson IDs before creating a training session.
- `lesson_id` inside each ticket is the primary key of an existing row in `training.lessons`.
- Lesson IDs are generated by the backend when lessons are created; session creation does not generate new lessons or new lesson IDs.
- Lesson create requests do not include an `id`; the backend generates it automatically.
- Session list supports pagination, sorting, and training-grid style filtering by student, instructor, or lesson.
- Session detail returns nested tickets, rubric scores, and performance-indicator snapshots when present.
- Session create and update accept nested training tickets.
- Ticket payloads include lesson id, pass/fail state, and rubric scores.
- Common mistakes are intentionally not accepted by Osmium even though the legacy site supported them.
- Performance-indicator payloads are only allowed when the first submitted lesson requires them.
- Passing lessons can create release requests, update certifications, remove solo certifications, write dossier entries, and create or remove OTS recommendations.
- Manual OTS recommendation creation requires `student_id` and non-empty `notes`.
- A student can only have one active OTS recommendation at a time.
- Assigning or unassigning an OTS recommendation updates `assigned_instructor_id`.
- Automatic OTS creation from a passed lesson does nothing when the student already has an active recommendation; it preserves the existing notes and assignment.
- Session deletes remove nested ticket and score data through database cascades.
