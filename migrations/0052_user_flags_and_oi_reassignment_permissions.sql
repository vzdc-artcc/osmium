-- Backs the website's admin "User Settings" form (opt-out flags: LOA/event/
-- training-assignment/trainer-release/progression-force-finish requests,
-- profile editing, VATUSA roster sync exclusion, hidden-from-roster) and
-- the oi-matrix admin tool's manual operating-initials reassignment, both
-- previously Prisma-only. identity.user_flags already has every column
-- needed (migration 0002/0006-era) — this only adds the permissions to
-- read/write it and to reassign operating initials administratively.
insert into access.permissions (name, description)
values
    ('users.flags.read', 'Read another user''s self-service opt-out flags'),
    ('users.flags.update', 'Update another user''s self-service opt-out flags'),
    ('users.operating_initials.update', 'Reassign a controller''s operating initials')
on conflict (name) do nothing;

-- Matches the live site's gating exactly: UserSettingsForm/oi-matrix are
-- both shown to any `Role.STAFF` holder (a broad coarse role), not just
-- ATM/DATM — granted to osmium's 'STAFF' role for the same reason
-- access.users.update (fine-grained permission editing) was in migration
-- 0047, not narrowed to ATM/DATM like the more sensitive roster-purge
-- permission in 0046.
insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'users.flags.read'),
    ('STAFF', 'users.flags.update'),
    ('STAFF', 'users.operating_initials.update')
on conflict (role_name, permission_name) do nothing;
