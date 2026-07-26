insert into access.permissions (name, description)
values
    ('users.controller_status.delete', 'Purge a controller from the roster (VATUSA removal + cleanup)')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('ATM', 'users.controller_status.delete'),
    ('DATM', 'users.controller_status.delete')
on conflict (role_name, permission_name) do nothing;
