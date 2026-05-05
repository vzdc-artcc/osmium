-- API Keys: user-owned bearer credentials with direct permission grants.
--
-- Treats an "API key" as a service account row (kind = 'api_key') plus a
-- credential row, owned by the user that created it.  Adds direct permission
-- grants for service accounts (mirroring access.user_permissions) so each
-- key carries its own permission set independent of role assignments.

alter table access.service_accounts
    add column if not exists kind text not null default 'service';

alter table access.service_accounts
    drop constraint if exists service_accounts_kind_check;

alter table access.service_accounts
    add constraint service_accounts_kind_check
        check (kind in ('service', 'api_key'));

alter table access.service_accounts
    add column if not exists created_by_user_id text
        references identity.users(id) on delete set null;

alter table access.service_account_credentials
    add column if not exists prefix text;

alter table access.service_account_credentials
    add column if not exists last_four text;

create table if not exists access.service_account_permissions (
    service_account_id text not null references access.service_accounts(id) on delete cascade,
    permission_name text not null references access.permissions(name) on delete cascade,
    granted boolean not null default true,
    created_at timestamptz not null default now(),
    primary key (service_account_id, permission_name)
);

create index if not exists idx_access_service_account_permissions_sa
    on access.service_account_permissions(service_account_id);

create index if not exists idx_access_service_accounts_created_by
    on access.service_accounts(created_by_user_id);

create or replace view access.v_effective_service_account_permissions as
with role_permissions as (
    select distinct sar.service_account_id, rp.permission_name
    from access.service_account_roles sar
    join access.role_permissions rp on rp.role_name = sar.role_name
    where sar.ends_at is null or sar.ends_at > now()
),
granted_permissions as (
    select sap.service_account_id, sap.permission_name
    from access.service_account_permissions sap
    where sap.granted is true
),
denied_permissions as (
    select sap.service_account_id, sap.permission_name
    from access.service_account_permissions sap
    where sap.granted is false
),
candidate_permissions as (
    select * from role_permissions
    union
    select * from granted_permissions
)
select cp.service_account_id, cp.permission_name
from candidate_permissions cp
left join denied_permissions dp
    on dp.service_account_id = cp.service_account_id
   and dp.permission_name = cp.permission_name
where dp.service_account_id is null;

insert into access.permissions (name, description)
values
    ('api_keys.read', 'Read API keys created by other users'),
    ('api_keys.create', 'Create API keys'),
    ('api_keys.update', 'Update API keys created by other users'),
    ('api_keys.delete', 'Revoke API keys created by other users')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('SERVER_ADMIN', 'api_keys.read'),
    ('SERVER_ADMIN', 'api_keys.create'),
    ('SERVER_ADMIN', 'api_keys.update'),
    ('SERVER_ADMIN', 'api_keys.delete')
on conflict (role_name, permission_name) do nothing;
