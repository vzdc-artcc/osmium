insert into access.permissions (name, description)
values
    ('training.assignments.update', 'Update training assignments'),
    ('training.assignments.delete', 'Delete training assignments'),
    ('training.assignment_requests.delete', 'Delete training assignment requests'),
    ('training.release_requests.delete', 'Delete trainer release requests')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'training.assignments.update'),
    ('STAFF', 'training.assignments.delete'),
    ('STAFF', 'training.assignment_requests.delete'),
    ('STAFF', 'training.release_requests.delete')
on conflict (role_name, permission_name) do nothing;
