-- The website's controller profile page (app/controllers/[cid]/page.tsx)
-- still reads bio/timezone/preferred_name/avatar straight from Prisma
-- because org.v_user_roster_profile exposed bio/timezone/avatar_asset_id
-- but never u.preferred_name. Add it so GET /api/v1/users/{cid} can return
-- everything that page needs.
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
    m.is_active,
    coalesce(f.hidden_from_roster, false) as hidden_from_roster,
    u.preferred_name
from identity.users u
left join access.v_user_primary_role pr on pr.user_id = u.id
left join identity.user_profiles p on p.user_id = u.id
left join org.memberships m on m.user_id = u.id
left join identity.user_flags f on f.user_id = u.id;
