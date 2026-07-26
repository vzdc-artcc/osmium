insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'access.catalog.read'),
    ('STAFF', 'access.users.read'),
    ('STAFF', 'access.users.update')
on conflict (role_name, permission_name) do nothing;
