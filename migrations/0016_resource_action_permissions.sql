create temp table tmp_permission_mapping (
    legacy_name text primary key,
    canonical_name text not null
) on commit drop;

insert into tmp_permission_mapping (legacy_name, canonical_name)
values
    ('read_own_profile', 'auth.read'),
    ('logout', 'auth.delete'),
    ('read_system_readiness', 'system.read'),
    ('view_all_users', 'users.read'),
    ('manage_users', 'users.update'),
    ('manage_training', 'training.update'),
    ('manage_feedback', 'feedback.update'),
    ('upload_files', 'files.create'),
    ('manage_files', 'files.update'),
    ('dev_login_as_cid', 'auth.manage'),
    ('manage_events', 'events.update'),
    ('publish_events', 'events.update'),
    ('manage_stats', 'stats.manage'),
    ('manage_integrations', 'integrations.manage'),
    ('manage_web_content', 'web.update');

insert into access.permissions (name, description)
values
    ('auth.read', 'Read own profile'),
    ('auth.delete', 'Logout current session'),
    ('auth.manage', 'Development login as CID'),
    ('system.read', 'Read readiness endpoints'),
    ('users.read', 'View all users'),
    ('users.update', 'Manage users'),
    ('training.update', 'Manage training'),
    ('feedback.update', 'Manage feedback'),
    ('files.create', 'Upload files'),
    ('files.read', 'Read files'),
    ('files.update', 'Manage files'),
    ('files.delete', 'Delete files'),
    ('events.read', 'Read events'),
    ('events.create', 'Create events'),
    ('events.update', 'Manage events'),
    ('events.delete', 'Delete events'),
    ('stats.manage', 'Manage stats sync'),
    ('integrations.manage', 'Manage integrations'),
    ('web.update', 'Manage website content')
on conflict (name) do nothing;

create temp table tmp_role_permissions on commit drop as
select
    rp.role_name,
    coalesce(mapping.canonical_name, rp.permission_name) as permission_name,
    min(rp.created_at) as created_at
from access.role_permissions rp
left join tmp_permission_mapping mapping
    on mapping.legacy_name = rp.permission_name
group by rp.role_name, coalesce(mapping.canonical_name, rp.permission_name);

delete from access.role_permissions;

insert into access.role_permissions (role_name, permission_name, created_at)
select role_name, permission_name, created_at
from tmp_role_permissions;

create temp table tmp_user_permissions on commit drop as
select
    up.user_id,
    coalesce(mapping.canonical_name, up.permission_name) as permission_name,
    bool_and(up.granted) as granted,
    min(up.created_at) as created_at
from access.user_permissions up
left join tmp_permission_mapping mapping
    on mapping.legacy_name = up.permission_name
group by up.user_id, coalesce(mapping.canonical_name, up.permission_name);

delete from access.user_permissions;

insert into access.user_permissions (user_id, permission_name, granted, created_at)
select user_id, permission_name, granted, created_at
from tmp_user_permissions;

delete from access.permissions
where name in (
    select legacy_name from tmp_permission_mapping
);
