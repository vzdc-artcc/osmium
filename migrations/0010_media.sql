create table if not exists media.file_categories (
    id text primary key default gen_random_uuid()::text,
    name text not null,
    order_num integer not null default 0,
    key text unique,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists media.file_assets (
    id text primary key default gen_random_uuid()::text,
    alias text unique,
    name text,
    filename text not null,
    order_num integer not null default 0,
    description text,
    category_id text references media.file_categories(id) on delete set null,
    key text,
    content_type text not null,
    size_bytes bigint not null check (size_bytes >= 0),
    checksum_sha256 text,
    etag text not null,
    storage_provider text not null default 'local_fs' check (storage_provider in ('local_fs', 's3')),
    storage_key text not null unique,
    is_public boolean not null default false,
    visibility text not null default 'private' check (visibility in ('public', 'authenticated', 'restricted', 'private')),
    highlight_color text not null default 'inherit' check (highlight_color in ('inherit', 'red', 'lightskyblue', 'orange', 'darkcyan', 'lightgreen', 'salmon', 'mediumpurple')),
    owner_user_id text references identity.users(id) on delete set null,
    uploaded_by text not null references identity.users(id) on delete cascade,
    viewer_roles text[] not null default '{}'::text[],
    domain_type text,
    domain_id text,
    is_encrypted boolean not null default false,
    retention_class text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz
);

create index if not exists idx_media_file_assets_uploaded_by on media.file_assets(uploaded_by);
create index if not exists idx_media_file_assets_owner on media.file_assets(owner_user_id);
create index if not exists idx_media_file_assets_visibility on media.file_assets(visibility);
create index if not exists idx_media_file_assets_domain on media.file_assets(domain_type, domain_id);
create index if not exists idx_media_file_assets_viewer_roles on media.file_assets using gin(viewer_roles);

create table if not exists media.file_asset_allowed_users (
    file_id text not null references media.file_assets(id) on delete cascade,
    user_id text not null references identity.users(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (file_id, user_id)
);

create table if not exists media.file_asset_allowed_roles (
    file_id text not null references media.file_assets(id) on delete cascade,
    role_name text not null references access.roles(name) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (file_id, role_name)
);

create table if not exists media.file_asset_versions (
    id text primary key default gen_random_uuid()::text,
    file_id text not null references media.file_assets(id) on delete cascade,
    version_number integer not null,
    storage_key text not null unique,
    checksum_sha256 text not null,
    size_bytes bigint not null check (size_bytes >= 0),
    uploaded_by_actor_id text references access.actors(id) on delete set null,
    created_at timestamptz not null default now(),
    unique (file_id, version_number)
);

create table if not exists media.file_audit_logs (
    id text primary key default gen_random_uuid()::text,
    action text not null check (action in ('upload', 'replace', 'download', 'signed_url_issued', 'delete')),
    file_id text references media.file_assets(id) on delete cascade,
    actor_user_id text references identity.users(id) on delete set null,
    actor_service_account_id text references access.service_accounts(id) on delete set null,
    ip_address inet,
    outcome text not null,
    details jsonb,
    created_at timestamptz not null default now()
);

create index if not exists idx_media_file_audit_logs_file_created on media.file_audit_logs(file_id, created_at desc);

create table if not exists media.signed_url_issues (
    id text primary key default gen_random_uuid()::text,
    file_id text not null references media.file_assets(id) on delete cascade,
    issued_to_type text not null check (issued_to_type in ('user', 'service_account')),
    issued_to_user_id text references identity.users(id) on delete set null,
    issued_to_service_account_id text references access.service_accounts(id) on delete set null,
    purpose text not null,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create trigger trg_media_file_categories_updated_at
before update on media.file_categories
for each row execute function platform.touch_updated_at();

create trigger trg_media_file_assets_updated_at
before update on media.file_assets
for each row execute function platform.touch_updated_at();
