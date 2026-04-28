# Training API

## Purpose

Manage training assignments, assignment requests, trainer-release requests, and trainer interest workflows.

## Main Routes

- `/api/v1/training/assignments`
- `/api/v1/training/assignment-requests`
- `/api/v1/training/assignment-requests/{request_id}`
- `/api/v1/training/assignment-requests/{request_id}/interest`
- `/api/v1/training/trainer-release-requests`

## Permissions

- staff management routes require `training.update`
- a normal authenticated user can create their own assignment or release requests and mark interest where allowed
