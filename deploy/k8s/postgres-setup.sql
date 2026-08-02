-- ============================================================================
-- next-osmium — Postgres database setup / reset
-- ============================================================================
-- Run as a Postgres SUPERUSER (or the DB owner) against your Postgres instance.
-- Connect to a DIFFERENT database than next-osmium (e.g. `postgres` on a normal
-- server, or `defaultdb` on DigitalOcean managed Postgres) — you cannot drop or
-- create the database you're currently connected to.
--
-- The database name is `next-osmium` (hyphen ⇒ it must be double-quoted in DDL).
-- osmium creates all of its own schemas (identity, access, org, training, events,
-- feedback, media, routes, stats, integration, platform, web, email) on first
-- startup via its embedded migrations — nothing else to create here.
-- ============================================================================

-- ── OPTIONAL RESET — uncomment to DROP the existing database first ───────────
-- ⚠️  DESTRUCTIVE: deletes ALL next-osmium data. Scale the app to 0 first
--     (`kubectl scale deployment/next-osmium --replicas=0`) so it releases its
--     connections, then run these two lines:
--
-- SELECT pg_terminate_backend(pid) FROM pg_stat_activity
--  WHERE datname = 'next-osmium' AND pid <> pg_backend_pid();
-- DROP DATABASE IF EXISTS "next-osmium";

-- 1) Create the database, owned by the role in your DATABASE_URL (here "vzdc";
--    drop the OWNER clause if that role is already the superuser). CREATE
--    DATABASE cannot run in a transaction/DO block; if it already exists Postgres
--    errors — safe to ignore.
CREATE DATABASE "next-osmium" OWNER "vzdc";

-- 2) Install the required extensions (per-database; needs a superuser). Migration
--    0001 also runs `CREATE EXTENSION IF NOT EXISTS`, but pre-creating them here
--    means the app role never needs elevated rights. `\connect` switches DB in
--    psql; in a GUI, open a new connection to `next-osmium` and run these two.
\connect "next-osmium"

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;

-- ----------------------------------------------------------------------------
-- Verify (optional):
--   \l next-osmium   -- shows the DB + owner
--   \dx              -- (while connected to next-osmium) lists pgcrypto + citext
-- ----------------------------------------------------------------------------
--
-- After this, (re)start next-osmium — with RUN_MIGRATIONS_ON_STARTUP=true it
-- applies every migration to the fresh database on boot:
--   kubectl scale deployment/next-osmium --replicas=1
--   kubectl logs -f deploy/next-osmium      # migrations → "starting osmium api"
--
-- Dedicated least-privilege login instead of reusing an existing role? Point
-- next-osmium-secret's DATABASE_URL at it:
--   CREATE ROLE osmium WITH LOGIN PASSWORD '...';
--   CREATE DATABASE "next-osmium" OWNER osmium;
--   \connect "next-osmium"
--   CREATE EXTENSION IF NOT EXISTS pgcrypto;
--   CREATE EXTENSION IF NOT EXISTS citext;
-- ============================================================================
