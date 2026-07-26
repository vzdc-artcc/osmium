# Captcha API

## Purpose

Server-side Cloudflare Turnstile verification proxy — keeps the Turnstile secret key off the client. Protects the Staffing Request and Feedback forms as a pre-submit bot check.

## Main Routes

- `POST /api/v1/captcha/verify`

## Access

Public — no authentication or permission required. This is a bot gate, not a permission gate.

## Notes

- this is Cloudflare Turnstile, which is pass/fail — there is no score. The client obtains a token from the Turnstile widget and posts it here; the response is a single `success` boolean. (This replaced the previous Google reCAPTCHA v3 integration, which was score-based and returned an additional `score` field — that field is gone.)
- requires `TURNSTILE_SECRET_KEY` to be configured; the route returns `service_unavailable` if it isn't set, rather than silently passing verification.
- this is a client-side pre-submit check only, not bound to the request that follows it — there is no server-side enforcement tying a verified token to a specific subsequent form submission.
- response is Cloudflare's siteverify result reduced to `{ "success": boolean }`; Turnstile's other metadata fields (`error-codes`, `challenge_ts`, `hostname`, …) are not relayed.

Request body:

```json
{
  "token": "<token from the Turnstile client widget>"
}
```

Response:

```json
{
  "success": true
}
```
