create table if not exists feedback.feedback_items (
    id text primary key default gen_random_uuid()::text,
    submitter_user_id text not null references identity.users(id) on delete cascade,
    target_user_id text not null references identity.users(id) on delete cascade,
    pilot_callsign text not null,
    controller_position text not null,
    rating integer not null check (rating between 1 and 5),
    comments text,
    staff_comments text,
    status text not null default 'PENDING' check (status in ('PENDING', 'RELEASED', 'STASHED')),
    submitted_at timestamptz not null default now(),
    decided_at timestamptz,
    decided_by text references identity.users(id) on delete set null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_feedback_items_target on feedback.feedback_items(target_user_id);
create index if not exists idx_feedback_items_status on feedback.feedback_items(status);

create table if not exists feedback.dossier_entries (
    id text primary key default gen_random_uuid()::text,
    user_id text not null references identity.users(id) on delete cascade,
    writer_id text not null references identity.users(id) on delete cascade,
    message text not null,
    timestamp timestamptz not null,
    created_at timestamptz not null default now()
);

create table if not exists feedback.incident_reports (
    id text primary key default gen_random_uuid()::text,
    reporter_id text not null references identity.users(id) on delete cascade,
    reportee_id text not null references identity.users(id) on delete cascade,
    timestamp timestamptz not null,
    reason text not null,
    closed boolean not null default false,
    reporter_callsign text,
    reportee_callsign text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create trigger trg_feedback_items_updated_at
before update on feedback.feedback_items
for each row execute function platform.touch_updated_at();

create trigger trg_incident_reports_updated_at
before update on feedback.incident_reports
for each row execute function platform.touch_updated_at();
