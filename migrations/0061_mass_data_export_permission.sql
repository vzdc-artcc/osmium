-- Admin mass data export (post-parity backlog): a SERVER_ADMIN bulk export of the
-- full GDPR data-export document for every on-roster controller, in one download.
--
-- Extremely sensitive — it is the entire roster's personal data in one payload —
-- so the permission is granted to NO role by default. Only SERVER_ADMIN holds it
-- (via the effective-permissions cross-join), and it is kept out of the assignable
-- catalog (acl.rs NON_ASSIGNABLE) so facility admins can't grant it onward.
insert into access.permissions (name, description)
values
    ('users.data_export.read', 'Bulk-export every on-roster controller''s full personal-data document')
on conflict (name) do nothing;
