-- The website's roster page shows operating initials, HOME/VISITOR status,
-- and ARTCC for every controller to every logged-in controller, not just
-- staff. That data is gated by the `full` (UserPrivateInfo) half of
-- GET /api/v1/users — src/handlers/users.rs now gates it on the narrower
-- `users.directory.read` (added to the baseline self-service set new logins
-- get), kept deliberately separate from the staff-only
-- `users.directory_private.read` (admin endpoints + hidden_from_roster
-- bypass) so this grant can't be handed out broadly by accident. This
-- migration only needs to backfill everyone who logged in before that
-- baseline change.
insert into access.user_permissions (user_id, permission_name, granted)
select id, 'users.directory.read', true
from identity.users
on conflict (user_id, permission_name) do nothing;
