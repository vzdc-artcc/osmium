# Deploying Osmium to Kubernetes (via Portainer)

Runs osmium in the **`default`** namespace, alongside the website, reusing the
same shared cluster secrets the website uses.

| File | Purpose |
| --- | --- |
| `postgres-setup.sql` | One-time: create the `osmium` database + extensions. |
| `osmium.yaml` | The k8s manifest — PVC, Deployment, Service only. |
| `osmium-config.example.yaml` | Template for the `osmium-config` ConfigMap (non-secret env). |
| `osmium-secret.example.yaml` | Template for the `osmium-secret` Secret (osmium's app secrets). |
| `osmium-all-in-one.yaml` | **Single-file option** — all five objects (ConfigMap + Secret + PVC + Deployment + Service) in one manifest to import once. |

Two ways to deploy:

- **Separate objects (Steps 2–5 below):** create `osmium-config` and
  `osmium-secret` in Portainer, then apply `osmium.yaml`. Mirrors the website's
  `website-config` / `website-secret` split; secrets live only in the Secret object.
- **One file (`osmium-all-in-one.yaml`):** fill it in and import once — Kubernetes
  routes each object to the right place by its `kind` (the ConfigMap lands under
  ConfigMaps, the Secret under Secrets). Convenient, but the filled-in file holds
  real secret values, so **don't commit it** (copy it to `*.local.yaml`, which is
  gitignored, and import that). Skip Steps 2–3 and 5 if you use this.

Either way: run `postgres-setup.sql` first (Step 1), make sure the shared
`db-secret` + `email-secret` exist, and get the hostname + OAuth callbacks sorted
(Step 4). External exposure (the DigitalOcean load balancer + TLS) is handled
outside this namespace, so no Ingress is included — osmium's ClusterIP Service is
ready for the LB to route to, mirroring the website.

## How it fits your cluster

Osmium mirrors the website's pattern and **reuses your shared secrets**:

- **`db-secret`** — Postgres host/port/user/password. osmium builds its own
  `DATABASE_URL` from these (`postgres://$(POSTGRES_USER):$(POSTGRES_PASSWORD)@$(POSTGRES_HOST):$(POSTGRES_PORT)/osmium`),
  hardcoding the **`osmium`** database so it can never touch the website's DB.
- **`email-secret`** — AWS SES credentials.
- **`osmium-secret`** — osmium's own app secrets (you create this; it's the only
  new secret).

Osmium connects as the **same Postgres user** as the website, just into a
separate `osmium` database. Data is fully isolated; nothing existing is touched.
(osmium's pool is capped at 10 connections, so it won't strain a shared Postgres.)

## Step 1 — Create the database (once)

Run [`postgres-setup.sql`](postgres-setup.sql) as a Postgres **superuser**. Set
`REPLACE_DB_USER` to the `POSTGRES_USER` value from your `db-secret` (or drop the
`OWNER` clause if that's already the `postgres` superuser).

```bash
psql "postgres://<superuser>:<pw>@<postgres-host>:5432/postgres" -f postgres-setup.sql
```

It creates the `osmium` database and pre-installs the `pgcrypto` + `citext`
extensions (osmium's migrations need them). osmium builds all its own tables on
first startup, so there's no migration job to run.

## Step 2 — Create the `osmium-config` ConfigMap

All non-secret env lives here (Portainer → **Configurations → Create ConfigMap**,
namespace `default`, name `osmium-config`). Fastest path: **Create from manifest**
with a filled-in copy of [`osmium-config.example.yaml`](osmium-config.example.yaml),
or add the keys via the form editor.

Fill in these `REPLACE_` values (everything else has a working default):

| Key | Value |
| --- | --- |
| `REPLACE_OSMIUM_HOST` (in `VATSIM_REDIRECT_URI`, `DISCORD_REDIRECT_URI`, `CDN_BASE_URL`) | osmium's public hostname, e.g. `api.dev.vzdc.org` |
| `CORS_ALLOWED_ORIGINS` (`REPLACE_WEBSITE_ORIGINS`) | the website origin(s) allowed to call osmium — comma-separated list, e.g. `https://dev.vzdc.org` |
| `EMAIL_UNSUBSCRIBE_BASE_URL` (`REPLACE_WEBSITE_ORIGIN`) | the website origin used in email links, e.g. `https://dev.vzdc.org` |
| `VATSIM_CLIENT_ID` / `DISCORD_CLIENT_ID` | OAuth client ids (public) |
| `OSMIUM_SERVER_ADMIN_CID` | comma-separated VATSIM CID(s) to bootstrap as `SERVER_ADMIN` |
| `BOT_API_BASE_URL` | the Discord bot's in-cluster URL |

## Step 3 — Create the `osmium-secret` Secret

The DB and SES creds come from the shared secrets, so this Secret only holds
osmium's own app secrets. Portainer → **Configurations → Create secret**,
namespace `default`, name `osmium-secret`, type Opaque. Add each key (plaintext;
Portainer base64-encodes it), or **Create from manifest** with a filled-in
[`osmium-secret.example.yaml`](osmium-secret.example.yaml).

| Key | Value |
| --- | --- |
| `VATSIM_CLIENT_SECRET` | VATSIM OAuth client secret |
| `VATUSA_API_KEY` | VATUSA API key |
| `FILE_SIGNING_SECRET` | `openssl rand -hex 32` |
| `FILE_ENCRYPTION_KEY_HEX` | `openssl rand -hex 32` — optional, `""` to disable at-rest file encryption |
| `DISCORD_CLIENT_SECRET` | Discord app client secret |
| `EMAIL_UNSUBSCRIBE_SECRET` | `openssl rand -hex 32` |
| `ATC_BOOKING_TOKEN` | VATSIM ATC-booking service token |
| `TURNSTILE_SECRET_KEY` | Cloudflare Turnstile secret |
| `BOT_API_SECRET_KEY` | must equal the bot's `BOT_API_SHARED_KEY` |

> **Reusing `email-secret`:** osmium expects the standard AWS SDK env names
> `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` (and optionally `AWS_REGION`). If
> your `email-secret` uses those exact keys, SES works as-is. If it uses
> different names, either add those keys to `email-secret` or drop the
> `email-secret` reference from `osmium.yaml` and add the AWS keys to
> `osmium-secret`. `AWS_REGION` is set in the ConfigMap (`us-east-1`) unless
> `email-secret` overrides it.

Create these **before** deploying — the pod won't start referencing a ConfigMap
or Secret that doesn't exist yet.

## Step 4 — Get osmium a public hostname + register the OAuth callbacks

osmium needs a public HTTPS hostname routed to its Service. Since the DO load
balancer is managed outside your access, hand whoever runs it:

- the hostname you want (e.g. `api.dev.vzdc.org`) — the value you put in
  `osmium-config` for `REPLACE_OSMIUM_HOST`
- **Service:** `osmium`, namespace `default`, **port 3000** (same wiring as the
  `website` Service)
- a note that osmium serves file uploads up to ~25 MiB, so the LB/proxy
  body-size limit must allow that

Once the hostname exists, register these exact HTTPS URLs (osmium owns both
OAuth callbacks):

- **VATSIM Connect** → Redirect URI: `https://<osmium-host>/api/v1/auth/vatsim/callback`
- **Discord** → OAuth2 Redirects: `https://<osmium-host>/api/v1/me/discord/link/callback`

If login or Discord linking fails with a redirect/`invalid_client` error, this
registration (or the client id/secret) is the usual cause.

## Step 5 — Deploy in Portainer

With `osmium-config` and `osmium-secret` created (Steps 2–3), Portainer →
**Applications → Add with a manifest** → paste [`osmium.yaml`](osmium.yaml) →
deploy. The Deployment's `envFrom` binds `osmium-config`, `db-secret`,
`email-secret`, and `osmium-secret` by name.

## Step 6 — Verify

```bash
kubectl -n default get pods -l app=osmium
kubectl -n default logs deploy/osmium         # "running startup migrations" → "starting osmium api"
kubectl -n default port-forward deploy/osmium 3000:3000 &
curl -s http://127.0.0.1:3000/health          # verify the pod itself
curl -s https://<osmium-host>/health          # verify once the LB route is wired
```

Log in as one of `OSMIUM_SERVER_ADMIN_CID`; `GET /api/v1/me` should return
`role: "SERVER_ADMIN"`.

## Notes

- **Persistent storage:** the PVC for uploaded files binds automatically to
  DOKS's default `do-block-storage` StorageClass — no action needed. (The website
  is stateless, so it has no PVC; osmium does because it stores files.)
- **Exposure / TLS:** handled by the DigitalOcean load balancer outside this
  namespace — not part of this manifest (see Step 4). osmium's Service is
  `ClusterIP` on port 3000, ready for the LB to route to, same as the website.
- **`TRUSTED_PROXY_COUNT`:** set to the number of proxy hops in front of osmium
  (DO LB → any ingress controller → osmium). Default here is `1`; if osmium logs
  the LB/controller IP as the client instead of real client IPs, bump it. This
  only affects rate-limiting and IP logging, not availability.

## Upgrades

Pinned to `ghcr.io/vzdc-artcc/osmium:latest` (master) with `imagePullPolicy:
Always`. Roll out a new image with:

```bash
kubectl -n default rollout restart deploy/osmium
```

For reproducible deploys, pin `:latest-<sha>` instead of the moving `:latest`.
