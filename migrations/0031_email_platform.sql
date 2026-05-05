create schema if not exists email;

create table if not exists email.templates (
    id text primary key,
    name text not null,
    category text not null,
    description text not null,
    is_transactional boolean not null,
    allow_arbitrary_addresses boolean not null default false,
    respect_user_event_pref boolean not null default false,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create table if not exists email.suppression_categories (
    id text primary key,
    name text not null,
    description text not null,
    is_transactional boolean not null
);

create table if not exists email.outbox (
    id uuid primary key,
    template_id text not null references email.templates(id),
    category text not null,
    is_transactional boolean not null,
    requested_by_actor_id text null references access.actors(id),
    requested_by_user_id text null references identity.users(id),
    requested_by_service_account_id text null references access.service_accounts(id),
    request_source text not null,
    subject_override text null,
    payload jsonb not null,
    recipient_mode text not null check (recipient_mode in ('explicit', 'audience', 'mixed')),
    audience_filter jsonb null,
    status text not null check (status in ('pending', 'processing', 'sent', 'failed', 'cancelled', 'suppressed')),
    attempt_count integer not null default 0,
    next_attempt_at timestamptz not null default now(),
    last_error text null,
    provider text null,
    provider_message_id text null,
    queued_at timestamptz not null default now(),
    sent_at timestamptz null,
    failed_at timestamptz null,
    cancelled_at timestamptz null
);

create table if not exists email.outbox_recipients (
    id uuid primary key,
    outbox_id uuid not null references email.outbox(id) on delete cascade,
    user_id text null references identity.users(id),
    email citext not null,
    display_name text null,
    source text not null,
    suppression_reason text null,
    delivery_status text not null check (delivery_status in ('pending', 'processing', 'sent', 'failed', 'suppressed', 'cancelled')),
    provider_message_id text null,
    sent_at timestamptz null,
    failed_at timestamptz null,
    last_error text null,
    created_at timestamptz not null default now()
);

create table if not exists email.suppressions (
    id uuid primary key,
    category_id text not null references email.suppression_categories(id),
    user_id text null references identity.users(id),
    email citext not null,
    reason text not null,
    source text not null,
    created_at timestamptz not null default now(),
    revoked_at timestamptz null
);

create unique index if not exists suppressions_active_category_email_idx
    on email.suppressions (category_id, lower(email::text))
    where revoked_at is null;

create index if not exists email_outbox_status_next_attempt_idx
    on email.outbox (status, next_attempt_at);

create index if not exists email_outbox_recipients_outbox_idx
    on email.outbox_recipients (outbox_id);

create index if not exists email_outbox_recipients_email_idx
    on email.outbox_recipients (email);

create index if not exists email_suppressions_user_id_idx
    on email.suppressions (user_id)
    where user_id is not null;

insert into email.suppression_categories (id, name, description, is_transactional)
values
    ('transactional', 'Transactional', 'Required product and operational mail', true),
    ('event_notifications', 'Event Notifications', 'Event-related notifications that users may opt out of', false),
    ('announcements', 'Announcements', 'General staff or facility announcements', false),
    ('marketing', 'Marketing', 'Promotional or campaign-style email', false)
on conflict (id) do update
set name = excluded.name,
    description = excluded.description,
    is_transactional = excluded.is_transactional;

insert into email.templates (
    id,
    name,
    category,
    description,
    is_transactional,
    allow_arbitrary_addresses,
    respect_user_event_pref
)
values
    ('announcements.generic', 'Generic Announcement', 'announcements', 'Generic formatted announcement email', false, true, false),
    ('events.position_published', 'Event Position Published', 'event_notifications', 'Event position publication notice', false, false, true),
    ('events.reminder', 'Event Reminder', 'event_notifications', 'Reminder for an upcoming event', false, false, true),
    ('system.test_email', 'System Test Email', 'transactional', 'Simple diagnostic email for SES connectivity', true, true, false)
on conflict (id) do update
set name = excluded.name,
    category = excluded.category,
    description = excluded.description,
    is_transactional = excluded.is_transactional,
    allow_arbitrary_addresses = excluded.allow_arbitrary_addresses,
    respect_user_event_pref = excluded.respect_user_event_pref,
    updated_at = now();

insert into access.permissions (name, description)
values
    ('emails.templates.read', 'Read available email templates'),
    ('emails.preview.create', 'Preview rendered email templates'),
    ('emails.send.create', 'Queue email template sends'),
    ('emails.outbox.read', 'Read email outbox and delivery state'),
    ('emails.suppressions.read', 'Read email suppressions'),
    ('emails.suppressions.update', 'Update email suppressions')
on conflict (name) do nothing;
