# Events API

## Purpose

Create, update, delete, and staff events.

## Main Routes

- `/api/v1/events`
- `/api/v1/events/{event_id}`
- `/api/v1/events/{event_id}/positions`
- `/api/v1/events/{event_id}/positions/{position_id}`
- `/api/v1/events/{event_id}/positions/publish`

## Access

- list and get are public to the API consumer side
- mutation currently requires `events.update`
