-- Add dedicated permission for viewing full user profiles.

insert into permissions (name)
values ('view_all_users')
on conflict (name) do nothing;

insert into role_permissions (role_name, permission_name)
values ('STAFF', 'view_all_users')
on conflict (role_name, permission_name) do nothing;

