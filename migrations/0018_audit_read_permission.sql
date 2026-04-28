insert into access.permissions (name, description)
values ('audit.read', 'Read audit logs')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values ('STAFF', 'audit.read')
on conflict (role_name, permission_name) do nothing;
