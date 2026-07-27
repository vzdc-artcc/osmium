-- Explicit per-section staff-page access permissions.
--
-- Staff-page (tab) access is gated on these permissions, NOT on coarse roles and
-- NOT on staff positions (which are roster-only display tags). One permission per
-- staff section so each can be granted/revoked independently — per role or per
-- user — in the permissions UI:
--   pages.facility_admin.read    -> the Facility Admin section (/admin)
--   pages.training_admin.read    -> the Training Admin section (/training)
--   pages.event_management.read  -> the Event Management section (/events/admin)
--
-- They are seeded onto the roles that previously had implicit access so nobody is
-- locked out at cutover, but access is now an explicit, editable permission grant.
-- SERVER_ADMIN holds them automatically via the effective-permissions cross-join.
insert into access.permissions (name, description)
values
    ('pages.facility_admin.read', 'Access the Facility Admin section'),
    ('pages.training_admin.read', 'Access the Training Admin section'),
    ('pages.event_management.read', 'Access the Event Management section')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'pages.facility_admin.read'),
    ('STAFF', 'pages.training_admin.read'),
    ('INS', 'pages.training_admin.read'),
    ('MTR', 'pages.training_admin.read'),
    ('STAFF', 'pages.event_management.read'),
    ('EVENT_STAFF', 'pages.event_management.read')
on conflict do nothing;
