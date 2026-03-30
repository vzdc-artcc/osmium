create table if not exists roles (
    name text primary key
);

create table if not exists permissions (
    name text primary key
);

create table if not exists role_permissions (
    role_name text not null references roles(name) on delete cascade,
    permission_name text not null references permissions(name) on delete cascade,
    primary key (role_name, permission_name)
);

create table if not exists user_roles (
    user_id text not null references users(id) on delete cascade,
    role_name text not null references roles(name) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (user_id, role_name)
);

create table if not exists user_permissions (
    user_id text not null references users(id) on delete cascade,
    permission_name text not null references permissions(name) on delete cascade,
    granted boolean not null default true,
    created_at timestamptz not null default now(),
    primary key (user_id, permission_name)
);

create index if not exists idx_user_roles_user_id on user_roles(user_id);
create index if not exists idx_user_permissions_user_id on user_permissions(user_id);

insert into roles (name)
values ('USER'), ('STAFF')
on conflict (name) do nothing;

insert into permissions (name)
values
    ('read_own_profile'),
    ('logout'),
    ('read_system_readiness'),
    ('manage_users'),
    ('dev_login_as_cid')
on conflict (name) do nothing;

insert into role_permissions (role_name, permission_name)
values
    ('USER', 'read_own_profile'),
    ('USER', 'logout'),
    ('STAFF', 'read_own_profile'),
    ('STAFF', 'logout'),
    ('STAFF', 'read_system_readiness'),
    ('STAFF', 'manage_users'),
    ('STAFF', 'dev_login_as_cid')
on conflict (role_name, permission_name) do nothing;

insert into user_roles (user_id, role_name)
select id,
       case
           when upper(role) = 'STAFF' then 'STAFF'
           else 'USER'
       end
from users
on conflict (user_id, role_name) do nothing;

