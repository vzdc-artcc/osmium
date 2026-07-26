-- Certifications & Progression domain (the last website->osmium migration
-- domain). All tables already exist (org.certification_types /
-- certification_type_allowed_options / user_certifications from 0004_org.sql;
-- training.training_progressions / _steps / user_progressions from
-- 0006_training_curriculum.sql). This only adds the permissions gating the
-- new write side: certification-type CRUD and writing a controller's
-- certification grid (org.rs::save_user_certifications). Reading a user's
-- grid stays data-dependent (self via auth.profile.read, else
-- users.directory.read) and is intentionally not gated by these.
insert into access.permissions (name, description)
values
    ('org.certifications.read', 'List certification types and their allowed options'),
    ('org.certifications.update', 'Create/update/reorder/delete certification types and write a controller''s certifications')
on conflict (name) do nothing;

-- Matches the live site's gating: the certification-type manager and the
-- controller certification editor are both shown to any `Role.STAFF` holder,
-- so both permissions go to osmium's 'STAFF' role. SERVER_ADMIN bypasses via
-- the server_admin short-circuit and needs no explicit grant.
insert into access.role_permissions (role_name, permission_name)
values
    ('STAFF', 'org.certifications.read'),
    ('STAFF', 'org.certifications.update')
on conflict (role_name, permission_name) do nothing;
