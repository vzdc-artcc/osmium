-- Phase 1.3: Admin Operations & Audit Foundation
-- Add audit logging tables to track ACL changes and admin actions

create table if not exists audit_logs (
    id text primary key,
    actor_id text not null references users(id) on delete cascade,
    action text not null,
    resource_type text not null,
    resource_id text,
    previous_value jsonb,
    new_value jsonb,
    created_at timestamptz not null default now()
);

create index if not exists idx_audit_logs_actor_id on audit_logs(actor_id);
create index if not exists idx_audit_logs_resource_type_id on audit_logs(resource_type, resource_id);
create index if not exists idx_audit_logs_created_at on audit_logs(created_at desc);

-- Add support for granular role and permission assignment tracking
-- Track when roles/permissions are modified with temporal awareness
create table if not exists role_assignments_history (
    id text primary key,
    user_id text not null references users(id) on delete cascade,
    role_name text not null references roles(name) on delete cascade,
    assigned_by text references users(id) on delete set null,
    assigned_at timestamptz not null default now(),
    revoked_by text references users(id) on delete set null,
    revoked_at timestamptz,
    reason text
);

create index if not exists idx_role_assignments_history_user_id on role_assignments_history(user_id);
create index if not exists idx_role_assignments_history_assigned_at on role_assignments_history(assigned_at desc);

-- Track permission overrides with audit trail
create table if not exists permission_overrides_history (
    id text primary key,
    user_id text not null references users(id) on delete cascade,
    permission_name text not null references permissions(name) on delete cascade,
    modified_by text not null references users(id) on delete set null,
    granted boolean not null,
    modified_at timestamptz not null default now(),
    reason text
);

create index if not exists idx_permission_overrides_history_user_id on permission_overrides_history(user_id);
create index if not exists idx_permission_overrides_history_modified_at on permission_overrides_history(modified_at desc);

comment on table audit_logs is 'Audit trail for all admin actions and ACL changes';
comment on table role_assignments_history is 'Temporal history of role assignments for compliance and debugging';
comment on table permission_overrides_history is 'History of permission override modifications';

