-- access.user_roles has been a purely-manual table so far (the one-time
-- SERVER_ADMIN claim, dev seeds). Slice 5 of the Users/Roster migration
-- auto-syncs STAFF/INS/MTR from VATUSA facility roles, mirroring the
-- manual-override-respecting pattern already built for
-- identity.staff_positions (see set_staff_position_auto/manual in
-- src/repos/users.rs). Default 'manual' preserves every existing row as
-- never-auto-touched.
alter table access.user_roles
    add column if not exists source text not null default 'manual'
        check (source in ('auto', 'manual')),
    add column if not exists updated_by text,
    add column if not exists updated_at timestamptz not null default now();
