create temp table tmp_permission_catalog_v2 (
    name text primary key,
    description text not null
) on commit drop;

insert into tmp_permission_catalog_v2 (name, description)
values
    ('auth.profile.read', 'Read current user profile'),
    ('auth.profile.update', 'Update current user profile'),
    ('auth.teamspeak_uids.read', 'Read linked TeamSpeak UIDs'),
    ('auth.teamspeak_uids.create', 'Create linked TeamSpeak UIDs'),
    ('auth.teamspeak_uids.delete', 'Delete linked TeamSpeak UIDs'),
    ('auth.sessions.delete', 'Delete current session'),
    ('auth.dev_login.create', 'Use development login as CID'),
    ('access.self.read', 'Read current actor access state'),
    ('access.catalog.read', 'Read access catalog'),
    ('access.users.read', 'Read user access assignments'),
    ('access.users.update', 'Update user access assignments'),
    ('users.directory.read', 'Read user directory'),
    ('users.directory_private.read', 'Read private user directory details'),
    ('users.controller_status.update', 'Update user controller status'),
    ('users.visit_artcc.request', 'Request visitor ARTCC enrollment'),
    ('users.visitor_applications.self.read', 'Read own visitor application'),
    ('users.visitor_applications.self.request', 'Submit own visitor application'),
    ('users.visitor_applications.read', 'Read visitor applications'),
    ('users.visitor_applications.decide', 'Decide visitor applications'),
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
    ('training.assignment_requests.self.request', 'Submit own training assignment requests'),
    ('training.assignment_requests.decide', 'Decide training assignment requests'),
    ('training.assignment_requests.interest.request', 'Register trainer interest on training assignment requests'),
    ('training.assignment_requests.interest.delete', 'Remove trainer interest on training assignment requests'),
    ('training.release_requests.read', 'Read trainer release requests'),
    ('training.release_requests.self.request', 'Submit own trainer release requests'),
    ('training.release_requests.decide', 'Decide trainer release requests'),
    ('feedback.items.self.read', 'Read own feedback items'),
    ('feedback.items.read', 'Read feedback items'),
    ('feedback.items.create', 'Create feedback items'),
    ('feedback.items.decide', 'Decide feedback items'),
    ('events.items.create', 'Create events'),
    ('events.items.update', 'Update events'),
    ('events.items.delete', 'Delete events'),
    ('events.positions.self.request', 'Request event positions'),
    ('events.positions.assign', 'Assign event positions'),
    ('events.positions.delete', 'Delete event positions'),
    ('events.positions.publish', 'Publish event positions'),
    ('files.audit.read', 'Read file audit logs'),
    ('files.assets.read', 'Read file metadata'),
    ('files.assets.create', 'Create file metadata'),
    ('files.assets.update', 'Update file metadata'),
    ('files.assets.policy.update', 'Update file access policy'),
    ('files.assets.delete', 'Delete file metadata'),
    ('files.content.read', 'Read file content'),
    ('files.content.create', 'Create file content'),
    ('files.content.update', 'Update file content'),
    ('files.content.delete', 'Delete file content'),
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
    ('system.read', 'Read system readiness');

insert into access.permissions (name, description)
select name, description
from tmp_permission_catalog_v2
on conflict (name) do update
set description = excluded.description,
    updated_at = now();

create temp table tmp_permission_mapping_v2 (
    source_name text not null,
    target_name text not null
) on commit drop;

insert into tmp_permission_mapping_v2 (source_name, target_name)
values
    ('auth.profile.read', 'auth.profile.read'),
    ('auth.profile.update', 'auth.profile.update'),
    ('auth.teamspeak_uids.read', 'auth.teamspeak_uids.read'),
    ('auth.teamspeak_uids.create', 'auth.teamspeak_uids.create'),
    ('auth.teamspeak_uids.delete', 'auth.teamspeak_uids.delete'),
    ('auth.sessions.delete', 'auth.sessions.delete'),
    ('auth.dev_login.create', 'auth.dev_login.create'),
    ('users.access.read', 'access.self.read'),
    ('users.access.read', 'access.catalog.read'),
    ('users.access.read', 'access.users.read'),
    ('users.access.update', 'access.users.read'),
    ('users.access.update', 'access.users.update'),
    ('users.directory.read', 'users.directory.read'),
    ('users.directory.read', 'users.directory_private.read'),
    ('users.controller_status.update', 'users.controller_status.update'),
    ('users.visitor_applications.read', 'users.visitor_applications.read'),
    ('users.visitor_applications.create', 'users.visitor_applications.self.read'),
    ('users.visitor_applications.create', 'users.visitor_applications.self.request'),
    ('users.visitor_applications.update', 'users.visitor_applications.decide'),
    ('audit.logs.read', 'audit.logs.read'),
    ('training.assignments.read', 'training.assignments.read'),
    ('training.assignments.create', 'training.assignments.create'),
    ('training.ots_recommendations.read', 'training.ots_recommendations.read'),
    ('training.ots_recommendations.create', 'training.ots_recommendations.create'),
    ('training.ots_recommendations.update', 'training.ots_recommendations.update'),
    ('training.ots_recommendations.delete', 'training.ots_recommendations.delete'),
    ('training.lessons.read', 'training.lessons.read'),
    ('training.lessons.create', 'training.lessons.create'),
    ('training.lessons.update', 'training.lessons.update'),
    ('training.lessons.delete', 'training.lessons.delete'),
    ('training.appointments.read', 'training.appointments.read'),
    ('training.appointments.create', 'training.appointments.create'),
    ('training.appointments.update', 'training.appointments.update'),
    ('training.appointments.delete', 'training.appointments.delete'),
    ('training.sessions.read', 'training.sessions.read'),
    ('training.sessions.create', 'training.sessions.create'),
    ('training.sessions.update', 'training.sessions.update'),
    ('training.sessions.delete', 'training.sessions.delete'),
    ('training.assignment_requests.read', 'training.assignment_requests.read'),
    ('training.assignment_requests.create', 'training.assignment_requests.self.request'),
    ('training.assignment_requests.update', 'training.assignment_requests.decide'),
    ('training.assignment_requests.interest.create', 'training.assignment_requests.interest.request'),
    ('training.assignment_requests.interest.delete', 'training.assignment_requests.interest.delete'),
    ('training.release_requests.read', 'training.release_requests.read'),
    ('training.release_requests.create', 'training.release_requests.self.request'),
    ('training.release_requests.update', 'training.release_requests.decide'),
    ('feedback.items.read', 'feedback.items.self.read'),
    ('feedback.items.read', 'feedback.items.read'),
    ('feedback.items.create', 'feedback.items.create'),
    ('feedback.items.update', 'feedback.items.decide'),
    ('events.items.create', 'events.items.create'),
    ('events.items.update', 'events.items.update'),
    ('events.items.delete', 'events.items.delete'),
    ('events.positions.create', 'events.positions.self.request'),
    ('events.positions.update', 'events.positions.assign'),
    ('events.positions.delete', 'events.positions.delete'),
    ('events.positions.publish', 'events.positions.publish'),
    ('files.audit.read', 'files.audit.read'),
    ('files.assets.read', 'files.assets.read'),
    ('files.assets.create', 'files.assets.create'),
    ('files.assets.update', 'files.assets.update'),
    ('files.assets.update', 'files.assets.policy.update'),
    ('files.assets.delete', 'files.assets.delete'),
    ('files.content.read', 'files.content.read'),
    ('files.content.update', 'files.content.create'),
    ('files.content.update', 'files.content.update'),
    ('publications.categories.read', 'publications.categories.read'),
    ('publications.categories.create', 'publications.categories.create'),
    ('publications.categories.update', 'publications.categories.update'),
    ('publications.categories.delete', 'publications.categories.delete'),
    ('publications.items.read', 'publications.items.read'),
    ('publications.items.create', 'publications.items.create'),
    ('publications.items.update', 'publications.items.update'),
    ('publications.items.delete', 'publications.items.delete'),
    ('stats.artcc.read', 'stats.artcc.read'),
    ('stats.controller_events.read', 'stats.controller_events.read'),
    ('stats.controller_history.read', 'stats.controller_history.read'),
    ('stats.controller_totals.read', 'stats.controller_totals.read'),
    ('integrations.stats.update', 'integrations.stats.update'),
    ('system.read', 'system.read');

create temp table tmp_human_permission_grants_v2 on commit drop as
select distinct eup.user_id, map.target_name as permission_name
from access.v_effective_user_permissions eup
join tmp_permission_mapping_v2 map on map.source_name = eup.permission_name
where not exists (
    select 1
    from access.user_roles ur
    where ur.user_id = eup.user_id
      and ur.role_name = 'SERVER_ADMIN'
);

insert into tmp_human_permission_grants_v2 (user_id, permission_name)
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
        ('users.visit_artcc.request'),
        ('users.visitor_applications.self.read'),
        ('users.visitor_applications.self.request'),
        ('feedback.items.self.read'),
        ('feedback.items.create'),
        ('events.positions.self.request')
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
from tmp_human_permission_grants_v2
on conflict (user_id, permission_name) do update
set granted = true;

create temp table tmp_role_permissions_v2 on commit drop as
select
    rp.role_name,
    map.target_name as permission_name,
    min(rp.created_at) as created_at
from access.role_permissions rp
join tmp_permission_mapping_v2 map on map.source_name = rp.permission_name
group by rp.role_name, map.target_name;

delete from access.role_permissions;

insert into access.role_permissions (role_name, permission_name, created_at)
select role_name, permission_name, created_at
from tmp_role_permissions_v2
on conflict (role_name, permission_name) do nothing;

delete from access.permissions
where name not in (
    select name from tmp_permission_catalog_v2
);
