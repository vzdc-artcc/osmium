-- Phase 6: File CDN workflow

create table if not exists file_assets (
    id text primary key,
    filename text not null,
    content_type text not null,
    size_bytes bigint not null check (size_bytes >= 0),
    etag text not null,
    storage_key text not null unique,
    is_public boolean not null default true,
    uploaded_by text not null references users(id) on delete cascade,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_file_assets_uploaded_by on file_assets(uploaded_by);
create index if not exists idx_file_assets_created_at on file_assets(created_at desc);
create index if not exists idx_file_assets_is_public on file_assets(is_public);

insert into permissions (name)
values
    ('manage_files')
on conflict (name) do nothing;

insert into role_permissions (role_name, permission_name)
values
    ('STAFF', 'manage_files')
on conflict (role_name, permission_name) do nothing;

