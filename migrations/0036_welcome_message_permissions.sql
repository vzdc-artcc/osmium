insert into access.permissions (name, description)
values
    ('web.welcome_messages.read', 'Read welcome message content'),
    ('web.welcome_messages.update', 'Update welcome message content')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'web.welcome_messages.read'),
    ('STAFF', 'web.welcome_messages.update')
on conflict (role_name, permission_name) do nothing;
