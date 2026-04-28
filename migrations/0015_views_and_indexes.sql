create or replace view access.v_user_primary_role as
select
    ur.user_id,
    (
        array_agg(
            ur.role_name
            order by
                case
                    when ur.role_name = 'STAFF' then 0
                    when ur.role_name = 'USER' then 1
                    else 2
                end,
                ur.role_name
        )
    )[1] as primary_role
from access.user_roles ur
group by ur.user_id;

create or replace view access.v_effective_user_permissions as
with role_permissions as (
    select distinct ur.user_id, rp.permission_name
    from access.user_roles ur
    join access.role_permissions rp on rp.role_name = ur.role_name
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
    select * from granted_permissions
)
select cp.user_id, cp.permission_name
from candidate_permissions cp
left join denied_permissions dp
    on dp.user_id = cp.user_id
   and dp.permission_name = cp.permission_name
where dp.user_id is null;

create or replace view access.v_effective_service_account_permissions as
select distinct sar.service_account_id, rp.permission_name
from access.service_account_roles sar
join access.role_permissions rp on rp.role_name = sar.role_name
where sar.ends_at is null or sar.ends_at > now();

create or replace view org.v_user_roster_profile as
select
    u.id,
    u.cid,
    u.email,
    u.display_name,
    coalesce(pr.primary_role, 'USER') as role,
    u.first_name,
    u.last_name,
    m.artcc,
    m.rating,
    m.division,
    m.controller_status,
    u.status,
    m.membership_status,
    m.operating_initials,
    p.bio,
    p.avatar_asset_id,
    p.timezone,
    p.preferences,
    p.receive_email,
    p.new_event_notifications,
    p.show_welcome_message
from identity.users u
left join access.v_user_primary_role pr on pr.user_id = u.id
left join identity.user_profiles p on p.user_id = u.id
left join org.memberships m on m.user_id = u.id;

create or replace view training.v_active_assignments as
select
    ta.id,
    ta.student_id,
    ta.primary_trainer_id,
    array_remove(array_agg(taot.trainer_id), null) as other_trainer_ids,
    ta.created_at,
    ta.updated_at
from training.training_assignments ta
left join training.training_assignment_other_trainers taot on taot.assignment_id = ta.id
group by ta.id;

create or replace view events.v_event_staffing_summary as
select
    e.id as event_id,
    count(ep.id) as total_positions,
    count(*) filter (where ep.user_id is not null) as assigned_positions,
    count(*) filter (where ep.published is true) as published_positions
from events.events e
left join events.event_positions ep on ep.event_id = e.id
group by e.id;

create index if not exists idx_identity_user_identities_metadata on identity.user_identities using gin(metadata);
create index if not exists idx_identity_user_profiles_preferences on identity.user_profiles using gin(preferences);
create index if not exists idx_web_pages_body on web.pages using gin(body);
create index if not exists idx_web_announcements_body on web.announcements using gin(body);
create index if not exists idx_integration_webhook_payload on integration.webhook_deliveries using gin(payload);
create index if not exists idx_media_file_audit_details on media.file_audit_logs using gin(details);
create index if not exists idx_active_user_roles on access.user_roles(user_id, role_name);
create index if not exists idx_active_service_roles on access.service_account_roles(service_account_id, role_name)
    where ends_at is null;
create index if not exists idx_active_assets on media.file_assets(created_at desc)
    where deleted_at is null;
