-- Display-only staff position tags (ATM, DATM, TA, EC, WM, FE, AEC, AWM,
-- AFE, EP, TMU, FC, INS, MTR). These never grant permissions themselves —
-- they're purely a roster/profile display concept, distinct from
-- access.user_roles. `source` tracks whether roster sync or a manual admin
-- action last touched the row, so manual overrides survive future syncs.
create table if not exists identity.staff_positions (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    position text not null check (position in (
        'ATM', 'DATM', 'TA', 'EC', 'WM', 'FE', 'AEC', 'AWM', 'AFE', 'EP',
        'TMU', 'FC', 'INS', 'MTR'
    )),
    held boolean not null default true,
    source text not null check (source in ('auto', 'manual')),
    updated_by text references identity.users(id),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    unique (user_id, position)
);

create index if not exists idx_staff_positions_user_id_held
    on identity.staff_positions (user_id)
    where held;

insert into access.permissions (name, description)
values
    ('users.staff_positions.read', 'Read a controller''s display-only staff position tags'),
    ('users.staff_positions.update', 'Manually assign or revoke a controller''s display-only staff position tags')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'users.staff_positions.read'),
    ('STAFF', 'users.staff_positions.update')
on conflict (role_name, permission_name) do nothing;
