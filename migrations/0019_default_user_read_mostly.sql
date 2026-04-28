insert into access.role_permissions (role_name, permission_name)
values
    ('USER', 'files.read'),
    ('STAFF', 'files.read')
on conflict (role_name, permission_name) do nothing;

delete from access.role_permissions
where role_name = 'USER'
  and permission_name = 'files.create';
