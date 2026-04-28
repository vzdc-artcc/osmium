create table if not exists integration.discord_configs (
    id text primary key default gen_random_uuid()::text,
    name text not null,
    guild_id text unique,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists integration.discord_channels (
    id text primary key default gen_random_uuid()::text,
    discord_config_id text not null references integration.discord_configs(id) on delete cascade,
    name text not null,
    channel_id text not null unique,
    created_at timestamptz not null default now(),
    unique (discord_config_id, name)
);

create table if not exists integration.discord_roles (
    id text primary key default gen_random_uuid()::text,
    discord_config_id text not null references integration.discord_configs(id) on delete cascade,
    name text not null,
    role_id text not null unique,
    created_at timestamptz not null default now(),
    unique (discord_config_id, name)
);

create table if not exists integration.discord_categories (
    id text primary key default gen_random_uuid()::text,
    discord_config_id text not null references integration.discord_configs(id) on delete cascade,
    name text not null,
    category_id text not null unique,
    created_at timestamptz not null default now(),
    unique (discord_config_id, name)
);

create table if not exists integration.webhook_deliveries (
    id text primary key default gen_random_uuid()::text,
    source text not null,
    event_type text not null,
    external_id text,
    received_at timestamptz not null,
    processed_at timestamptz,
    status text not null,
    payload jsonb not null,
    error text,
    created_at timestamptz not null default now()
);

create table if not exists integration.outbound_jobs (
    id text primary key default gen_random_uuid()::text,
    job_type text not null,
    subject_type text,
    subject_id text,
    status text not null,
    attempt_count integer not null default 0,
    last_attempt_at timestamptz,
    next_attempt_at timestamptz,
    payload jsonb not null,
    error text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists integration.external_sync_mappings (
    id text primary key default gen_random_uuid()::text,
    system_code text not null,
    entity_type text not null,
    local_id text not null,
    external_id text not null,
    metadata jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (system_code, entity_type, local_id),
    unique (system_code, entity_type, external_id)
);

create trigger trg_integration_discord_configs_updated_at
before update on integration.discord_configs
for each row execute function platform.touch_updated_at();

create trigger trg_integration_outbound_jobs_updated_at
before update on integration.outbound_jobs
for each row execute function platform.touch_updated_at();

create trigger trg_integration_sync_mappings_updated_at
before update on integration.external_sync_mappings
for each row execute function platform.touch_updated_at();
