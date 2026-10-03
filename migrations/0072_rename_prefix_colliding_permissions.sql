-- Same defect 0048 fixed for feedback.items_self.read, for every remaining
-- pair: a permission whose path is a strict prefix of another's (e.g.
-- training.assignment_requests vs training.assignment_requests.self) cannot
-- both appear in the nested permission tree (src/auth/acl.rs), because a node
-- is either a leaf action array or a parent of further segments. The access
-- editor posts that tree back as the user's full grant set, so a save
-- silently revoked whichever permission the tree dropped. Underscore-joining
-- the deeper segment removes the prefix.
--
-- Order matters: the grant tables reference access.permissions(name) with
-- `on delete cascade` and no `on update`, so the new names must exist and
-- every grant must move before the old names are deleted.
create temporary table tmp_permission_renames (old_name text primary key, new_name text not null)
on commit drop;

insert into tmp_permission_renames (old_name, new_name) values
    ('training.assignment_requests.self.request', 'training.assignment_requests_self.request'),
    ('training.assignment_requests.interest.request', 'training.assignment_requests_interest.request'),
    ('training.assignment_requests.interest.delete', 'training.assignment_requests_interest.delete'),
    ('training.release_requests.self.request', 'training.release_requests_self.request'),
    ('events.positions.self.request', 'events.positions_self.request'),
    ('users.vatusa_refresh.self.request', 'users.vatusa_refresh_self.request'),
    ('users.visitor_applications.self.read', 'users.visitor_applications_self.read'),
    ('users.visitor_applications.self.request', 'users.visitor_applications_self.request'),
    ('files.assets.policy.update', 'files.assets_policy.update');

insert into access.permissions (name, description)
select r.new_name, p.description
from tmp_permission_renames r
join access.permissions p on p.name = r.old_name
on conflict (name) do nothing;

update access.role_permissions rp
set permission_name = r.new_name
from tmp_permission_renames r
where rp.permission_name = r.old_name;

update access.user_permissions up
set permission_name = r.new_name
from tmp_permission_renames r
where up.permission_name = r.old_name;

update access.service_account_permissions sp
set permission_name = r.new_name
from tmp_permission_renames r
where sp.permission_name = r.old_name;

delete from access.permissions p
using tmp_permission_renames r
where p.name = r.old_name;
