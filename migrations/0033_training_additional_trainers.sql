alter table training.training_appointments
    add column if not exists notes text not null default '';

create table if not exists training.training_session_additional_trainers (
    session_id text not null references training.training_sessions(id) on delete cascade,
    trainer_id text not null references identity.users(id) on delete cascade,
    description text not null,
    created_at timestamptz not null default now(),
    primary key (session_id, trainer_id)
);

create table if not exists training.training_appointment_additional_trainers (
    appointment_id text not null references training.training_appointments(id) on delete cascade,
    trainer_id text not null references identity.users(id) on delete cascade,
    description text not null,
    created_at timestamptz not null default now(),
    primary key (appointment_id, trainer_id)
);
