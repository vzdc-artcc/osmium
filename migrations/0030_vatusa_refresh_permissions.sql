insert into access.permissions (name, description)
values
    ('users.vatusa_refresh.self.request', 'Request own VATUSA roster refresh'),
    ('users.vatusa_refresh.request', 'Request manual VATUSA roster refresh for any user')
on conflict (name) do nothing;

insert into access.role_permissions (role_name, permission_name)
values
    ('SERVER_ADMIN', 'users.vatusa_refresh.request'),
    ('STAFF', 'users.vatusa_refresh.request')
on conflict (role_name, permission_name) do nothing;

insert into access.user_permissions (user_id, permission_name, granted)
select u.id, 'users.vatusa_refresh.self.request', true
from identity.users u
where not exists (
    select 1
    from access.user_permissions up
    where up.user_id = u.id
      and up.permission_name = 'users.vatusa_refresh.self.request'
);
