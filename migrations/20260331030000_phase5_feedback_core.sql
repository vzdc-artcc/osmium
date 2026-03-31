-- Phase 5: Feedback core workflow

create table if not exists feedback_items (
    id text primary key,
    submitter_user_id text not null references users(id) on delete cascade,
    target_user_id text not null references users(id) on delete cascade,
    pilot_callsign text not null,
    controller_position text not null,
    rating integer not null check (rating between 1 and 5),
    comments text,
    staff_comments text,
    status text not null default 'PENDING' check (status in ('PENDING', 'RELEASED', 'STASHED')),
    submitted_at timestamptz not null default now(),
    decided_at timestamptz,
    decided_by text references users(id)
);

create index if not exists idx_feedback_items_submitter on feedback_items(submitter_user_id);
create index if not exists idx_feedback_items_target on feedback_items(target_user_id);
create index if not exists idx_feedback_items_status on feedback_items(status);
create index if not exists idx_feedback_items_submitted_at on feedback_items(submitted_at desc);

insert into permissions (name)
values ('manage_feedback')
on conflict (name) do nothing;

insert into role_permissions (role_name, permission_name)
values ('STAFF', 'manage_feedback')
on conflict (role_name, permission_name) do nothing;

