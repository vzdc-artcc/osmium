insert into access.permissions (name, description)
values
    ('training.read', 'Read training data'),
    ('training.create', 'Create training data'),
    ('training.update', 'Update training data'),
    ('training.manage', 'Manage training workflows')
on conflict (name) do update
set description = excluded.description,
    updated_at = now();

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'training.read'),
    ('STAFF', 'training.create'),
    ('STAFF', 'training.update'),
    ('STAFF', 'training.manage')
on conflict (role_name, permission_name) do nothing;
