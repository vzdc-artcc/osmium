-- Phase 1.4: Enhanced ACL Enforcement & Policy Foundation
-- Add policy evaluation tables and reference data for sophisticated access control
-- This supports both role-based and attribute-based access control patterns

create table if not exists acl_policies (
    id text primary key,
    name text not null unique,
    description text,
    rule_engine text not null default 'RBAC',  -- RBAC, ABAC, or combined
    is_active boolean not null default true,
    priority integer not null default 100,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now()
);

create index if not exists idx_acl_policies_is_active on acl_policies(is_active);
create index if not exists idx_acl_policies_priority on acl_policies(priority desc);

-- Policy rules: define conditions under which permissions are granted/denied
create table if not exists acl_policy_rules (
    id text primary key,
    policy_id text not null references acl_policies(id) on delete cascade,
    condition_type text not null,  -- 'role', 'attribute', 'time_based', 'resource_scoped'
    condition_key text not null,
    condition_value text not null,
    effect text not null check (effect in ('ALLOW', 'DENY')),
    permissions text[] not null,  -- array of permission names
    created_at timestamptz not null default now()
);

create index if not exists idx_acl_policy_rules_policy_id on acl_policy_rules(policy_id);
create index if not exists idx_acl_policy_rules_condition_type on acl_policy_rules(condition_type);

-- Seed default policies
insert into acl_policies (id, name, description, rule_engine, priority)
values
    ('policy-default-rbac', 'Default RBAC Policy', 'Standard role-based access control', 'RBAC', 100),
    ('policy-staff-escalation', 'Staff Escalation', 'Elevated permissions for staff operations', 'RBAC', 110),
    ('policy-dev-access', 'Developer Access', 'Development-only access patterns', 'RBAC', 90)
on conflict (name) do nothing;

-- Seed default RBAC rules
insert into acl_policy_rules (id, policy_id, condition_type, condition_key, condition_value, effect, permissions)
select
    'rule-' || gen_random_uuid()::text,
    'policy-default-rbac',
    'role',
    'role_name',
    'USER',
    'ALLOW',
    array['read_own_profile', 'logout']
where not exists (
    select 1 from acl_policy_rules
    where policy_id = 'policy-default-rbac'
    and condition_key = 'role_name'
    and condition_value = 'USER'
);

insert into acl_policy_rules (id, policy_id, condition_type, condition_key, condition_value, effect, permissions)
select
    'rule-' || gen_random_uuid()::text,
    'policy-default-rbac',
    'role',
    'role_name',
    'STAFF',
    'ALLOW',
    array['read_own_profile', 'logout', 'read_system_readiness', 'manage_users', 'dev_login_as_cid']
where not exists (
    select 1 from acl_policy_rules
    where policy_id = 'policy-default-rbac'
    and condition_key = 'role_name'
    and condition_value = 'STAFF'
);

-- Create view for effective ACL policy evaluation
create or replace view v_user_effective_permissions as
select
    ur.user_id,
    array_agg(distinct up.permission_name) filter (where up.granted is true) ||
    array_agg(distinct rp.permission_name)
    as permissions
from user_roles ur
left join role_permissions rp on rp.role_name = ur.role_name
left join user_permissions up on up.user_id = ur.user_id
group by ur.user_id;

comment on table acl_policies is 'Policy definitions for access control evaluation';
comment on table acl_policy_rules is 'Individual rules within policies that grant/deny permissions';
comment on view v_user_effective_permissions is 'Materialized view of a user''s effective permissions across roles and overrides';

