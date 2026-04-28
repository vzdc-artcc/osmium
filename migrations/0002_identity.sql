create table if not exists identity.users (
    id text primary key default gen_random_uuid()::text,
    cid bigint unique,
    email citext unique,
    email_verified_at timestamptz,
    first_name text,
    last_name text,
    full_name text not null,
    preferred_name text,
    display_name text not null,
    status text not null default 'ACTIVE' check (status in ('ACTIVE', 'INACTIVE', 'SUSPENDED')),
    joined_at timestamptz not null default now(),
    last_seen_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_identity_users_status on identity.users(status);

create table if not exists identity.user_profiles (
    user_id text primary key references identity.users(id) on delete cascade,
    bio text,
    avatar_asset_id text,
    timezone text not null default 'America/New_York',
    preferences jsonb not null default '{}'::jsonb,
    receive_email boolean not null default true,
    new_event_notifications boolean not null default false,
    show_welcome_message boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists identity.user_flags (
    user_id text primary key references identity.users(id) on delete cascade,
    no_request_loas boolean not null default false,
    no_request_training_assignments boolean not null default false,
    no_request_trainer_release boolean not null default false,
    no_force_progression_finish boolean not null default false,
    no_event_signup boolean not null default false,
    no_edit_profile boolean not null default false,
    excluded_from_roster_sync boolean not null default false,
    hidden_from_roster boolean not null default false,
    flag_auto_assign_single_pass boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists identity.user_identities (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    provider text not null check (provider in ('VATSIM', 'DISCORD', 'TEAMSPEAK', 'OTHER')),
    provider_subject text not null,
    provider_username text,
    provider_email citext,
    access_token text,
    refresh_token text,
    id_token text,
    token_expires_at timestamptz,
    scopes text[] not null default '{}'::text[],
    metadata jsonb not null default '{}'::jsonb,
    linked_at timestamptz not null default now(),
    last_refreshed_at timestamptz,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (provider, provider_subject)
);

create index if not exists idx_identity_user_identities_user_id on identity.user_identities(user_id);

create table if not exists identity.sessions (
    id text primary key default gen_random_uuid()::text,
    session_token text not null unique,
    user_id text not null references identity.users(id) on delete cascade,
    ip_address inet,
    user_agent text,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create index if not exists idx_identity_sessions_user_id on identity.sessions(user_id);
create index if not exists idx_identity_sessions_expires_at on identity.sessions(expires_at);
create index if not exists idx_identity_sessions_active on identity.sessions(user_id, expires_at)
    where revoked_at is null;

create table if not exists identity.verification_tokens (
    id text primary key default gen_random_uuid()::text,
    subject text not null,
    token text not null unique,
    purpose text not null check (purpose in ('EMAIL_VERIFY', 'PASSWORDLESS_LOGIN', 'ACCOUNT_LINK')),
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create table if not exists identity.oauth_states (
    id text primary key default gen_random_uuid()::text,
    state text not null unique,
    purpose text not null,
    user_id text references identity.users(id) on delete cascade,
    provider text not null,
    expires_at timestamptz not null,
    consumed_at timestamptz,
    created_at timestamptz not null default now()
);

create trigger trg_identity_users_updated_at
before update on identity.users
for each row execute function platform.touch_updated_at();

create trigger trg_identity_user_profiles_updated_at
before update on identity.user_profiles
for each row execute function platform.touch_updated_at();

create trigger trg_identity_user_flags_updated_at
before update on identity.user_flags
for each row execute function platform.touch_updated_at();

create trigger trg_identity_user_identities_updated_at
before update on identity.user_identities
for each row execute function platform.touch_updated_at();
