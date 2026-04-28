create table if not exists events.events (
    id text primary key default gen_random_uuid()::text,
    title text not null,
    type text not null default 'STANDARD' check (type in ('STANDARD', 'HOME', 'SUPPORT_REQUIRED', 'SUPPORT_OPTIONAL', 'GROUP_FLIGHT', 'FRIDAY_NIGHT_OPERATIONS', 'SATURDAY_NIGHT_OPERATIONS', 'TRAINING')),
    host text,
    description text,
    status text not null default 'SCHEDULED' check (status in ('DRAFT', 'SCHEDULED', 'PUBLISHED', 'ARCHIVED', 'CANCELLED')),
    published boolean not null default false,
    banner_asset_id text,
    hidden boolean not null default false,
    positions_locked boolean not null default false,
    manual_positions_open boolean not null default false,
    archived_at timestamptz,
    starts_at timestamptz not null,
    ends_at timestamptz not null,
    featured_fields text[] not null default '{}'::text[],
    preset_positions text[] not null default '{}'::text[],
    enable_buffer_times boolean not null default false,
    featured_field_configs jsonb,
    tmis text,
    ops_free_text text,
    ops_plan_published boolean not null default false,
    ops_planner_id text references identity.users(id) on delete set null,
    created_by text not null references identity.users(id) on delete restrict,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    check (ends_at >= starts_at)
);

create index if not exists idx_events_starts_at on events.events(starts_at);
create index if not exists idx_events_status on events.events(status);

create table if not exists events.event_positions (
    id text primary key default gen_random_uuid()::text,
    event_id text not null references events.events(id) on delete cascade,
    callsign text not null,
    user_id text references identity.users(id) on delete cascade,
    requested_slot integer,
    assigned_slot integer,
    requested_position text,
    requested_secondary_position text not null default 'UNKNOWN',
    notes text,
    requested_start_time timestamptz,
    requested_end_time timestamptz,
    final_start_time timestamptz,
    final_end_time timestamptz,
    final_position text,
    final_notes text,
    controlling_category text,
    is_instructor boolean not null default false,
    is_solo boolean not null default false,
    is_ots boolean not null default false,
    is_tmu boolean not null default false,
    is_cic boolean not null default false,
    published boolean not null default false,
    status text not null default 'OPEN' check (status in ('OPEN', 'REQUESTED', 'ASSIGNED', 'PUBLISHED', 'CANCELLED')),
    submitted_at timestamptz not null default now(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (event_id, callsign)
);

create unique index if not exists idx_event_positions_event_user_unique
on events.event_positions(event_id, user_id)
where user_id is not null;

create table if not exists events.event_position_presets (
    id text primary key default gen_random_uuid()::text,
    name text not null unique,
    positions text[] not null default '{}'::text[],
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists events.event_tmis (
    id text primary key default gen_random_uuid()::text,
    event_id text not null references events.events(id) on delete cascade,
    tmi_type text not null,
    start_time timestamptz not null,
    notes text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists events.ops_plan_files (
    id text primary key default gen_random_uuid()::text,
    event_id text not null references events.events(id) on delete cascade,
    asset_id text,
    filename text not null,
    url text,
    file_type text,
    uploaded_by text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger trg_events_updated_at
before update on events.events
for each row execute function platform.touch_updated_at();

create trigger trg_event_positions_updated_at
before update on events.event_positions
for each row execute function platform.touch_updated_at();

create trigger trg_event_position_presets_updated_at
before update on events.event_position_presets
for each row execute function platform.touch_updated_at();

create trigger trg_event_tmis_updated_at
before update on events.event_tmis
for each row execute function platform.touch_updated_at();

create trigger trg_ops_plan_files_updated_at
before update on events.ops_plan_files
for each row execute function platform.touch_updated_at();
