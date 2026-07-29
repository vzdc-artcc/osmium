-- Instructor/mentor-initiated impromptu training session offers, and the student
-- claims against them. Distinct from the Discord impromptu role-preference
-- selector: training staff post an offer (session type(s) + optional time), the
-- bot pings the matching impromptu_* roles with a Claim button, students claim,
-- and the mentor accepts one from the website.
create table if not exists training.impromptu_offers (
    id text primary key,
    created_by_user_id text not null references identity.users(id) on delete cascade,
    -- Facility session types offered: any of ground/tower/approach/center.
    session_types text[] not null,
    -- Null means "available now"; otherwise a scheduled availability time.
    available_at timestamptz,
    notes text,
    -- open | accepted | cancelled
    status text not null default 'open',
    accepted_user_id text references identity.users(id),
    discord_channel_id text,
    discord_message_id text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);
create index if not exists idx_impromptu_offers_creator
    on training.impromptu_offers (created_by_user_id, created_at desc);
create index if not exists idx_impromptu_offers_status
    on training.impromptu_offers (status);

create table if not exists training.impromptu_claims (
    id text primary key,
    offer_id text not null references training.impromptu_offers(id) on delete cascade,
    user_id text not null references identity.users(id) on delete cascade,
    -- pending | accepted | rejected
    status text not null default 'pending',
    claimed_at timestamptz not null default now(),
    unique (offer_id, user_id)
);
create index if not exists idx_impromptu_claims_offer
    on training.impromptu_claims (offer_id, claimed_at);

-- New permission: post impromptu training offers. Granted to the training roles
-- that previously handled impromptu coordination; SERVER_ADMIN holds it via the
-- effective-permissions cross-join.
insert into access.permissions (name, description)
values ('training.impromptu.create', 'Post impromptu training session offers')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'training.impromptu.create'),
    ('INS', 'training.impromptu.create'),
    ('MTR', 'training.impromptu.create')
on conflict do nothing;
