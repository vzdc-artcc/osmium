insert into access.roles (name, description)
values ('SERVER_ADMIN', 'Singleton server administrator role')
on conflict (name) do nothing;

create unique index if not exists idx_access_single_server_admin
    on access.user_roles (role_name)
    where role_name = 'SERVER_ADMIN';

create or replace view access.v_user_primary_role as
select
    ur.user_id,
    (
        array_agg(
            ur.role_name
            order by
                case
                    when ur.role_name = 'SERVER_ADMIN' then 0
                    when ur.role_name = 'STAFF' then 1
                    when ur.role_name = 'USER' then 2
                    else 3
                end,
                ur.role_name
        )
    )[1] as primary_role
from access.user_roles ur
group by ur.user_id;

create or replace view access.v_effective_user_permissions as
with server_admin_users as (
    select distinct user_id
    from access.user_roles
    where role_name = 'SERVER_ADMIN'
),
role_permissions as (
    select distinct ur.user_id, rp.permission_name
    from access.user_roles ur
    join access.role_permissions rp on rp.role_name = ur.role_name
),
server_admin_permissions as (
    select sau.user_id, p.name as permission_name
    from server_admin_users sau
    cross join access.permissions p
),
granted_permissions as (
    select up.user_id, up.permission_name
    from access.user_permissions up
    where up.granted is true
),
denied_permissions as (
    select up.user_id, up.permission_name
    from access.user_permissions up
    where up.granted is false
),
candidate_permissions as (
    select * from role_permissions
    union
    select * from server_admin_permissions
    union
    select * from granted_permissions
)
select cp.user_id, cp.permission_name
from candidate_permissions cp
left join denied_permissions dp
    on dp.user_id = cp.user_id
   and dp.permission_name = cp.permission_name
left join server_admin_users sau
    on sau.user_id = cp.user_id
where sau.user_id is not null
   or dp.user_id is null;
