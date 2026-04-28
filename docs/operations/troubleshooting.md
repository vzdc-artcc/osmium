# Troubleshooting

## App Starts But DB Routes Fail

Cause:

- `DATABASE_URL` is unset
- DB is unreachable
- migrations have not been applied

Symptoms:

- `service_unavailable`
- degraded `/ready`

## OAuth Problems

Check:

- `VATSIM_CLIENT_ID`
- `VATSIM_CLIENT_SECRET`
- `VATSIM_REDIRECT_URI`
- `VATSIM_CLIENT_AUTH_METHOD`

## Service-Account Bearer Auth Fails

Check:

- raw bearer token value
- stored credential active state
- credential expiry
- whether the stored value is the SHA-256 hash of the raw secret

## Signed URL Fails

Check:

- `FILE_SIGNING_SECRET`
- `expires` timestamp
- `sig` value

## File Encryption Problems

Check:

- `FILE_ENCRYPTION_KEY_HEX` length and format
- whether existing blobs were written with or without encryption

## Docs Route Problems

Check:

- the compiled markdown registry in `src/docs.rs`
- docs route coverage tests
- OpenAPI generation errors from handler annotations
