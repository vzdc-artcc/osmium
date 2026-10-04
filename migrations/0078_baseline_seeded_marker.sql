-- Baseline self-service permissions used to be seeded only when a login created
-- the identity.users row. Accounts the legacy migrator created already existed,
-- so their first osmium login never seeded them and every self-service action
-- was refused. Mark which accounts have been seeded, and seed every existing
-- account once now. Login seeds any account still unmarked, then never touches
-- its direct permissions again, so a baseline permission an admin removes stays
-- removed.
--
-- The list covers BASELINE_SELF_SERVICE_PERMISSIONS in src/handlers/auth.rs (a
-- test checks it), and also carries the underscore forms of the four `.self`
-- names that migration 0073 renames. Only names present in access.permissions
-- are granted, so this is correct whether or not that rename has run.
-- `on conflict do nothing` keeps any existing row, including an explicit deny.
alter table identity.users add column if not exists baseline_seeded_at timestamptz;

insert into access.user_permissions (user_id, permission_name, granted)
select u.id, baseline.permission_name, true
from identity.users u
cross join (values
    ('auth.profile.read'),
    ('auth.profile.update'),
    ('auth.teamspeak_uids.read'),
    ('auth.teamspeak_uids.create'),
    ('auth.teamspeak_uids.delete'),
    ('auth.sessions.delete'),
    ('users.vatusa_refresh.self.request'),
    ('users.vatusa_refresh_self.request'),
    ('users.visit_artcc.request'),
    ('users.visitor_applications.self.read'),
    ('users.visitor_applications_self.read'),
    ('users.visitor_applications.self.request'),
    ('users.visitor_applications_self.request'),
    ('users.directory.read'),
    ('feedback.items_self.read'),
    ('feedback.items.create'),
    ('events.positions.self.request'),
    ('events.positions_self.request')
) as baseline(permission_name)
join access.permissions p on p.name = baseline.permission_name
where u.baseline_seeded_at is null
on conflict (user_id, permission_name) do nothing;

update identity.users set baseline_seeded_at = now() where baseline_seeded_at is null;
