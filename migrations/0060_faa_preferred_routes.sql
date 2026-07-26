-- Self-hosted FAA preferred-routes data (spec 012, Worker B "New Data Domains").
--
-- Replaces the old request-time proxy of a third-party aviation API (the removed
-- app/prd/ page) with a locally-owned copy of the FAA's own National Flight Data
-- Center (NFDC) preferred IFR routes, ingested on a 28-day AIRAC cadence from the
-- NASR subscription's `PFR_RMT_FMT.csv` (see src/jobs/faa_preferred_routes.rs).
--
-- One row per (origin, destination, route_type, sequence) — the natural key of a
-- preferred route in the FAA data (ORIGIN_ID, DSTN_ID, PFR_TYPE_CODE, ROUTE_NO).
-- Columns mirror the fields the old PRD UI displayed: origin, destination, the
-- assembled route string, hours, type, area, altitude, aircraft, direction/flow,
-- sequence, and the departure/arrival ARTCC boundaries.
create schema if not exists routes;

create table if not exists routes.preferred_routes (
    id text primary key default gen_random_uuid()::text,
    origin text not null,
    destination text not null,
    route_type text not null,
    sequence integer not null,
    route_string text not null default '',
    hours text not null default '',
    area text not null default '',
    altitude text not null default '',
    aircraft text not null default '',
    direction text not null default '',
    departure_artcc text not null default '',
    arrival_artcc text not null default '',
    -- The 28-day NASR cycle effective date this row was ingested from
    -- (e.g. '2026-07-09'), so stale-cycle detection and diffing are possible.
    effective_date date,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (origin, destination, route_type, sequence)
);

-- Search is by origin and/or destination airport identifier; both are queried
-- case-insensitively (identifiers are stored upper-cased on ingest).
create index if not exists idx_preferred_routes_origin on routes.preferred_routes(origin);
create index if not exists idx_preferred_routes_destination on routes.preferred_routes(destination);

create trigger trg_preferred_routes_updated_at
before update on routes.preferred_routes
for each row execute function platform.touch_updated_at();
