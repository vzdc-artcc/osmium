# ATC Bookings API

## Purpose

Server-side proxy for the VATSIM ATC-booking service
(`atc-bookings.vatsim.net`). Keeps the bearer credential
(`ATC_BOOKING_TOKEN`) off the client and centralizes the ARTCC's booking
business rules that previously lived in the website's `actions/atcBooking.ts`
server action. Backs the website's `/bookings/calendar`, `/profile/bookings*`
pages, the homepage "upcoming bookings" widget, and the training-appointment
scheduler's automatic booking sync.

When `ATC_BOOKING_TOKEN` is unset, every endpoint returns
`503 service_unavailable`.

## Routes

- `GET /api/v1/bookings` — proxies `GET /api/booking?key_only=1&sort=start`.
  Optional `?cid=<int>` restricts to one controller (self "your bookings"
  view); omit for the ARTCC-wide calendar. Returns `{ items: AtcBookingItem[] }`.
- `GET /api/v1/bookings/{id}` — proxies `GET /api/booking/{id}` (the upstream
  returns an array; the first entry is unwrapped). `404` if not found.
- `POST /api/v1/bookings` — create. Returns the created `AtcBookingItem` on
  success, or `400 { message }` on a validation / upstream error.
- `PUT /api/v1/bookings/{id}` — update. A `PUT` against a booking the upstream
  no longer has falls back to a create (parity with the legacy action).
- `DELETE /api/v1/bookings/{id}` — delete. `404` if the booking doesn't exist.

`AtcBookingItem`: `{ id, callsign, cid, type?, division?, subdivision?, start,
end }`. Times are the upstream `"YYYY-MM-DD HH:mm:ss"` UTC strings, passed
through untouched.

## Authorization

- Reads (`GET`) — any authenticated user (the calendar is ARTCC-wide).
- Writes (`POST`/`PUT`/`DELETE`) are data-dependent: a controller may
  self-serve their own **non-training** bookings with just `auth.profile.update`
  (`body.cid == caller.cid` and `type != "training"`); editing another
  controller's booking, or any `training` booking (the appointment-scheduler
  path), requires `training.appointments.update`. `DELETE` evaluates this
  against the fetched booking's `cid`/`type`.

## Business rules (non-training bookings only)

Mirrors the legacy action exactly — applied only when `type` is not
`"training"`:

- at most 2 active bookings per controller (checked on create)
- start must be 2–72 hours in the future
- duration at most 2 hours

The callsign-prefix check (against `stats.statistics_prefixes`) runs only when a
booking `type` is set (e.g. `training`/`event`/`exam`) — plain user bookings are
not prefix-restricted, matching the website. Training bookings bypass the
advance/duration/limit rules.
