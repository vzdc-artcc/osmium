insert into access.permissions (name, description)
values
    ('org.staffing_requests.read', 'Read staffing requests in the admin queue'),
    ('org.staffing_requests.delete', 'Delete (resolve) a staffing request')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'org.staffing_requests.read'),
    ('STAFF', 'org.staffing_requests.delete'),
    ('EVENT_STAFF', 'org.staffing_requests.read'),
    ('EVENT_STAFF', 'org.staffing_requests.delete')
on conflict (role_name, permission_name) do nothing;
