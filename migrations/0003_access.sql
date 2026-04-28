create table if not exists access.roles (
    name text primary key,
    description text,
    is_system boolean not null default true,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists access.permissions (
    name text primary key,
    description text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists access.role_permissions (
    role_name text not null references access.roles(name) on delete cascade,
    permission_name text not null references access.permissions(name) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (role_name, permission_name)
);

create table if not exists access.user_roles (
    user_id text not null references identity.users(id) on delete cascade,
    role_name text not null references access.roles(name) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (user_id, role_name)
);

create table if not exists access.user_permissions (
    user_id text not null references identity.users(id) on delete cascade,
    permission_name text not null references access.permissions(name) on delete cascade,
    granted boolean not null default true,
    created_at timestamptz not null default now(),
    primary key (user_id, permission_name)
);

create index if not exists idx_access_user_roles_user_id on access.user_roles(user_id);
create index if not exists idx_access_user_permissions_user_id on access.user_permissions(user_id);

create table if not exists access.service_accounts (
    id text primary key default gen_random_uuid()::text,
    key text not null unique,
    name text not null,
    description text,
    owner_team text,
    status text not null default 'active' check (status in ('active', 'disabled')),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists access.service_account_credentials (
    id text primary key default gen_random_uuid()::text,
    service_account_id text not null references access.service_accounts(id) on delete cascade,
    credential_type text not null check (credential_type in ('api_key', 'bearer_token', 'oauth_client_secret')),
    public_key text,
    secret_hash text not null,
    last_used_at timestamptz,
    expires_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create table if not exists access.service_account_roles (
    id text primary key default gen_random_uuid()::text,
    service_account_id text not null references access.service_accounts(id) on delete cascade,
    role_name text not null references access.roles(name) on delete cascade,
    scope_type text not null default 'global' check (scope_type in ('global', 'event', 'file', 'training_progression', 'training_session', 'web_page', 'service_account')),
    scope_key text,
    granted_by_actor_id text,
    reason text,
    starts_at timestamptz not null default now(),
    ends_at timestamptz,
    created_at timestamptz not null default now()
);

create table if not exists access.actors (
    id text primary key default gen_random_uuid()::text,
    actor_type text not null check (actor_type in ('user', 'service_account', 'system')),
    user_id text references identity.users(id) on delete cascade,
    service_account_id text references access.service_accounts(id) on delete cascade,
    display_name text not null,
    created_at timestamptz not null default now(),
    check (
        (actor_type = 'user' and user_id is not null and service_account_id is null)
        or (actor_type = 'service_account' and user_id is null and service_account_id is not null)
        or (actor_type = 'system' and user_id is null and service_account_id is null)
    )
);

create table if not exists access.audit_logs (
    id text primary key default gen_random_uuid()::text,
    actor_id text references access.actors(id) on delete set null,
    action text not null,
    resource_type text not null,
    resource_id text,
    scope_type text not null default 'global' check (scope_type in ('global', 'event', 'file', 'training_progression', 'training_session', 'web_page', 'service_account')),
    scope_key text,
    before_state jsonb,
    after_state jsonb,
    ip_address inet,
    created_at timestamptz not null default now()
);

create index if not exists idx_access_audit_logs_actor_id on access.audit_logs(actor_id);
create index if not exists idx_access_audit_logs_resource on access.audit_logs(resource_type, resource_id);

create trigger trg_access_roles_updated_at
before update on access.roles
for each row execute function platform.touch_updated_at();

create trigger trg_access_permissions_updated_at
before update on access.permissions
for each row execute function platform.touch_updated_at();

create trigger trg_access_service_accounts_updated_at
before update on access.service_accounts
for each row execute function platform.touch_updated_at();
