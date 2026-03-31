-- Phase 4: Stats foundation (vNAS sync support)

create table if not exists sync_times (
    id text primary key,
    stats timestamptz,
    updated_at timestamptz not null default now()
);

create table if not exists statistics_prefixes (
    id text primary key,
    prefixes text[] not null default '{}',
    updated_at timestamptz not null default now()
);

create table if not exists controller_logs (
    id text primary key,
    user_id text not null unique references users(id) on delete cascade
);

create table if not exists controller_log_months (
    id text primary key,
    log_id text not null references controller_logs(id) on delete cascade,
    month integer not null,
    year integer not null,
    delivery_hours double precision not null default 0,
    ground_hours double precision not null default 0,
    tower_hours double precision not null default 0,
    approach_hours double precision not null default 0,
    center_hours double precision not null default 0,
    unique (log_id, month, year)
);

create table if not exists controller_positions (
    id text primary key,
    log_id text not null references controller_logs(id) on delete cascade,
    position text not null,
    facility integer,
    start timestamptz not null,
    "end" timestamptz,
    active boolean not null default true
);

create index if not exists idx_controller_positions_log_id_active
    on controller_positions(log_id, active);

-- Minimal LOA table used by stats sync to clear active approved windows.
create table if not exists loas (
    id text primary key,
    user_id text not null references users(id) on delete cascade,
    start timestamptz not null,
    "end" timestamptz not null,
    status text not null
);

create index if not exists idx_loas_user_id_time
    on loas(user_id, start, "end", status);

insert into sync_times (id, stats)
values ('default', null)
on conflict (id) do nothing;

insert into statistics_prefixes (id, prefixes)
values ('default', array['ZDC'])
on conflict (id) do nothing;

