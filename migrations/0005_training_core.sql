create table if not exists training.training_assignments (
    id text primary key default gen_random_uuid()::text,
    student_id text not null unique references identity.users(id) on delete cascade,
    primary_trainer_id text not null references identity.users(id) on delete cascade,
    created_by_actor_id text references access.actors(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.training_assignment_other_trainers (
    assignment_id text not null references training.training_assignments(id) on delete cascade,
    trainer_id text not null references identity.users(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (assignment_id, trainer_id)
);

create table if not exists training.training_assignment_requests (
    id text primary key default gen_random_uuid()::text,
    student_id text not null unique references identity.users(id) on delete cascade,
    submitted_at timestamptz not null,
    status text not null default 'PENDING' check (status in ('PENDING', 'APPROVED', 'DENIED')),
    decided_at timestamptz,
    decided_by text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.training_assignment_request_interested_trainers (
    assignment_request_id text not null references training.training_assignment_requests(id) on delete cascade,
    trainer_id text not null references identity.users(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (assignment_request_id, trainer_id)
);

create table if not exists training.trainer_release_requests (
    id text primary key default gen_random_uuid()::text,
    student_id text not null unique references identity.users(id) on delete cascade,
    submitted_at timestamptz not null,
    status text not null default 'PENDING' check (status in ('PENDING', 'APPROVED', 'DENIED')),
    decided_at timestamptz,
    decided_by text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.training_appointments (
    id text primary key default gen_random_uuid()::text,
    student_id text not null references identity.users(id) on delete cascade,
    trainer_id text not null references identity.users(id) on delete cascade,
    start timestamptz not null,
    environment text,
    double_booking boolean not null default false,
    preparation_completed boolean not null default false,
    warning_email_sent boolean not null default false,
    atc_booking_id text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists training.ots_recommendations (
    id text primary key default gen_random_uuid()::text,
    student_id text not null references identity.users(id) on delete cascade,
    assigned_instructor_id text references identity.users(id) on delete set null,
    notes text not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger trg_training_assignments_updated_at
before update on training.training_assignments
for each row execute function platform.touch_updated_at();

create trigger trg_training_assignment_requests_updated_at
before update on training.training_assignment_requests
for each row execute function platform.touch_updated_at();

create trigger trg_trainer_release_requests_updated_at
before update on training.trainer_release_requests
for each row execute function platform.touch_updated_at();

create trigger trg_training_appointments_updated_at
before update on training.training_appointments
for each row execute function platform.touch_updated_at();

create trigger trg_ots_recommendations_updated_at
before update on training.ots_recommendations
for each row execute function platform.touch_updated_at();
