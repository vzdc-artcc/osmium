create table if not exists org.memberships (
    user_id text primary key references identity.users(id) on delete cascade,
    artcc text not null default 'ZDC',
    division text not null default 'USA',
    rating text,
    controller_status text not null default 'NONE' check (controller_status in ('HOME', 'VISITOR', 'NONE')),
    membership_status text not null default 'ACTIVE' check (membership_status in ('ACTIVE', 'INACTIVE', 'SUSPENDED')),
    operating_initials text,
    join_date timestamptz not null default now(),
    home_facility text,
    visitor_home_facility text,
    is_active boolean not null default true,
    updated_by_user_id text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists org.staff_positions (
    id text primary key default gen_random_uuid()::text,
    name text not null unique,
    sort_order integer not null default 0,
    created_at timestamptz not null default now()
);

create table if not exists org.user_staff_positions (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    staff_position_id text not null references org.staff_positions(id) on delete cascade,
    starts_at timestamptz not null default now(),
    ends_at timestamptz,
    assigned_by_actor_id text references access.actors(id) on delete set null,
    reason text,
    created_at timestamptz not null default now()
);

create table if not exists org.visitor_applications (
    id text primary key default gen_random_uuid()::text,
    user_id text not null unique references identity.users(id) on delete cascade,
    home_facility text not null,
    why_visit text not null,
    status text not null default 'PENDING' check (status in ('PENDING', 'APPROVED', 'DENIED')),
    reason_for_denial text,
    submitted_at timestamptz not null,
    decided_at timestamptz,
    decided_by_actor_id text references access.actors(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists org.loas (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    start timestamptz not null,
    "end" timestamptz not null,
    reason text not null default '',
    status text not null default 'PENDING' check (status in ('PENDING', 'APPROVED', 'DENIED', 'INACTIVE')),
    submitted_at timestamptz not null default now(),
    decided_at timestamptz,
    decided_by_actor_id text references access.actors(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_org_loas_user_time on org.loas(user_id, start, "end", status);

create table if not exists org.certification_types (
    id text primary key default gen_random_uuid()::text,
    name text not null unique,
    sort_order integer not null default 0,
    can_solo_cert boolean not null default false,
    auto_assign_unrestricted boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists org.certification_type_allowed_options (
    certification_type_id text not null references org.certification_types(id) on delete cascade,
    option_key text not null check (option_key in ('NONE', 'UNRESTRICTED', 'DEL', 'GND', 'TWR', 'APP', 'CTR', 'TIER_1', 'CERTIFIED', 'SOLO')),
    primary key (certification_type_id, option_key)
);

create table if not exists org.user_certifications (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    certification_type_id text not null references org.certification_types(id) on delete cascade,
    certification_option text not null check (certification_option in ('NONE', 'UNRESTRICTED', 'DEL', 'GND', 'TWR', 'APP', 'CTR', 'TIER_1', 'CERTIFIED', 'SOLO')),
    granted_at timestamptz not null default now(),
    granted_by_actor_id text references access.actors(id) on delete set null,
    unique (user_id, certification_type_id)
);

create table if not exists org.user_solo_certifications (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    certification_type_id text not null references org.certification_types(id) on delete cascade,
    position text not null,
    expires timestamptz not null,
    granted_at timestamptz not null default now(),
    granted_by_actor_id text references access.actors(id) on delete set null
);

create table if not exists org.staffing_requests (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    name text not null,
    description text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists org.sua_blocks (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    start_at timestamptz not null,
    end_at timestamptz not null,
    afiliation text not null,
    details text not null,
    mission_number text not null unique,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists org.sua_block_airspace (
    id text primary key default gen_random_uuid()::text,
    sua_block_id text not null references org.sua_blocks(id) on delete cascade,
    identifier text not null,
    bottom_altitude text not null,
    top_altitude text not null
);

create trigger trg_org_memberships_updated_at
before update on org.memberships
for each row execute function platform.touch_updated_at();

create trigger trg_org_visitor_applications_updated_at
before update on org.visitor_applications
for each row execute function platform.touch_updated_at();

create trigger trg_org_loas_updated_at
before update on org.loas
for each row execute function platform.touch_updated_at();

create trigger trg_org_certification_types_updated_at
before update on org.certification_types
for each row execute function platform.touch_updated_at();

create trigger trg_org_staffing_requests_updated_at
before update on org.staffing_requests
for each row execute function platform.touch_updated_at();

create trigger trg_org_sua_blocks_updated_at
before update on org.sua_blocks
for each row execute function platform.touch_updated_at();
