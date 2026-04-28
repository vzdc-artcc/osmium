# Users API

## Purpose

Expose roster, user detail, visitor membership, and user feedback views.

## Main Routes

- `GET /api/v1/user`
- `GET /api/v1/user/{cid}`
- `POST /api/v1/user/visit-artcc`
- `GET /api/v1/user/{cid}/feedback`

## Access Rules

- all routes require an authenticated user session
- viewing private fields depends on `users.read`, `users.update`, or self-access
- user detail responses expose grouped effective permissions
