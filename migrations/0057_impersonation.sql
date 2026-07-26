-- Authenticated user impersonation (spec 012 — impersonation feature).
--
-- A single session row toggles between the admin and the impersonated target:
-- while impersonating, `user_id` is the target (so ACL/UI resolve as them) and
-- `impersonator_user_id` records the real admin for accountability and stop. On
-- stop, `user_id` is restored to `impersonator_user_id` and the impersonation
-- columns are cleared. This keeps a single httpOnly cookie and never leaves a
-- durable orphaned "as target" session (security checklist #5, #8).
alter table identity.sessions
    add column impersonator_user_id text references identity.users(id) on delete set null,
    add column impersonation_started_at timestamptz,
    add column impersonation_reason text;

create index if not exists idx_identity_sessions_impersonator
    on identity.sessions (impersonator_user_id)
    where impersonator_user_id is not null;

-- Register the impersonation permission. Granted to NO role: SERVER_ADMIN holds it
-- implicitly (the `v_effective_user_permissions` cross-join gives every server admin
-- every permission), and no facility role should hold it (checklist #9). Because
-- only SERVER_ADMIN effectively holds it, the actor-scope guard on the access editor
-- also prevents any facility admin from granting it onward.
insert into access.permissions (name, description)
values (
    'auth.impersonate.create',
    'Start authenticated impersonation of another user (SERVER_ADMIN only)'
)
on conflict (name) do nothing;

-- Retire the dev-only login-as permission that the impersonation feature replaces.
-- The route + flag are deleted in the same change set; drop the conceptual
-- permission (cascades to any stray grants) so it can't be referenced.
delete from access.permissions where name = 'auth.dev_login.create';
