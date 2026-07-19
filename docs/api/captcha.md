# Captcha API

## Purpose

Server-side reCAPTCHA verification proxy — keeps the Google secret key off the client. Protects public-facing forms (currently the Staffing Request and Feedback forms) as a pre-submit bot check.

## Main Routes

- `POST /api/v1/captcha/verify`

## Access

Public — no authentication or permission required. This is a bot gate, not a permission gate, matching how the equivalent legacy-site server action had no auth check either.

## Notes

- this is Google reCAPTCHA v3 (score-based), not a checkbox/challenge captcha. The client obtains a token from the reCAPTCHA widget and posts it here; the response includes a `score` the caller is expected to threshold against (the legacy site used `< 0.7` as its rejection cutoff — this API does not enforce a threshold itself, it just relays Google's response).
- requires `GOOGLE_CAPTCHA_SECRET_KEY` to be configured; the route returns `service_unavailable` if it isn't set, rather than silently passing verification.
- this is a client-side pre-submit check only, not bound to the request that follows it — there is no server-side enforcement tying a verified token to a specific subsequent form submission. This matches the legacy site's own architecture; it is not a stronger guarantee than what already existed.
- response shape is unchanged from Google's siteverify response (`success`, `score`), no additional wrapping.

Request body:

```json
{
  "token": "<token from the reCAPTCHA client widget>"
}
```

Response:

```json
{
  "success": true,
  "score": 0.9
}
```
