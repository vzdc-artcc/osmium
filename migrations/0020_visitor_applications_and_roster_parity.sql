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
    p.show_welcome_message,
    m.join_date,
    m.home_facility,
    m.visitor_home_facility,
    m.is_active
from identity.users u
left join access.v_user_primary_role pr on pr.user_id = u.id
left join identity.user_profiles p on p.user_id = u.id
left join org.memberships m on m.user_id = u.id;
