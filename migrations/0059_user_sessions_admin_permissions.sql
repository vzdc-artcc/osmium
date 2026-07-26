-- User session manager (post-parity backlog): admin list/revoke of a user's
-- osmium auth sessions (identity.sessions).
--
-- Security-sensitive (reveals login IPs/times and can force-logout a user), so the
-- permissions are granted to NO role by default — only SERVER_ADMIN holds them
-- (via the effective-permissions cross-join), and the actor-scope guard on the
-- access editor keeps facility admins from granting them onward. Home is the
-- SERVER_ADMIN-gated Website Management area. Raw session tokens are never exposed.
insert into access.permissions (name, description)
values
    ('users.sessions.read', 'List a user''s active auth sessions (metadata only, never tokens)'),
    ('users.sessions.delete', 'Revoke a user''s auth sessions (single or all)')
on conflict (name) do nothing;
