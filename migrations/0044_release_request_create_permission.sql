insert into access.permissions (name, description)
values
    ('training.release_requests.create', 'Create a trainer release request on behalf of another student (trainer-initiated release)')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'training.release_requests.create')
on conflict (role_name, permission_name) do nothing;
