insert into access.permissions (name, description)
values
    ('training.assignment_requests.create', 'Create a training assignment request on behalf of another student (manual/backdated logging)')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'training.assignment_requests.create')
on conflict (role_name, permission_name) do nothing;
