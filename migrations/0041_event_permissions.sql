-- Two real gaps found while building out the events domain to website parity:
-- 1. STAFF was missing events.items.create, events.items.delete, and
--    events.positions.delete — it could update events/assign positions/publish
--    positions but not create or delete an event, nor delete a position. Every
--    STAFF-only test in this repo passed anyway because dev/test sessions used
--    SERVER_ADMIN, which bypasses role_permissions entirely.
-- 2. EVENT_STAFF (a real, distinct role the live site's events-admin section
--    gates on alongside STAFF) had zero permission grants at all in this
--    schema — a role that existed but could do nothing.
insert into access.permissions (name, description)
values
    ('events.presets.read', 'Read named event position preset bundles'),
    ('events.presets.create', 'Create named event position preset bundles'),
    ('events.presets.update', 'Update named event position preset bundles'),
    ('events.presets.delete', 'Delete named event position preset bundles'),
    ('events.ops_plan_files.create', 'Attach files to an event ops plan'),
    ('events.ops_plan_files.delete', 'Remove files from an event ops plan')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'events.items.create'),
    ('STAFF', 'events.items.delete'),
    ('STAFF', 'events.positions.delete'),
    ('STAFF', 'events.presets.read'),
    ('STAFF', 'events.presets.create'),
    ('STAFF', 'events.presets.update'),
    ('STAFF', 'events.presets.delete'),
    ('STAFF', 'events.ops_plan_files.create'),
    ('STAFF', 'events.ops_plan_files.delete'),
    ('EVENT_STAFF', 'events.items.create'),
    ('EVENT_STAFF', 'events.items.update'),
    ('EVENT_STAFF', 'events.items.delete'),
    ('EVENT_STAFF', 'events.positions.assign'),
    ('EVENT_STAFF', 'events.positions.delete'),
    ('EVENT_STAFF', 'events.positions.publish'),
    ('EVENT_STAFF', 'events.presets.read'),
    ('EVENT_STAFF', 'events.presets.create'),
    ('EVENT_STAFF', 'events.presets.update'),
    ('EVENT_STAFF', 'events.presets.delete'),
    ('EVENT_STAFF', 'events.ops_plan_files.create'),
    ('EVENT_STAFF', 'events.ops_plan_files.delete')
on conflict (role_name, permission_name) do nothing;
