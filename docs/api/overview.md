# API Overview

The generated API reference lives at `/docs/api/v1`. This page explains how to read the API as a platform consumer.

## Main Route Groups

- auth
- users
- admin
- training
- events
- feedback
- files
- stats

## Auth Patterns

- session cookie auth for human clients
- bearer token auth for service accounts

## Error Patterns

Common error values:

- `bad_request`
- `unauthorized`
- `service_unavailable`
- `internal_error`

## Route Prefix

Business routes live under:

```text
/api/v1
```
