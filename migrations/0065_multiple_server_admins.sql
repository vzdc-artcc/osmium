-- SERVER_ADMIN is no longer a singleton. Multiple CIDs can be configured as
-- server admins via OSMIUM_SERVER_ADMIN_CID (comma-separated), so more than one
-- access.user_roles row may hold the role. Drop the single-admin unique index
-- (migration 0023) that previously allowed only one holder.
--
-- The effective-permission and primary-role views already handle multiple
-- SERVER_ADMIN users correctly (they select all such users), so no view changes
-- are needed. SERVER_ADMIN remains env-only — it is filtered out of the
-- assignable permissions/roles UI catalog.
drop index if exists access.idx_access_single_server_admin;

update access.roles
set description = 'Server administrator role (env-configured via OSMIUM_SERVER_ADMIN_CID; one or more CIDs)'
where name = 'SERVER_ADMIN';
