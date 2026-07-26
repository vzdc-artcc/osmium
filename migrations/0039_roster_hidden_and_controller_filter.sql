-- org.v_user_roster_profile never exposed identity.user_flags.hidden_from_roster,
-- so the public roster listing (GET /api/v1/users) could not exclude
-- roster-hidden users or filter to active controllers only, unlike the
-- live site's equivalent queries. Append hidden_from_roster to the view so
-- the roster listing can filter on it.
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
    coalesce(f.hidden_from_roster, false) as hidden_from_roster
from identity.users u
left join access.v_user_primary_role pr on pr.user_id = u.id
left join identity.user_profiles p on p.user_id = u.id
left join org.memberships m on m.user_id = u.id
left join identity.user_flags f on f.user_id = u.id;
