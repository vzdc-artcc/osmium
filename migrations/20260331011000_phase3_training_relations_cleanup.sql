-- Phase 3.1: Training relation parity + request payload cleanup

create table if not exists training_assignment_other_trainers (
    assignment_id text not null references training_assignments(id) on delete cascade,
    trainer_id text not null references users(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (assignment_id, trainer_id)
);

create index if not exists idx_training_assignment_other_trainers_trainer_id
    on training_assignment_other_trainers(trainer_id);

create table if not exists training_assignment_request_interested_trainers (
    assignment_request_id text not null references training_assignment_requests(id) on delete cascade,
    trainer_id text not null references users(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (assignment_request_id, trainer_id)
);

create index if not exists idx_training_assignment_request_interested_trainers_trainer_id
    on training_assignment_request_interested_trainers(trainer_id);

alter table if exists training_assignment_requests
    drop column if exists notes;

alter table if exists trainer_release_requests
    drop column if exists notes;

