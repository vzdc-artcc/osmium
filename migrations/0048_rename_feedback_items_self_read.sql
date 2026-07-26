-- feedback.items.self.read was a strict path-prefix of feedback.items
-- (feedback.items.create/decide) in the JSON permission tree used by
-- GET /admin/access/catalog and GET/POST /admin/users/{cid}/access
-- (src/auth/acl.rs). A tree node is either a leaf action-array or a parent
-- of further segments, never both, so whichever of the two permissions got
-- inserted into the tree second was silently dropped. Every user holds both
-- from their first-login baseline grant, so this was live, reproducible
-- data loss on every request that serialized a user's feedback permissions.
-- Renaming the deeper one to feedback.items_self.read removes the prefix
-- collision entirely (no permission path is ever a prefix of another).
insert into access.permissions (name, description)
values ('feedback.items_self.read', 'Read own feedback items')
on conflict (name) do nothing;

update access.role_permissions
set permission_name = 'feedback.items_self.read'
where permission_name = 'feedback.items.self.read';

update access.user_permissions
set permission_name = 'feedback.items_self.read'
where permission_name = 'feedback.items.self.read';

update access.service_account_permissions
set permission_name = 'feedback.items_self.read'
where permission_name = 'feedback.items.self.read';

delete from access.permissions where name = 'feedback.items.self.read';
