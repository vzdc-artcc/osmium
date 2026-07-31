-- ============================================================================
-- Osmium — one-time Postgres setup
-- ============================================================================
-- Run this ONCE, as a Postgres SUPERUSER, against your existing Postgres
-- instance BEFORE deploying osmium.
--
-- osmium reuses the shared `db-secret` credentials (the SAME POSTGRES_USER the
-- website connects as), so all we need here is a NEW, SEPARATE `osmium`
-- database + the extensions osmium's migrations require. This is purely
-- additive — the website's database is never touched, so it cannot break
-- existing data. osmium's connection pool is also hard-capped at 10 connections,
-- so it won't exhaust a shared Postgres.
-- ============================================================================

-- 1) Create the osmium database, owned by the role osmium connects as (the
--    POSTGRES_USER value inside db-secret). Replace REPLACE_DB_USER with that
--    username.
--
--    If that user is already the `postgres` superuser (common), you can drop the
--    OWNER clause entirely: CREATE DATABASE osmium;
--
--    (CREATE DATABASE cannot run inside a transaction/DO block. If the database
--    already exists Postgres will error — that is safe to ignore; just skip it.)
CREATE DATABASE osmium OWNER "vzdc";

-- 2) Install the required extensions (per-database; needs a superuser). osmium's
--    first migration also runs `CREATE EXTENSION IF NOT EXISTS`, but that path
--    requires superuser only when the extension is missing — pre-creating them
--    here means the app role never needs elevated rights.
--
--    In psql, `\connect` switches databases. In a GUI, open a new connection to
--    the `osmium` database and run the two CREATE EXTENSION lines.
\connect osmium

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;

-- ----------------------------------------------------------------------------
-- Verify (optional):
--   \l osmium        -- shows the DB, owned by REPLACE_DB_USER
--   \dx              -- (while connected to osmium) lists pgcrypto + citext
-- ----------------------------------------------------------------------------
--
-- osmium creates all of its own schemas (identity, access, org, training,
-- events, feedback, media, routes, stats, integration, platform, web, email)
-- automatically on first startup via its embedded migrations — nothing else to
-- run here.
--
-- Prefer a dedicated, least-privilege login instead of reusing the shared user?
-- Create a separate role and database, and put a full DATABASE_URL in
-- osmium-secret instead of building it from db-secret:
--   CREATE ROLE osmium WITH LOGIN PASSWORD '...';
--   CREATE DATABASE osmium OWNER osmium;
--   \connect osmium
--   CREATE EXTENSION IF NOT EXISTS pgcrypto;
--   CREATE EXTENSION IF NOT EXISTS citext;
--   GRANT USAGE ON SCHEMA public TO osmium;
-- ============================================================================
