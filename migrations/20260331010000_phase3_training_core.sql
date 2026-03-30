-- Phase 3: Training core vertical slice

create table if not exists training_assignments (
    id text primary key,
    student_id text not null unique references users(id) on delete cascade,
    primary_trainer_id text not null references users(id) on delete cascade,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_training_assignments_primary_trainer_id
    on training_assignments(primary_trainer_id);

create table if not exists training_assignment_requests (
    id text primary key,
    student_id text not null unique references users(id) on delete cascade,
    submitted_at timestamptz not null default now(),
    status text not null default 'PENDING' check (status in ('PENDING', 'APPROVED', 'DENIED')),
    decided_at timestamptz,
    decided_by text references users(id),
    notes text
);

create index if not exists idx_training_assignment_requests_status
    on training_assignment_requests(status);

create table if not exists trainer_release_requests (
    id text primary key,
    student_id text not null unique references users(id) on delete cascade,
    submitted_at timestamptz not null default now(),
    status text not null default 'PENDING' check (status in ('PENDING', 'APPROVED', 'DENIED')),
    decided_at timestamptz,
    decided_by text references users(id),
    notes text
);

create index if not exists idx_trainer_release_requests_status
    on trainer_release_requests(status);

-- Add training permission and grant to staff role.
insert into permissions (name)
values ('manage_training')
on conflict (name) do nothing;

insert into role_permissions (role_name, permission_name)
values ('STAFF', 'manage_training')
on conflict (role_name, permission_name) do nothing;

