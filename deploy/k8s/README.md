# Deploying next-osmium to Kubernetes (via Portainer)

Runs osmium's **next** environment in the **`default`** namespace, grouped under
the Portainer **`next`** stack alongside `next-website` and `next-discord-bot`.

| File | Purpose |
| --- | --- |
| `postgres-setup.sql` | Create (or reset) the `next-osmium` database + extensions. |
| `next-osmium-all-in-one.yaml` | The whole deployment in one manifest — ConfigMap + Secret + PVC + Deployment + Service. Fill it in, import once. **Gitignored** (holds real secrets). |

## How config works (read this)

The Deployment carries **no inline env** — it reads everything from
`next-osmium-config` (ConfigMap) and `next-osmium-secret` (Secret) via `envFrom`.
Those live objects are the single source of truth:

- Edit a value in Portainer's UI → **restart the pod** (env is read at pod start)
  → it takes effect. Nothing in the Deployment can override your UI edit.
- The manifest is for **initial creation**. Re-importing the *whole* file rewrites
  the ConfigMap/Secret back to the file's values (clobbering UI edits). To push
  only a Deployment change later, delete the ConfigMap + Secret docs from your
  copy and import just the rest.

Self-contained: `DATABASE_URL` and the AWS SES creds live in `next-osmium-secret`
(no shared `db-secret`/`email-secret`), so the entire config is UI-editable.

## Step 1 — Create the database (once)

Run [`postgres-setup.sql`](postgres-setup.sql) as a Postgres **superuser**. It
creates the `next-osmium` database and pre-installs `pgcrypto` + `citext`
(migrations need them). osmium builds its own tables on first startup — no
migration job. It also has an optional (commented) **reset** block that drops and
recreates the database — see "Reset / re-run migrations" below.

```bash
psql "postgres://<superuser>:<pw>@<postgres-host>:5432/postgres" -f postgres-setup.sql
```

## Step 2 — Fill in the manifest

Edit [`next-osmium-all-in-one.yaml`](next-osmium-all-in-one.yaml) and replace every
`REPLACE`:

- **`DATABASE_URL`** — full connection string ending in `/next-osmium`; point the
  host at the in-cluster Postgres Service (or your managed DB host).
- **`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`** — SES creds (only needed while
  `EMAIL_ENABLED=true`).
- **`ATC_BOOKING_TOKEN`, `TURNSTILE_SECRET_KEY`, `BOT_API_SECRET_KEY`** — app
  secrets. `BOT_API_SECRET_KEY` must equal the bot's `BOT_API_SHARED_KEY`.

The public host is preset to `next-api.vzdc.org` in the VATSIM/Discord callbacks
and `CDN_BASE_URL` — change all three if different.

## Step 3 — Register the OAuth callbacks

osmium owns both callbacks; register these exact HTTPS URLs at the public host:

- **VATSIM Connect** → `https://<osmium-host>/api/v1/auth/vatsim/callback`
- **Discord** → `https://<osmium-host>/api/v1/me/discord/link/callback`

## Step 4 — Deploy in Portainer

Portainer → **Applications → Add with a manifest** → paste the filled-in
`next-osmium-all-in-one.yaml`. Name the deployment/stack **`next`** so it joins the
`next` stack (the `io.portainer.kubernetes.application.stack: next` label
reinforces this). Kubernetes routes each object by its `kind`.

The DO load balancer (managed outside this namespace) routes osmium's public host
→ the **`next-osmium`** Service on **port 3900**; make sure its body-size limit
allows ~25 MiB uploads. No Ingress is included.

## Step 5 — Verify

```bash
kubectl -n default get pods -l app=next-osmium
kubectl -n default logs deploy/next-osmium        # migrations → "starting osmium api"
kubectl -n default port-forward deploy/next-osmium 3900:3900 &
curl -s http://127.0.0.1:3900/health
```

Log in as one of `OSMIUM_SERVER_ADMIN_CID`; `GET /api/v1/me` should return
`role: "SERVER_ADMIN"`.

## Reset / re-run migrations (wipe the DB clean)

Because osmium applies migrations on startup, a full reset is just: drop the
database, recreate it empty, and restart the pod.

1. **Scale osmium to 0** so it releases its DB connections (you can't drop a
   database with active connections):
   ```bash
   kubectl -n default scale deployment/next-osmium --replicas=0
   ```
2. **Drop + recreate** the `next-osmium` database as a superuser. Connect to a
   *different* database (`postgres`, or `defaultdb` on DO managed Postgres) and run
   the reset — uncomment the DROP block at the top of
   [`postgres-setup.sql`](postgres-setup.sql), then run the whole file:
   ```bash
   psql "postgres://<superuser>:<pw>@<host>:5432/postgres" -f postgres-setup.sql
   ```
   Or inline, without the file:
   ```sql
   SELECT pg_terminate_backend(pid) FROM pg_stat_activity
    WHERE datname = 'next-osmium' AND pid <> pg_backend_pid();
   DROP DATABASE IF EXISTS "next-osmium";
   CREATE DATABASE "next-osmium" OWNER "vzdc";
   \connect "next-osmium"
   CREATE EXTENSION IF NOT EXISTS pgcrypto;
   CREATE EXTENSION IF NOT EXISTS citext;
   ```
3. **Scale back up** — the pod re-applies every migration to the fresh DB:
   ```bash
   kubectl -n default scale deployment/next-osmium --replicas=1
   kubectl -n default logs -f deploy/next-osmium   # migrations → "starting osmium api"
   ```

No psql client handy? Run one as a throwaway pod in-cluster:
```bash
kubectl -n default run pg-reset --rm -it --restart=Never --image=postgres:16 -- \
  psql "postgres://<superuser>:<pw>@<host>:5432/postgres" \
  -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='next-osmium' AND pid<>pg_backend_pid();" \
  -c "DROP DATABASE IF EXISTS \"next-osmium\";" \
  -c "CREATE DATABASE \"next-osmium\" OWNER \"vzdc\";"
```
(then create the extensions with a second `psql ... /next-osmium -c 'CREATE EXTENSION ...'`).

After a reset, re-run the [`next-db-migrator`](next-db-migrator-job.yaml) Job to
repopulate from the website DB.

## Cross-service wiring (the `next` stack)

- `next-website` → osmium: `OSMIUM_INTERNAL_API_URL=http://next-osmium:3900`.
- `next-discord-bot` → osmium: `OSMIUM_BASE_URL=http://next-osmium:3900`.
- osmium → bot: `BOT_API_BASE_URL=http://next-discord-bot:3010`, and the shared
  key `BOT_API_SECRET_KEY` (here) == `BOT_API_SHARED_KEY` (bot).

If you rename the `next-osmium` Service, update those two client URLs **and** the
load-balancer backend that fronts `next-api.vzdc.org`.

## Upgrades

Pinned to `ghcr.io/vzdc-artcc/osmium:latest` with `imagePullPolicy: Always`:

```bash
kubectl -n default rollout restart deploy/next-osmium
```
