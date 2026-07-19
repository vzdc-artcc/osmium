insert into access.permissions (name, description)
values
    ('stats.prefixes.read', 'Read statistics prefixes'),
    ('stats.prefixes.update', 'Update statistics prefixes')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'stats.prefixes.read'),
    ('STAFF', 'stats.prefixes.update')
on conflict (role_name, permission_name) do nothing;
