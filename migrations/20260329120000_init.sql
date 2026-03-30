create table if not exists users (
    id text primary key,
    cid bigint not null unique,
    email text not null unique,
    display_name text not null,
    role text not null default 'USER',
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists sessions (
    session_token text primary key,
    user_id text not null references users(id) on delete cascade,
    expires_at timestamptz not null,
    created_at timestamptz not null default now()
);

create index if not exists idx_sessions_user_id on sessions(user_id);
create index if not exists idx_sessions_expires_at on sessions(expires_at);

create table if not exists events (
    id text primary key,
    title text not null,
    starts_at timestamptz not null,
    ends_at timestamptz not null,
    created_by text not null references users(id),
    created_at timestamptz not null default now()
);

create index if not exists idx_events_starts_at on events(starts_at);

create table if not exists event_positions (
    id text primary key,
    event_id text not null references events(id) on delete cascade,
    callsign text not null,
    user_id text references users(id),
    created_at timestamptz not null default now()
);

create index if not exists idx_event_positions_event_id on event_positions(event_id);

insert into users (id, cid, email, display_name, role)
values ('seed-admin', 10000001, 'admin@example.com', 'Seed Admin', 'STAFF')
on conflict (id) do nothing;

