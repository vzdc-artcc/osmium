insert into access.permissions (name, description)
values
    ('web.broadcasts.read', 'Read change broadcasts'),
    ('web.broadcasts.create', 'Create change broadcasts'),
    ('web.broadcasts.update', 'Update change broadcasts'),
    ('web.broadcasts.delete', 'Delete change broadcasts')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'web.broadcasts.read'),
    ('STAFF', 'web.broadcasts.create'),
    ('STAFF', 'web.broadcasts.update'),
    ('STAFF', 'web.broadcasts.delete')
on conflict (role_name, permission_name) do nothing;
