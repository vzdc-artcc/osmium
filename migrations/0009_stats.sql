create table if not exists stats.sync_times (
    id text primary key,
    roster timestamptz,
    stats timestamptz,
    loas timestamptz,
    events timestamptz,
    solo_cert timestamptz,
    appointments timestamptz,
    updated_at timestamptz not null default now()
);

create table if not exists stats.statistics_prefixes (
    id text primary key,
    prefixes text[] not null default '{}'::text[],
    updated_at timestamptz not null default now()
);

create table if not exists stats.controller_logs (
    id text primary key default gen_random_uuid()::text,
    user_id text not null unique references identity.users(id) on delete cascade,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists stats.controller_log_months (
    id text primary key default gen_random_uuid()::text,
    log_id text not null references stats.controller_logs(id) on delete cascade,
    month integer not null,
    year integer not null,
    delivery_hours double precision not null default 0,
    ground_hours double precision not null default 0,
    tower_hours double precision not null default 0,
    approach_hours double precision not null default 0,
    center_hours double precision not null default 0,
    created_at timestamptz not null default now(),
    unique (log_id, month, year)
);

create table if not exists stats.controller_positions (
    id text primary key default gen_random_uuid()::text,
    log_id text not null references stats.controller_logs(id) on delete cascade,
    position text not null,
    facility integer,
    start timestamptz not null,
    "end" timestamptz,
    active boolean not null default true,
    created_at timestamptz not null default now()
);

create index if not exists idx_stats_controller_positions_log_active
on stats.controller_positions(log_id, active);

create trigger trg_sync_times_updated_at
before update on stats.sync_times
for each row execute function platform.touch_updated_at();

create trigger trg_statistics_prefixes_updated_at
before update on stats.statistics_prefixes
for each row execute function platform.touch_updated_at();

create trigger trg_controller_logs_updated_at
before update on stats.controller_logs
for each row execute function platform.touch_updated_at();
