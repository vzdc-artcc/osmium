create temp table tmp_permission_catalog (
    name text primary key,
    description text not null
) on commit drop;

insert into tmp_permission_catalog (name, description)
values
    ('auth.profile.read', 'Read current user profile'),
    ('auth.profile.update', 'Update current user profile'),
    ('auth.teamspeak_uids.read', 'Read linked TeamSpeak UIDs'),
    ('auth.teamspeak_uids.create', 'Create linked TeamSpeak UIDs'),
    ('auth.teamspeak_uids.delete', 'Delete linked TeamSpeak UIDs'),
    ('auth.sessions.delete', 'Delete current session'),
    ('auth.dev_login.create', 'Use development login as CID'),
    ('users.directory.read', 'Read user directory'),
    ('users.access.read', 'Read user access state'),
    ('users.access.update', 'Update user access state'),
    ('users.controller_status.update', 'Update user controller status'),
    ('users.visitor_applications.read', 'Read visitor applications'),
    ('users.visitor_applications.create', 'Create visitor applications'),
    ('users.visitor_applications.update', 'Update visitor applications'),
    ('audit.logs.read', 'Read audit logs'),
    ('training.assignments.read', 'Read training assignments'),
    ('training.assignments.create', 'Create training assignments'),
    ('training.ots_recommendations.read', 'Read OTS recommendations'),
    ('training.ots_recommendations.create', 'Create OTS recommendations'),
    ('training.ots_recommendations.update', 'Update OTS recommendations'),
    ('training.ots_recommendations.delete', 'Delete OTS recommendations'),
    ('training.lessons.read', 'Read training lessons'),
    ('training.lessons.create', 'Create training lessons'),
    ('training.lessons.update', 'Update training lessons'),
    ('training.lessons.delete', 'Delete training lessons'),
    ('training.appointments.read', 'Read training appointments'),
    ('training.appointments.create', 'Create training appointments'),
    ('training.appointments.update', 'Update training appointments'),
    ('training.appointments.delete', 'Delete training appointments'),
    ('training.sessions.read', 'Read training sessions'),
    ('training.sessions.create', 'Create training sessions'),
    ('training.sessions.update', 'Update training sessions'),
    ('training.sessions.delete', 'Delete training sessions'),
    ('training.assignment_requests.read', 'Read training assignment requests'),
    ('training.assignment_requests.create', 'Create training assignment requests'),
    ('training.assignment_requests.update', 'Update training assignment requests'),
    ('training.assignment_requests.interest.create', 'Create training assignment request interest'),
    ('training.assignment_requests.interest.delete', 'Delete training assignment request interest'),
    ('training.release_requests.read', 'Read trainer release requests'),
    ('training.release_requests.create', 'Create trainer release requests'),
    ('training.release_requests.update', 'Update trainer release requests'),
    ('feedback.items.read', 'Read feedback items'),
    ('feedback.items.create', 'Create feedback items'),
    ('feedback.items.update', 'Update feedback items'),
    ('events.items.read', 'Read events'),
    ('events.items.create', 'Create events'),
    ('events.items.update', 'Update events'),
    ('events.items.delete', 'Delete events'),
    ('events.positions.read', 'Read event positions'),
    ('events.positions.create', 'Create event positions'),
    ('events.positions.update', 'Update event positions'),
    ('events.positions.delete', 'Delete event positions'),
    ('events.positions.publish', 'Publish event positions'),
    ('files.audit.read', 'Read file audit logs'),
    ('files.assets.read', 'Read file metadata'),
    ('files.assets.create', 'Create file metadata'),
    ('files.assets.update', 'Update file metadata'),
    ('files.assets.delete', 'Delete file metadata'),
    ('files.content.read', 'Read file content'),
    ('files.content.update', 'Update file content'),
    ('publications.categories.read', 'Read publication categories'),
    ('publications.categories.create', 'Create publication categories'),
    ('publications.categories.update', 'Update publication categories'),
    ('publications.categories.delete', 'Delete publication categories'),
    ('publications.items.read', 'Read publications'),
    ('publications.items.create', 'Create publications'),
    ('publications.items.update', 'Update publications'),
    ('publications.items.delete', 'Delete publications'),
    ('stats.artcc.read', 'Read ARTCC stats'),
    ('stats.controller_events.read', 'Read controller events'),
    ('stats.controller_history.read', 'Read controller history'),
    ('stats.controller_totals.read', 'Read controller totals'),
    ('integrations.stats.update', 'Update stats integrations'),
    ('system.read', 'Read system readiness')
on conflict do nothing;

insert into access.permissions (name, description)
select name, description
from tmp_permission_catalog
on conflict (name) do update
set description = excluded.description,
    updated_at = now();

create temp table tmp_permission_mapping (
    source_name text not null,
    target_name text not null
) on commit drop;

insert into tmp_permission_mapping (source_name, target_name)
values
    ('auth.read', 'auth.profile.read'),
    ('auth.delete', 'auth.sessions.delete'),
    ('auth.manage', 'auth.dev_login.create'),
    ('system.read', 'system.read'),
    ('users.read', 'users.directory.read'),
    ('users.update', 'users.directory.read'),
    ('users.update', 'users.access.read'),
    ('users.update', 'users.access.update'),
    ('users.update', 'users.controller_status.update'),
    ('users.update', 'users.visitor_applications.read'),
    ('users.update', 'users.visitor_applications.update'),
    ('audit.read', 'audit.logs.read'),
    ('training.read', 'training.assignments.read'),
    ('training.read', 'training.ots_recommendations.read'),
    ('training.read', 'training.lessons.read'),
    ('training.read', 'training.appointments.read'),
    ('training.read', 'training.sessions.read'),
    ('training.read', 'training.assignment_requests.read'),
    ('training.read', 'training.release_requests.read'),
    ('training.create', 'training.assignments.create'),
    ('training.create', 'training.lessons.create'),
    ('training.create', 'training.appointments.create'),
    ('training.create', 'training.sessions.create'),
    ('training.update', 'training.lessons.update'),
    ('training.update', 'training.appointments.update'),
    ('training.update', 'training.sessions.update'),
    ('training.manage', 'training.assignments.read'),
    ('training.manage', 'training.assignments.create'),
    ('training.manage', 'training.ots_recommendations.read'),
    ('training.manage', 'training.ots_recommendations.create'),
    ('training.manage', 'training.ots_recommendations.update'),
    ('training.manage', 'training.ots_recommendations.delete'),
    ('training.manage', 'training.lessons.read'),
    ('training.manage', 'training.lessons.create'),
    ('training.manage', 'training.lessons.update'),
    ('training.manage', 'training.lessons.delete'),
    ('training.manage', 'training.appointments.read'),
    ('training.manage', 'training.appointments.create'),
    ('training.manage', 'training.appointments.update'),
    ('training.manage', 'training.appointments.delete'),
    ('training.manage', 'training.sessions.read'),
    ('training.manage', 'training.sessions.create'),
    ('training.manage', 'training.sessions.update'),
    ('training.manage', 'training.sessions.delete'),
    ('training.manage', 'training.assignment_requests.read'),
    ('training.manage', 'training.assignment_requests.create'),
    ('training.manage', 'training.assignment_requests.update'),
    ('training.manage', 'training.assignment_requests.interest.create'),
    ('training.manage', 'training.assignment_requests.interest.delete'),
    ('training.manage', 'training.release_requests.read'),
    ('training.manage', 'training.release_requests.create'),
    ('training.manage', 'training.release_requests.update'),
    ('feedback.update', 'feedback.items.read'),
    ('feedback.update', 'feedback.items.create'),
    ('feedback.update', 'feedback.items.update'),
    ('files.read', 'files.assets.read'),
    ('files.read', 'files.content.read'),
    ('files.create', 'files.assets.create'),
    ('files.update', 'files.assets.update'),
    ('files.update', 'files.content.update'),
    ('files.delete', 'files.assets.delete'),
    ('events.read', 'events.items.read'),
    ('events.read', 'events.positions.read'),
    ('events.create', 'events.items.create'),
    ('events.create', 'events.positions.create'),
    ('events.update', 'events.items.update'),
    ('events.update', 'events.positions.update'),
    ('events.update', 'events.positions.publish'),
    ('events.delete', 'events.items.delete'),
    ('events.delete', 'events.positions.delete'),
    ('stats.manage', 'stats.controller_events.read'),
    ('integrations.manage', 'integrations.stats.update'),
    ('web.update', 'publications.categories.create'),
    ('web.update', 'publications.categories.update'),
    ('web.update', 'publications.categories.delete'),
    ('web.update', 'publications.items.create'),
    ('web.update', 'publications.items.update'),
    ('web.update', 'publications.items.delete'),
    ('publications.categories.read', 'publications.categories.read'),
    ('publications.categories.create', 'publications.categories.create'),
    ('publications.categories.update', 'publications.categories.update'),
    ('publications.categories.delete', 'publications.categories.delete'),
    ('publications.items.read', 'publications.items.read'),
    ('publications.items.create', 'publications.items.create'),
    ('publications.items.update', 'publications.items.update'),
    ('publications.items.delete', 'publications.items.delete');

create temp table tmp_human_permission_grants on commit drop as
select distinct eup.user_id, map.target_name as permission_name
from access.v_effective_user_permissions eup
join tmp_permission_mapping map on map.source_name = eup.permission_name
where not exists (
    select 1
    from access.user_roles ur
    where ur.user_id = eup.user_id
      and ur.role_name = 'SERVER_ADMIN'
);

insert into tmp_human_permission_grants (user_id, permission_name)
select distinct u.id, baseline.permission_name
from identity.users u
cross join (
    values
        ('auth.profile.read'),
        ('auth.profile.update'),
        ('auth.teamspeak_uids.read'),
        ('auth.teamspeak_uids.create'),
        ('auth.teamspeak_uids.delete'),
        ('auth.sessions.delete'),
        ('users.visitor_applications.create'),
        ('feedback.items.read'),
        ('feedback.items.create'),
        ('events.positions.create')
) as baseline(permission_name)
where not exists (
    select 1
    from access.user_roles ur
    where ur.user_id = u.id
      and ur.role_name = 'SERVER_ADMIN'
);

delete from access.user_permissions up
where exists (
    select 1
    from identity.users u
    where u.id = up.user_id
)
and not exists (
    select 1
    from access.user_roles ur
    where ur.user_id = up.user_id
      and ur.role_name = 'SERVER_ADMIN'
);

insert into access.user_permissions (user_id, permission_name, granted)
select distinct user_id, permission_name, true
from tmp_human_permission_grants
on conflict (user_id, permission_name) do update
set granted = true;

create temp table tmp_role_permissions on commit drop as
select
    rp.role_name,
    map.target_name as permission_name,
    min(rp.created_at) as created_at
from access.role_permissions rp
join tmp_permission_mapping map on map.source_name = rp.permission_name
group by rp.role_name, map.target_name;

delete from access.role_permissions;

insert into access.role_permissions (role_name, permission_name, created_at)
select role_name, permission_name, created_at
from tmp_role_permissions
on conflict (role_name, permission_name) do nothing;

delete from access.user_roles
where role_name in ('USER', 'STAFF');

delete from access.permissions
where name not in (
    select name from tmp_permission_catalog
);
