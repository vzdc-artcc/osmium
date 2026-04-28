create table if not exists stats.controller_feed_state (
    environment text primary key check (environment in ('live', 'sweatbox1', 'sweatbox2')),
    endpoint_url text not null,
    last_polled_at timestamptz,
    last_source_updated_at timestamptz,
    last_success_at timestamptz,
    last_error text,
    last_snapshot_count integer not null default 0,
    last_zdc_count integer not null default 0
);

create table if not exists stats.controller_sessions (
    id text primary key default gen_random_uuid()::text,
    environment text not null check (environment in ('live', 'sweatbox1', 'sweatbox2')),
    artcc_id text not null,
    cid bigint not null,
    user_id text references identity.users(id) on delete set null,
    real_name text,
    role text,
    user_rating text,
    requested_rating text,
    login_at timestamptz not null,
    logout_at timestamptz,
    online_seconds bigint,
    primary_facility_id text,
    primary_position_id text,
    source_login_time_raw text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (environment, cid, login_at)
);

create table if not exists stats.controller_activations (
    id text primary key default gen_random_uuid()::text,
    session_id text not null references stats.controller_sessions(id) on delete cascade,
    environment text not null check (environment in ('live', 'sweatbox1', 'sweatbox2')),
    cid bigint not null,
    position_id text not null,
    facility_id text,
    facility_name text not null,
    position_name text not null,
    position_type text not null,
    radio_name text,
    default_callsign text,
    frequency bigint,
    is_primary boolean not null default false,
    started_at timestamptz not null,
    ended_at timestamptz,
    active_seconds bigint,
    created_at timestamptz not null default now()
);

create unique index if not exists idx_stats_controller_activations_open
on stats.controller_activations(session_id, position_id)
where ended_at is null;

create index if not exists idx_stats_controller_activations_environment_cid
on stats.controller_activations(environment, cid);

create table if not exists stats.controller_monthly_rollups (
    environment text not null check (environment in ('live', 'sweatbox1', 'sweatbox2')),
    cid bigint not null,
    year integer not null,
    month integer not null check (month between 0 and 11),
    online_seconds bigint not null default 0,
    delivery_seconds bigint not null default 0,
    ground_seconds bigint not null default 0,
    tower_seconds bigint not null default 0,
    tracon_seconds bigint not null default 0,
    center_seconds bigint not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    primary key (environment, cid, year, month)
);

create table if not exists stats.controller_events (
    id bigserial primary key,
    environment text not null check (environment in ('live', 'sweatbox1', 'sweatbox2')),
    event_type text not null,
    cid bigint not null,
    user_id text,
    session_id text,
    activation_id text,
    occurred_at timestamptz not null,
    payload jsonb not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_stats_controller_sessions_environment_open
on stats.controller_sessions(environment, cid)
where logout_at is null;

create index if not exists idx_stats_controller_monthly_rollups_environment_year_month
on stats.controller_monthly_rollups(environment, year, month);

create index if not exists idx_stats_controller_events_environment_id
on stats.controller_events(environment, id);

create trigger trg_stats_controller_sessions_updated_at
before update on stats.controller_sessions
for each row execute function platform.touch_updated_at();

create trigger trg_stats_controller_monthly_rollups_updated_at
before update on stats.controller_monthly_rollups
for each row execute function platform.touch_updated_at();
