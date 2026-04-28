create table if not exists web.pages (
    id text primary key default gen_random_uuid()::text,
    slug text not null unique,
    title text not null,
    summary text,
    body jsonb not null,
    status text not null default 'draft',
    published_at timestamptz,
    created_by_user_id text references identity.users(id) on delete set null,
    updated_by_user_id text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists web.announcements (
    id text primary key default gen_random_uuid()::text,
    title text not null,
    summary text,
    body jsonb not null,
    status text not null default 'draft',
    published_at timestamptz,
    expires_at timestamptz,
    created_by_user_id text references identity.users(id) on delete set null,
    updated_by_user_id text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists web.versions (
    id text primary key default gen_random_uuid()::text,
    version_number text not null unique,
    created_at timestamptz not null default now()
);

create table if not exists web.version_change_details (
    id text primary key default gen_random_uuid()::text,
    version_id text not null references web.versions(id) on delete cascade,
    detail text not null,
    created_at timestamptz not null default now()
);

create table if not exists web.change_broadcasts (
    id text primary key default gen_random_uuid()::text,
    title text not null,
    description text not null,
    file_id text references media.file_assets(id) on delete set null,
    exempt_staff boolean not null default false,
    timestamp timestamptz not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists web.change_broadcast_user_state (
    broadcast_id text not null references web.change_broadcasts(id) on delete cascade,
    user_id text not null references identity.users(id) on delete cascade,
    seen_at timestamptz,
    agreed_at timestamptz,
    primary key (broadcast_id, user_id)
);

create table if not exists web.site_settings (
    key text primary key,
    value jsonb not null,
    updated_by_user_id text references identity.users(id) on delete set null,
    updated_at timestamptz not null default now()
);

create trigger trg_web_pages_updated_at
before update on web.pages
for each row execute function platform.touch_updated_at();

create trigger trg_web_announcements_updated_at
before update on web.announcements
for each row execute function platform.touch_updated_at();

create trigger trg_change_broadcasts_updated_at
before update on web.change_broadcasts
for each row execute function platform.touch_updated_at();
