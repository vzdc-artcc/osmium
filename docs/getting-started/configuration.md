# Configuration

This page documents the main environment variables used by Osmium.

## App and Server

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `BIND_ADDR` | No | `0.0.0.0:3000` | Socket address for the Axum server. |
| `RUST_LOG` | No | app default | Standard tracing filter. |
| `RUN_MIGRATIONS_ON_STARTUP` | No | `true` | Applies SQLx migrations on boot when the DB is configured. |
| `OSMIUM_SERVER_ADMIN_CID` | No | unset | When set to a VATSIM CID, the matching user claims or transfers the singleton `SERVER_ADMIN` role on successful login. |

## Database

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes for DB-backed routes | none | If unset, many endpoints return `service_unavailable`. |

## OAuth and User Auth

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `VATSIM_CLIENT_ID` | Yes for OAuth | none | VATSIM OAuth client id. |
| `VATSIM_CLIENT_SECRET` | Yes for OAuth | none | VATSIM OAuth client secret. |
| `VATSIM_REDIRECT_URI` | Yes for OAuth | none | Must exactly match the registered redirect URI. Use one canonical local origin. |
| `VATSIM_AUTHORIZE_URL` | No | VATSIM default | Authorization endpoint. In local dev, `auth-dev.vatsim.net` is recommended. |
| `VATSIM_TOKEN_URL` | No | VATSIM default | Token endpoint. In local dev, `auth-dev.vatsim.net` is recommended. |
| `VATSIM_USERINFO_URL` | No | VATSIM default | User info endpoint. In local dev, `auth-dev.vatsim.net` is recommended. |
| `VATSIM_SCOPE` | No | app default | Requested OAuth scopes. |
| `VATSIM_CLIENT_AUTH_METHOD` | No | `basic` | `basic` or `post`. Use `post` with `auth-dev.vatsim.net`. |
| `COOKIE_SECURE` | No | `false` | Set to `true` behind HTTPS. |

## Dev Mode

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `API_DEV_MODE` | No | `false` | Enables dev login and seed routes. |
| `VATSIM_DEV_MODE` | No | `false` | Switches to VATSIM dev OAuth defaults when appropriate. |

### Local OAuth Recommendation

For normal local development, prefer:

```bash
VATSIM_DEV_MODE=true
VATSIM_CLIENT_AUTH_METHOD=post
VATSIM_REDIRECT_URI=http://127.0.0.1:3000/api/v1/auth/vatsim/callback
VATSIM_AUTHORIZE_URL=https://auth-dev.vatsim.net/oauth/authorize
VATSIM_TOKEN_URL=https://auth-dev.vatsim.net/oauth/token
VATSIM_USERINFO_URL=https://auth-dev.vatsim.net/api/user
COOKIE_SECURE=false
```

Do not mix `localhost` and `127.0.0.1` during the same login flow.

## File Storage and CDN

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `FILE_STORAGE_ROOT` | No | `./storage/files` | Root directory for file blobs in local/dev mode. |
| `FILE_MAX_UPLOAD_BYTES` | No | `26214400` | Max upload size in bytes. |
| `FILE_SIGNING_SECRET` | Yes for signed URLs | none | Required for signing and validating CDN URLs. |
| `FILE_ENCRYPTION_KEY_HEX` | No | unset | Optional AES-256-GCM key for encryption at rest. |
| `CDN_BASE_URL` | No | `http://localhost:3000` | Base URL used for signed link generation. |

## Jobs

| Variable | Required | Default | Notes |
| --- | --- | --- | --- |
| `STATS_SYNC_ENABLED` | No | `true` | Enables the controller stats collector worker. |
| `STATS_SYNC_INTERVAL_SECS` | No | `5` | Poll interval for live and sweatbox controller feeds. |
| `STATS_SYNC_STALE_SECS` | No | `300` | Controls readiness staleness threshold for stats sync. |
| `VNAS_CONTROLLER_FEED_URL_LIVE` | No | live VNAS URL | Optional override for the live controller feed. |
| `VNAS_CONTROLLER_FEED_URL_SWEATBOX1` | No | sweatbox1 VNAS URL | Optional override for the Sweatbox 1 controller feed. |
| `VNAS_CONTROLLER_FEED_URL_SWEATBOX2` | No | sweatbox2 VNAS URL | Optional override for the Sweatbox 2 controller feed. |
| `ROSTER_SYNC_ENABLED` | No | `true` | Enables the VATUSA roster sync worker. |
| `ROSTER_SYNC_INTERVAL_SECS` | No | `900` | Poll interval for the VATUSA roster sync worker. |
| `VATUSA_API_KEY` | Yes when roster sync or visitor approval is enabled | none | API key used for VATUSA roster, user-detail, and visitor-management requests. |
| `VATUSA_FACILITY_ID` | No | `ZDC` | Facility id to sync from VATUSA. |
| `VATUSA_API_BASE_URL` | No | `https://api.vatusa.net/v2` | Base URL for VATUSA API requests. |

## Docs Behavior

There are no separate runtime variables for the markdown docs set in this pass. Docs pages are compiled into the binary with `include_str!`, and the OpenAPI reference is generated from handler metadata at runtime.
